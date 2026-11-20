# rinui-rs

Independent, reusable Rust + C++/Objective-C++ Qt 6 host library for the upstream [RinUI](https://github.com/RinLit-233-shiroko/Rin-UI) Qt Quick / QML library.

`rinui-rs` replaces RinUI's Python / PySide6 runtime (`RinUI/core/`) with a native Qt 6 / CXX-Qt host while keeping `upstream/Rin-UI` completely unmodified as a pinned Git submodule.

---

## Key Capabilities

- **Unmodified Upstream Submodule**: `upstream/Rin-UI` stays untouched (`46d77fa8371ff0e9f46a10a2eb66612dab123269`). Broken upstream `qmldir` declarations are sanitized identically at build time (`$OUT_DIR/rinui_sanitized/`) and at dev-mode launch (`ResourceView` / `GalleryDevView` staging views) via one shared sanitizer module.
- **Full `RinUI` QML Contract**: Exposes `ThemeManager` (signals `themeChanged`, `backdropChanged`; slots `toggle_theme`, `get_theme`, `get_theme_name`, `apply_backdrop_effect`, `get_backdrop_effect`, `set_theme_color`, `get_theme_color`), `WinEventManager` on Windows, and pre-first-frame `RinUINativeMacFrame` framing control on macOS.
- **Qt 6 Native Color Scheme Monitoring**: Replaces Python 1-second `darkdetect` polling with Qt 6 `QStyleHints::colorSchemeChanged` signal notifications.
- **Isolated Configuration Persistence (`rinui-platform`)**: Atomic writes, `schema_version` migration from legacy Python `rin_ui.json`, corruption backup recovery, and per-application configuration isolation via `AppIdentity`.
- **Decoupled Internationalization**: Qt locale detection and normalization (`zh-CN`, `zh_Hans_CN`, `zh`), independent per-catalog resolution (RinUI `en_US` fallback never overrides a valid application active locale or `QLocale::setDefault()`), atomic 2-phase replacement with rollback and optional catalog unloading, embedded QRC and filesystem `.qm` loading, and multi-engine `QQmlEngine::retranslate()`.
- **4-Layer Ownership, Existing Qt App Adoption & Generic Interop**:
  - `RinUIRuntime<'app>` (owning `new` or borrowing `attach_existing` / `attach_existing_raw` over an external `QGuiApplication` / `QApplication`), `RinUISession<'app>` (`EngineMode::Shared` or `EngineMode::Private`), `SessionHandle<'app>` / `HostHandle` (weak-backed handles returning `HostError::HostDisposed` after teardown), and standalone `RinUIHost`.
  - Checked root object property & method interop (`set_root_property`, `get_root_property`, `invoke_root_method`) plus scoped `configure_engine` extensibility.
- **Native Platform Integration**:
  - **macOS**: Explicit framing choice (`MacWindowOptions::self_drawn()` default vs `MacWindowOptions::upstream_compatible()` / `native()`), ARC Objective-C++ `NSWindow` transparent titlebar, `NSWindowStyleMaskFullSizeContentView`, `NSAppearance` sync, idempotent traffic-light positioning, and per-window `_rinuiMacFrameDiagnostic`. *(Verified locally under Qt `6.12.0` via `offscreen` and isolated `.app` Cocoa QPA in both framing modes; interactive desktop visual QA pending.)*
  - **Windows**: Runtime-lifetime C++ `WinEventFilter` & `WinEventManager` (`WM_NCCALCSIZE`, `WM_GETMINMAXINFO`, `WM_NCHITTEST`, Windows 11 Snap Layouts `HTMAXBUTTON`, DWM Mica/Acrylic/Tabbed, Win10 `SetWindowCompositionAttribute` including `ACCENT_DISABLED` cleanup), plus host-owned directory & zero-Python `package_msix.ps1` MSIX workflows and optional Authenticode signing. *(Prior baseline compiled/CI-tested on `Qt 6.9.3`; post-parity Windows runner CI, MSIX install/launch, and physical Win10/11 desktop QA pending.)*
  - **Linux**: X11 / Wayland Qt compositor integration with graceful `-2` fallback for Windows-only backdrop APIs. *(Prior baseline compiled/CI-tested via `offscreen` + headless `xvfb-xcb`; post-parity Linux CI, real X11 WM, and native Wayland compositor QA pending.)*
- **Dual Resource Modes**:
  - **Embedded (`ResourceMode::Embedded`)**: All RinUI QML, JS, fonts, images, and `.qm` files bundled via Qt `rcc` under `qrc:/rinui/RinUI`—release binaries run completely independently of the source checkout.
  - **Developer Filesystem (`ResourceMode::Filesystem` / `--dev` / `RINUI_DEV_PATH`)**: Fresh per-launch sanitized view owned by `ResourceView` & `GalleryDevView` (`UpstreamGallery.qml`, optional `--examples-dir` / `RINUI_EXAMPLES_DIR`) loading disk edits on restart without QRC shadowing.

---

## Version Matrix

| Component | Version / Baseline |
|---|---|
| `rinui-rs` | `0.1.0-alpha.1` |
| Upstream RinUI (`upstream/Rin-UI`) | `46d77fa8371ff0e9f46a10a2eb66612dab123269` (`beta-0.5.0-16-g46d77fa`, RinUI `0.5.0`) |
| Rust Toolchain | CI: `rustc 1.100.0`; Local macOS: `rustc 1.97.1` (MSRV `1.85`) |
| `cxx-qt` / `cxx-qt-lib` / `cxx-qt-build` / `qt-build-utils` | `0.10.0` |
| `cxx` | `1.0` |
| Qt 6 | CI: Qt `6.9.3` (macOS arm64, Windows MSVC, Ubuntu GCC+`lld`); Local macOS: `6.12.0` & `6.9.3` (`>= 6.8` in CMake) with `Qt5Compat` & `QtShaderTools` |
| CMake | `>= 3.24` (verified locally on `4.4.0`) |

### Platform Verification Status

| Platform | Implemented | Post-Parity Local Verification | CI / Physical Desktop & Signing Status |
|---|---|---|---|
| **macOS (arm64)** | Yes | Yes (`Qt 6.12.0`: `audit_upstream.py`, `cargo test --workspace`, `qt-widgets-test`, `engine_config_demo`, isolated `.app` strict Cocoa QPA in default & `--upstream-mac-frame` modes) | Prior `alpha.1` baseline CI passed on `Qt 6.9.3`; interactive post-parity visual/fullscreen QA & Developer ID signing/notarization not yet run |
| **Windows (x64 MSVC)** | Yes | N/A on macOS host (`scripts/package_windows.ps1`, `scripts/package_msix.ps1` + `packaging/windows/` implemented) | Prior `alpha.1` baseline CI passed on `Qt 6.9.3`; post-parity CI (`windows` QPA + unsigned MSIX unpack), MSIX install/launch, Authenticode signing, and Win10/11 desktop QA pending Windows runner/desktop |
| **Linux (x64 GCC + `lld`)** | Yes | N/A on macOS host (`scripts/package_linux.sh`) | Prior `alpha.1` baseline CI passed headless `xvfb-xcb`; post-parity Linux CI, real X11 WM, and native Wayland compositor QA pending Linux runner/desktop |

*(Source-resource coverage of RinUI's runtime tree and automated `--smoke-test` checks establish resource loading, startup, frame mode selection, NavigationView/ErrorPage/ScrollView behavior, theme toggle, multi-window, and i18n, rather than replacing full manual GUI QA of every upstream control.)*

---

## Quick Start

### 1. Clone with Submodules

```bash
git clone --recurse-submodules <repo-url>
cd rinui-rs
```

If already cloned without submodules:

```bash
git submodule update --init --recursive
```

### 2. Prerequisites

- **macOS**: Xcode Command Line Tools, Rust (`rustup`), Qt 6 with `qt5compat` and `qtshadertools` (e.g. `brew install qt` — ensure `qmake6` is in `PATH` or set `QMAKE=/path/to/qmake6`). On Apple Silicon with Xcode 26 (Apple Clang 21) and Qt < 6.11.1, export `CXXFLAGS="-include arm_acle.h"` to work around [QTBUG-145239](https://qt-project.atlassian.net/browse/QTBUG-145239) in Qt's `qyieldcpu.h`.
- **Windows**: MSVC Build Tools 2022, Rust (`msvc` toolchain), Qt 6 (`6.9+` with `Qt 5 Compatibility Module` and `Qt Shader Tools`), `qmake` in `PATH` or `QMAKE` set.
- **Linux**: GCC/Clang, Mesa/X11/Wayland dev packages, Rust, Qt 6 with `qt5compat` + `qtshadertools`, `qmake6`/`qmake` in `PATH`, and `lld` (or `gold`/`mold`) in `PATH`. CXX-Qt links generated Qt resource/QML initializers with `+whole-archive`; GNU `ld.bfd` does not link Cargo binaries correctly, so `cxx-qt-build` selects `lld` automatically when available.

### 3. Build, Test, and Run the Gallery

```bash
# Audit upstream submodule cleanliness and qmldir contract
python3 scripts/audit_upstream.py

# Run all Rust unit + Qt integration + standalone packaging tests
cargo test --workspace

# Run the Rust-hosted RinUI Gallery (embedded QRC resources)
cargo run -p rinui-gallery

# Run in developer filesystem mode (loads RinUI directly from upstream/Rin-UI)
cargo run -p rinui-gallery -- --dev
# Or via environment variable:
RINUI_DEV_PATH="$PWD/upstream/Rin-UI" cargo run -p rinui-gallery
```

Release build & packaging:

```bash
cargo build --release -p rinui-gallery

# macOS (ad-hoc by default; pass --sign-identity / --notarize / --notary-profile with credentials):
./scripts/package_macos.sh target/release/rinui-gallery dist
# Windows directory & MSIX (PowerShell; pass -Sign / -CertPfx / -CertThumbprint when signing):
.\scripts\package_windows.ps1 -BinaryPath target\release\rinui-gallery.exe -OutputDir dist\RinUIGallery-win64
.\scripts\package_msix.ps1 -SourceDirectory dist\RinUIGallery-win64 -OutputDirectory dist\msix -SkipSign -VerifyLayout
# Linux:
./scripts/package_linux.sh target/release/rinui-gallery dist/rinui-gallery-linux
```

CMake wrapper targets (`rinui_audit`, `rinui_host_rust`, `rinui_gallery_rust`, `rinui_test`, `rinui_package`):

```bash
cmake -B build
cmake --build build --target rinui_gallery_rust
```

---

## Gallery CLI Options & Screenshots

`rinui-gallery` accepts the following CLI options:
- `--dev [PATH]` — load RinUI + examples via a fresh per-launch sanitized filesystem view (`RINUI_DEV_PATH` or `upstream/Rin-UI` fallback in debug builds) instead of embedded QRC resources
- `--examples-dir <DIR>` — explicit upstream `examples/` directory when `--dev` targets a RinUI-only library checkout (also `RINUI_EXAMPLES_DIR`)
- `--smoke-test` — verify host contracts, NavigationView sample push / `ErrorPage` recovery / `ScrollView`, macOS frame diagnostics, multi-window, theme toggle, decoupled i18n switching/fallback, and config persistence, then exit
- `--auto-close-ms <MS>` — exit cleanly after `<MS>` milliseconds
- `--locale <LOCALE>` — start with an explicit locale (e.g., `en_US`, `zh_CN`)
- `--upstream-mac-frame` — select `MacWindowOptions::upstream_compatible()` (native `NSWindow` titlebar on macOS; normal frame elsewhere)
- `--native-mac-frame` — opt into native macOS `NSWindow` titlebar integration
- `--no-native-mac-frame` — explicitly use the default QML titlebar and traffic-light controls
- `--config-dir <DIR>` — override configuration base directory

On macOS, both the Gallery and `MacWindowOptions::default()` use RinUI's self-drawn QML titlebar and traffic-light controls by conservative host policy (whereas upstream RinUI QML defaults to native framing on macOS when PyObjC is present). Opt into upstream macOS framing explicitly with `--upstream-mac-frame` or `MacWindowOptions::upstream_compatible()`.

> **Gallery Screenshots**: Automated `offscreen`, Xvfb `xcb`, Windows QPA, and Cocoa QPA startup checks pass, while interactive desktop visual QA screenshots have not yet been captured for `v0.1.0-alpha.1`.

---

## Minimal Consumer Integration

Use a drop-safe `ExitCode` pattern rather than calling `std::process::exit` directly before `RinUIHost` drops so configuration saving and ordered Qt teardown execute cleanly:

```rust
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
};
use std::process::ExitCode;

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut host = RinUIHost::new(HostConfig {
        identity: AppIdentity::new("Acme", "Goosy"),
        resource_mode: ResourceMode::Auto,
        config_location: ConfigLocation::Standard,
        locale: None,
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    })?;

    let loaded = host.load(&QmlSource::Url("qrc:/app/Main.qml".to_owned()))?;
    host.set_root_string_property(loaded.root_id, "title", "Goosy");
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

---

## Known Limitations (`v0.1.0-alpha.1`)

- Interactive desktop visual QA (macOS Cocoa traffic-light geometry/fullscreen restore on latest changes, Windows 10 Acrylic & Windows 11 DWM Mica/Acrylic/Snap Layouts/multi-monitor DPI, real Linux X11 WM and native Wayland compositors) and post-parity Windows/Linux CI runs remain open until run on physical desktops / CI runners.
- Credentialed signing and distribution checks (Apple Developer ID signing/notarization, Windows Authenticode signing, and signed MSIX install/launch/uninstall) have not been executed without signing certificates; scripts default to ad-hoc (macOS) or `-SkipSign` unsigned MSIX (Windows).
- Building on Apple Silicon with Xcode 26 (Clang 21) and Qt `< 6.11.1` requires `CXXFLAGS="-include arm_acle.h"` ([QTBUG-145239](https://qt-project.atlassian.net/browse/QTBUG-145239)).

---

See the detailed guides in [`docs/`](docs/) and release documentation:
- [`CHANGELOG.md`](CHANGELOG.md) — Release changelog (`0.1.0-alpha.1`)
- [`docs/compatibility.md`](docs/compatibility.md) — Upstream Python vs Rust compatibility audit & contract matrix
- [`docs/architecture.md`](docs/architecture.md) — Ownership, QRC anti-shadowing, and native window architecture
- [`docs/consumer-guide.md`](docs/consumer-guide.md) — Integrating `rinui-host`, custom Rust `QObject`s, i18n, and `WindowCustomizer`
- [`docs/packaging-and-licensing.md`](docs/packaging-and-licensing.md) — Desktop deployment & Qt/MIT licensing obligations
- [`docs/updating-upstream.md`](docs/updating-upstream.md) — Updating the pinned `upstream/Rin-UI` commit safely
- [`docs/validation-report.md`](docs/validation-report.md) — Phase 2 cross-platform CI & local validation evidence
- [`docs/release-checklist.md`](docs/release-checklist.md) — Pre-release checklist for `v0.1.0-alpha.1`
- [`docs/release-notes-v0.1.0-alpha.1.md`](docs/release-notes-v0.1.0-alpha.1.md) — Proposed GitHub release notes (`v0.1.0-alpha.1`)

---

## License

- `rinui-rs` host code: [MIT License](LICENSE)
- Upstream RinUI (`upstream/Rin-UI`): [MIT License](upstream/Rin-UI/LICENSE) (`Copyright (c) 2025 RinLit`)
- Additional notices (Qt 6, CXX-Qt, Fluent System Icons): [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
