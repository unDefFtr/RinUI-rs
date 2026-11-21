[CmdletBinding()]
param(
    [string]$SourceDirectory = "dist\RinUIGallery-win64",
    [string]$OutputDirectory = "dist\msix",
    [string]$ManifestTemplate = "packaging\windows\AppxManifest.template.xml",
    [string]$AssetsDirectory = "packaging\windows\assets",
    [string]$IdentityName = "io.rinui.gallery.rs",
    [string]$Publisher = "CN=RinUI.rs",
    [string]$PublisherDisplayName = "rinui-rs",
    [string]$DisplayName = "RinUI Gallery",
    [string]$Description = "RinUI Gallery (rinui-rs)",
    [string]$ApplicationId = "RinUIGallery",
    [string]$Executable = "rinui-gallery.exe",
    [ValidateSet("x64", "arm64", "x86")]
    [string]$Architecture = "x64",
    [string]$Version = "",
    [string]$ResourceLanguage = "en-US",
    [string]$MinWindowsVersion = "10.0.19041.0",
    [string]$MaxVersionTested = "10.0.26100.0",
    [string]$PackageFilePrefix = "RinUI.Gallery",
    [switch]$SkipSign,
    [switch]$VerifyLayout,
    [string]$CertPfx = "",
    [string]$CertPassword = "",
    [string]$CertThumbprint = "",
    [string]$TimestampUrl = "",
    [switch]$NoTimestamp
)

$ErrorActionPreference = "Stop"

# PowerShell 5.1 and 7 compatible environment fallbacks (no ??, &&, ||, or ?:)
if (-not $CertPfx -and $env:RINUI_WIN_CERT_PFX) {
    $CertPfx = $env:RINUI_WIN_CERT_PFX
}
if (-not $CertPassword -and $env:RINUI_WIN_CERT_PASSWORD) {
    $CertPassword = $env:RINUI_WIN_CERT_PASSWORD
}
if (-not $CertThumbprint -and $env:RINUI_WIN_CERT_THUMBPRINT) {
    $CertThumbprint = $env:RINUI_WIN_CERT_THUMBPRINT
}
if (-not $TimestampUrl -and $env:RINUI_WIN_TIMESTAMP_URL) {
    $TimestampUrl = $env:RINUI_WIN_TIMESTAMP_URL
}
if (-not $TimestampUrl -and -not $NoTimestamp) {
    $TimestampUrl = "http://timestamp.digicert.com"
}

$repoRoot = (Resolve-Path (Split-Path -Parent $PSScriptRoot)).Path

function Resolve-RepoPath([string]$Candidate) {
    if ([IO.Path]::IsPathRooted($Candidate)) {
        return [IO.Path]::GetFullPath($Candidate)
    }
    return [IO.Path]::GetFullPath((Join-Path $repoRoot $Candidate))
}

function Get-WorkspaceVersion([string]$CargoTomlPath) {
    if (-not (Test-Path -LiteralPath $CargoTomlPath)) {
        throw "Workspace Cargo.toml not found at $CargoTomlPath"
    }
    $inWorkspacePkg = $false
    foreach ($line in Get-Content -LiteralPath $CargoTomlPath) {
        $trimmed = $line.Trim()
        if ($trimmed -match '^\[(.+)\]$') {
            $inWorkspacePkg = ($Matches[1] -eq "workspace.package")
            continue
        }
        if ($inWorkspacePkg -and $trimmed -match '^version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }
    throw "Could not locate [workspace.package] version in $CargoTomlPath"
}

function ConvertTo-MsixVersion([string]$RawVersion) {
    $clean = $RawVersion.Trim()
    # Strip SemVer build metadata (+...)
    $plusIdx = $clean.IndexOf("+")
    if ($plusIdx -ge 0) {
        $clean = $clean.Substring(0, $plusIdx)
    }

    $fourParts = $null
    if ($clean -match '^(\d+)\.(\d+)\.(\d+)\.(\d+)$') {
        $fourParts = @([int]$Matches[1], [int]$Matches[2], [int]$Matches[3], [int]$Matches[4])
    } elseif ($clean -match '^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$') {
        $maj = [int]$Matches[1]
        $min = [int]$Matches[2]
        $pat = [int]$Matches[3]
        $rev = 0
        if ($Matches[4]) {
            $pre = $Matches[4]
            if ($pre -match '(?:^|\.)(\d+)$') {
                $rev = [int]$Matches[1]
            }
        }
        $fourParts = @($maj, $min, $pat, $rev)
    } else {
        throw "Version '$RawVersion' cannot be mapped to a 4-part MSIX numeric version (A.B.C.D or X.Y.Z[-tag.N])."
    }

    foreach ($part in $fourParts) {
        if ($part -lt 0 -or $part -gt 65535) {
            throw "MSIX version component '$part' in '$RawVersion' is outside valid range 0..65535."
        }
    }
    return ($fourParts -join ".")
}

function Find-WindowsSdkTool([string]$ToolName, [string]$PreferredArch) {
    $cmd = Get-Command $ToolName -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }

    $kitRoots = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
        "C:\Program Files\Windows Kits\10\bin"
    )
    $searchArches = @($PreferredArch, "x64", "x86", "arm64") | Select-Object -Unique
    foreach ($root in $kitRoots) {
        if (-not (Test-Path -LiteralPath $root)) {
            continue
        }
        foreach ($archName in $searchArches) {
            $candidate = Get-ChildItem -LiteralPath $root -Recurse -File -Filter $ToolName -ErrorAction SilentlyContinue |
                Where-Object { $_.FullName -match ("\\" + [regex]::Escape($archName) + "\\") } |
                Sort-Object FullName -Descending |
                Select-Object -First 1
            if ($candidate) {
                return $candidate.FullName
            }
        }
    }
    throw "Required Windows SDK tool '$ToolName' was not found on PATH or under Windows Kits\10\bin."
}

$sourceDir = Resolve-RepoPath $SourceDirectory
$outputDir = Resolve-RepoPath $OutputDirectory
$templatePath = Resolve-RepoPath $ManifestTemplate
$assetsDir = Resolve-RepoPath $AssetsDirectory

if (-not (Test-Path -LiteralPath $sourceDir -PathType Container)) {
    throw "Packaged Windows input directory not found: $sourceDir (run scripts/package_windows.ps1 first)."
}
$sourceExe = Join-Path $sourceDir $Executable
if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
    throw "Expected executable '$Executable' not found in source directory '$sourceDir'."
}
if (-not (Test-Path -LiteralPath $templatePath -PathType Leaf)) {
    throw "Manifest template not found: $templatePath"
}
foreach ($assetName in @("StoreLogo.png", "Square44x44Logo.png", "Square150x150Logo.png")) {
    $assetFile = Join-Path $assetsDir $assetName
    if (-not (Test-Path -LiteralPath $assetFile -PathType Leaf)) {
        throw "Required MSIX store icon asset missing: $assetFile"
    }
}

if (-not $Version) {
    $Version = Get-WorkspaceVersion (Join-Path $repoRoot "Cargo.toml")
}
$msixVersion = ConvertTo-MsixVersion $Version

if ($ApplicationId -notmatch '^[A-Za-z0-9.]+$') {
    throw "Invalid ApplicationId '$ApplicationId': must match [A-Za-z0-9.]+."
}
if (-not $IdentityName -or $IdentityName -match '\s') {
    throw "Invalid IdentityName '$IdentityName': must be non-empty without whitespace."
}

if (-not $SkipSign) {
    if (-not $CertPfx -and -not $CertThumbprint) {
        throw "MSIX signing requested (without -SkipSign), but neither -CertPfx/RINUI_WIN_CERT_PFX nor -CertThumbprint/RINUI_WIN_CERT_THUMBPRINT was supplied."
    }
    if ($CertPfx -and -not (Test-Path -LiteralPath $CertPfx -PathType Leaf)) {
        throw "Authenticode certificate PFX file not found: $CertPfx"
    }
}

$makeappx = Find-WindowsSdkTool "makeappx.exe" $Architecture
$signtool = $null
if (-not $SkipSign) {
    $signtool = Find-WindowsSdkTool "signtool.exe" $Architecture
}

New-Item -ItemType Directory -Path $outputDir -Force | Out-Null

$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("rinui-msix-" + [Guid]::NewGuid().ToString("N"))
$stageDir = Join-Path $workRoot "stage"
$unpackDir = Join-Path $workRoot "unpack"

try {
    New-Item -ItemType Directory -Path $stageDir -Force | Out-Null
    Copy-Item -Path (Join-Path $sourceDir "*") -Destination $stageDir -Recurse -Force

    $stageAssets = Join-Path $stageDir "assets"
    New-Item -ItemType Directory -Path $stageAssets -Force | Out-Null
    foreach ($assetName in @("StoreLogo.png", "Square44x44Logo.png", "Square150x150Logo.png")) {
        Copy-Item -LiteralPath (Join-Path $assetsDir $assetName) -Destination (Join-Path $stageAssets $assetName) -Force
    }

    $manifestText = Get-Content -LiteralPath $templatePath -Raw -Encoding UTF8
    $replacements = @{
        "__IDENTITY_NAME__" = [System.Security.SecurityElement]::Escape($IdentityName)
        "__PUBLISHER__" = [System.Security.SecurityElement]::Escape($Publisher)
        "__VERSION__" = [System.Security.SecurityElement]::Escape($msixVersion)
        "__ARCHITECTURE__" = [System.Security.SecurityElement]::Escape($Architecture)
        "__DISPLAY_NAME__" = [System.Security.SecurityElement]::Escape($DisplayName)
        "__PUBLISHER_DISPLAY_NAME__" = [System.Security.SecurityElement]::Escape($PublisherDisplayName)
        "__DESCRIPTION__" = [System.Security.SecurityElement]::Escape($Description)
        "__RESOURCE_LANGUAGE__" = [System.Security.SecurityElement]::Escape($ResourceLanguage)
        "__MIN_WINDOWS_VERSION__" = [System.Security.SecurityElement]::Escape($MinWindowsVersion)
        "__MAX_WINDOWS_VERSION_TESTED__" = [System.Security.SecurityElement]::Escape($MaxVersionTested)
        "__APPLICATION_ID__" = [System.Security.SecurityElement]::Escape($ApplicationId)
        "__EXECUTABLE__" = [System.Security.SecurityElement]::Escape($Executable)
    }
    foreach ($key in $replacements.Keys) {
        $manifestText = $manifestText.Replace($key, $replacements[$key])
    }
    if ($manifestText -match '__[A-Z0-9_]+__') {
        throw "Unresolved placeholder remaining in rendered AppxManifest.xml: $($Matches[0])"
    }

    $manifestOut = Join-Path $stageDir "AppxManifest.xml"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($manifestOut, $manifestText, $utf8NoBom)

    $suffix = if ($SkipSign) { "-unsigned.msix" } else { ".msix" }
    $msixName = "$PackageFilePrefix-$msixVersion-$Architecture$suffix"
    $msixPath = Join-Path $outputDir $msixName
    if (Test-Path -LiteralPath $msixPath) {
        Remove-Item -LiteralPath $msixPath -Force
    }

    & $makeappx pack /o /d $stageDir /p $msixPath
    if ($LASTEXITCODE -ne 0) {
        throw "makeappx pack failed with exit code $LASTEXITCODE"
    }

    if ($VerifyLayout) {
        New-Item -ItemType Directory -Path $unpackDir -Force | Out-Null
        & $makeappx unpack /o /p $msixPath /d $unpackDir
        if ($LASTEXITCODE -ne 0) {
            throw "makeappx unpack failed with exit code $LASTEXITCODE"
        }

        $expectedEntries = @(
            "AppxManifest.xml",
            $Executable,
            "platforms\qwindows.dll",
            "LICENSE",
            "LICENSE.RinUI",
            "THIRD_PARTY_NOTICES.md",
            "assets\StoreLogo.png",
            "assets\Square44x44Logo.png",
            "assets\Square150x150Logo.png"
        )
        if ($env:RINUI_QT_LICENSES) {
            $expectedEntries += @("LICENSE.FluentUI", "qt-licenses\GPL-3.0-only.txt", "qt-licenses\LGPL-3.0-only.txt", "qt-licenses\SOURCE.txt")
        }
        foreach ($rel in $expectedEntries) {
            $full = Join-Path $unpackDir $rel
            if (-not (Test-Path -LiteralPath $full)) {
                throw "MSIX layout verification failed: missing '$rel' in unpacked package"
            }
        }
        [xml]$unpackedManifest = Get-Content -Raw -LiteralPath (Join-Path $unpackDir "AppxManifest.xml")
        if ($unpackedManifest.Package.Identity.Version -ne $msixVersion -or
            $unpackedManifest.Package.Identity.ProcessorArchitecture -ne $Architecture) {
            throw "MSIX layout verification failed: unexpected version or architecture in AppxManifest.xml"
        }
        $qt5Compat = Get-ChildItem -Path $unpackDir -Recurse -Filter "GraphicalEffects" -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match "Qt5Compat[\\/]GraphicalEffects$" }
        if (-not $qt5Compat) {
            throw "MSIX layout verification failed: Qt5Compat\GraphicalEffects missing from unpacked package"
        }
        Write-Host "MSIX layout unpack verification PASSED (exe, Qt plugins, QML GraphicalEffects, assets, licenses, AppxManifest.xml)"
    }

    if ($SkipSign) {
        Write-Host "Packaged unsigned MSIX at $msixPath (Identity=$IdentityName, Version=$msixVersion, Arch=$Architecture; signing skipped - install/launch NOT verified)"
    } else {
        $signArgs = New-Object System.Collections.Generic.List[string]
        $signArgs.Add("sign")
        $signArgs.Add("/fd")
        $signArgs.Add("SHA256")
        if ($CertPfx) {
            $signArgs.Add("/f")
            $signArgs.Add($CertPfx)
            if ($CertPassword) {
                $signArgs.Add("/p")
                $signArgs.Add($CertPassword)
            }
        } elseif ($CertThumbprint) {
            $signArgs.Add("/sha1")
            $signArgs.Add($CertThumbprint)
        }
        if (-not $NoTimestamp -and $TimestampUrl) {
            $signArgs.Add("/td")
            $signArgs.Add("SHA256")
            $signArgs.Add("/tr")
            $signArgs.Add($TimestampUrl)
        }
        $signArgs.Add($msixPath)

        & $signtool @signArgs
        if ($LASTEXITCODE -ne 0) {
            throw "signtool sign failed with exit code $LASTEXITCODE"
        }

        & $signtool verify /pa $msixPath
        if ($LASTEXITCODE -ne 0) {
            throw "signtool verify /pa failed with exit code $LASTEXITCODE"
        }
        Write-Host "Packaged and Authenticode-signed MSIX at $msixPath (Identity=$IdentityName, Version=$msixVersion, Arch=$Architecture)"
    }
} finally {
    if (Test-Path -LiteralPath $workRoot) {
        Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
