# Packaging and Licensing Guide

## 1. Runtime Dependencies Required by RinUI Applications

Even when all RinUI QML files, JavaScript indexes, fonts (`FluentSystemIcons-Resizable.ttf`), images (`noise.png`), and `.qm` translations are embedded into the executable via Qt `rcc`, a dynamically linked Qt 6 application still requires the following Qt runtime artifacts:

### Qt Shared Libraries / Frameworks
- `Qt6Core`
- `Qt6Gui`
- `Qt6Network` (transitive QML dependency)
- `Qt6Qml` / `Qt6QmlModels` / `Qt6QmlWorkerScript`
- `Qt6Quick` / `Qt6QuickTemplates2` / `Qt6QuickControls2` / `Qt6QuickControls2Impl` / `Qt6QuickLayouts` / `Qt6QuickShapes` / `Qt6QuickEffects`
- `Qt6Core5Compat` + `Qt6GraphicalEffects` (`Qt5Compat.GraphicalEffects`, required by 19 upstream RinUI components)
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

### macOS (`.app` Bundle)

```bash
cargo build --release -p rinui-gallery
./scripts/package_macos.sh target/release/rinui-gallery dist
```

`scripts/package_macos.sh` builds `dist/RinUIGallery.app`, writes `Info.plist` with `NSHighResolutionCapable = true`, copies licenses/notices into `Contents/Resources/`, and invokes `macdeployqt` with `-qmldir` flags pointing at `upstream/Rin-UI/RinUI`, `upstream/Rin-UI/examples`, and `examples/gallery/qml`.

### Windows (Self-Contained Directory / MSIX / Installer)

```powershell
cargo build --release -p rinui-gallery
.\scripts\package_windows.ps1 -BinaryPath target\release\rinui-gallery.exe -OutputDir dist\RinUIGallery-win64
```

`scripts/package_windows.ps1` copies the executable and license notices and runs `windeployqt --release --qmldir ...` to collect all Qt 6 DLLs, QML plugins (including `Qt5Compat/GraphicalEffects`), and `qwindows.dll`. Native Win32 system DLLs (`dwmapi.dll`, `user32.dll`, `shell32.dll`) are OS-provided.

### Linux (Portable Staging Directory / AppImage Base)

```bash
cargo build --release -p rinui-gallery
./scripts/package_linux.sh target/release/rinui-gallery dist/rinui-gallery-linux
```

`scripts/package_linux.sh` copies `Qt6` shared libraries, platform plugins (`platforms/`, `wayland-*`), QML modules (`QtQml`, `QtQuick`, `Qt5Compat`), licenses, and generates a `run-gallery.sh` launcher setting `LD_LIBRARY_PATH`, `QT_PLUGIN_PATH`, and `QML_IMPORT_PATH`.

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
