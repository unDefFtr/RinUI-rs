# rinui-rs

Independent, reusable Rust + C++/Objective-C++ Qt 6 host library for the upstream [RinUI](https://github.com/RinLit-233-shiroko/Rin-UI) Qt Quick / QML library.

`rinui-rs` replaces RinUI's Python / PySide6 runtime (`RinUI/core/`) with a native Qt 6 / CXX-Qt host while keeping `upstream/Rin-UI` completely unmodified as a pinned Git submodule.

---

## Key Capabilities

- **Unmodified Upstream Submodule**: `upstream/Rin-UI` stays untouched (`46d77fa8371ff0e9f46a10a2eb66612dab123269`). Broken upstream `qmldir` declarations are sanitized at build time inside `$OUT_DIR/rinui_sanitized/`.
- **Full `RinUI` QML Contract**: Exposes `ThemeManager` (signals `themeChanged`, `backdropChanged`; slots `toggle_theme`, `get_theme`, `get_theme_name`, `apply_backdrop_effect`, `get_backdrop_effect`, `set_theme_color`, `get_theme_color`) and `WinEventManager` on Windows.
- **Qt 6 Native Color Scheme Monitoring**: Replaces Python 1-second `darkdetect` polling with Qt 6 `QStyleHints::colorSchemeChanged` signal notifications.
- **Isolated Configuration Persistence (`rinui-platform`)**: Atomic writes, `schema_version` migration from legacy Python `rin_ui.json`, corruption backup recovery, and per-application configuration isolation via `AppIdentity`.
- **Internationalization**: Qt locale detection, embedded QRC and filesystem `.qm` loading, `en_US` fallback, application translation catalogs, and live `QQmlEngine::retranslate()`.
- **Multi-Window & Thread-Safe Ownership**: `!Send + !Sync` `RinUIHost` and `HostHandle`, `QPointer`-backed window tracking, deterministic Qt teardown ordering, and `WindowCustomizer` hook.
- **Native Platform Integration**:
  - **macOS**: ARC Objective-C++ `NSWindow` transparent titlebar, `NSWindowStyleMaskFullSizeContentView`, `NSAppearance` sync, and idempotent traffic-light positioning. *(Compiled & CI-tested via `offscreen` + isolated Cocoa `.app`; interactive desktop visual QA pending.)*
  - **Windows**: C++ `WinEventFilter` & `WinEventManager` (`WM_NCCALCSIZE`, `WM_GETMINMAXINFO`, `WM_NCHITTEST`, Windows 11 Snap Layouts `HTMAXBUTTON`, DWM Mica/Acrylic/Tabbed & Win10 `SetWindowCompositionAttribute`). *(Compiled & CI-tested via `offscreen` + isolated `windeployqt` `windows` QPA; interactive desktop visual QA pending.)*
  - **Linux**: X11 / Wayland Qt compositor integration with graceful `-2` fallback for Windows-only backdrop APIs. *(Compiled & CI-tested via `offscreen` + `xvfb-run` `xcb`; real X11 WM & Wayland visual QA pending.)*
- **Dual Resource Modes**:
  - **Embedded (`ResourceMode::Embedded`)**: All RinUI QML, JS, fonts, images, and `.qm` files bundled via Qt `rcc` under `qrc:/rinui/RinUI`—release binaries run completely independently of the source checkout.
  - **Developer Filesystem (`ResourceMode::Filesystem` / `--dev` / `RINUI_DEV_PATH`)**: Loads RinUI directly from disk without QRC shadowing.

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

| Platform | Implemented | Compiled | Automated Tests & Isolated Packaged Smoke | Visually Verified (Interactive Desktop) |
|---|---|---|---|---|
| **macOS (arm64)** | Yes | Yes | Yes (`cargo test --workspace`, isolated `.app` Cocoa QPA on Qt `6.9.3` & `6.12.0`) | Pending desktop screenshot/visual QA |
| **Windows (x64 MSVC)** | Yes | Yes | Yes (CI `cargo test --workspace`, isolated `windeployqt` `windows` QPA on Qt `6.9.3`) | Pending physical Windows 10/11 desktop QA |
| **Linux (x64 GCC + `lld`)** | Yes | Yes | Yes (CI `cargo test --workspace`, isolated staging `offscreen` + Xvfb `xcb` on Qt `6.9.3`) | Pending real X11 WM & Wayland QA |

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

# macOS:
./scripts/package_macos.sh target/release/rinui-gallery dist
# Windows (PowerShell):
.\scripts\package_windows.ps1 -BinaryPath target\release\rinui-gallery.exe -OutputDir dist\RinUIGallery-win64
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
- `--dev [PATH]` — load RinUI directly from disk (`RINUI_DEV_PATH` or `upstream/Rin-UI` fallback in debug builds) instead of embedded QRC resources
- `--smoke-test` — verify host contracts, multi-window, theme toggle, i18n switching/fallback, and config persistence, then exit
- `--auto-close-ms <MS>` — exit cleanly after `<MS>` milliseconds
- `--locale <LOCALE>` — start with an explicit locale (e.g., `en_US`, `zh_CN`)
- `--no-native-mac-frame` — disable native macOS `NSWindow` titlebar integration
- `--config-dir <DIR>` — override configuration base directory

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

- Interactive desktop visual QA (macOS Cocoa traffic-light geometry, Windows 11 DWM Mica/Acrylic/Snap Layouts/multi-monitor DPI, real Linux X11 WM and native Wayland compositors) is still pending manual visual inspection.
- `scripts/package_macos.sh` applies local ad-hoc Mach-O signatures only (not Developer ID signed or Gatekeeper notarized).
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
