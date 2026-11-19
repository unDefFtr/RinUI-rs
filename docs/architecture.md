# `rinui-rs` Architecture

## 1. Design Principles

`rinui-rs` replaces RinUI's Python/PySide6 runtime (`RinUI/core/`) with a reusable Rust + C++/Objective-C++ Qt 6 host while keeping `upstream/Rin-UI` completely unmodified as a pinned Git submodule.

Key invariants:

1. **Unmodified Upstream Submodule**: Compatibility fixes (`qmldir` sanitization, broken file exclusion, versionless singleton normalization) happen in `$OUT_DIR/rinui_sanitized/` at Cargo build time, never inside `upstream/Rin-UI`.
2. **Thread-Safe Main-Thread Ownership**: `RinUIHost` and `HostHandle` are `!Send + !Sync` (`PhantomData<Rc<()>>`). No public API allows raw cross-thread `QObject` mutation.
3. **Deterministic Qt Destruction Order**: Rust struct drop order enforces:
   1. Persist config (`ConfigStore::save()`).
   2. Mark `HostHandle` disposed (`shared = None`).
   3. Release all QML root trees, detach Windows `WinEventFilter`, and remove macOS `NSNotificationCenter` observers (`rinui_release_all_roots_and_platform()`).
   4. Remove and destroy all `QTranslator` instances (`rinui_translator_clear_all()`).
   5. Clear QML component cache and collect garbage.
   6. Drop `QQmlApplicationEngine`.
   7. Drop application-owned `QObject` context properties (`owned_context_objects`).
   8. Drop `ThemeManager`.
   9. Drop `QGuiApplication` last.
4. **Consumer Isolation**: `rinui-host` contains zero Goosy-specific business logic, rendering, or navigation. Each consumer passes its own `AppIdentity` so configuration and logs remain strictly isolated per application.

---

## 2. Repository Layout

| Directory / File | Purpose |
|---|---|
| `upstream/Rin-UI` | Pinned upstream RinUI Git submodule (read-only) |
| `crates/rinui-platform` | Pure Rust OS/Windows build detection, `PlatformCapabilities`, `RinConfig`, `ConfigStore`, and atomic JSON persistence (`AppJsonStore`) |
| `crates/rinui-host` | Public Rust host API (`RinUIHost`, `HostHandle`, `ThemeManager` CXX-Qt QObject, `TranslationManager`, `ResourceMode`, `WindowCustomizer`) and build-time RinUI QRC generator |
| `native/` | C++ & Objective-C++ Qt/OS adapters (`rinui_qt_bridge`, `rinui_theme_bridge`, `rinui_mac_window.mm`, `rinui_win_bridge`, `rinui_linux_window`) |
| `qml/` | Host smoke-test and multi-window QML fixtures (`SmokeTest.qml`, `MultiWindowDialog.qml`) |
| `examples/gallery` | Standalone Rust-hosted RinUI Gallery reusing upstream `examples/` QML + custom `RustHostDemoPage` |
| `cmake/` | CMake integration helpers (`RinUIHost.cmake`, `BundleHelpers.cmake`) |
| `scripts/` | Upstream submodule audit (`audit_upstream.py`) and platform deployment scripts |
| `docs/` | Architecture, compatibility contract, consumer guide, packaging, and upstream update guide |

---

## 3. Resource Packaging & Anti-Shadowing

RinUI's upstream QML files mix `import RinUI` with relative imports such as `import "../../themes"`, `import "../utils"`, and `Qt.resolvedUrl("../assets/fonts/FluentSystemIcons-Resizable.ttf")`.

To keep QML canonical URL deduplication intact (avoiding duplicate singletons) **and** prevent compiled QRC resources from shadowing filesystem development mode:

- Embedded RinUI resources are mounted at QRC prefix **`/rinui/RinUI`** (never `/qt/qml/RinUI`, because `qrc:/qt/qml` is always searched implicitly by Qt).
- Under `ResourceMode::Embedded`, `RinUIHost` adds `qrc:/rinui` to `QQmlApplicationEngine::addImportPath` and loads `.qm` files from `:/rinui/RinUI/languages/<locale>.qm`.
- Under `ResourceMode::Filesystem(dir)`, `RinUIHost` adds `<dir>` (parent of `RinUI/`) to `addImportPath` and omits `qrc:/rinui`, loading `.qm` files directly from `<dir>/RinUI/languages/<locale>.qm`.

---

## 4. Platform Window Integration

- **macOS (`native/src/rinui_mac_window.mm`)**: Uses ARC Objective-C++ to configure `NSWindowTitleHidden`, `setTitlebarAppearsTransparent:YES`, `setMovableByWindowBackground:NO`, `NSWindowStyleMaskFullSizeContentView`, `NSAppearance` Light/Dark/Auto synchronization, and idempotent traffic-light container offsetting across retries, `visibleChanged`, resize, and fullscreen exit. Guarded on `QGuiApplication::platformName() == "cocoa"` so headless `offscreen` runs are safe.
- **Windows (`native/include/rinui_win_bridge.h`, `native/src/rinui_win_bridge.cpp`)**: Implements `WinEventManager` (`Q_OBJECT`) and `WinEventFilter` (`QAbstractNativeEventFilter`) for `WM_NCCALCSIZE`, `WM_GETMINMAXINFO`, `WM_NCHITTEST` (with `titleBarLeadingHost`/`titleBarHost` interactive item traversal), Win11 Snap Layouts `HTMAXBUTTON` states, DWM `DWMWA_SYSTEMBACKDROP_TYPE` / `DWMWA_MICA_EFFECT` / Win10 `SetWindowCompositionAttribute`, and transparent render clear hooks.
- **Linux (`native/src/rinui_linux_window.cpp`)**: Uses Qt's compositor-supported `FramelessWindowHint` and `startSystemResize()`, and returns `-2` gracefully from `apply_backdrop_effect`.
