# RinUI Upstream Compatibility Audit & Contract Matrix

## 1. Pinned Upstream Revision

- **Upstream Repository**: `https://github.com/RinLit-233-shiroko/Rin-UI`
- **Submodule Path**: `upstream/Rin-UI`
- **Pinned Commit SHA**: `46d77fa8371ff0e9f46a10a2eb66612dab123269` (`beta-0.5.0-16-g46d77fa`, `RinUI/__init__.py` `__version__ = "0.5.0"`)
- **Upstream Runtime Stack**: Python >= 3.9, `PySide6>=6.9.3`, `darkdetect~=0.8.0`, `pywin32` (Windows only)
- **Submodule Policy**: `upstream/Rin-UI` is treated as an immutable external dependency. No files inside `upstream/Rin-UI` are modified; all compatibility fixes are performed in `rinui-rs` at build time (`$OUT_DIR/rinui_sanitized/`) or inside native/Rust host adapters.

---

## 2. Python Host File Audit (`upstream/Rin-UI/RinUI/core/`)

| Python Source File | Responsibilities Audited | Rust / C++ / ObjC++ Replacement |
|---|---|---|
| `RinUI/__init__.py` | Sets `QGuiApplication.setHighDpiScaleFactorRoundingPolicy(PassThrough)` before app construction; re-exports `core.*`. | `rinui_set_high_dpi_pass_through()` in `native/src/rinui_qt_bridge.cpp`, called inside `RinUIHost::new()` prior to `QGuiApplication::new()`. |
| `RinUI/core/config.py` | OS/Windows build checks (`is_windows`, `is_win10`, `is_win11`, `is_win11_22h2`), `Theme` & `BackdropEffect` enums, `DEFAULT_CONFIG`, `ConfigManager`, `RinConfig`, `_resolve_config_dir()` (`AppLocalDataLocation / "RinUI" / "config"`). | `rinui-platform` (`PlatformCapabilities`, `ThemeMode`, `BackdropEffect`, `RinConfig`, `ConfigStore`, `AppJsonStore`, `resolve_app_base_dir`). Adds atomic writes, schema migration (`schema_version`), corrupt-file backup recovery, and per-app directory isolation. |
| `RinUI/core/launcher.py` | `RinUIWindow`: validates `QCoreApplication`, calls `init_config()`, creates/shares `QQmlApplicationEngine`, hooks `warnings` & failure log, registers `ThemeManager` & Windows `WinEventManager`, adds import path, discovers `isRinUIWindow` root + child `QQuickWindow`s, attaches Windows `WinEventFilter`, macOS `NSWindow` tweaks, `setIcon()`, `release()` & cleanup. | `RinUIRuntime<'app>`, `RinUISession<'app>`, `SessionHandle<'app>`, and standalone wrapper `RinUIHost` (`crates/rinui-host/src/{runtime,app}.rs`) + `native/src/rinui_qt_bridge.cpp`. Supports owning or borrowing `QGuiApplication`/`QApplication`, `EngineMode::Shared` vs `EngineMode::Private`, generic checked root property/method interop (`set_root_property`, `get_root_property`, `invoke_root_method`), controlled `EngineConfigurator`, and owner-scoped cleanup. |
| `RinUI/core/theme.py` | `ThemeManager(QObject)`: singleton state, `ThemeListener` polling `darkdetect` every 1s, `themeChanged`/`backdropChanged` signals, slots (`toggle_theme`, `get_theme`, `get_theme_name`, `apply_backdrop_effect`, `get_backdrop_effect`, `set_theme_color`, `get_theme_color`), Windows 10/11 DWM & `SetWindowCompositionAttribute`. | `rinui::ThemeManager` (`crates/rinui-host/src/theme.rs`, CXX-Qt `#[qobject]`). Replaces 1s `darkdetect` polling with Qt 6 `QGuiApplication::styleHints()->colorSchemeChanged()` signal notifications; preserves exact QML slot/signal signatures, return codes (`0`, `-1`, `-2`), and DWM/Win10 WCA behavior (including explicit `ACCENT_DISABLED` cleanup when disabling Acrylic); adds macOS `NSAppearance` sync. |
| `RinUI/core/translator.py` | `RinUITranslator(QTranslator)`: resolves `<RINUI_PATH>/RinUI/languages/<locale>.qm`, falls back to `en_US.qm`, calls `QLocale.setDefault(...)` and `QTranslator.load(...)`. | `TranslationManager` (`crates/rinui-host/src/translator.rs`) + atomic 2-phase C++ `QTranslator` owner (`rinui_qt_bridge.cpp`). Decouples active locale / `QLocale::setDefault()` from RinUI `en_US` catalog fallback, normalizes locale tags (`zh-CN`, `zh_Hans_CN`, `zh`), rolls back on corrupted `.qm` loads, unloads missing optional app catalogs, supports empty catalogs (`en_US.qm`) cleanly on Qt 6.9, and retranslates all registered engines. |
| `RinUI/core/logger.py` | Opt-in `RinUI` Python logger (`configure_logging`, `get_logger`). | `tracing` + `tracing-subscriber` diagnostics + `qml-runtime.log` QML failure persistence (`RINUI_QML_LOG` opt-in for warnings). |
| `RinUI/core/windows/winapi.py` | Win32 structs, constants, `is_maximized`, `is_composition_enabled`, `find_window`, `get_resize_border_thickness`, `is_exact_monitor_sized_window`. | `native/src/rinui_win_bridge.cpp` using direct Win32/DWM headers + `RtlGetVersion`. |
| `RinUI/core/windows/window_state.py` | Reads `titleBarHeight`, `titleBarButtonWidth`, `titleBarSafeWidth`, `titleBarMaxButtonRect`, `titleBarLeadingHost`, `titleBarHost`; traverses interactive `QQuickItem` children (skipping `QQuickText`, `QQuickPlaceholderText`, `Text_QMLTYPE*`, `Label_QMLTYPE*`) with 200ms cache. | `native/src/rinui_win_bridge.cpp` traversing `QQuickItem` tree directly in C++ (no Python reflection overhead). |
| `RinUI/core/windows/window_manager.py` | `WinEventManager(QObject)`: `getWindowId`, `drag_window_event`, `syncWindowFrame`, `maximizeWindow`, `set_window_styles`, `extend_frame_into_client_area`, `apply_fullscreen_opengl_border_workaround`. | `WinEventManager` (`Q_OBJECT` in `native/include/rinui_win_bridge.h` + `native/src/rinui_win_bridge.cpp`), compiled with `moc`. |
| `RinUI/core/windows/non_client.py` | `WinEventFilter(QAbstractNativeEventFilter)`: `WM_NCHITTEST`, `WM_NCCALCSIZE` (with auto-hide taskbar adjustment), `WM_GETMINMAXINFO`, Win11 Snap Layouts `HTMAXBUTTON` hover/press state machine, `WM_ENTERSIZEMOVE`/`WM_EXITSIZEMOVE`. | `WinEventFilter` in `native/src/rinui_win_bridge.cpp`, cleaned up via `removeNativeEventFilter` on window release and host drop. |

---

## 3. QML Dependency Contracts

### 3.1 Context Properties & Invokable Contracts

Verified directly against `RinUI/themes/theme.qml`, `RinUI/windows/FluentWindowBase.qml`, and `RinUI/windows/WindowManager.qml`:

#### `ThemeManager` (Context Property: `"ThemeManager"`)

| Member | Kind | QML Signature / Type | Exact Semantics Verified in `themes/theme.qml` |
|---|---|---|---|
| `themeChanged` | Signal | `(theme: QString)` | Emits resolved `"Light"` or `"Dark"` (never `"Auto"`) on theme toggle or when system color scheme changes while configured theme is `"Auto"`. |
| `backdropChanged` | Signal | `(effect: QString)` | Emits requested `effect` string (`"mica"`, `"acrylic"`, `"tabbed"`, `"none"`). Connected in `theme.qml` to set `Utils.backdropEnabled = effect !== "none"`. |
| `toggle_theme` | Invokable | `(theme: QString) -> void` | Accepts `"Auto"`, `"Light"`, or `"Dark"`; ignores invalid values; persists config, updates native window theme, emits `themeChanged(actual_theme)`. |
| `get_theme` | Invokable | `() -> QString` | Returns resolved theme `"Light"` or `"Dark"`. |
| `get_theme_name` | Invokable | `() -> QString` | Returns configured theme `"Auto"`, `"Light"`, or `"Dark"`. |
| `apply_backdrop_effect` | Invokable | `(effect_type: QString) -> i32` | Calls `_update_window_theme()`. Returns `-2` if non-Windows or no windows registered; emits `backdropChanged(effect_type)`; applies Win11 21H2 fallback (`acrylic`/`tabbed` -> `mica`); returns `-1` if unsupported; applies DWM/WCA effect and persists `"backdrop_effect"`, returning `0`. |
| `get_backdrop_effect` | Invokable | `() -> QString` | Returns current configured `"backdrop_effect"` string. |
| `set_theme_color` | Invokable | `(color: QString) -> void` | Persists `"theme_color"` hex string immediately. |
| `get_theme_color` | Invokable | `() -> QString` | Returns persisted `"theme_color"` string. |

> **CXX-Qt 0.10.0 Naming Note**: CXX-Qt 0.7+ does not auto-convert `snake_case` to `camelCase`. `ThemeManager` exposes exact `snake_case` `#[qinvokable]` names (`toggle_theme`, `get_theme`, etc.) and uses `#[cxx_name = "themeChanged"]` / `#[cxx_name = "backdropChanged"]` on its `#[qsignal]` declarations to match QML's `onThemeChanged` / `onBackdropChanged` handlers.

#### `WinEventManager` (Context Property: `"WinEventManager"`, Windows)

| Member | Kind | QML Signature / Type | Exact Semantics Verified in `FluentWindowBase.qml` & `WindowManager.qml` |
|---|---|---|---|
| `getWindowId` | Invokable | `(window: QObject*) -> qint64` | Returns native window handle (`winId()`). |
| `drag_window_event` | Invokable | `(hwnd: qint64) -> void` | Releases capture and sends `WM_SYSCOMMAND` (`SC_MOVE | HTCAPTION`) to `hwnd` on Windows. |
| `syncWindowFrame` | Invokable | `(window: QObject*) -> void` | Applies `WS_CAPTION | WS_THICKFRAME`, extends DWM frame if `backdropEnabled`, sends activation messages + `SWP_FRAMECHANGED`, schedules `apply_windows_effects`. Defers to pending queue if `winId()` or registered windows are not yet ready. |
| `maximizeWindow` | Invokable | `(window: QObject*) -> void` | Toggles `showNormal()` / `showMaximized()` based on Win32 `GetWindowPlacement`. |

### 3.2 Window Properties Expected by Host

Verified in `RinUI/windows/FluentWindowBase.qml`, `RinUI/windows/window/Window.qml`, `RinUI/windows/TitleBar.qml`, `RinUI/windows/CtrlBtn.qml`:

| Property | Read/Write by Host | Purpose |
|---|---|---|
| `isRinUIWindow` | Read (`bool`) | Identifies root and child `QQuickWindow` instances managed by RinUI. |
| `useNativeMacFrame` | Read/Write (`bool`) | Controls macOS native `NSWindow` titlebar transparency & traffic lights; host writes `false` if disabled or Cocoa unavailable. |
| `backdropEnabled` | Read (`bool`) | Controls Windows DWM frame extension (`MARGINS(-1)`) and transparent render clear hooks. |
| `snapLayoutsSupported` | Write (`bool`) | Set to `true` on Windows 11 (`build >= 22000`) to suppress custom QML tooltip on `CtrlBtn` maximize button. |
| `titleBarHeight` | Read (`real`) | DIP titlebar height (default `32`). |
| `titleBarButtonWidth` | Read (`real`) | Width of caption button cluster (`fallback: 144.0`). |
| `titleBarSafeWidth` | Read (`real`) | Safe margin left of caption buttons (`fallback: 32.0`). |
| `titleBarMaxButtonRect` | Read (`QRectF`) | Local DIP rect of maximize button for `HTMAXBUTTON`. |
| `titleBarMaxButtonHovered` | Write (`bool`) | Driven by Win32 `WM_NCHITTEST`/`WM_NCMOUSEMOVE` for Snap Layouts hover rendering. |
| `titleBarMaxButtonPressed` | Write (`bool`) | Driven by Win32 `WM_NCLBUTTONDOWN`/`WM_NCLBUTTONUP` for Snap Layouts press state. |
| `titleBarLeadingHost` | Read (`QObject*`) | Titlebar leading overlay host item traversed for interactive client hit-testing. |
| `titleBarHost` | Read (`QObject*`) | Main custom titlebar host item traversed for interactive client hit-testing. |
| `enableFullscreenOpenGLBorderWorkaround` | Read (`bool`) | Enables `WS_BORDER` workaround on exact-monitor-sized OpenGL windows. |
| `icon` | Write (`QUrl`) | Set by host `set_icon()`. |

---

## 4. Qt 6 Module, Plugin & Resource Audit

### 4.1 Required Qt Modules

- **Compiled/Linked C++ Modules**: `Qt6::Core`, `Qt6::Gui`, `Qt6::Qml`, `Qt6::Quick`, `Qt6::QuickControls2` (for `QQuickStyle::setStyle("Basic")`).
- **Additional Runtime Modules Required by RinUI QML**: `Qt5Compat` (`Qt6Core5Compat`, `Qt5Compat.GraphicalEffects`) and `Qt6ShaderTools` (`qtshadertools`, required by `Qt5Compat.GraphicalEffects` on Qt 6.9+).
- **Required Runtime QML Imports** (audited across all `.qml` files in `upstream/Rin-UI/RinUI`):
  - `QtQuick` (`2.12`, `2.15`, unversioned)
  - `QtQuick.Controls` (`2.3`, `2.15`, unversioned)
  - `QtQuick.Controls.Basic` (`2.15`, unversioned)
  - `QtQuick.Templates` (`2.15`)
  - `QtQuick.Layouts` (`2.15`, unversioned)
  - `QtQuick.Window` (`2.3`, `2.15`)
  - `QtQuick.Effects`
  - `QtQuick.Shapes` (`1.15`)
  - `QtQml` (`2.15`)
  - `Qt5Compat.GraphicalEffects` (imported by 19 upstream RinUI files)

### 4.2 Broken `qmldir` Declarations & Relative Resource Analysis

| File in `upstream/Rin-UI/RinUI/` | Upstream Issue Found | Build-Time Sanitization in `$OUT_DIR/rinui_sanitized/` |
|---|---|---|
| `qmldir` | Lists 3 non-existent files: `DateField 1.0 components/DateAndTime/DateField.qml`, `TeachingTip 1.0 components/DialogsAndFlyouts/TeachingTip.qml`, `ScrollViewer 1.0 components/ScrollViewer.qml`. Missing `ScrollView.qml`. | Drop the 3 missing entries; register `ScrollView 1.0 components/ScrollView.qml`. |
| `components/qmldir` | Lists non-existent `HyperlinkButton 1.0 BasicInput/HyperlinkButton.qml`. Also contains `../themes/...` and `../utils/...` relative paths for `Theme`, `Typography`, `Severity`, `Position`, `FloatLayer`. | Drop `HyperlinkButton`. Preserve `../themes/` and `../utils/` directory hierarchy in QRC under `/rinui/RinUI` so Qt resolves canonical URLs without duplicate singletons. |
| `themes/qmldir` | Versionless singleton entries `singleton Colors Colors.qml` and `singleton Appearance Appearance.qml`. | Normalize to `singleton Colors 1.0 Colors.qml` and `singleton Appearance 1.0 Appearance.qml` while keeping `Theme` and `Utils` consistent. |
| `windows/qmldir` | Lists non-existent `ErrorPage 1.0 ErrorPage.qml` (`ErrorPage.qml` actually lives in `components/Navigation/ErrorPage.qml` and is pushed by relative URL). | Drop `ErrorPage 1.0 ErrorPage.qml`. |
| `components/DateAndTime/Calendar.qml`, `DatePicker.qml`, `components/Text/SpinBox.qml` | Upstream `property var locale: Qt.locale()` and `DatePicker.monthModel` (`Qt.locale().monthName(i)`) / `SpinBox` inherited `locale` only evaluate once at creation time, leaving pre-existing controls stale when switching languages live via `set_language()`. | Host-side `sanitize_runtime_qml` (applied identically in `$OUT_DIR/rinui_sanitized/` and dev `ResourceView` without editing `upstream/Rin-UI`) injects `qsTr("")`-dependent default `locale` bindings (and updates `DatePicker.monthModel` to read `datePicker.locale.monthName(i)`) while preserving explicit caller declarative `locale:`/`calendarLocale:` and imperative `cal.locale = ...` overrides; `rinui_engine_retranslate()` also delivers deduplicated `QEvent::LocaleChange` across registered roots, items, and dynamic windows. |

### 4.3 Dynamic QML & Resource Resolution Strategy

- **Dynamic QML creation**:
  - `RinUI/themes/theme.qml`: `Qt.createQmlObject("import '../themes'; " + themeName + " {}", themeManager)`
  - `RinUI/components/Navigation/NavigationView.qml`: `Qt.createComponent(page)` and `stackView.push("ErrorPage.qml", ...)`
  - `RinUI/utils/FloatLayer.qml`: `infoBarComponent.createObject(...)`
- **Relative non-QML assets**:
  - `RinUI/themes/utils.qml`: `import "../assets/fonts/FluentSystemIcons-Index.js" as Icons`, `Qt.resolvedUrl("../assets/fonts/FluentSystemIcons-Resizable.ttf")`, `Qt.include(fontIconIndexSource)`.
  - `RinUI/components/Styles/AcrylicBrush.qml`: `source: "../../assets/img/noise.png"`.
  - `RinUI/languages/`: `en_US.qm`, `zh_CN.qm`.
- **Anti-Shadowing QRC Prefix**:
  - RinUI embedded resources are mounted at QRC prefix `/rinui/RinUI` (**not** `/qt/qml/RinUI`, because `qrc:/qt/qml` is always searched implicitly by Qt and would shadow filesystem dev mode or trigger duplicate singleton instantiation).
  - In `ResourceMode::Embedded`, only `qrc:/rinui` is added to `QQmlApplicationEngine::addImportPath`.
  - In `ResourceMode::Filesystem(path)`, only `path` (the parent of `RinUI/`) is added to `addImportPath`, and `qrc:/rinui` is omitted.

---

## 5. Feature Compatibility Matrix (Python Host vs `rinui-rs`)

| Capability | Upstream Python Host | `rinui-rs` Rust Host | Notes |
|---|---|---|---|
| HiDPI `PassThrough` before app creation | `RinUI/__init__.py` | `RinUIHost::new()` (`rinui_set_high_dpi_pass_through`) | Identical timing & policy. |
| QML Style (`Basic`) | PySide6 defaults / templates | Explicit `QQuickStyle::setStyle("Basic")` | Guarantees custom RinUI controls work across OS themes. |
| `ThemeManager` QML signals & slots | `RinUI/core/theme.py` | `rinui::ThemeManager` (CXX-Qt) | Exact method/signal names & return codes (`0`/`-1`/`-2`), guarded by dynamic RW `theme_borrow_state` + `mut_slot_active` QML slot re-entrancy checks (`ThemeManagerRef`/`ThemeManagerMut` RAII guards return `Err(HostError::ReentrantCall)` on overlapping borrows or QML re-entry). |
| System theme detection | 1s `QThread` polling `darkdetect` | Qt 6 `QStyleHints::colorSchemeChanged` signal | Zero polling overhead; immediate live updates. |
| Theme accent color & multi-window sync | `ThemeManager` singleton + `windows` list | `ThemeManager` + `Rc<RefCell<ConfigStore>>` + owner-scoped `RinUISession` window registry | Singletons shared engine-wide; `sync_windows()` refuses auto-claiming ambiguous shared-engine `session_id == 0` windows (use `register_window_from_property()`); QML callback self/peer session drops defer engine/window teardown cleanly across `QmlExecGuard` and Qt event loops. |
| Configuration persistence | `ConfigManager` (`rin_ui.json`) | `rinui-platform` (`ConfigStore`, `AppJsonStore`) | Atomic write, `schema_version` migration, corrupt backup recovery, isolated per-app dirs. |
| Internationalization | `RinUITranslator` (`QTranslator`) | `TranslationManager` + C++ `QTranslator` owner + `sanitize_runtime_qml` | Supports QRC `.qm`, filesystem `.qm`, `en_US` fallback, app catalogs, live `retranslate()` + `QEvent::LocaleChange`, and reactive updates on pre-existing `Calendar` / `CalendarDatePicker` / `DatePicker` / `SpinBox` controls without overriding explicit `locale` / `calendarLocale`. |
| `QGuiApplication` adoption & multi-session | Hosts attached to external Qt apps / `QApplication` share one app | `RinUIRuntime::attach_existing`, `attach_existing_raw`, `EngineMode::{Shared, Private}` | Enforces process-level `QGuiApplication`, `QLocale`, Quick style, and translator invariants while isolating per-session roots/windows and optional per-engine QML contexts. |
| `TranslationManager` | Coupled RinUI `.qm` selection with `QLocale` | Decoupled `active_locale` vs `rinui_locale` + per-catalog results | Preserves app-unique locales (e.g., `fr_FR` app `.qm` + `en_US` RinUI fallback) with atomic rollback on error. |
| Dev filesystem view | Direct disk import | Per-launch sanitized staging copy owned by `ResourceView` & `GalleryDevView` | Shares exact `qmldir` sanitizer rules with `build.rs`; reloads disk edits on restart without touching `upstream/Rin-UI`. |
| Windows native window & DWM | `pywin32` + `ctypes` (`WinEventFilter`, `WinEventManager`) | Runtime-lifetime C++ `WinEventFilter` & `WinEventManager` + `dwmapi`/`user32`/`shell32` | Compiles/runs in `windows-latest` CI (`Qt 6.9.3` MSVC); includes Win10 Acrylic `ACCENT_DISABLED` disable fix; physical Win10/11 desktop/multi-monitor visual QA pending. |
| macOS native window (`NSWindow`) | `PyObjC` (`AppKit` + `objc`) | Objective-C++ `rinui_mac_window.mm` (ARC) + `RinUINativeMacFrame` context signal | Tested locally (`Qt 6.12.0`) & CI (`Qt 6.9.3`) under `offscreen` and strict Cocoa QPA in both self-drawn & `--upstream-mac-frame` modes; interactive visual QA pending. |
| Linux X11 / Wayland | Qt `FramelessWindowHint` + `startSystemResize` | Qt `FramelessWindowHint` + `rinui_linux_window.cpp` | Tested in `ubuntu-latest` CI (`offscreen` + headless `xvfb-xcb`); graceful `-2` for DWM backdrops; real X11 WM & native Wayland compositor QA pending. |
| Standalone release & MSIX | PyInstaller spec + `package_msix.ps1` | Embedded RCC (`rinui_resources.qrc`) + host `package_msix.ps1` (`SkipSign`/`VerifyLayout`) | Rust directory/MSIX builds without Python/PyInstaller; CI checks unsigned MSIX unpack; MSIX Windows install/launch & Authenticode/Developer ID signing pending credentials/desktop. |

### macOS titlebar modes and interactive validation

Upstream QML (`RinUI/windows/FluentWindow.qml` and `RinUI/windows/window/Window.qml`) sets
`property bool useNativeMacFrame: isMacOS`. When the upstream Python runtime can import
`AppKit` / `objc` (`PyObjC`), macOS defaults to the native frame; when `PyObjC` is
unavailable, Python falls back to RinUI's self-drawn QML frame. Upstream `FluentWindow`
uses a 36-point titlebar when `useNativeMacFrame` is enabled, and the theme's 48-point
titlebar with QML traffic-light buttons when it is disabled.

By intentional compatibility policy, `rinui-rs` does **not** mimic Python's implicit
native default on macOS: `MacWindowOptions::default()` resolves to
`MacWindowOptions::self_drawn()` (`enabled: false`) so host embeddings opt in explicitly
rather than altering `NSWindow` style masks unexpectedly. Consumers who want upstream's
macOS framing behavior should explicitly select
`MacWindowOptions::upstream_compatible()` (or `HostConfig::with_upstream_mac_frame()` /
`RuntimeConfig::with_upstream_mac_frame()`), or `MacWindowOptions::native(offset_x,
offset_down)`. Gallery keeps self-drawn as its default and supports
`--upstream-mac-frame`, `--native-mac-frame`, and `--no-native-mac-frame` (last flag
wins). Both `GalleryMain.qml` and `MultiWindowDialog.qml` bind `useNativeMacFrame` to
the pre-first-frame `RinUINativeMacFrame` context property set by the runtime before
loading QML, and `rinui_mac_window.mm` records `_rinuiMacFrameDiagnostic` on every window
(`self_drawn`, `native_cocoa`, `fallback:non_cocoa_qpa`, `fallback:no_nswindow`, or
`self_drawn:overridden_post_load`). Compare screenshots using the same frame mode, Qt
version, window size, scale factor, and upstream revision; note also that the 36 vs 48
point `titleBarHeight` invariant applies specifically to `FluentWindow` roots, not
arbitrary `Window` dialogs such as `MultiWindowDialog.qml`. Upstream files remain
unmodified.

In native mode, upstream disables its titlebar drag `MouseArea`. The macOS host
therefore installs a per-window input filter to start a system move on empty
titlebar presses and toggle maximization on double-click. Hit testing preserves
interactive QML controls, including the navigation back button outside the
titlebar content host, and checks the actual QML `enabled` property because
`MouseArea` shadows the non-virtual `QQuickItem` accessor. Qt Quick Controls'
full-window content container is excluded from hit testing after inspecting
its children. If the QPA rejects a system move, a global-coordinate drag
fallback keeps the titlebar functional. The filter is removed with the
window's native integration.

Offscreen tests and startup smoke tests establish startup, resource resolution,
window frame selection/diagnostics, `FluentWindow` titlebar height, navigation/error
recovery, theme switching, secondary window inheritance, and decoupled i18n, but they do
not replace interactive physical desktop QA:

- Prior baseline manual macOS desktop spot-checks on `Qt 6.12.0` (`2026-11-19`) observed
  titlebar dragging (`(120, 88) -> (20, 188)` native, `(120, 120) -> (20, 220)` QML),
  search-field focus/input, double-click maximize/restore, All Samples navigation, and
  back button return, with the native drag exercising Qt's coordinate fallback.
- Full interactive visual/desktop QA after the upstream-parity changes (physical-mouse
  macOS system-move & traffic-light geometry/fullscreen restore, Windows 10 Acrylic &
  Windows 11 21H2/22H2+ backdrop/Snap/workarea/DPI transitions, and Linux real X11 WM /
  native Wayland compositor checks) remains open and is tracked separately in
  [`docs/validation-report.md`](validation-report.md).
