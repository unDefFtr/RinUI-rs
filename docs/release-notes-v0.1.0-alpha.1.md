# `rinui-rs` `v0.1.0-alpha.1` Release Notes (Draft)

> **Pre-release (`alpha.1`)** — Pinned upstream RinUI revision: [`46d77fa8371ff0e9f46a10a2eb66612dab123269`](https://github.com/RinLit-233-shiroko/Rin-UI/commit/46d77fa8371ff0e9f46a10a2eb66612dab123269) (`RinUI 0.5.0` / `beta-0.5.0-16-g46d77fa`).

`rinui-rs` is an independent, reusable Rust + C++/Objective-C++ Qt 6 host library that replaces upstream RinUI's Python / PySide6 runtime (`RinUI/core/`) while keeping `upstream/Rin-UI` completely unmodified as a Git submodule.

---

## Highlights in `v0.1.0-alpha.1`

- **Reusable Workspace Crates**:
  - `rinui-platform`: OS/Windows build capability checks, `ThemeMode` / `BackdropEffect` resolution, `AppIdentity` directory isolation, atomic JSON persistence (`ConfigStore`, `AppJsonStore`), legacy Python `rin_ui.json` schema migration, and corrupt-config backup recovery.
  - `rinui-host`: Main-thread-affine (`!Send + !Sync`) `RinUIHost` and `HostHandle`, full CXX-Qt `ThemeManager` QML contract, Qt 6 `QStyleHints::colorSchemeChanged` live system theme detection, `TranslationManager` (`en_US` fallback + `QQmlEngine::retranslate()`), `WindowCustomizer`, and `ResourceMode::{Embedded, Filesystem, Auto}`.
  - `rinui-gallery`: Reference RinUI Gallery combining upstream `examples/` QML with native Rust host demonstrations (`RustHostDemoPage.qml`, `MultiWindowDialog.qml`).
- **Unmodified Upstream Submodule**: Build-time `qmldir` sanitization runs inside Cargo's `$OUT_DIR/rinui_sanitized/`, and QRC resources mount under `/rinui/RinUI` (`qrc:/rinui`) to prevent `qrc:/qt/qml` shadowing.
- **Phase 2 Hardening**:
  - Process-wide `RinUIHost` single-active-instance lock (`HostError::ApplicationAlreadyInitialized`), re-entrancy guards (`HostError::ReentrantCall`), partial-init cleanup, and deterministic Qt destruction ordering.
  - `std::panic::catch_unwind` FFI panic boundaries and `try_borrow` guards across CXX-Qt slots in `ThemeManager` and `GalleryBackend`.
  - Qt 6.9 empty-catalog translation compatibility (`en_US.qm` header-only catalog), Windows `\\?\` verbatim path stripping, Windows Per-Monitor-V2 DPI / `WM_NCHITTEST` / `WM_NCCALCSIZE` / `WM_GETMINMAXINFO` / `OsVersionInfoW` fixes, and self-contained macOS `.app` relocation/verification.

---

## Supported Platforms & Verification Status

| Platform | Target / Toolchain Tested | Implemented | Compiled | Automated Tests (`offscreen`) | Packaged Isolated Smoke Test | Visually Verified (Interactive Desktop) |
|---|---|---|---|---|---|---|
| **macOS (arm64)** | Apple Clang + Qt `6.9.3` (CI) & `6.12.0` / `6.9.3` (local) | Yes | Yes | Yes (`cargo test --workspace`) | Yes (`RinUIGallery.app`, Cocoa QPA) | Pending interactive QA |
| **Windows (x64)** | MSVC 2022 + Qt `6.9.3` (`win64_msvc2022_64`, CI) | Yes | Yes | Yes (`cargo test --workspace`) | Yes (`windeployqt`, `windows` QPA) | Pending interactive QA |
| **Linux (x64)** | GCC + `lld` + Qt `6.9.3` (`linux_gcc_64`, CI) | Yes | Yes | Yes (`cargo test --workspace`) | Yes (`package_linux.sh`, `offscreen` + Xvfb `xcb`) | Pending real X11 WM & Wayland QA |

---

## Quick Consumer Integration (`Cargo.toml`)

```toml
[dependencies]
rinui-host = { git = "https://github.com/unDefFtr/rinui-rs", tag = "v0.1.0-alpha.1" }
```

Use a drop-safe `ExitCode` pattern (avoid `std::process::exit` directly before `RinUIHost` drops) so configuration saving and Qt teardown run cleanly:

```rust
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
};
use std::process::ExitCode;

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut host = RinUIHost::new(HostConfig {
        identity: AppIdentity::new("Acme", "MyApp"),
        resource_mode: ResourceMode::Auto,
        config_location: ConfigLocation::Standard,
        locale: None,
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    })?;

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

---

## Build Requirements & Reproducibility

- **Qt 6**: Install **`qt5compat`** and **`qtshadertools`** (`Qt 6.9.3` tested in CI across all OS targets; `Qt >= 6.8` in CMake).
- **Linux Linker**: Install **`lld`** (or `gold` / `mold`); CXX-Qt requires a non-`ld.bfd` linker for `+whole-archive` QRC/QML initializers.
- **macOS Apple Silicon + Xcode 26 (Clang 21) + Qt < 6.11.1**: Export `CXXFLAGS="-include arm_acle.h"` before building ([QTBUG-145239](https://qt-project.atlassian.net/browse/QTBUG-145239); resolved in Qt 6.11.1/6.12).
- **Submodule Checkout**: `git clone --recurse-submodules https://github.com/unDefFtr/rinui-rs`

---

## Known Issues & Limitations

1. **Interactive Desktop Visual QA Pending**: Native DWM Mica/Acrylic/Snap Layouts on physical Windows 10/11 monitors, macOS titlebar geometry/traffic lights on desktop displays, and real Linux X11 WM / native Wayland compositor sessions have not yet undergone manual visual inspection (automated validation covers CI compilation, `offscreen` tests, Linux `xvfb` `xcb`, and isolated packaged QPA smoke tests).
2. **macOS Signing / Notarization**: `scripts/package_macos.sh` performs local ad-hoc Mach-O signing only (`codesign --force --deep --sign -`); Gatekeeper notarization and Developer ID signing are not configured for `v0.1.0-alpha.1`.
3. **Pre-release API Stability**: `0.1.0-alpha.1` is an early release; minor ergonomic adjustments may occur before `0.1.0`.

---

## Licensing & Qt Redistribution

- `rinui-rs` is licensed under the [MIT License](../LICENSE).
- Upstream RinUI (`upstream/Rin-UI`) is licensed under the [MIT License](../upstream/Rin-UI/LICENSE) (`Copyright (c) 2025 RinLit`).
- See [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) and [`docs/packaging-and-licensing.md`](packaging-and-licensing.md) for Qt 6 **LGPLv3 / GPLv3 / Commercial** obligations (compiling QML into Qt `rcc` does not remove Qt framework redistribution requirements).
