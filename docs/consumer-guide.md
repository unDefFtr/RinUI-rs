# Consumer Integration Guide

This guide explains how external Rust desktop applications (such as **Goosy**) embed and interact with `rinui-rs`.

---

## 1. Adding `rinui-host` to Your Cargo Project

Add `rinui-host` (and `cxx` / `cxx-qt` / `cxx-qt-lib` if defining your own `QObject` types) to your application's `Cargo.toml`:

```toml
[dependencies]
rinui-host = { path = "../rinui-rs/crates/rinui-host" }
cxx = "1.0"
cxx-qt = "0.10.0"
cxx-qt-lib = { version = "0.10.0", features = ["qt_full"] }

[build-dependencies]
cxx-qt-build = "0.10.0"
```

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

```rust
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    std::process::exit(host.exec());
}
```

### Configuration Directory Isolation

`AppIdentity::new("Acme", "Goosy")` isolates configuration under:
- **macOS**: `~/Library/Application Support/Acme/Goosy/`
- **Windows**: `%LOCALAPPDATA%\Acme\Goosy\`
- **Linux**: `$XDG_DATA_HOME/Acme/Goosy/` (or `~/.local/share/Acme/Goosy/`)

RinUI writes `RinUI/config/rin_ui.json` inside that app-scoped base directory. For your application's own JSON config, use `rinui_host::AppJsonStore` (writes `<base>/config/<filename>` atomically with corruption backup recovery).

---

## 3. Exposing Custom Rust `QObject` Models/Controllers to QML

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

## 4. Application Translations & Live Switching

Register one or more `.qm` catalogs with `RinUIHost`:

```rust
use rinui_host::TranslationCatalog;

host.register_translation_catalog(TranslationCatalog::QrcPrefix {
    id: "goosy".to_owned(),
    prefix: ":/goosy/languages".to_owned(),
})?;

// Switch language at runtime and trigger QQmlEngine::retranslate():
host.set_language("zh_CN")?;
```

---

## 5. Customizing Native Windows (`WindowCustomizer`)

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

If your application opens extra `FluentWindow` / `Window` objects dynamically from QML (`Qt.createComponent`), call `host.handle().sync_windows()` to register and apply native platform hooks to those new top-level windows.
