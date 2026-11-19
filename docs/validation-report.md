# `rinui-rs` Phase 2 Validation Report

## 1. Initial Phase 2 Baseline (Before Modifications)

- **Repository**: `https://github.com/unDefFtr/rinui-rs`
- **Branch / Commit at Start**: `main` at `dd1fd96` (`chore: track upstream RinUI`), synchronized with `origin/main`
- **Working Tree State at Start**: All Phase 1 source files (`.github/`, `.gitignore`, `CMakeLists.txt`, `Cargo.lock`, `Cargo.toml`, `README.md`, `README.zh-CN.md`, `THIRD_PARTY_NOTICES.md`, `cmake/`, `crates/`, `docs/`, `examples/`, `native/`, `qml/`, `scripts/`) were present as untracked files on `main`. An unrelated untracked directory `mtktools/` was present and kept completely untouched.
- **Pinned Upstream Submodule (`upstream/Rin-UI`)**:
  - Expected SHA: `46d77fa8371ff0e9f46a10a2eb66612dab123269`
  - Observed SHA: `46d77fa8371ff0e9f46a10a2eb66612dab123269` (`beta-0.5.0-16-g46d77fa`) — **EXACT MATCH**
  - Cleanliness (`git -C upstream/Rin-UI status --porcelain`): **Clean** (empty output)
- **GitHub Actions History at Start**: `gh run list --limit 10` returned `no runs found` (authenticated as `unDefFtr` with `repo` and `workflow` scopes).

### 1.1 Baseline Local macOS Toolchain & Checks

- **OS / Toolchain**: macOS (`rustc 1.97.1`, `cargo 1.97.1`, `clang++`, `qmake6` / Qt `6.12.0` in `/opt/homebrew/lib`, `macdeployqt` at `/opt/homebrew/bin/macdeployqt`)
- **Upstream Audit (`python3 scripts/audit_upstream.py`)**: **PASSED** (`115` RinUI runtime files inventoried, `23` host context references, zero unexpected `qmldir` issues, clean submodule)
- **Formatting (`cargo fmt --all -- --check`)**: **PASSED**
- **Lint (`cargo clippy --workspace --all-targets -- -D warnings`)**: **PASSED**
- **Tests (`cargo test --workspace`)**: **PASSED**
  - `rinui-platform`: `7 passed; 0 failed` (capabilities, isolation, config persistence, migration, corruption backup recovery, in-memory, app store)
  - `rinui-host` unit tests: `2 passed; 0 failed` (packaged manifest + resource mode resolution)
  - `rinui-host` integration `embedded_runtime` (`QT_QPA_PLATFORM=offscreen`): **PASSED**
  - `rinui-host` integration `filesystem_dev_mode` (`QT_QPA_PLATFORM=offscreen`): **PASSED**
  - `rinui-gallery` `standalone_packaging`: `1 passed; 0 failed` (`QT_QPA_PLATFORM=offscreen` executable copy smoke test)
- **macOS `.app` Bundle Packaging (`scripts/package_macos.sh`)**: Not yet executed at baseline.
- **Windows / Linux CI Execution**: Not yet executed at baseline (**implemented only; uncompiled/untested at baseline**).

### 1.2 Baseline Compatibility & Code Audit Findings

1. **Windows Clippy & Verbatim Path Risk**:
   - `crates/rinui-platform/src/platform.rs`: `std::mem::transmute(proc)` in `#[cfg(target_os = "windows")] WindowsVersion::detect()` lacked explicit type annotations (`clippy::missing_transmute_annotations`).
   - `fs::canonicalize()` on Windows emits `\\?\` verbatim paths that can break Qt `rcc`, `QUrl::fromLocalFile`, and `QQmlApplicationEngine::addImportPath`.
2. **Windows Native Adapter (`native/src/rinui_win_bridge.cpp`)**:
   - `trigger_theme_manager_refresh()` used `QMetaObject::invokeMethod(..., Q_RETURN_ARG(int32_t, ...))`, which fails to match the moc-registered return type `::std::int32_t` on `rinui::ThemeManager::apply_backdrop_effect`.
   - `s_event_manager` was stored as a raw pointer rather than `QPointer<WinEventManager>`.
   - `hit_test` used window coordinates instead of client coordinates for the titlebar band, did not bypass resize border hit-testing when maximized, and did not scale resize borders or use `GetDpiForWindow`/`GetSystemMetricsForDpi`.
   - `apply_nccalcsize` trimmed 1px at the bottom even when auto-hide taskbar edge query returned `-1` (monitor without taskbar).
   - `apply_getminmaxinfo` used `rcWork.right - rcMonitor.left` instead of `rcWork.right - rcWork.left` and multiplied unscaled `work_w`/`work_h` by `dp_ratio` when `maximumWidth`/`maximumHeight` was unset.
   - `WM_NCHITTEST` did not clear `titleBarMaxButtonHovered` when returning `false`, and `WM_CAPTURECHANGED` was not handled alongside `WM_CANCELMODE`.
3. **Host Ownership, Re-Init, & FFI Panic Boundaries (`crates/rinui-host`, `native/src/rinui_qt_bridge.cpp`)**:
   - `RinUIHost::new()` lacked a process-wide single-active-host guard (`QGuiApplication` singleton protection) and partial-initialization cleanup on error.
   - `RinUIHost::Drop` cleared `QQmlApplicationEngine` component cache before flushing `QEvent::DeferredDelete`.
   - CXX-Qt `#[qinvokable]` methods in `ThemeManager` and `GalleryBackend` could theoretically abort across `noexcept` C++ boundaries if a panic occurred (e.g., `RefCell` contention).
   - `rinui_root_release` could invoke `teardown_window` / `releaseResources()` twice when a root object itself was a tracked `QQuickWindow`.
   - `rinui_translator_load_rinui` tore down the existing translator before verifying the replacement `.qm` loaded successfully.
   - `rinui_engine_register_latest_root` did not clear `s_roots` entries if QML destroyed a root externally.
4. **Packaging & Release Alignment**:
   - Workspace crate versions were `0.1.0` instead of `0.1.0-alpha.1`.
   - `scripts/package_macos.sh` only warned when `macdeployqt` was missing, did not verify `otool -L` or required QML plugins (`Qt5Compat/GraphicalEffects`), and did not re-sign ad-hoc after `install_name_tool` changes.
   - `scripts/package_windows.ps1` used PowerShell 7-only `??` syntax.
   - `examples/gallery/src/main.rs` unconditionally baked `env!("CARGO_MANIFEST_DIR")` into release binaries for `--dev` fallback.

---

*Subsequent sections below will be populated after Phase 2 hardening, macOS `.app` bundle validation, and GitHub Actions Windows/Linux/macOS CI runs complete.*
