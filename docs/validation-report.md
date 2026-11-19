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

- **Active Branch**: `phase2-validation`
- **Green CI Baseline**: [`fe5fc3383cfbf7c0fd10392ab946b1ea0007adc1`](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633) ([run #44516338633](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633)) and [`60ca934ae72638990b92a2551f1f54d0d87b6110`](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691) ([run #44518777691](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691))
- **Result (`60ca934`)**: All 4 jobs passed — `Upstream Audit & Rust Format`, `macos-latest (Qt 6.9.3)`, `windows-latest (Qt 6.9.3)`, and `ubuntu-latest (Qt 6.9.3)`.

### 2.0 Chronological CI Run History

| Run ID & URL | Commit SHA | Audit & Format | macOS (`Qt 6.9.3`) | Windows (`Qt 6.9.3`) | Linux (`Qt 6.9.3`) | Summary |
|---|---|---|---|---|---|---|
| [#44501554498](https://github.com/unDefFtr/rinui-rs/actions/runs/44501554498) | `9761424a9ec2d0a146aab7de0b663d82f7674b9d` | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44501554498/job/135482371101) | [fail](https://github.com/unDefFtr/rinui-rs/actions/runs/44501554498/job/135482546390) | [fail](https://github.com/unDefFtr/rinui-rs/actions/runs/44501554498/job/135482546646) | [fail](https://github.com/unDefFtr/rinui-rs/actions/runs/44501554498/job/135482546559) | Workflow read unset `${Qt6_DIR}` instead of `${QT_ROOT_DIR}` exported by `install-qt-action@v4`. |
| [#44502079534](https://github.com/unDefFtr/rinui-rs/actions/runs/44502079534) | `42af6fa0fa31660e1e19dc8cdc7ba8e3533a7726` | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44502079534/job/135484152256) | [fail](https://github.com/unDefFtr/rinui-rs/actions/runs/44502079534/job/135484409315) | [fail](https://github.com/unDefFtr/rinui-rs/actions/runs/44502079534/job/135484409538) | [fail](https://github.com/unDefFtr/rinui-rs/actions/runs/44502079534/job/135484409562) | Exposed Qt 6.9.3 arm64 `__yield` header bug, Windows FFI clippy lints, and Qt 6.9 empty `.qm` `installTranslator()` return behavior. |
| [#44512401748](https://github.com/unDefFtr/rinui-rs/actions/runs/44512401748) | `515172dd8c80b8e9ff1c4d4a51236d8db3e7bcbd` | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44512401748/job/135519688683) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44512401748/job/135519842018) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44512401748/job/135519842010) | [fail](https://github.com/unDefFtr/rinui-rs/actions/runs/44512401748/job/135519841963) | macOS and Windows passed completely; Linux failed in the linker diagnostics step (`ld --version \| head -n 1` / generic `lld --version` under `pipefail`; switched to `ld --version` and `ld.lld --version` in `fe5fc33`). |
| [#44516338633](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633) | `fe5fc3383cfbf7c0fd10392ab946b1ea0007adc1` | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633/job/135533183460) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633/job/135533312024) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633/job/135533311739) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44516338633/job/135533311947) | All 4 jobs passed across code, tests, release builds, and isolated packaged smoke tests. |
| [#44518777691](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691) | `60ca934ae72638990b92a2551f1f54d0d87b6110` | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691/job/135541655562) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691/job/135541801546) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691/job/135541801556) | [pass](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691/job/135541801806) | All 4 jobs passed (`rustc 1.100.0`, `Qt 6.9.3`) with uploaded `rinui-gallery-{macOS,Windows,Linux}` artifacts. |

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

---

## 3. macOS `.app` Packaging & Runtime Validation (Commit `60ca934`)

Re-validated locally on macOS arm64 after `515172d`/`60ca934` (`Qt 6.12.0` Homebrew, `rustc 1.97.1`), supplementing the CI (`Qt 6.9.3`) and local side-by-side `Qt 6.9.3` runs:

- **Build & Script Execution**: `cargo build --release -p rinui-gallery` followed by `./scripts/package_macos.sh target/release/rinui-gallery dist` exited `0`.
- **Bundle Layout & Metadata**:
  - Verified `RinUIGallery.app/Contents/` contains `Info.plist` (`plutil -lint`: `OK`, `CFBundleShortVersionString = 0.1.0-alpha.1`), `MacOS/RinUIGallery`, `Frameworks/` (`QtCore.framework`, `QtGui.framework`, `QtQml.framework`, `QtQuick.framework`, `QtQuickControls2.framework`, `QtOpenGL.framework`, `QtNetwork.framework`, etc.), `PlugIns/platforms/libqcocoa.dylib`, and `Resources/` (`qt.conf`, `gallery.png`, `qml/Qt5Compat/GraphicalEffects`, `LICENSE`, `LICENSE.RinUI`, `THIRD_PARTY_NOTICES.md`).
  - `qt.conf` maps `Plugins = PlugIns`, `Imports = Resources/qml`, `Qml2Imports = Resources/qml`, `QmlImports = Resources/qml`.
- **Binary Dependency & Path Hygiene**:
  - `otool -L dist/RinUIGallery.app/Contents/MacOS/RinUIGallery` resolves all Qt frameworks exclusively via `@executable_path/../Frameworks/...` plus Apple `/System/Library/Frameworks/*` and `/usr/lib/*`.
  - `otool -l` shows a single `LC_RPATH` entry: `@executable_path/../Frameworks`.
  - `package_macos.sh` post-packaging verification checked every Mach-O binary in `Contents/` and confirmed zero `/opt/homebrew/`, `/usr/local/`, `/Users/`, `/Cellar/`, or unresolved `@rpath/` references remain; unbundled optional plugins (`QtVirtualKeyboard`, `Qt3D`, `QtPdf`, `QtQuickTimeline`, `QtStateMachine`) were pruned cleanly.
  - `strings` inspection of `RinUIGallery` confirmed zero development checkout source/upstream paths (`rinui-rs/upstream`, `rinui-rs/crates`, `rinui-rs/examples`) are baked into the release binary.
- **Isolated Runtime Execution**:
  - Copied `dist/RinUIGallery.app` to a fresh temporary directory outside the source checkout with `RINUI_DEV_PATH`, `RINUI_UPSTREAM_DIR`, `QT_PLUGIN_PATH`, `QML_IMPORT_PATH`, `QML2_IMPORT_PATH`, `DYLD_LIBRARY_PATH`, and `DYLD_FRAMEWORK_PATH` unset.
  - `QT_QPA_PLATFORM=cocoa ./RinUIGallery.app/Contents/MacOS/RinUIGallery --smoke-test --config-dir "$TMP/cfg-smoke"` exited `0` (`platform=cocoa`, `resource_mode=embedded (qrc:/rinui)`, zero Objective-C duplicate class warnings).
  - `QT_QPA_PLATFORM=cocoa ./RinUIGallery.app/Contents/MacOS/RinUIGallery --auto-close-ms 1500 --config-dir "$TMP/cfg-run"` exited `0`, creating isolated `RinUI/config/rin_ui.json` and `config/config.json`.
- **Signing Boundary**: `scripts/package_macos.sh` applies a **local ad-hoc Mach-O signature (`codesign --force --deep --sign -`) only**; binaries are **not** Apple Developer ID signed or Gatekeeper notarized.

---

## 4. Public API, Hardening & Regression Test Summary

### 4.1 Hardening Applied (`crates/rinui-host`, `crates/rinui-platform`, `native/`, `examples/gallery`)

- **Ownership & Destruction Ordering**: Enforced deterministic `RinUIHost::drop` sequence (save config -> invalidate `HostHandle` -> drop customizer -> `rinui_release_all_roots_and_platform()` -> `rinui_flush_deferred_deletes()` -> `rinui_translator_clear_all()` -> clear QML component cache & GC -> drop `QQmlApplicationEngine` -> drop `owned_context_objects` -> drop `ThemeManager` -> drop `QGuiApplication` -> release `HOST_ACTIVE` via `_active_guard`).
- **Process Singleton, Thread Affinity & Re-Entrancy**: `!Send + !Sync` `RinUIHost`/`HostHandle`, `is_qt_main_thread()`, atomic `HOST_ACTIVE` guard (`HostError::ApplicationAlreadyInitialized`), `InitCleanupGuard` on partial init failure, `LoadingGuard` + non-panicking `try_borrow`/`try_borrow_mut` (`HostError::ReentrantCall`).
- **FFI Panic Boundaries**: Wrapped CXX-Qt `#[qinvokable]` entry points and slots in `ThemeManager` and `GalleryBackend` with `std::panic::catch_unwind`.
- **Root, Translator & Window Lifetime**: Connected `QObject::destroyed` in `rinui_engine_register_latest_root` so externally destroyed roots drop from `s_roots`; prevented duplicate teardown in `rinui_root_release`; made `rinui_translator_load_rinui` / `rinui_translator_load_app` transactional with Qt 6.9 empty-catalog support (`install_translator_or_empty`) and locale tag normalization (`zh-CN`, `zh_Hans_CN`, `zh`).
- **Platform Native Adapters**:
  - **Windows (`native/src/rinui_win_bridge.cpp`, `crates/rinui-platform/src/platform.rs`)**: Stripped `\\?\` verbatim path prefixes, replaced brittle `QMetaObject::invokeMethod` string lookup with `qobject_cast<rinui::ThemeManager*>`, guarded `s_event_manager` with `QPointer`, added dynamic `GetDpiForWindow`/`GetSystemMetricsForDpi`, fixed `hit_test` client coordinates / resize border DPI / `WM_CAPTURECHANGED` / `WM_DPICHANGED`, fixed `apply_nccalcsize` and `apply_getminmaxinfo`, guarded `kDwmwaWindowCornerPreference` on Win11, and replaced Win32 FFI `OSVERSIONINFOW` lints with compile-time-asserted `OsVersionInfoW` + `c"RtlGetVersion"`.
  - **macOS (`native/src/rinui_mac_window.mm`)**: Tracked `observed_ns_window` to cleanly reattach `NSNotificationCenter` observers across `NSWindow` changes.
- **Consumer Independence**: `rinui-platform` and `rinui-host` contain zero Goosy-specific business logic, UI flows, or paths.

### 4.2 Local Re-Verification on `60ca934` (`Qt 6.12.0`, `rustc 1.97.1`)

- `python3 scripts/audit_upstream.py`: **PASSED**
- `cargo fmt --all -- --check`: **PASSED**
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASSED**
- `cargo test --workspace`: **PASSED** (`rinui-platform`: `10 passed`; `rinui-host` unit: `2 passed`; integration suites `embedded_runtime`, `filesystem_dev_mode`, `hardening`: passed; `rinui-gallery` `standalone_packaging`: `1 passed`).

---

## 5. Upstream Isolation Verification (`60ca934`)

- `git submodule status`: `46d77fa8371ff0e9f46a10a2eb66612dab123269 upstream/Rin-UI (beta-0.5.0-16-g46d77fa)` (unchanged).
- `git -C upstream/Rin-UI status --porcelain`: **empty** (clean before and after all local builds, tests, and packaging).
- **Read-Only Upstream Verification**: Copied `upstream/Rin-UI` to a temporary directory, removed write permissions (`chmod -R a-w`), and executed `RINUI_UPSTREAM_DIR="$RO_TMP/ro-upstream" cargo check -p rinui-host --target-dir "$RO_TMP/target"` — exited `0` (`Finished` cleanly).
- **Generated Artifacts**: All `qmldir` sanitization, manifest generation (`rinui_manifest.rs`), and `rinui_resources.qrc` outputs are written exclusively to Cargo `$OUT_DIR/rinui_sanitized/`.
- **Mitigation Overrides Preserved Outside Upstream**:
  - Drop 5 non-existent upstream `qmldir` entries and normalize 2 versionless singleton entries in `crates/rinui-host/build.rs`.
  - Mount embedded QRC at `/rinui/RinUI` (`qrc:/rinui`) to prevent shadowing filesystem mode.
  - Require `QtShaderTools` alongside `Qt5Compat` on Qt 6.9+.
  - Accept empty header-only `.qm` catalogs (`en_US.qm`) via `install_translator_or_empty` on Qt 6.9.
  - Force-include `<arm_acle.h>` in CI on macOS arm64 for `Qt 6.9.3` + Apple Clang 21 (`QTBUG-145239`).

---

## 6. Verification Status Matrix (Distinguishing Implemented / Compiled / Tested / Visually Verified)

| Area / Target | Implemented | Compiled | Automated Tests & Smoke (`CI` + Local) | Visually Verified (Interactive Desktop) |
|---|---|---|---|---|
| **Upstream Submodule Isolation & Audit** | Yes | Yes | Yes (`audit_upstream.py` + `chmod a-w` `RINUI_UPSTREAM_DIR` check) | N/A |
| **`rinui-platform` Config, Migration & Caps** | Yes | Yes | Yes (`10` unit tests on macOS/Windows/Linux) | N/A |
| **`rinui-host` Runtime, i18n, Theme & Hardening** | Yes | Yes | Yes (`2` unit + `embedded_runtime`, `filesystem_dev_mode`, `hardening` on all 3 OSes) | Pending interactive QA |
| **macOS Host & `.app` Bundle (`arm64`)** | Yes | Yes | Yes (CI `Qt 6.9.3` + local `6.12.0`/`6.9.3`; isolated Cocoa QPA `--smoke-test` & `--auto-close-ms`) | Pending desktop screenshot/visual QA |
| **Windows Host & `windeployqt` (`x64` MSVC)** | Yes | Yes | Yes (CI `Qt 6.9.3`; `offscreen` tests + isolated `windeployqt` `windows` QPA `--smoke-test`) | Pending physical Windows 10/11 desktop QA |
| **Linux Host & Staging Bundle (`x64` GCC + `lld`)** | Yes | Yes | Yes (CI `Qt 6.9.3`; `offscreen` tests + `offscreen` & Xvfb `xcb` isolated `--smoke-test`) | Pending real X11 WM & native Wayland QA |

---

## 7. Remaining Limitations & Release Readiness Assessment

### Remaining Limitations

1. Interactive desktop visual inspection (macOS native titlebar/traffic lights & light/dark transitions on a desktop screen, Windows 11 DWM Mica/Acrylic/Tabbed backdrops, Snap Layouts hover/press, and multi-monitor DPI on a physical Windows desktop, and real Linux X11/Wayland desktop compositors) has **not** yet been performed; automated validation proves compilation, API contracts, `offscreen`/Xvfb `xcb`, and isolated startup smoke execution.
2. macOS `.app` distribution uses local ad-hoc signing (`codesign --force --deep --sign -`), not Apple Developer ID signing or Gatekeeper notarization.
3. No Gallery visual screenshots have been committed yet.

### Release Readiness Decision: **CONDITIONAL GO** for `v0.1.0-alpha.1` (Pre-Release)

`rinui-rs` satisfies the engineering, isolation, API-boundary, packaging, and cross-platform automated CI criteria for an **`alpha.1` pre-release**, subject to the conditions tracked in [`docs/release-checklist.md`](release-checklist.md):

1. Final documentation commit CI run on `phase2-validation` passes all 4 jobs.
2. Any prebuilt public binaries include Qt 6 LGPLv3 dynamic-linking compliance disclosures ([`docs/packaging-and-licensing.md`](packaging-and-licensing.md)).
3. Interactive desktop visual spot-checks (and Gallery screenshots) are completed by a maintainer on physical macOS/Windows/Linux desktops if releasing public binary installers, and `phase2-validation` is merged to `main` prior to manually tagging `v0.1.0-alpha.1`.
4. **No Git tag, crate publish, or GitHub Release has been executed automatically.**
