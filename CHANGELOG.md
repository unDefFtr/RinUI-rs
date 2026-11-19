# Changelog

All notable changes to `rinui-rs` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-alpha.1] - 2026-11-19

First cross-platform pre-release (`alpha.1`) of the independent Rust + Qt 6 host for upstream RinUI (`46d77fa8371ff0e9f46a10a2eb66612dab123269`, RinUI `0.5.0`).

### Added

- **Workspace Crates**:
  - `rinui-platform`: OS/Windows build detection (`WindowsVersion`, `PlatformCapabilities`), `ThemeMode` / `BackdropEffect` resolution, isolated per-application directories (`AppIdentity`), atomic JSON configuration persistence (`ConfigStore`, `AppJsonStore`), legacy Python `rin_ui.json` schema migration (`schema_version`), and corruption backup recovery.
  - `rinui-host`: `!Send + !Sync` `RinUIHost` and `HostHandle`, complete CXX-Qt `ThemeManager` QML contract (`themeChanged`, `backdropChanged`, `toggle_theme`, `get_theme`, `get_theme_name`, `apply_backdrop_effect`, `get_backdrop_effect`, `set_theme_color`, `get_theme_color`), Qt 6 `QStyleHints::colorSchemeChanged` live system theme monitoring, `TranslationManager` (`TranslationCatalog`, `QQmlEngine::retranslate()`), `WindowCustomizer`, `is_qt_main_thread()`, and `qt_file_exists()`.
  - `rinui-gallery`: Reference Rust-hosted RinUI Gallery (`GalleryBackend`) reusing upstream `examples/` QML alongside `RustHostDemoPage.qml`, `MultiWindowDialog.qml`, and CLI flags (`--dev`, `--smoke-test`, `--auto-close-ms`, `--locale`, `--no-native-mac-frame`, `--config-dir`).
- **Upstream Isolation & Dual Resource Loading**:
  - Unmodified `upstream/Rin-UI` submodule pinned at `46d77fa8371ff0e9f46a10a2eb66612dab123269` (`beta-0.5.0-16-g46d77fa`).
  - Build-time sanitization in `$OUT_DIR/rinui_sanitized/` removes non-existent `qmldir` entries and normalizes versionless singletons without touching the submodule.
  - `ResourceMode::Embedded` mounts compiled QRC assets under `/rinui/RinUI` (`qrc:/rinui`) to prevent `qrc:/qt/qml` from shadowing `ResourceMode::Filesystem`.
- **Native Platform Integration (`native/`)**:
  - **macOS (`rinui_mac_window.mm`)**: ARC Objective-C++ `NSWindow` transparent titlebar, `NSWindowStyleMaskFullSizeContentView`, `NSAppearance` synchronization, and idempotent traffic-light positioning, guarded on the Cocoa QPA.
  - **Windows (`rinui_win_bridge.h`, `rinui_win_bridge.cpp`)**: C++ `WinEventManager` and `WinEventFilter` implementing `WM_NCCALCSIZE`, `WM_GETMINMAXINFO`, `WM_NCHITTEST` (with interactive `titleBarLeadingHost`/`titleBarHost` traversal), Windows 11 Snap Layouts `HTMAXBUTTON` states, DWM backdrop attributes, and Win10 `SetWindowCompositionAttribute` fallback.
  - **Linux (`rinui_linux_window.cpp`)**: Qt compositor `FramelessWindowHint` and `startSystemResize()` compatibility with graceful `-2` fallback for unsupported Windows DWM backdrop APIs.
- **Build, Tooling, CI & Packaging**:
  - CMake wrapper (`CMakeLists.txt`, `cmake/RinUIHost.cmake`, `cmake/BundleHelpers.cmake`).
  - Upstream contract audit script (`scripts/audit_upstream.py`).
  - Packaging scripts: `scripts/package_macos.sh`, `scripts/package_windows.ps1`, and `scripts/package_linux.sh`.
  - GitHub Actions matrix (`.github/workflows/ci.yml`) running `audit_upstream.py`, `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, release builds, and isolated packaged smoke tests across macOS (`arm64`), Windows (`msvc`), and Linux (`gcc_64` + `lld` + Xvfb).

### Changed & Hardened

- **Host Lifetime & Concurrency Guards (`rinui-host`, `native/src/rinui_qt_bridge.cpp`)**:
  - Added process-wide `HOST_ACTIVE` lock (`HostError::ApplicationAlreadyInitialized`), `InitCleanupGuard` on partial init failure, and `LoadingGuard` / `try_borrow` re-entrancy protection (`HostError::ReentrantCall`).
  - Enforced deterministic `RinUIHost::drop` ordering with `rinui_flush_deferred_deletes()` before `clearComponentCache()`, `QObject::destroyed` root cleanup in `s_roots`, and single-teardown guards in `rinui_root_release`.
  - Wrapped CXX-Qt `#[qinvokable]` FFI boundaries in `ThemeManager` and `GalleryBackend` with `std::panic::catch_unwind` and `try_borrow` fallbacks.
- **Internationalization & Qt 6.9 Compatibility**:
  - Normalized hyphenated/script locale tags (`zh-CN`, `zh_Hans_CN`, `zh` -> `zh_CN`; `en` -> `en_US`) and made `QTranslator` replacement transactional with clean unloading of absent optional app catalogs.
  - Updated `install_translator_or_empty` in `native/src/rinui_qt_bridge.cpp` so valid header-only/empty Qt 6.9 `.qm` catalogs (such as upstream `en_US.qm`) succeed consistently with Qt 6.10+.
- **Cross-Platform Build, Windows & Packaging Fixes**:
  - Stripped Windows `\\?\` / `\\?\UNC\` verbatim path prefixes before `.qrc` generation and QML URL/import conversion.
  - Replaced `QMetaObject::invokeMethod` return-type mismatch with type-safe `qobject_cast<rinui::ThemeManager*>`, switched `s_event_manager` to `QPointer<WinEventManager>`, added dynamic `GetDpiForWindow`/`GetSystemMetricsForDpi` Per-Monitor-V2 DPI scaling, fixed `hit_test` client coordinates and `WM_CAPTURECHANGED` / `WM_DPICHANGED` handling, fixed `apply_nccalcsize` auto-hide edge handling and `apply_getminmaxinfo` max bounds, and fixed Win32 FFI lints (`OsVersionInfoW`, `c"RtlGetVersion"`).
  - Reattached `NSWindow` observers cleanly on window changes (`observed_ns_window`) and resolved `NSConcreteNotification` edge cases on macOS.
  - Gated `CARGO_MANIFEST_DIR` `--dev` fallback behind `#[cfg(debug_assertions)]` in `examples/gallery/src/main.rs`.
  - Hardened `scripts/package_macos.sh` to generate `Contents/Resources/qt.conf`, verify `Qt5Compat/GraphicalEffects`, prune unbundled optional Qt plugins (including host-only `sqldrivers` / `QtPdf` / `Qt3D` / `QtVirtualKeyboard`), rewrite external dylib IDs/deps (`otool -L` / `LC_RPATH`), re-sign ad-hoc with `codesign --force --deep --sign -`, and verify zero unresolved `@rpath` or external paths.
  - Removed PowerShell 7-only `??` syntax and added artifact assertions in `scripts/package_windows.ps1`; preserved SONAME symlinks, ICU libs, and XCB/input plugins in `scripts/package_linux.sh`.
  - Added `qtshadertools` (required at runtime by `Qt5Compat.GraphicalEffects` on Qt 6.9), `lld` on Linux (`+whole-archive` CXX-Qt linkage), and CI `<arm_acle.h>` workaround for Qt 6.9.3 `qyieldcpu.h` with Apple Clang 21 (`QTBUG-145239`).

### Known Limitations

- Automated validation covers compilation, Clippy, Rust/Qt integration tests (`offscreen`), Linux `xvfb-run` (`xcb`), and isolated packaged QPA startup smoke tests on CI; **interactive desktop visual QA** (macOS window geometry/traffic lights, Windows 11 DWM Mica/Acrylic/Snap Layouts/multi-monitor DPI, Linux X11 window manager & native Wayland compositor) has not yet been visually verified.
- `scripts/package_macos.sh` applies local ad-hoc Mach-O signatures only; Apple Developer ID code-signing and Gatekeeper notarization are not configured.
- Building on Apple Silicon with Xcode 26 / Apple Clang 21 against Qt < 6.11.1 requires `CXXFLAGS="-include arm_acle.h"` (`QTBUG-145239`; fixed in upstream Qt 6.11.1/6.12).
- Qt framework LGPLv3/GPLv3/Commercial obligations apply when redistributing binaries (compiling QML into Qt `rcc` does not exempt Qt shared-library requirements).

[0.1.0-alpha.1]: https://github.com/unDefFtr/rinui-rs/commits/phase2-validation
