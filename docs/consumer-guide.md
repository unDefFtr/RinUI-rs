# Consumer Integration Guide

This guide explains how external Rust desktop applications (such as **Goosy**) embed and interact with `rinui-rs`.

---

## 1. Adding `rinui-host` to Your Cargo Project

Add `rinui-host` (and `cxx` / `cxx-qt` / `cxx-qt-lib` if defining your own `QObject` types) to your application's `Cargo.toml`:

```toml
[dependencies]
# From a pinned Git pre-release tag:
rinui-host = { git = "https://github.com/unDefFtr/rinui-rs", tag = "v0.1.0-alpha.1" }
# Or from a local workspace checkout:
# rinui-host = { path = "../rinui-rs/crates/rinui-host" }
cxx = "1.0"
cxx-qt = "0.10.0"
cxx-qt-lib = { version = "0.10.0", features = ["qt_full"] }

[build-dependencies]
cxx-qt-build = "0.10.0"
```

### Build Toolchain Prerequisites

- Ensure Qt 6 includes **`qt5compat`** and **`qtshadertools`** (`Qt 6.9.3` verified in CI; `Qt 6.12.0` verified locally).
- **Linux**: Install `lld` (or `gold` / `mold`); CXX-Qt links generated Qt resource/QML initializers with `+whole-archive`, which GNU `ld.bfd` does not support cleanly.
- **macOS arm64 + Xcode 26 (Apple Clang 21) + Qt < 6.11.1**: Export `CXXFLAGS="-include arm_acle.h"` to work around [QTBUG-145239](https://qt-project.atlassian.net/browse/QTBUG-145239) in Qt's `qyieldcpu.h`.

Because `rinui-host` sets `links = "rinui-host"` and re-exports `cxx-qt-lib` via `cxx_qt_build::Interface`, your crate's `build.rs` automatically inherits Qt module links (`Core`, `Gui`, `Qml`, `Quick`, `QuickControls2`) and RinUI's embedded QML resources when using `CxxQtBuilder`:

```rust
// build.rs
use cxx_qt_build::CxxQtBuilder;

fn main() {
    CxxQtBuilder::new()
        .file("src/controller.rs")
        .qrc("resources/app.qrc")
        .build();
}
```

> **Note**: If your consumer crate has no custom `#[cxx_qt::bridge]` QObjects of its own, you do not even need `cxx-qt-build` in your `build.rs`—depending on `rinui-host` links Qt and RinUI automatically.

---

## 2. Minimal Host Startup

Use a helper function returning `Result<i32, ...>` with `ExitCode` in `main()` (avoid calling `std::process::exit(host.exec())` directly while `RinUIHost` is still in scope, so `RinUIHost::drop` always persists configuration and runs ordered Qt teardown):

```rust
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
};
use std::process::ExitCode;

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let config = HostConfig {
        identity: AppIdentity::new("Acme", "Goosy")
            .with_domain("goosy.example")
            .with_version("1.0.0"),
        resource_mode: ResourceMode::Auto, // Uses RINUI_DEV_PATH if set, else embedded QRC
        config_location: ConfigLocation::Standard,
        locale: None, // System locale with automatic fallback to en_US
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    };

    let mut host = RinUIHost::new(config)?;
    let loaded = host.load(&QmlSource::Url("qrc:/app/Main.qml".to_owned()))?;
    host.set_root_string_property(loaded.root_id, "title", "My App");

    Ok(host.exec())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from((code & 0xFF) as u8),
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
```

### macOS Titlebar Mode

`MacWindowOptions::default()` equals `MacWindowOptions::self_drawn()` and uses RinUI's self-drawn QML titlebar and traffic-light controls on macOS. Upstream QML defaults to native framing on macOS, so opting into upstream framing is explicit in Rust:

```rust
// Explicit upstream-compatible macOS native frame (no-op on Windows/Linux):
let config = HostConfig::default().with_upstream_mac_frame();
// Or customize traffic-light offsets explicitly:
let custom = MacWindowOptions::native(12, 12);
```

Before any QML loads, `RinUIRuntime`/`RinUISession` binds the context property `RinUINativeMacFrame` (`bool`), allowing both main `FluentWindow`s and secondary `Window` dialogs to follow the configured mode on their first frame. On macOS, `rinui_mac_window.mm` records `_rinuiMacFrameDiagnostic` on each registered `QQuickWindow` (`"self_drawn"`, `"native_cocoa"`, `"fallback:non_cocoa_qpa"`, `"fallback:no_nswindow"`, or `"self_drawn:overridden_post_load"`).

Gallery mirrors the same policy via `--no-native-mac-frame` (default), `--upstream-mac-frame`, and `--native-mac-frame` (last flag wins).

### Filesystem Dev Mode (`--dev` / `RINUI_DEV_PATH`)

When using `ResourceMode::Filesystem(path)` or `ResourceMode::Auto` with `RINUI_DEV_PATH` set, `rinui-host` copies the latest disk checkout (`RinUI/` QML, JS, fonts, images, translations) into a fresh per-launch staging directory owned by `ResourceView`, applying the exact same `qmldir` sanitization rules as embedded `build.rs` without ever mutating `upstream/Rin-UI`. `ResourceView::describe()` reports both `source` and `load` roots, and the staging directory stays alive until the associated engine/runtime drops.

Gallery also prepares an isolated `GalleryDevView` (requiring upstream `examples/gallery.qml`, configurable via `--examples-dir` / `RINUI_EXAMPLES_DIR`) with an `UpstreamGallery.qml` alias next to the upstream `pages/`, `components/`, and `assets/` directories so disk edits appear on the next launch.

### Configuration Directory Isolation

`AppIdentity::new("Acme", "Goosy")` isolates configuration under:
- **macOS**: `~/Library/Application Support/Acme/Goosy/`
- **Windows**: `%LOCALAPPDATA%\Acme\Goosy\`
- **Linux**: `$XDG_DATA_HOME/Acme/Goosy/` (or `~/.local/share/Acme/Goosy/`)

RinUI writes `RinUI/config/rin_ui.json` inside that app-scoped base directory. For your application's own JSON config, use `rinui_host::AppJsonStore` (writes `<base>/config/<filename>` atomically with corruption backup recovery).

---

## 2.5 Process Runtime, Borrowing Existing Qt Apps & Multi-Session Engines

For hosts that embed RinUI inside an existing `QGuiApplication` / `QApplication` (including Qt Widgets hosts) or manage multiple QML windows across shared or isolated QML engines, use `RinUIRuntime` + `RinUISession`:

- **Standalone process runtime**: `RinUIRuntime::new(RuntimeConfig)` creates and owns `QGuiApplication`.
- **Borrowing an existing Qt app**: `RinUIRuntime::attach_existing(&qgui_app, config)` (compile-time `'app` lifetime tie) or `unsafe RinUIRuntime::attach_existing_raw(app_ptr, config)` (for foreign C++/Widgets pointers, enforcing `'app` outlives the runtime and sessions). Rejects bare `QCoreApplication` (`NotGuiApplication`), wrong thread (`WrongThread`), and duplicate runtime attachment (`ApplicationAlreadyInitialized`).
  - Borrowed mode records and restores app identity and global settings on drop, detaches only RinUI-installed translators/filters/signals, never destroys the caller's `QGuiApplication`/`QApplication`, and rejects `runtime.exec()` (`BorrowedApplicationExecNotAllowed`) so the caller drives its own event loop.
  - Because Qt Quick Controls style and `Qt::AA_` / HiDPI rounding policies apply globally before window/app initialization, borrowed mode requires callers to set HiDPI before app creation and errors if switching Qt Quick Controls style after `QQuickWindow` creation (`QuickControlsStyleConflict`).
- **Sessions (`EngineMode::Shared` vs `EngineMode::Private`)**:
  - `runtime.create_session(SessionConfig)` creates either a session on the runtime's shared `QQmlApplicationEngine` or an isolated private `QQmlApplicationEngine` (isolating QML context properties, singletons, and component cache while still sharing process-wide `ThemeManager`, active locale, and config).
  - `ContextPropertyConflict::Reject` / `Overwrite` controls shared-engine context collisions cleanly.
  - Disposing a session immediately makes its `SessionHandle`s return `HostError::HostDisposed`. Its roots/windows use `deleteLater()` and its Private engine always enters a deferred queue. This includes QML Timer callbacks dispatched directly by the caller's `app.process_events()` or `app.exec()`. Sibling sessions remain callable; engine-owned context objects and temporary filesystem views stay alive until cleanup.
  - Dropping the last Runtime/Session owner in such a callback uses two-phase teardown: detach owned windows, translators and app signals and restore borrowed settings immediately; retain engines, `ThemeManager`, services and the runtime reservation until QML execution has returned. Each retained Engine has an independent Qt `deleteLater()` fence. Cleanup waits for actual DeferredDelete delivery; a zero-delay timer or aboutToQuit wakeup alone cannot establish safety. Application post routines remain the final shutdown fallback.
  - **Drain before reattaching**: continue driving the external event loop after releasing RinUI. Until pending teardown finishes, `attach_existing` returns `ApplicationAlreadyInitialized`. If no more events are processed, normally destroying the external app runs the post-routine fallback and releases the reservation. Do not destroy the app from an active QML callback; the raw API's application lifetime contract still applies. `process::exit` / abort skips normal destructor cleanup. For borrowed applications, `runtime.process_events()` / `session.process_events()` only schedule cleanup on execution-guard release, including nested calls from a QML callback; a zero Qt loop level does not prove that outer callback has returned. Continue external event processing after it returns so the queue can drain. Standalone runtimes retain the guard-controlled flush outside an event loop.
  - **External manual event drivers**: direct nested `app.process_events()` is covered even without a RinUI execution guard. Borrowed cleanup does not force global DeferredDeletes from that callback. When driving Qt by repeated process_events calls without exec, dispatch DeferredDeletes explicitly at an outer driver boundary after callbacks return: `QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete)` in C++ (or an equivalent application binding). Qt does not promise to process events posted outside an event loop through processEvents alone. Do not put this forced flush in the nested QML callback. Pending cleanup can remain until explicit dispatch, exec resumes, or normal app destruction; a timer is not used to bypass this Qt rule. See [Qt deleteLater documentation](https://doc.qt.io/qt-6/qobject.html#deleteLater).

---

## 3. Interacting with Loaded Root Objects & Extending `QQmlApplicationEngine`

### Generic Root Property & Method Interop

`RinUIHost`, `RinUISession`, and `HostHandle` / `SessionHandle` expose checked root object APIs (without creating bogus dynamic properties on typoed names and without JS `eval`):

```rust
use rinui_host::cxx_qt_lib::{QString, QVariant};

host.set_root_property(loaded.root_id, "titleProp", &QVariant::from(&QString::from("Updated")))?;
let val = host.get_root_property(loaded.root_id, "titleProp")?;
let sum = host.invoke_root_method(
    loaded.root_id,
    "addInts",
    &[QVariant::from(&10i32), QVariant::from(&32i32)],
)?;
```

- Unknown property names, read-only properties, type mismatches, non-existent/unsupported method signatures, or disposed/foreign `RootId`s return structured `HostError` variants (`PropertyNotFound`, `PropertyReadOnly`, `PropertyTypeMismatch`, `MethodNotFound`, `MethodInvokeFailed`, `RootDisposed`, `NotOwned`).
- Method calls, property writes, QML loads, translations, and event processing that trigger QML -> Rust callbacks (including dropping a session mid-callback) remain protected by re-entrancy checks, `InvokeMethodGuard`, and `QmlExecGuard` deferred deletion fencing.

### Controlled Engine Configuration (`configure_engine`)

Configure offline storage paths, extra QML file selectors, import paths, or advanced Qt engine hooks (e.g. image providers / network access manager factories) inside a scoped main-thread closure without leaking raw engine pointers:

```rust
host.configure_engine(|cfg| {
    cfg.set_offline_storage_path(&offline_storage_dir);
    cfg.set_extra_file_selectors(&["compact"]);
    // For advanced C++ interop only: unsafe { cfg.with_raw_engine_mut(|eng| ...) }
})?;
```

See [`crates/rinui-host/examples/engine_config_demo.rs`](../crates/rinui-host/examples/engine_config_demo.rs) for a complete runnable example.

---

## 4. Exposing Custom Rust `QObject` Models/Controllers to QML

1. Define a `#[cxx_qt::bridge]` module in your crate (with `#[namespace = ""]` on `unsafe extern "C++"` for `QString` / `QStringList` if the bridge itself uses a custom namespace):

```rust
#[cxx_qt::bridge(namespace = "goosy")]
pub mod qobject {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = rinui_host::cxx_qt_lib::QString;

        include!("goosy/controller_factory.h");
        pub fn goosy_controller_new() -> UniquePtr<AppController>;
    }

    extern "RustQt" {
        #[qobject]
        type AppController = super::AppControllerRust;

        #[qinvokable]
        #[cxx_name = "startRender"]
        fn start_render(self: Pin<&mut AppController>);
    }

    unsafe extern "RustQt" {
        #[qsignal]
        #[cxx_name = "progressChanged"]
        fn progress_changed(self: Pin<&mut AppController>, percent: i32);
    }
}
```

2. Transfer ownership of the constructed `UniquePtr<AppController>` to `RinUIHost` **before** calling `host.load(...)`:

```rust
let mut controller = qobject::goosy_controller_new();
// Attach a main-thread HostHandle if the controller needs multi-window or i18n calls:
controller.as_mut().unwrap().attach_host(host.handle());
host.expose_qobject("AppController", controller)?;
```

`RinUIHost` guarantees `AppController` outlives `QQmlApplicationEngine` and is destroyed safely on the Qt main thread before `QGuiApplication`.

---

## 5. Application Translations & Live Switching

Register one or more `.qm` catalogs with `RinUIHost` or `RinUIRuntime`:

```rust
use rinui_host::TranslationCatalog;

host.register_translation_catalog(TranslationCatalog::QrcPrefix {
    id: "goosy".to_owned(),
    prefix: ":/goosy/languages".to_owned(),
})?;

// Switch language at runtime and retranslate all registered QML engines:
let res = host.set_language("zh_CN")?;
assert_eq!(res.active_locale, "zh_CN");
assert_eq!(res.effective_locale, "zh_CN"); // Follows active_locale even if RinUI falls back
```

**Decoupled fallback & transactional rollback semantics**:
- Requested locale tags are normalized (`zh-CN`, `zh_Hans_CN`, `zh` -> `zh_CN`; unknown tags -> `en_US` with `res.active_locale_fallback == true`).
- Application catalogs and RinUI's built-in catalog resolve independently: if your app ships `fr_FR.qm` while RinUI lacks `fr_FR.qm`, `res.active_locale == "fr_FR"`, `QLocale::setDefault()` is set to French, `res.rinui_locale == "en_US"` (`res.rinui_used_fallback == true`), and `res.app_catalogs` reports your catalog's loaded French `.qm`.
- Switching commits only after the atomic C++ 2-phase load succeeds (corrupted `.qm` files roll back cleanly; missing optional app `.qm` files unload any stale prior translator instead of leaving mismatched strings).

---

## 6. Customizing Native Windows (`WindowCustomizer`)

Implement `rinui_host::WindowCustomizer` and register it before `host.load(...)` to receive every discovered `NativeWindowRef`:

```rust
use rinui_host::{NativeWindowRef, WindowCustomizer};

struct MyWindowCustomizer;

impl WindowCustomizer for MyWindowCustomizer {
    fn on_windows_discovered(&mut self, windows: &[NativeWindowRef]) {
        for win in windows {
            let _raw_id = win.win_id(); // NSView* (macOS), HWND (Windows), XID (X11)
        }
    }
}

host.set_window_customizer(Box::new(MyWindowCustomizer));
```

If your application opens extra `FluentWindow` / `Window` objects dynamically from QML (`Qt.createComponent`), call `session.sync_windows()` (or `host.sync_windows()`). On `EngineMode::Shared`, `sync_windows()` attributes windows via `_rinuiOwnerSessionId`, `parent`/`transientParent` chains, and QML creation context; it refuses to auto-claim ambiguous `session_id == 0` windows (for example, windows spawned by a shared QML singleton). In that case, assign the window reference to a property on your loaded root QML object and claim ownership explicitly:

```rust
let win = session.register_window_from_property(loaded.root_id, "detachedWindow")?;
```

Explicitly registered detached windows are also cleaned up automatically when `release_root(loaded.root_id)` runs or the owning session drops.

---

## 7. Lifecycle, `ThemeManager` Borrow Protection & Migration

- **Main-Thread Affinity (`!Send + !Sync`)**: `RinUIRuntime`, `RinUISession`, `SessionHandle`, `RinUIHost`, `HostHandle`, `ThemeManagerRef`, and `ThemeManagerMut` opt out of `Send`/`Sync`. You can inspect thread affinity via `rinui_host::is_qt_main_thread()`.
- **Single Active Host Guard**: Qt permits one `QGuiApplication` per process. Creating a second `RinUIHost` while one is active returns `Err(HostError::ApplicationAlreadyInitialized)` without touching live Qt state. Once the previous `RinUIHost` drops cleanly, sequential re-initialization succeeds.
- **Re-Entrancy & Disposal**: `SessionHandle`/`HostHandle` use non-panicking `try_borrow`/`try_borrow_mut` guards (`Err(HostError::ReentrantCall)` on re-entrant access) and return `Err(HostError::HostDisposed)` cleanly after `RinUISession` / `RinUIHost` is dropped.
- **FFI Panic Safety**: All `#[qinvokable]` entry points exposed by `ThemeManager` catch unwinds at the C++ FFI boundary instead of aborting across `noexcept` boundaries.

### `ThemeManager` Reader-Writer Borrow Protection (API Safety Change)

Because `RinUIRuntime`, sibling `RinUISession`s, `RinUIHost`, and Qt QML signal slots all share access to a single process-wide `ThemeManager` QObject via `Rc<RuntimeCore>`, returning unguarded `&ThemeManager` or `Pin<&mut ThemeManager>` references previously allowed Rust code (for example, `with_theme_manager(|r| runtime.with_theme_manager_mut(...))`) or re-entrant QML slots to alias `&ThemeManager` and `&mut ThemeManager` simultaneously.

To eliminate Rust aliasing UB safely, **all** `ThemeManager` accessors on `RinUIRuntime`, `RinUISession`, and `RinUIHost` now enforce dynamic single-writer / multi-reader borrow tracking (`theme_borrow_state`) and block QML/session/engine mutations while borrowed:

- `theme_manager(&self) -> Result<ThemeManagerRef<'_>, HostError>` (`!Send + !Sync`, `Deref<Target = ThemeManager>`)
- `theme_manager_mut(&mut self) -> Result<ThemeManagerMut<'_>, HostError>` (`!Send + !Sync`, `Deref<Target = ThemeManager>`, `as_mut() -> Pin<&mut ThemeManager>`, plus `toggle_theme` / `apply_backdrop_effect` / `set_theme_color` forwarding methods)
- `with_theme_manager<R>(&self, f: impl FnOnce(&ThemeManager) -> R) -> Result<R, HostError>`
- `with_theme_manager_mut<R>(&self, f: impl FnOnce(Pin<&mut ThemeManager>) -> R) -> Result<R, HostError>`

Overlapping read+write, write+write, or write+read borrows—and QML execution (`load`, `set_language`, `set_root_property`, `invoke_root_method`, `create_session`, etc.) attempted while a `ThemeManagerRef`/`ThemeManagerMut` guard is live—return `Err(HostError::ReentrantCall(...))`.

**Migration from pre-fix signatures**:

```rust
// Before (v0.1.0-alpha.1):
let theme = host.theme_manager().get_theme();
host.theme_manager_mut().as_mut().toggle_theme(QString::from("Dark"));
host.with_theme_manager(|tm| tm.get_theme());
let rc = host.with_theme_manager_mut(|tm| tm.apply_backdrop_effect(QString::from("mica")))?;

// After (borrow-checked):
let theme = host.theme_manager()?.get_theme();
host.theme_manager_mut()?.toggle_theme(QString::from("Dark")); // or .as_mut().toggle_theme(...)
host.with_theme_manager(|tm| tm.get_theme())?;
let rc = host.with_theme_manager_mut(|tm| tm.apply_backdrop_effect(QString::from("mica")))?;
```
