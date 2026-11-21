param(
    [string]$BinaryPath = "target\release\rinui-gallery.exe",
    [string]$OutputDir = "dist\RinUIGallery-win64",
    [switch]$Sign,
    [string]$CertPfx = "",
    [string]$CertPassword = "",
    [string]$CertThumbprint = "",
    [string]$TimestampUrl = "",
    [switch]$NoTimestamp
)

$ErrorActionPreference = "Stop"

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

if ($Sign) {
    if (-not $CertPfx -and -not $CertThumbprint) {
        throw "Authenticode signing requested (-Sign), but neither -CertPfx/RINUI_WIN_CERT_PFX nor -CertThumbprint/RINUI_WIN_CERT_THUMBPRINT was supplied."
    }
    if ($CertPfx -and -not (Test-Path -LiteralPath $CertPfx -PathType Leaf)) {
        throw "Authenticode certificate PFX file not found: $CertPfx"
    }
}

if (-not (Test-Path $BinaryPath)) {
    throw "Executable not found at $BinaryPath"
}

if ($env:QMAKE) {
    $qmake = $env:QMAKE
} else {
    $qmakeCmd = Get-Command qmake6 -ErrorAction SilentlyContinue
    if (-not $qmakeCmd) {
        $qmakeCmd = Get-Command qmake -ErrorAction SilentlyContinue
    }
    $qmake = if ($qmakeCmd) { $qmakeCmd.Source } else { $null }
}
if (-not $qmake) {
    throw "qmake6/qmake not found in PATH and QMAKE is unset"
}

$qtBins = (& $qmake -query QT_INSTALL_BINS).Trim()
$windeployqt = Join-Path $qtBins "windeployqt.exe"
if (-not (Test-Path $windeployqt)) {
    $windeployqt = Get-Command windeployqt -ErrorAction Stop | Select-Object -ExpandProperty Source
}

if (Test-Path $OutputDir) {
    Remove-Item -Recurse -Force $OutputDir
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$targetExe = Join-Path $OutputDir (Split-Path $BinaryPath -Leaf)
Copy-Item $BinaryPath $targetExe
Copy-Item "LICENSE" (Join-Path $OutputDir "LICENSE")
Copy-Item "upstream\Rin-UI\LICENSE" (Join-Path $OutputDir "LICENSE.RinUI")
Copy-Item "crates\rinui-host\vendor\LICENSE.FluentUI" (Join-Path $OutputDir "LICENSE.FluentUI")
if ($env:RINUI_QT_LICENSES) {
    Copy-Item -LiteralPath $env:RINUI_QT_LICENSES -Recurse -Destination (Join-Path $OutputDir "qt-licenses")
}
Copy-Item "THIRD_PARTY_NOTICES.md" (Join-Path $OutputDir "THIRD_PARTY_NOTICES.md")

& $windeployqt `
    --release `
    --no-compiler-runtime `
    --qmldir "upstream\Rin-UI\RinUI" `
    --qmldir "upstream\Rin-UI\examples" `
    --qmldir "examples\gallery\qml" `
    $targetExe

if ($LASTEXITCODE -ne 0) {
    throw "windeployqt failed with exit code $LASTEXITCODE"
}

$requiredPaths = @(
    $targetExe,
    (Join-Path $OutputDir "platforms\qwindows.dll"),
    (Join-Path $OutputDir "LICENSE"),
    (Join-Path $OutputDir "LICENSE.RinUI"),
    (Join-Path $OutputDir "THIRD_PARTY_NOTICES.md")
)
foreach ($req in $requiredPaths) {
    if (-not (Test-Path $req)) {
        throw "Expected packaged Windows artifact missing: $req"
    }
}

$qt5Compat = Get-ChildItem -Path $OutputDir -Recurse -Filter "GraphicalEffects" -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "Qt5Compat[\\/]GraphicalEffects$" }
if (-not $qt5Compat) {
    throw "Qt5Compat\GraphicalEffects QML plugin missing from $OutputDir"
}

function Find-SignTool {
    $cmd = Get-Command "signtool.exe" -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    $kitRoots = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10\bin",
        "C:\Program Files\Windows Kits\10\bin"
    )
    foreach ($root in $kitRoots) {
        if (-not (Test-Path -LiteralPath $root)) {
            continue
        }
        $candidate = Get-ChildItem -LiteralPath $root -Recurse -File -Filter "signtool.exe" -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match "\\x64\\" } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($candidate) {
            return $candidate.FullName
        }
    }
    throw "signtool.exe was not found on PATH or under Windows Kits\10\bin."
}

if ($Sign) {
    $signtool = Find-SignTool
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
    $signArgs.Add($targetExe)

    & $signtool @signArgs
    if ($LASTEXITCODE -ne 0) {
        throw "signtool sign failed on $targetExe with exit code $LASTEXITCODE"
    }
    & $signtool verify /pa $targetExe
    if ($LASTEXITCODE -ne 0) {
        throw "signtool verify /pa failed on $targetExe with exit code $LASTEXITCODE"
    }
    Write-Host "Packaged and Authenticode-signed Windows distribution at $OutputDir"
} else {
    Write-Host "Packaged unsigned Windows distribution at $OutputDir (signing skipped; pass -Sign to sign binary)"
}
