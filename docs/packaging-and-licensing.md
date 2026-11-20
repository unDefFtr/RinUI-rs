# Packaging and Licensing Guide

## 1. Runtime Dependencies Required by RinUI Applications

Even when all RinUI QML files, JavaScript indexes, fonts (`FluentSystemIcons-Resizable.ttf`), images (`noise.png`), and `.qm` translations are embedded into the executable via Qt `rcc`, a dynamically linked Qt 6 application still requires the following Qt runtime artifacts:

### Qt Shared Libraries / Frameworks
- `Qt6Core`
- `Qt6Gui`
- `Qt6Network` (transitive QML dependency)
- `Qt6Qml` / `Qt6QmlModels` / `Qt6QmlWorkerScript`
- `Qt6Quick` / `Qt6QuickTemplates2` / `Qt6QuickControls2` / `Qt6QuickControls2Impl` / `Qt6QuickLayouts` / `Qt6QuickShapes` / `Qt6QuickEffects`
- `Qt6Core5Compat` + `Qt6GraphicalEffects` + `Qt6ShaderTools` (`Qt5Compat.GraphicalEffects`, required by 19 upstream RinUI files; `Qt6ShaderTools` is required at runtime on Qt 6.9+)
- `Qt6Svg` / imageformat plugins if application icons or assets use SVG/PNG

### Required Qt QML Runtime Plugins
Run deployment tools with `-qmldir` pointing to `upstream/Rin-UI/RinUI` and your application's QML directory so these QML plugins are deployed:
- `QtQuick` (`QtQuick.Controls`, `QtQuick.Controls.Basic`, `QtQuick.Templates`, `QtQuick.Layouts`, `QtQuick.Window`, `QtQuick.Effects`, `QtQuick.Shapes`)
- `QtQml`
- `Qt5Compat/GraphicalEffects`

### Required Qt Platform Plugins (`platforms/`)
- **macOS**: `libqcocoa.dylib`
- **Windows**: `qwindows.dll` (plus `D3Dcompiler_47.dll` / `opengl32sw.dll` as deployed by `windeployqt`)
- **Linux**: `libqxcb.so` and/or `libqwayland-generic.so` / `libqwayland-egl.so` (+ `wayland-shell-integration`, `wayland-decoration-client`)

---

## 2. Platform Packaging Workflows

### macOS (`.app` Bundle + Optional Developer ID Signing & Notarization)

```bash
cargo build --release -p rinui-gallery
./scripts/package_macos.sh target/release/rinui-gallery dist
```

`scripts/package_macos.sh` builds `dist/RinUIGallery.app`, writes `Info.plist` with `NSHighResolutionCapable = true` and `CFBundleShortVersionString = 0.1.0-alpha.1`, copies licenses/notices into `Contents/Resources/`, invokes `macdeployqt` with `-qmldir` flags pointing at `upstream/Rin-UI/RinUI`, `upstream/Rin-UI/examples`, and `examples/gallery/qml`, ensures `Contents/Resources/qt.conf` and `Qt5Compat/GraphicalEffects` exist, prunes optional Qt plugins with unbundled external dependencies, relocates non-system dylib paths and strips external `LC_RPATH`s, and verifies every Mach-O binary via `otool -L`/`otool -l`.

- **Default (zero credentials)**: applies a local ad-hoc Mach-O signature (`codesign --force --deep --sign -`); bundles are **NOT** Developer ID signed or notarized.
- **Optional Developer ID Signing & Notarization**:
  ```bash
  ./scripts/package_macos.sh \
    --sign-identity "Developer ID Application: Example Corp (TEAMID)" \
    --notarize --notary-profile "AC_NOTARY" \
    target/release/rinui-gallery dist
  ```
  Supported flags / environment variables: `--sign-identity` / `RINUI_MACOS_SIGN_IDENTITY`, `--entitlements` / `RINUI_MACOS_ENTITLEMENTS`, `--notarize` / `RINUI_MACOS_NOTARIZE=1`, `--notary-profile` / `RINUI_MACOS_NOTARY_PROFILE`. Signs Mach-O plugins/frameworks inside-out with Hardened Runtime + timestamp, validates `--verify --deep --strict`, and rejects `--notarize` if either signing identity or notary profile is missing.

### Windows (Self-Contained Directory + Optional Authenticode + Rust MSIX)

#### 1. Directory Packaging (`scripts/package_windows.ps1`)

```powershell
cargo build --release -p rinui-gallery
.\scripts\package_windows.ps1 -BinaryPath target\release\rinui-gallery.exe -OutputDir dist\RinUIGallery-win64
```

Compatible with PowerShell 5.1 and PowerShell 7: copies the executable and license notices, runs `windeployqt --release --no-compiler-runtime --qmldir ...` to collect all Qt 6 DLLs (including `Qt6ShaderTools.dll`), QML plugins (`Qt5Compat\GraphicalEffects`, etc.), and `platforms\qwindows.dll`, and asserts those required artifacts exist. Native Win32 system DLLs (`dwmapi.dll`, `user32.dll`, `shell32.dll`) are OS-provided. Pass `-Sign` with `-CertPfx`/`RINUI_WIN_CERT_PFX` (and optional `-CertPassword`/`RINUI_WIN_CERT_PASSWORD`) or `-CertThumbprint`/`RINUI_WIN_CERT_THUMBPRINT` (+ `-TimestampUrl`/`RINUI_WIN_TIMESTAMP_URL` or `-NoTimestamp`) for optional Authenticode SHA-256 signing.

#### 2. Host-Owned MSIX Packaging (`scripts/package_msix.ps1`, Zero Python/PyInstaller)

```powershell
# Unsigned MSIX + unpack layout verification (used by CI without credentials):
.\scripts\package_msix.ps1 -SourceDirectory dist\RinUIGallery-win64 -OutputDirectory dist\msix -SkipSign -VerifyLayout

# Signed MSIX (with PFX or store thumbprint):
.\scripts\package_msix.ps1 -SourceDirectory dist\RinUIGallery-win64 -OutputDirectory dist\msix -CertPfx path\to\cert.pfx
```

`scripts/package_msix.ps1` uses `packaging/windows/AppxManifest.template.xml` and store tile PNGs under `packaging/windows/assets/` (copied cleanly without modifying `upstream/Rin-UI` or invoking Python/PyInstaller). Package identity, publisher, display names, executable, resource language, and architecture (`x64`/`arm64`/`x86`) are configurable. SemVer versions from `Cargo.toml` map automatically to strict 4-part numeric MSIX versions (`0.1.0-alpha.1` -> `0.1.0.1`, components validated within `0..65535`). With `-VerifyLayout`, `MakeAppx unpack` verifies the executable, `qwindows.dll`, `Qt5Compat\GraphicalEffects`, assets, licenses, and rendered `AppxManifest.xml`. Note that unsigned CI verification validates MSIX packaging and unpacked layout only—real MSIX installation, Start-menu launch, and uninstall require a Windows desktop + trusted signing certificate.

### Linux (Portable Staging Directory / AppImage Base)

```bash
cargo build --release -p rinui-gallery
./scripts/package_linux.sh target/release/rinui-gallery dist/rinui-gallery-linux
```

`scripts/package_linux.sh` copies `Qt6` and `ICU` shared libraries (preserving SONAME symlinks via `cp -a`), platform/input/GL plugins (`platforms/`, `xcbglintegrations/`, `platforminputcontexts/`, `wayland-*`), QML modules (`QtQml`, `QtQuick`, `Qt5Compat`, `QML`), and licenses, asserts required runtime plugins exist, and generates a `run-gallery.sh` launcher setting `LD_LIBRARY_PATH`, `QT_PLUGIN_PATH`, and `QML_IMPORT_PATH`.

---

## 3. Licensing & Redistribution Obligations

1. **RinUI (MIT License)**:
   - Upstream `RinUI` is licensed under the MIT License (`Copyright (c) 2025 RinLit`).
   - All distributions embedding RinUI QML/assets must preserve the upstream `LICENSE` text and copyright notice (bundled automatically by the packaging scripts via `LICENSE.RinUI` and `THIRD_PARTY_NOTICES.md`).
2. **Qt 6 Framework (LGPLv3 / GPLv3 / Commercial)**:
   - **Important**: Compiling QML files and assets into the binary via Qt's `rcc` resource compiler does **not** remove Qt framework licensing obligations.
   - Under **LGPLv3**, you must either dynamically link Qt (the default configuration in `rinui-rs`), preserve users' ability to relink or swap Qt shared libraries, provide prominent LGPLv3 notices and offer of Qt source code, and document the exact Qt version shipped.
   - If you choose static Qt linking for a closed-source product, a **Commercial Qt License** is required.
3. **Rust Host & CXX-Qt**:
   - `rinui-rs` is MIT-licensed (`LICENSE`).
   - `cxx`, `cxx-qt`, `cxx-qt-lib`, `serde`, `thiserror`, and `tracing` are dual-licensed under `MIT OR Apache-2.0`.
