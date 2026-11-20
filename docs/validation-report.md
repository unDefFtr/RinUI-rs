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

---

## 8. Upstream Parity & Post-Review Validation (Stages A–G + Review Fixes, `2026-11-20`)

Post-baseline upstream parity work and subsequent independent-review fixes (P1 QML callback self/peer Session drop safety, P1 Shared-engine dynamic window ownership + `register_window_from_property()`, P1 `ThemeManagerRef`/`ThemeManagerMut` reader-writer borrow protection & slot re-entrancy guards, and P2 live locale updates on pre-existing `Calendar`/`CalendarDatePicker`/`DatePicker`/`SpinBox` controls) were validated locally on **macOS arm64** (`Qt 6.12.0` in `/opt/homebrew/lib`, `rustc 1.97.1 (8bab26f4f 2026-07-14)`). `upstream/Rin-UI` remained untouched at `46d77fa8371ff0e9f46a10a2eb66612dab123269`.

### 8.1 Executed & Passed Checks After Review Fixes (Local macOS `Qt 6.12.0`)

| Layer / Check | Command | Observed Result |
|---|---|---|
| Audit | `python3 scripts/audit_upstream.py` | **PASSED** (`115` runtime files, `23` QML host context refs, clean submodule) |
| Format | `cargo fmt --all -- --check` | **PASSED** |
| Lint (Default) | `cargo clippy --workspace --all-targets --offline -- -D warnings` | **PASSED** |
| Lint (`qt-widgets-test`) | `cargo clippy -p rinui-host --features qt-widgets-test --all-targets --offline -- -D warnings` | **PASSED** |
| Layer 1 (`offscreen` workspace) | `QT_QPA_PLATFORM=offscreen cargo test --workspace --offline` | **PASSED**: `rinui-platform` (`10 passed`), `rinui-host` unit (`2 passed`), integration suites (`core_app_rejection`, `embedded_runtime`, `filesystem_dev_mode`, `hardening`, `i18n_semantics`, `object_interop`, `parity_review_regressions` [12 isolated subprocess workers], `runtime_sessions` all passed), `rinui-gallery` `standalone_packaging` (`2 passed`) |
| Layer 1 (Qt Widgets adoption) | `QT_QPA_PLATFORM=offscreen cargo test -p rinui-host --features qt-widgets-test --test external_qapplication --offline` | **PASSED** (`external_qapplication integration tests passed.`) |
| Layer 1 (Review regressions, subprocess + timeout) | `QT_QPA_PLATFORM=offscreen cargo test -p rinui-host --test parity_review_regressions --offline` | **PASSED** (`12/12` isolated subprocess workers: `load-drop`, `load-drop-shared`, `property-drop`, `translation-drop`, `event-drop-process`, `event-drop-exec`, `event-nested-prop-drop`, `theme-signal-drop`, `ownership`, `theme-alias`, `locale-live-embedded`, `locale-live-dev`) |
| Layer 1 (`EngineConfigurator` demo) | `QT_QPA_PLATFORM=offscreen cargo run -p rinui-host --example engine_config_demo --offline` | **PASSED** (`Demo result: titleProp=EngineConfigDemo Title, addInts(10,32)=42, typedRepeat=RinUI!RinUI!`) |

Prior to the 4 review fixes (same `Qt 6.12.0` / `2026-11-20` baseline; release `.app` bundle rebuild remains clean to re-run after final fixes):
- `QT_QPA_PLATFORM=offscreen cargo run -p rinui-gallery --offline -- --dev --smoke-test --config-dir <tmp>` (**PASSED**)
- `cargo build --release -p rinui-gallery --offline` + `strings` dev-path check (**PASSED**: `0` `rinui-rs/{upstream,crates,examples}` dev paths)
- `scripts/package_macos.sh` syntax (`bash -n`), `--help`, negative `--notarize` CLI checks (**PASSED**) and isolated `QT_QPA_PLATFORM=cocoa ./RinUIGallery.app/Contents/MacOS/RinUIGallery --smoke-test` in both default self-drawn (`mac_diagnostic=self_drawn`) and `--upstream-mac-frame` (`mac_diagnostic=native_cocoa`) modes (**PASSED**).

### 8.2 Not Executed on Local Host (`未执行` / External Blockers)

| Item | Status | Missing Prerequisites | Exact Reproduction Command(s) |
|---|---|---|---|
| Windows CI / MSVC build & `windeployqt` `windows` QPA smoke | **未执行** (post-parity) | Windows 10/11 host or GitHub `windows-latest` runner with MSVC 2022, Qt 6.9+ (`qt5compat`, `qtshadertools`), `windeployqt` | Push branch to trigger `.github/workflows/ci.yml` (`windows-latest`), or run `.\scripts\package_windows.ps1` then `QT_QPA_PLATFORM=windows .\rinui-gallery.exe --smoke-test --config-dir <tmp>` |
| Unsigned MSIX pack & `MakeAppx unpack` layout verification | **未执行** (local macOS) | Windows 10/11 SDK (`makeappx.exe`) on Windows | `.\scripts\package_msix.ps1 -SourceDirectory dist\RinUIGallery-win64 -OutputDirectory dist\msix -SkipSign -VerifyLayout` |
| Windows MSIX installation, launch & uninstall | **未执行** | Physical/VM Windows 10/11 desktop + trusted signing cert | Sign with `-CertPfx`/`-CertThumbprint`, install `.msix`, launch from Start Menu, uninstall cleanly |
| Windows Desktop Visual QA (Win10 Acrylic & disable, Win11 21H2 fallback, 22H2+ Mica/Acrylic/Tabbed, Snap Layouts, auto-hide taskbar, cross-DPI) | **未执行** | Physical Windows 10 + Windows 11 21H2/22H2+ desktops | Launch Gallery on target Windows desktops and exercise Theme/Backdrop, maximize, drag across DPI displays |
| Linux CI (`lld`, `package_linux.sh`, headless `xvfb-xcb`) | **未执行** (post-parity) | Linux host or GitHub `ubuntu-latest` runner with Qt 6, `lld`, `xvfb` | Push branch to trigger `.github/workflows/ci.yml` (`ubuntu-latest`), or `./scripts/package_linux.sh` + `xvfb-run -a env QT_QPA_PLATFORM=xcb ./run-gallery.sh --smoke-test` |
| Linux Desktop QA (real X11 WM + native Wayland compositor) | **未执行** | Interactive Linux X11 WM & Wayland desktop sessions | Run packaged Gallery on native X11 WM and Wayland (`QT_QPA_PLATFORM=wayland`), verify move/resize/maximize and `-2` backdrop fallback |
| macOS interactive physical-mouse QA (post-parity) | **未执行** | Interactive human GUI inspection on macOS desktop | Launch `RinUIGallery.app` in default and `--upstream-mac-frame` modes; verify drag, double-click, search field, back button, traffic-light geometry, fullscreen exit recovery |
| Apple Developer ID signing / Gatekeeper notarization & Windows Authenticode signing | **未执行** | Apple Developer ID cert + App Store Connect keychain notary profile; Windows Authenticode PFX/store cert | `./scripts/package_macos.sh --sign-identity "<ID>" --notarize --notary-profile "<PROFILE>"`; `.\scripts\package_windows.ps1 -Sign ...`; `.\scripts\package_msix.ps1 ...` |


## 9. Second-Round P1 Lifecycle Validation (2026-11-20)

This section records the second lifecycle pass; section 8 is the earlier review pass. The later nested-event-processing gap and its follow-up validation are recorded in section 10; these 20-worker results did not cover that path. Local host: macOS arm64, Qt **6.12.0**, rustc **1.97.1 (8bab26f4f 2026-07-14)**. No commit, reset, push, publish or release was performed. Neither upstream nor the user-owned `mtktools/` and `skills/` directories were edited.

### Fixes and cleanup contract

1. **External event processing Session self-drop**: `app.process_events()` can invoke QML with no `QmlExecGuard` and `loopLevel() == 0`. Session drop now always detaches its roots/windows and uses `deleteLater()`, always queues Private engines, and performs no synchronous window hide/releaseResources or deferred-delete flush. Handles return `HostDisposed` immediately; sibling sessions remain callable and QML executes the following `Relay.trigger("after")`.
2. **Last Runtime owner in a callback**: borrowed Runtime destruction is two-phase. It unregisters and saves configuration, detaches owned windows/translators/signals, restores borrowed state, and transfers retained engines, ThemeManager and RuntimeActiveGuard to `PENDING_RUNTIME_TEARDOWNS`. A safe drain flushes deletes while GUI-alive, drops engines, flushes, releases filters then services, drops ThemeManager, flushes, then releases the runtime reservation. Cache/GC calls are skipped once the GUI application is gone.

A coalesced zero-delay timer drains idle live runtimes and pending teardowns. Global QML/theme guard counters prevent unsafe normal drains; aboutToQuit and qAddPostRoutine cover event-loop exit and app destruction. Post routines do not reschedule. A borrowed app must drain its external event loop before a subsequent attach; pending cleanup retains `RUNTIME_ACTIVE` and returns `ApplicationAlreadyInitialized`. Normal `drop(app)` completes the fallback even without further process_events; abrupt process exit cannot run destructors. App lifetime and GUI-thread requirements still apply, including the raw API contract.

### Current command results

C++ completion preceded runtime changes; the host build passed before running any regression worker. Formatting was applied before the ordered audit/fmt/lint/test checks. Cargo uses offline mode (global `cargo --offline fmt` for formatting); Qt tests use `QT_QPA_PLATFORM=offscreen`.

| Command | Result in this pass |
|---|---|
| `cargo build --offline -p rinui-host` | **PASSED** |
| `cargo --offline fmt --all` | **PASSED** |
| `python3 scripts/audit_upstream.py` | **PASSED**, 115 runtime files, 23 context references, clean pinned upstream |
| `cargo --offline fmt --all -- --check` | **PASSED** |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | **PASSED** |
| `cargo clippy -p rinui-host --features qt-widgets-test --all-targets --offline -- -D warnings` | **PASSED** |
| `QT_QPA_PLATFORM=offscreen cargo test --workspace --offline` | **PASSED**: 2 Gallery standalone tests, 2 host unit tests, all eight host integration programs (including 20/20 parity workers), 10 platform tests and doc tests |
| `QT_QPA_PLATFORM=offscreen cargo test -p rinui-host --offline --features qt-widgets-test --test external_qapplication` | **PASSED**, external QApplication and caller widget remain usable |
| `git diff --check` | **PASSED**, no output |
| `git -C upstream/Rin-UI status --porcelain` | **PASSED**, empty |

Build output included the existing Xcode ranlib empty-archive warning for generated CXX-Qt init archives; both Clippy commands still exited 0 with `-D warnings`. The Qt Widgets test link also warned that a duplicate Homebrew rpath was ignored; it exited 0. Command logs for this run are in `/private/tmp/rinui-lifecycle-validation/` (local temporary evidence, not a repository artifact).

### Isolated worker results

`QT_QPA_PLATFORM=offscreen cargo test -p rinui-host --offline --test parity_review_regressions`: **PASSED, 20/20**, one subprocess and a 20-second timeout per mode. Every new worker asserts continued JS, immediate `HostDisposed`, a successful sibling method call after self-drop, pending resource retention inside the callback, eventual deletion of filesystem `dev_load_root` paths, blocked premature attachment and successful subsequent attachment, and normal application destruction. The safe-attach fixture owns its app until all callback captures and borrowed values have dropped; raw fixtures obey the same lifetime boundary. The app-drop worker reattaches to a newly constructed external app after the original app is destroyed.

| Worker | Observed result | Coverage |
|---|---|---|
| `load-drop` | PASSED | Existing load self-drop |
| `load-drop-shared` | PASSED | Existing Shared-engine load self-drop |
| `property-drop` | PASSED | Existing property callback |
| `translation-drop` | PASSED | Existing translation callback |
| `event-drop-process` | PASSED | Existing host process_events |
| `event-drop-exec` | PASSED | Existing host exec |
| `event-nested-prop-drop` | PASSED | Existing nested property callback |
| `theme-signal-drop` | PASSED | Existing ThemeManager signal |
| `ownership` | PASSED | Shared window ownership and explicit registration |
| `theme-alias` | PASSED | ThemeManager readers/writer and slot protection |
| `locale-live-embedded` | PASSED | Existing embedded live locale switching |
| `locale-live-dev` | PASSED | Existing filesystem live locale switching |
| `external-process-safe` | PASSED | Safe attach, direct external process_events |
| `external-process-nested-prop-drop` | PASSED | Safe attach, nested property callback and continued inner/outer JS |
| `external-exec-last-private` | PASSED | Private engines, Session then Runtime, external exec |
| `external-exec-last-reversed` | PASSED | Runtime then Sessions, schedule_quit(0) queued before drain, aboutToQuit |
| `external-exec-last-shared` | PASSED | Last Shared engine under external exec |
| `external-process-last-private` | PASSED | Last Private engines under external process_events |
| `external-process-last-shared-reversed` | PASSED | Shared engine, reversed ownership release |
| `external-process-last-app-drop` | PASSED | No further process_events, pending views deleted by app post routine, reservation released |

The first expanded-suite run stopped at the Shared fixture's QML loading stage (`File name case mismatch`): its second QML file was written after the shared engine cached the directory. Both files are now materialized before either load; the subsequent 20-worker run passed. No worker timed out or SIGABRTed in these runs.

### Not executed in this pass / remaining limits

- Windows/Linux builds, CI and physical desktop validation: **未执行**. Local macOS offscreen results do not validate those platforms or native visual behavior.
- MSIX packing, installation, startup and uninstall: **未执行**; requires Windows SDK and a suitable Windows host/certificate.
- Apple Developer ID / Gatekeeper notarization and Windows Authenticode signing: **未执行**, no signing credentials used.
- Release rebuilds, release-bundle packaging/native Cocoa isolated startup, interactive Cocoa QA and the standalone engine_config_demo command: **未执行 in this pass**. Earlier evidence remains historical.
- The external application must remain alive through all borrowed Runtime/Session/Handle lifetimes and pending teardown until it drains or completes normal destruction. No leak-free cleanup guarantee is possible for process::exit/abort.


## 10. Nested External Event Processing P1 Follow-Up (2026-11-20)

Historical 22-worker pass: it did not cover direct nested external app pumping. The subsequent defect and current validation are recorded in section 11.

The second-round claim was incomplete: the user subsequently reproduced SIGABRT with safe `attach_existing`, a QML Timer dropping its own Private Session, and the same callback calling a surviving Runtime's `process_events()`. The external application remained alive. Both Runtime and Session `process_events()` performed an extra synchronous flush after their QmlExecGuard dropped solely because `loopLevel() == 0`. An externally driven outer QML callback can still be executing at that point.

Removed those two supplemental flushes. QmlExecGuard release already calls `finish_deferred_work_or_schedule`: borrowed apps only schedule, and standalone apps retain synchronous flushing on outermost guard exit outside an event loop. Pending drain callbacks dispatched by a nested host pump see its active execution guard and reschedule. This follow-up does not change the two-phase Runtime teardown or its aboutToQuit / qAddPostRoutine fallbacks.

Two new safe-attach subprocess workers retain the original 20 workers and 20-second timeout per process:

| Worker | Result in this follow-up | Required behavior |
|---|---|---|
| `external-process-nested-pump` | PASSED | After self-drop, call Runtime process_events three times on the same QML callback stack |
| `external-process-nested-pump-session` | PASSED | After self-drop, call surviving sibling Session process_events three times on that stack |

Each checks the dropped engine's resource view still exists after every nested call, the callback finishes, subsequent JS executes, handles immediately return HostDisposed and the sibling remains callable. After the outer callback returns, direct external event processing must reclaim the dropped Private engine while the Runtime and sibling are still alive. This prevents final Runtime teardown from masking a stuck drain. Final resource removal, reattachment and normal app destruction remain required.

### Worker results from this follow-up

All **22/22** subprocess workers passed in the workspace run, with no timeout or SIGABRT:

| Worker | Result |
|---|---|
| `load-drop` | PASSED |
| `load-drop-shared` | PASSED |
| `property-drop` | PASSED |
| `translation-drop` | PASSED |
| `event-drop-process` | PASSED |
| `event-drop-exec` | PASSED |
| `event-nested-prop-drop` | PASSED |
| `theme-signal-drop` | PASSED |
| `ownership` | PASSED |
| `theme-alias` | PASSED |
| `locale-live-embedded` | PASSED |
| `locale-live-dev` | PASSED |
| `external-process-safe` | PASSED |
| `external-process-nested-prop-drop` | PASSED |
| `external-process-nested-pump` | PASSED |
| `external-process-nested-pump-session` | PASSED |
| `external-exec-last-private` | PASSED |
| `external-exec-last-reversed` | PASSED |
| `external-exec-last-shared` | PASSED |
| `external-process-last-private` | PASSED |
| `external-process-last-shared-reversed` | PASSED |
| `external-process-last-app-drop` | PASSED |

### Commands run for this follow-up

Local macOS arm64 / Qt 6.12.0 / rustc 1.97.1. Cargo runs offline; Qt tests use offscreen. New logs are in `/private/tmp/rinui-nested-pump-validation/`; section 9's logs describe the preceding pass.

| Command | Result |
|---|---|
| `cargo --offline fmt --all` | PASSED |
| `cargo build --offline -p rinui-host` | PASSED |
| `python3 scripts/audit_upstream.py` | PASSED, 115 files / 23 context references, pinned upstream clean |
| `cargo --offline fmt --all -- --check` | PASSED |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | PASSED |
| `cargo clippy -p rinui-host --features qt-widgets-test --all-targets --offline -- -D warnings` | PASSED |
| `QT_QPA_PLATFORM=offscreen cargo test --workspace --offline` | PASSED: 22/22 workers, all eight host integration programs, 2 host unit / 10 platform / 2 Gallery standalone tests, and doc tests |
| `QT_QPA_PLATFORM=offscreen cargo test -p rinui-host --offline --features qt-widgets-test --test external_qapplication` | PASSED |
| `git diff --check` | PASSED, no output |
| `git -C upstream/Rin-UI status --porcelain` | PASSED, empty |

Builds retained the generated CXX-Qt init archive ranlib warning; the Qt Widgets test linker also ignored a duplicate Homebrew rpath. Both Clippy checks and all tests exited 0.

Windows/Linux builds and physical desktop checks, MSIX packaging/install/startup, Developer ID / Authenticode signing, release packaging/native Cocoa startup and the standalone engine_config_demo command remain **未执行 in this follow-up**. Borrowed apps must still drain after the outer callback returns before reattachment, or complete normal application destruction. Abrupt process termination cannot execute cleanup hooks. No reset, commit, push or publication is authorized or performed; upstream, mtktools and skills remain untouched.


## 11. Direct External App Pump / Qt DeferredDelete Follow-Up (2026-11-20)

The user reproduced a remaining SIGABRT with safe attach_existing, self-drop of a Private Session in a Timer callback, and direct external app.process_events in that same callback. The external app remained alive. Sections 9–10 did not establish safety for that path: a queued timer could run without a host guard and force global deletion while the original QML handler was still active.

### Implementation and driver contract

- C++ creates one EngineDropFence QObject for each borrowed Private engine queued for destruction, and for a shared engine entering pending Runtime teardown. Its deleteLater event carries Qt's own scope/loop rules. The engine becomes ready only upon actual DeferredDelete delivery; the marker is deleted before its Rust continuation runs. Engine unregister cancels any outstanding marker and removes its ready entry.
- Normal Rust drains select only ready engines, with no forced global DeferredDelete flush for borrowed apps. Pending Runtime teardown waits for all retained engine markers. Existing QML/theme guards remain additional protection, and qAddPostRoutine still completes final destruction without another event pump. Old queued wakeups cannot destroy newer unqualified engines.
- The external manual driver has an explicit Qt requirement: processEvents alone does not automatically dispatch every DeferredDelete (including events posted outside exec/notification delivery). At an outer boundary after callbacks return, use QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete). This is standard Qt event driving, not a guessed-safe timer callback. It must not be forced from the nested QML handler. Without a driver dispatch/exec, cleanup may wait until normal app destruction. Reference: [Qt QObject::deleteLater](https://doc.qt.io/qt-6/qobject.html#deleteLater); the Qt 6.12 source was also inspected for its loop/scope checks.
- Test driver adjustment is explicit: the safe external workers explicitly dispatch DeferredDeletes only after the QML callback returns, including reclamation while the Runtime/sibling remain alive; runtime_sessions does the same before sequential reattachment. The test-only forced-dispatch binding is explicitly unsafe: its caller must establish that no Qt/QML callback remains on the stack. Their lifecycle assertions and existing modes remain. No RinUI guard or forced flush surrounds the new nested app call. This corrects the earlier assumption that process_events alone would always finish external teardown.

### Regression coverage

Retained all 22 worker modes, adding three 20-second subprocess workers:

| Worker | Result | Coverage |
|---|---|---|
| external-process-nested-app-pump | PASSED | Safe attach, self-drop, three direct app pumps in the same callback, continued JS; reclaim target while Runtime/sibling remain alive |
| external-process-nested-app-pump-last-private | PASSED | Drop Private Sessions and final Runtime, then three direct app pumps before QML returns |
| external-process-nested-app-pump-last-shared | PASSED | Same pending teardown path for the shared Engine |

Each checks resource retention after every nested call, immediate HostDisposed, sibling calls before sibling release, subsequent JS, final resource removal, blocked premature attach, eventual reattachment and normal app destruction. The original post-routine worker still performs no further event/DeferredDelete dispatch after its callback.

The first fence-based suite attempt did not abort, but the new direct-app worker reached its five-second resource-reclamation deadline. A backtrace isolated this to the second pump, after the continued-JS flag was already set. Work was stopped for a plan review: on this manual Cocoa/offscreen driver, recursive processing left DeferredDeletes pending even after callback return. The outer driver now explicitly dispatches them at that point. The in-callback direct app pump and resource-retention assertions were not weakened, and no RinUI guard was added.

### Final workspace worker results

Each worker below exited successfully in the final workspace run; each retains its 20-second subprocess limit.

| Worker | Result |
|---|---|
| load-drop | PASSED |
| load-drop-shared | PASSED |
| property-drop | PASSED |
| translation-drop | PASSED |
| event-drop-process | PASSED |
| event-drop-exec | PASSED |
| event-nested-prop-drop | PASSED |
| theme-signal-drop | PASSED |
| ownership | PASSED |
| theme-alias | PASSED |
| locale-live-embedded | PASSED |
| locale-live-dev | PASSED |
| external-process-safe | PASSED |
| external-process-nested-prop-drop | PASSED |
| external-process-nested-pump | PASSED |
| external-process-nested-pump-session | PASSED |
| external-process-nested-app-pump | PASSED |
| external-process-nested-app-pump-last-private | PASSED |
| external-process-nested-app-pump-last-shared | PASSED |
| external-exec-last-private | PASSED |
| external-exec-last-reversed | PASSED |
| external-exec-last-shared | PASSED |
| external-process-last-private | PASSED |
| external-process-last-shared-reversed | PASSED |
| external-process-last-app-drop | PASSED |

### Current checks

Local macOS arm64 / Qt 6.12.0. Logs for final lint/test commands: `/private/tmp/rinui-qt-deferred-delete-validation/`. All Cargo dependency operations use --offline; Qt tests use QT_QPA_PLATFORM=offscreen.

| Check | Result |
|---|---|
| Host build after C++/Rust changes | PASSED |
| Formatting applied | PASSED |
| 25-worker parity suite | PASSED, 25/25; no SIGABRT or subprocess timeout in the successful run |
| Upstream audit | PASSED, 115 files / 23 context refs / pinned clean upstream |
| Format check | PASSED |
| Workspace Clippy | PASSED: cargo clippy --workspace --all-targets --offline -- -D warnings |
| Qt Widgets Clippy | PASSED: cargo clippy -p rinui-host --features qt-widgets-test --all-targets --offline -- -D warnings |
| Workspace tests | PASSED: QT_QPA_PLATFORM=offscreen cargo test --workspace --offline; includes 25/25 workers, runtime_sessions, hardening and object_interop |
| external_qapplication | PASSED: QT_QPA_PLATFORM=offscreen cargo test -p rinui-host --offline --features qt-widgets-test --test external_qapplication |
| git diff --check | PASSED, empty |
| Final upstream status | PASSED: git -C upstream/Rin-UI status --porcelain, empty |

The generated CXX-Qt init archives retained the ranlib empty-symbol-table warning. The Widgets test linker also ignored a duplicate Homebrew rpath; expected QML fixture/font diagnostics remain. Both Clippy commands and both final test commands exited 0.

Windows/Linux builds and physical desktop checks, MSIX packaging/install/startup, Developer ID / Authenticode signing, release packaging/native Cocoa startup and the standalone engine_config_demo command remain **未执行 in this follow-up**. No reset, commit, push or publication is performed; upstream, mtktools and skills are not edited.
