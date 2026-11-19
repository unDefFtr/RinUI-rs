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

## 2. Phase 2 CI Validation

- **Branch / Commit**: `phase2-validation` at `fe5fc3383cfbf7c0fd10392ab946b1ea0007adc1`
- **Workflow Run**: [phase2-validation CI #44516338633](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633)
- **Result**: All jobs passed — `Upstream Audit & Rust Format`, `macos-latest`, `windows-latest`, `ubuntu-latest`.

### 2.1 Failure Diagnosis and Fixes

| Platform | Observed failure | Root cause | Applied fix |
|---|---|---|---|
| macOS arm64 | `qyieldcpu.h:36:5: error: implicitly declaring library function '__yield'` while compiling CXX-Qt C++ sources | [QTBUG-145239](https://qt-project.atlassian.net/browse/QTBUG-145239): Qt 6.9.x checks `__has_builtin(__yield)` before `__builtin_arm_yield` and calls `__yield()` without including `<arm_acle.h>`. Apple Clang 21 (Xcode 26) header-gates that declaration. | CI-only macOS arm64 `CXXFLAGS=-include arm_acle.h`; no Qt or upstream changes. |
| Windows x64 | `clippy::upper_case_acronyms` on `OSVERSIONINFOW`; `clippy::manual_c_str_literals` on `b"RtlGetVersion\0"` | Win32 FFI mirror used an all-caps type name and a manually NUL-terminated byte string. | Local struct renamed to `OsVersionInfoW` (same `#[repr(C)]` fields, plus compile-time `sizeof == 276` assertion); `GetProcAddress` takes `*const c_char` and receives `c"RtlGetVersion"`. |
| Linux x64 | `cargo test --workspace` ran and `standalone_packaging` failed with `failed to load translation ':/rinui/RinUI/languages/en_US.qm'` | Not a hang and not Linux-specific. Upstream `en_US.qm` is a valid header-only/empty catalog; Qt 6.9's `QCoreApplication::installTranslator()` installs an empty translator and then returns `false` (Qt 6.10+ returns `true`). `rinui-host` treated that `false` as a fatal load error. | `native/src/rinui_qt_bridge.cpp` now treats a successfully loaded but empty `QTranslator` as a valid no-op (`install_translator_or_empty`), matching Qt >= 6.10 semantics. |

Additional build-chain fixes required for the same CI run:

- **Qt Shader Tools**: Qt 6.9's `Qt5Compat.GraphicalEffects` private plugin links `QtShaderTools`; it is not installed by the `qt5compat` module alone. Added `qtshadertools` to the CI Qt module list and documented it in `README.md`.
- **Linux linker**: CXX-Qt emits a warning and does not support GNU `ld.bfd` for Cargo-linked executables (`+whole-archive` initialization). The Linux CI job now installs `lld`, so `cxx-qt-build` selects it automatically; the previous `Neither mold, lld, nor gold` warning is gone.
- **macOS packaging**: `macdeployqt` deployed optional SQL driver plugins that reference host-only absolute dylibs (Homebrew `libiodbc`, Postgres.app `libpq`, etc.). The packaging script rewrote those to `@rpath` without copying the missing files, leaving unresolved references. `scripts/package_macos.sh` now prunes optional plugins whose external dylibs cannot be bundled.
- **Cargo 1.100 note**: test binaries are placed under `target/<profile>/build/<package>/<hash>/out/` instead of `deps/`. The Linux job's long runtime and unusual test path were Cargo's new build-dir layout plus C++ compile time, not a hang.

### 2.2 CI Verification Evidence

| Job | Clippy | Tests | Release build | Packaging / isolated smoke |
|---|---|---|---|---|
| macOS arm64 | Passed with `<arm_acle.h>` workaround | Passed: `0`, `1`, `2`, `10` unit tests plus `embedded_runtime`, `filesystem_dev_mode`, `hardening` | Passed | `macOS Cocoa QPA smoke test inside isolated .app: PASSED` |
| Windows x64 | Passed (both FFI lints resolved) | Passed: same suite | Passed | `Windows QPA smoke test inside isolated windeployqt package: PASSED`; `windeployqt` deployed `Qt6ShaderTools.dll` |
| Linux x64 | Passed | Passed: same suite | Passed | `Packaged Linux staging directory`; raw binary and packaged `run-gallery.sh` smoke tests passed with both `offscreen` and Xvfb `xcb` |
| Upstream audit | — | — | — | `Audit passed: zero unexpected qmldir regressions and clean submodule state.` |

### 2.3 Local Cross-Check with Qt 6.9.3

To avoid relying only on the development machine's Homebrew Qt 6.12, Qt **6.9.3** (`qt5compat` + `qtshadertools`, aqt) was installed side-by-side on macOS arm64 and used for:

- `cargo test --workspace` (`QT_QPA_PLATFORM=offscreen`): passed.
- `cargo build --release -p rinui-gallery`: passed.
- `scripts/package_macos.sh`: passed after the optional-plugin pruning fix.
- Isolated `.app` smoke test: passed with Cocoa and with the temporary `qoffscreen` plugin fallback.
- Exact `__yield` reproduction: with Qt 6.9.3 and Clang 21, `-Werror=implicit-function-declaration` fails without `-include arm_acle.h` and compiles cleanly with it.
- Qt 6.9.3 `QTranslator` probe: `load=1`, `isEmpty=true`, `installTranslator=0`, confirming the empty-catalog diagnosis before the fix.

Homebrew Qt 6.12 also still passes `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.

### 2.4 Upstream Independence

No files under `upstream/Rin-UI` were modified. The submodule remains at the pinned commit
`46d77fa8371ff0e9f46a10a2eb66612dab123269`, and the `Verify submodule remains unmodified` step passed at the end of every CI job.

### 2.5 Conclusion

All observed failures were in the cross-platform compatibility and build/validation chain: a Qt/Clang header interaction, two Windows FFI lint issues, Qt 6.9 translation-install semantics for an empty upstream catalog, a missing Qt module in the CI toolchain, Linux linker selection, and an optional-plugin edge case in macOS packaging. No evidence was found of a defect in the `rinui-host` core architecture.
