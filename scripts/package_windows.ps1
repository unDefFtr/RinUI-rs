param(
    [string]$BinaryPath = "target\release\rinui-gallery.exe",
    [string]$OutputDir = "dist\RinUIGallery-win64"
)

$ErrorActionPreference = "Stop"

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

Write-Host "Packaged Windows distribution at $OutputDir"
