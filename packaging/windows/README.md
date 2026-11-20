# Windows MSIX Host-Owned Packaging Assets

This directory holds host-owned Windows MSIX manifest templates and store tile assets
used by `scripts/package_msix.ps1` without modifying `upstream/Rin-UI` or introducing
any Python / PyInstaller runtime dependency.

## Contents

- `AppxManifest.template.xml`: Template rendered into `AppxManifest.xml` during MSIX staging.
- `assets/StoreLogo.png`
- `assets/Square44x44Logo.png`
- `assets/Square150x150Logo.png`

The PNG store assets are copied verbatim from `upstream/Rin-UI/examples/assets/icons/store/package/`.

## Manifest Placeholders

`scripts/package_msix.ps1` substitutes XML-escaped values for:

- `__IDENTITY_NAME__`: Package Identity `Name` (default `io.rinui.gallery.rs`)
- `__PUBLISHER__`: Package `Publisher` DN (default `CN=RinUI.rs`; overridden via `-Publisher` or certificate subject when signing)
- `__VERSION__`: Strictly 4-part numeric MSIX version `major.minor.build.revision` (each component `0..65535`)
- `__ARCHITECTURE__`: `x64`, `arm64`, or `x86`
- `__DISPLAY_NAME__`: Display name in Store/Start Menu (default `RinUI Gallery`)
- `__PUBLISHER_DISPLAY_NAME__`: Publisher display name (default `rinui-rs`)
- `__DESCRIPTION__`: Package description
- `__RESOURCE_LANGUAGE__`: BCP-47 resource language tag (default `en-US`)
- `__MIN_WINDOWS_VERSION__`: Windows 10 min OS build (default `10.0.19041.0`)
- `__MAX_WINDOWS_VERSION_TESTED__`: Max tested OS build (default `10.0.26100.0`)
- `__APPLICATION_ID__`: AppX Application Id (default `RinUIGallery`)
- `__EXECUTABLE__`: Relative executable path inside the MSIX root (default `rinui-gallery.exe`)

## SemVer -> MSIX 4-Part Numeric Mapping

MSIX rejects SemVer pre-release suffixes (`-alpha.1`) and strictly requires `A.B.C.D` where each integer is in `0..65535`:

- `X.Y.Z` -> `X.Y.Z.0`
- `X.Y.Z-alpha.N` / `X.Y.Z-beta.N` / `X.Y.Z-rc.N` / `X.Y.Z-<tag>.N` -> `X.Y.Z.N`
- `X.Y.Z-<tag>` (no trailing numeric index) -> `X.Y.Z.0`
- Explicit 4-part `A.B.C.D` -> preserved after bounds checking `0..65535`

Example for workspace version `0.1.0-alpha.1`: mapped MSIX Identity version is `0.1.0.1`.
Note that if a downstream release publishes to Microsoft Store alongside non-Store builds, bump the 3rd or 4th numeric part monotonically (`Store-reserved` semantics require incrementing `Version` for upgrades).
