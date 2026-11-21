# Changelog

All notable changes to `rinui-rs` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Windows toolchain
- Move the Windows CI/package baseline from Qt 6.9.3 to Qt 6.10.3 for the
  Windows 10 Gallery white-border investigation. The local build and native
  smoke test pass, and the user confirmed no white borders in the upgraded
  window. Transition visual checks and one resource-cleanup regression
  remain open. See [validation details](https://github.com/unDefFtr/rinui-rs/blob/e9c6d3232e509f06d903ba9fd4991ed91097298c/docs/windows-10-border.md).

Upstream parity completion across `rinui-rs` (`upstream/Rin-UI` unchanged at `46d77fa8371ff0e9f46a10a2eb66612dab123269`).

### Added
- **Explicit Shared-Engine Detached Window Registration & Borrow-Checked `ThemeManager` Guards (`rinui-host`, `native/`)**:
  - Added `RinUIHost::register_window_from_property`, `RinUISession::register_window_from_property`, and `SessionHandle::register_window_from_property` (`rinui_register_window_from_root_property`, `HostError::{WindowNotOwned, WindowRegistrationFailed}`) to attribute singleton/detached `QQuickWindow`s on shared engines without letting unrelated sessions auto-claim them (`release_root` also cleans up registered detached windows).
  - Added `!Send + !Sync` `ThemeManagerRef<'_>` / `ThemeManagerMut<'_>` RAII borrow guards (`theme_borrow_state: Rc<Cell<isize>>`) and `mut_slot_active` recursive slot protection so concurrent read+write, write+write, write+read, and QML/session mutations while `ThemeManager` is borrowed return `Err(HostError::ReentrantCall(...))`.
  - Added subprocess-isolated timeout-guarded regression suite `crates/rinui-host/tests/parity_review_regressions.rs` initially covering 12 scenarios (extended below for external event loops) (`load-drop`, `load-drop-shared`, `property-drop`, `translation-drop`, `event-drop-process`, `event-drop-exec`, `event-nested-prop-drop`, `theme-signal-drop`, `ownership`, `theme-alias`, `locale-live-embedded`, `locale-live-dev`).
- **Process Runtime, Borrowed Qt Apps & Multi-Session Engines (`rinui-host`)**:
  - `RinUIRuntime<'app>`, `RinUISession<'app>`, `SessionHandle<'app>` (weak-backed handles returning `HostError::HostDisposed`), `EngineMode::{Shared, Private}`, `ContextPropertyConflict::{Reject, Overwrite}`, and `SessionConfig` / `RuntimeConfig`.
  - Borrowed application adoption via `RinUIRuntime::attach_existing(&'app QGuiApplication, ...)` and `unsafe RinUIRuntime::attach_existing_raw(ptr, ...)`, rejecting bare `QCoreApplication`, wrong threads, late incompatible Quick Controls style changes, and `exec()` on borrowed apps while snapshotting and restoring global settings on drop.
  - Owner-scoped C++ `(runtime_id, engine_id, session_id)` registries in `native/src/rinui_qt_bridge.cpp`, optional `qt-widgets-test` feature + `external_qapplication` (`QGuiApplication` & `QApplication`) integration tests, and `runtime_sessions` / `core_app_rejection` suites.
- **Generic Root Object Interaction & Engine Extension (`rinui-host`, `native/`)**:
  - Checked `set_root_property`, `get_root_property`, and `invoke_root_method` across `RinUIHost`, `RinUISession`, and `SessionHandle`, with `InvokeDepthGuard` deferred deletion fencing and `object_interop` integration coverage (`qml/ObjectInteropFixture.qml`).
  - Scoped `EngineConfigurator<'eng>` and `configure_engine(|cfg| ...)` (`set_offline_storage_path`, `set_extra_file_selectors`, `add_import_path`, `unsafe with_raw_engine_mut`) plus runnable `engine_config_demo` example.
- **Explicit macOS Upstream Framing & Diagnostics**:
  - `MacWindowOptions::self_drawn()`, `MacWindowOptions::upstream_compatible()`, `MacWindowOptions::native(offset_x, offset_down)`, `HostConfig`/`RuntimeConfig::with_upstream_mac_frame()`, pre-first-frame `RinUINativeMacFrame` context property propagation (including `MultiWindowDialog.qml`), per-window `_rinuiMacFrameDiagnostic`, and `--upstream-mac-frame` CLI flag in Gallery.
- **Gallery Smoke & Host-Owned Windows MSIX / Signing Workflows**:
  - Expanded `rinui-gallery --smoke-test` to drive `NavigationView` push (`TextArea.qml` / `ScrollableTextArea`), `"unexist/page"` `ErrorPage` load + `safePop()` recovery, macOS frame diagnostics on main + secondary windows, and structured `smoke_summary` reporting.
  - Added host-owned `packaging/windows/` manifest template & store icon assets, PowerShell 5.1/7-compatible `scripts/package_msix.ps1` (`-SkipSign`, `-VerifyLayout`, zero Python/PyInstaller, SemVer -> 4-part numeric mapping), optional Authenticode flags in `scripts/package_windows.ps1`, and optional Developer ID `--sign-identity` / `--entitlements` / `--notarize` / `--notary-profile` support in `scripts/package_macos.sh`.

### Changed & Fixed
- **Direct external nested event processing (P1)**: Qualify borrowed Engine destruction through a per-engine QObject DeferredDelete fence, including retained shared engines in last-Runtime teardown. Timer/aboutToQuit callbacks cannot destroy unqualified engines and ordinary borrowed drains no longer force global deletes. Added direct external app-pump workers for a live Runtime and last Private/Shared Runtime (25 modes total). Manual Qt drivers must dispatch DeferredDeletes at an outer boundary, as Qt requires; updated their test driver plumbing without adding a RinUI guard to the nested app call.
- **Nested external event processing (P1)**: Removed the extra post-guard synchronous flush from both Runtime and Session `process_events()`. Guard release already decides whether to flush or schedule; borrowed apps must only schedule because an outer QML Timer callback can still be executing at `loopLevel() == 0`. Added safe-attach `external-process-nested-pump` and `external-process-nested-pump-session` workers, retaining the original 20 and their 20-second subprocess timeout.
- **External event-loop lifecycle crashes (P1)**: Session destruction always defers roots/windows and Private engines, including direct external `QGuiApplication::process_events()` callbacks with no host guard. Last-owner borrowed Runtime destruction now retains engines, ThemeManager and the runtime reservation in a two-phase teardown queue. Coalesced timer draining, `aboutToQuit` and `qAddPostRoutine` complete cleanup after QML returns; reattach requires the external loop to drain first. Cache/GC calls are skipped after GUI application destruction.
- Added eight external-loop subprocess workers (20-second timeout each), extending the existing 12 to 20: safe attach, nested property self-drop, Private/Shared last Runtime release, reversed order, quit-before-drain, and app destruction without further event processing. Each checks resumed JS, immediate disposed handles, sibling calls, filesystem-view cleanup and reattachment.
- **QML Callback Session/Engine Teardown Safety (`rinui-host`, `native/src/rinui_qt_bridge.cpp`)**:
  - Replaced `InvokeDepthGuard`-only fencing with process-wide `QmlExecGuard` (`qml_exec_depth`, `rinui_qml_exec_enter`/`leave`), `is_deferred_context()` (`s_active_invoke_depth > 0 || QThread::loopLevel() > 0`), and event-loop queued engine draining (`rinui_schedule_deferred_drain`) across `load`, property read/write, `invoke_root_method`, translation switching, `process_events`, `NativeWindowRef`, and `ThemeManager` signals.
  - Added post-load disposal detection discarding newly loaded unregistered roots and returning `Err(HostError::HostDisposed)`, and changed `retranslate_all_engines()` to snapshot engine IDs and re-verify `live_engines` + registered roots per iteration.
- **Shared-Engine Dynamic Window Ownership (`native/src/rinui_qt_bridge.cpp`)**: Updated `resolve_window_owner()` to trace `_rinuiOwnerSessionId`, `parent` chains, `transientParent` chains, and `QQmlContext::parentContext()->contextObject()` creation contexts, and updated `rinui_sync_session_windows()` on `EngineMode::Shared` so ambiguous `session_id == 0` windows are never auto-claimed by the first caller.
- **`ThemeManager` Aliasing & Borrow Safety (Breaking Rust API Adjustment)**:
  - Changed `theme_manager()`, `theme_manager_mut()`, `with_theme_manager()`, and `with_theme_manager_mut()` across `RinUIRuntime`, `RinUISession`, and `RinUIHost` to return `Result<..., HostError>` (`ThemeManagerRef<'_>` / `ThemeManagerMut<'_>`), preventing overlapping `&ThemeManager` / `Pin<&mut ThemeManager>` references across shared handles and active QML calls.
  - **Migration**: Append `?` (or handle `HostError::ReentrantCall`) on `theme_manager()`, `theme_manager_mut()`, `with_theme_manager(...)`, and `with_theme_manager_mut(...)`. `ThemeManagerMut` provides both `.as_mut() -> Pin<&mut ThemeManager>` and direct `.toggle_theme(...)` / `.apply_backdrop_effect(...)` / `.set_theme_color(...)` helpers.
- **Live Locale Updates on Pre-Existing Date Controls (`qmldir_sanitize.rs`, `native/src/rinui_qt_bridge.cpp`)**: Rewrote default `locale` bindings in `components/DateAndTime/Calendar.qml`, `DatePicker.qml` (and `monthModel` using `datePicker.locale`), and `components/Text/SpinBox.qml` in sanitized build/dev outputs (leaving `upstream/Rin-UI` untouched) and paired `QQmlEngine::retranslate()` with deduplicated `QEvent::LocaleChange` dispatch so pre-existing controls update live on `set_language()` while preserving explicit declarative and imperative overrides.
- **Decoupled i18n & Gallery Locale Handling**: Decoupled `active_locale` / `QLocale::setDefault()` from RinUI `en_US` catalog fallback; exposed per-catalog `CatalogTranslationResult`s; implemented atomic 2-phase C++ translator updates with corrupted `.qm` rollback and stale optional-catalog unloading; retranslated all live engines (`crates/rinui-host/tests/i18n_semantics.rs`).
- **Unified Sanitized Dev Mode**: Extracted shared `qmldir_sanitize` module used identically by `build.rs` and fresh per-launch `ResourceView` / `GalleryDevView` staging copies (`UpstreamGallery.qml`, `--examples-dir` / `RINUI_EXAMPLES_DIR`, no release dev paths).
- **Platform Native Hooks**: Moved Windows `WinEventManager` to Runtime lifetime `std::unique_ptr`, fixed Win10 Acrylic `ACCENT_DISABLED` disable transition, tracked/disconnected Qt signal connections (`colorSchemeChanged`, macOS `visibleChanged`) cleanly across runtime/window teardown, and tagged releasing windows `_rinuiReleased` against premature `sync_windows()` resurrection.
- **Layered CI**: Removed silent offscreen fallbacks on macOS Cocoa and Windows `windows` QPA in `.github/workflows/ci.yml`, added `qt-widgets-test` + `engine_config_demo` + unsigned MSIX unpack verification, labeled Linux `xvfb-run` explicitly as headless `xvfb-xcb`, and emitted a layered summary table distinguishing automated layers from manual desktop QA / MSIX install / signing.

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
