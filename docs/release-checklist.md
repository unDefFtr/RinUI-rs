# Release Checklist — `v0.1.0-alpha.1`

This checklist tracks release readiness for **`rinui-rs` `v0.1.0-alpha.1`** (pre-release).

> **Scope Boundary**: Phase 2 stops at release readiness assessment. No Git tag (`v0.1.0-alpha.1`), crate publication, or GitHub Release has been created automatically.

---

## A. Upstream & Repository Integrity

- [x] Pinned upstream submodule `upstream/Rin-UI` matches expected SHA `46d77fa8371ff0e9f46a10a2eb66612dab123269` (`beta-0.5.0-16-g46d77fa`, RinUI `0.5.0`).
- [x] `git -C upstream/Rin-UI status --porcelain` is empty (zero modifications inside submodule).
- [x] Build-time `qmldir` sanitization outputs exclusively to `$OUT_DIR/rinui_sanitized/` (`crates/rinui-host/build.rs`); confirmed `rinui-host` builds cleanly against a `chmod -R a-w` read-only `RINUI_UPSTREAM_DIR` copy.
- [x] `python3 scripts/audit_upstream.py` passes (`115` runtime files, `23` host context references, zero unexpected `qmldir` regressions).
- [x] Embedded QRC mounts under `/rinui/RinUI` (`qrc:/rinui`), avoiding `qrc:/qt/qml` shadowing of filesystem dev mode.
- [x] No duplicated RinUI component tree committed outside `upstream/Rin-UI`.

## B. Version Consistency

- [x] Workspace `[workspace.package].version` in `Cargo.toml` set to `"0.1.0-alpha.1"`.
- [x] Member crates (`crates/rinui-platform/Cargo.toml`, `crates/rinui-host/Cargo.toml`, `examples/gallery/Cargo.toml`) inherit `version.workspace = true`, `repository.workspace = true`.
- [x] `Cargo.lock` updated to `0.1.0-alpha.1` for `rinui-platform`, `rinui-host`, and `rinui-gallery`.
- [x] `CMakeLists.txt` sets numeric `project(rinui-rs VERSION 0.1.0 ...)` and `RINUI_RS_VERSION_STRING "0.1.0-alpha.1"`.
- [x] `scripts/package_macos.sh` sets `CFBundleShortVersionString` to `0.1.0-alpha.1`.
- [x] `examples/gallery/src/backend.rs` reports `0.5.0 (rinui-rs 0.1.0-alpha.1)`.
- [x] `CHANGELOG.md` documents `[0.1.0-alpha.1]`.

## C. Cross-Platform Build, Lint, and Test Verification

- [x] `cargo fmt --all -- --check` passes cleanly.
- [x] `cargo clippy --workspace --all-targets --offline -- -D warnings` and `cargo clippy -p rinui-host --features qt-widgets-test --all-targets --offline -- -D warnings` pass on **macOS arm64** (`Qt 6.12.0`, `rustc 1.97.1`); prior `v0.1.0-alpha.1` baseline (`60ca934`) passed Windows x64 and Linux x64 CI (`Qt 6.9.3`, `rustc 1.100.0`).
- [x] `cargo test --workspace --offline` (`QT_QPA_PLATFORM=offscreen`) + `external_qapplication` (`--features qt-widgets-test`) + `parity_review_regressions` + `engine_config_demo` pass locally on **macOS arm64** (`Qt 6.12.0`, `rustc 1.97.1`):
  - `rinui-platform`: `10 passed; 0 failed`
  - `rinui-host` unit tests: `2 passed; 0 failed`
  - `rinui-host` integration suites (`harness = false`): `core_app_rejection`, `embedded_runtime`, `filesystem_dev_mode`, `hardening`, `i18n_semantics`, `object_interop`, `parity_review_regressions` (`25/25` subprocess workers), `runtime_sessions`, and `external_qapplication` (`qt-widgets-test`) passed
  - `rinui-gallery` `standalone_packaging`: `2 passed; 0 failed`
- [ ] **Post-Parity CI Run on Updated Workflow**: Push parity & review-fix commits to trigger updated `.github/workflows/ci.yml` across **macOS arm64**, **Windows x64**, and **Linux x64** (`audit_upstream.py`, strict Cocoa / Windows QPA without offscreen fallbacks, unsigned MSIX pack & unpack verify). *(Prior baseline `60ca934` passed [#44518777691](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691); post-parity CI run pending push.)*

### Second-round P1 lifecycle review (2026-11-20)

- [x] Session roots/windows always defer deletion and Private engines always queue, including direct external `app.process_events()` callbacks.
- [x] Borrowed last-owner Runtime teardown retains engines, ThemeManager and runtime reservation until a safe drain; timer, aboutToQuit and qAddPostRoutine paths have dedicated subprocess workers (20-second timeout each).
- [x] All eight new external-loop workers and the original twelve pass locally; filesystem views are removed, handles invalidate immediately, siblings remain callable and reattachment succeeds after draining (post-routine worker reattaches to a new external app).
- External event-loop draining must finish before reattaching; `ApplicationAlreadyInitialized` is expected while a teardown owns the reservation. See [validation report, section 9](validation-report.md#9-second-round-p1-lifecycle-validation-2026-11-20) for this run's command results and limitations. Earlier engine_config_demo / packaging evidence is historical and was not rerun in this lifecycle pass.
- [ ] Windows/Linux physical desktop validation, MSIX installation/startup and Developer ID / Authenticode signing: **未执行**.

### Nested event processing follow-up (2026-11-20)

- [x] Remove the extra synchronous flush from both Runtime and Session process_events; guard release controls cleanup, and borrowed apps only schedule even at loopLevel zero.
- [x] Validate safe-attach self-drop followed by nested Runtime/Session event processing with two new 20-second subprocess workers; all 22/22 workers pass, including all 20 preceding modes. See [validation report, section 10](validation-report.md#10-nested-external-event-processing-p1-follow-up-2026-11-20) for this follow-up's actual results. Prior 20/20 evidence did not cover these nested pumps.

### Direct external app pump / Qt DeferredDelete follow-up (2026-11-20)

- [x] Borrowed engines require independent Qt DeferredDelete fences; ordinary drains never force global deletion and Runtime teardown waits for all engines.
- [x] Validate direct nested app.process_events through safe attach, including last Private/Shared Runtime teardown (three new 20-second workers, 25/25 passed).
- [x] Correct manual-driver contract: dispatch DeferredDeletes at an outer boundary after callbacks return. Prior 22/22 evidence did not cover the direct external app pump; the first new attempt reached a resource-reclamation deadline, with continued JS and no SIGABRT, and prompted a driver-contract review. No guard/forced flush was added to the nested app call. See [section 11](validation-report.md#11-direct-external-app-pump--qt-deferreddelete-follow-up-2026-11-20).
- [x] Rerun audit, fmt, both Clippy configurations, workspace and Qt Widgets tests, whitespace and final upstream checks for this pass: all passed; final upstream status is empty. Results and warnings are recorded in validation report section 11.

## D. Packaging & Self-Containment

- [ ] **macOS (`scripts/package_macos.sh`)**: Verified on `Qt 6.12.0` prior to the latest review-fix pass (`--smoke-test` in both `self_drawn` and `--upstream-mac-frame` `native_cocoa` modes + optional `--sign-identity` / `--notarize` CLI checks); rebuild `.app` on next release packaging after review fixes.
- [ ] **Windows Directory + Rust MSIX (`scripts/package_windows.ps1`, `scripts/package_msix.ps1`)**: Scripts & `packaging/windows/` manifest/assets implemented (zero Python/PyInstaller, PowerShell 5.1/7 compatible, optional Authenticode & `-SkipSign -VerifyLayout`); **not executed on this macOS host** (requires Windows SDK `makeappx.exe` / Qt `windeployqt.exe` on Windows runner/desktop).
- [ ] **Linux Staging (`scripts/package_linux.sh`)**: Prior baseline `60ca934` verified on CI under `offscreen` and headless `xvfb-xcb`; post-parity Linux CI run pending push.
- [ ] **Apple Developer ID Code-Signing & Notarization**: Optional workflow implemented in `scripts/package_macos.sh`; **未执行** (requires Apple Developer ID cert & App Store Connect notary profile).
- [ ] **Windows Authenticode Signing & Signed MSIX Install/Launch/Uninstall**: Optional workflow implemented in `scripts/package_windows.ps1` and `scripts/package_msix.ps1`; **未执行** (requires Windows code-signing cert & physical/VM Windows desktop).

## E. API Boundary, Docs, and Licensing

- [x] Public APIs in `rinui-host` and `rinui-platform` contain zero Goosy-specific business logic.
- [x] Upstream MIT license (`upstream/Rin-UI/LICENSE`), host MIT license (`LICENSE`), and third-party / Qt notices (`THIRD_PARTY_NOTICES.md`) preserved and copied by all packaging scripts.
- [x] Documentation updated: `README.md`, `README.zh-CN.md`, `CHANGELOG.md`, `docs/compatibility.md`, `docs/consumer-guide.md`, `docs/packaging-and-licensing.md`, `docs/validation-report.md`, `docs/release-notes-v0.1.0-alpha.1.md`.
- [ ] If distributing prebuilt binaries with GitHub Releases: attach Qt LGPLv3 compliance notes (bundled Qt exact version `6.9.3`, dynamic linking preserved, link/offer for corresponding Qt source code).

## F. Remaining Manual Actions Before Publishing `v0.1.0-alpha.1`

- [ ] **Interactive Visual Verification (Recommended Before Public Alpha Tag)**:
  - Launch packaged `RinUIGallery.app` visually on macOS Desktop and verify transparent titlebar, traffic lights alignment, Light/Dark switching, and HiDPI scaling.
  - Launch packaged Windows Gallery on Windows 10/11 desktop and verify DWM Mica/Acrylic/Tabbed backdrop, `WM_NCHITTEST` titlebar buttons, Windows 11 Snap Layouts (`HTMAXBUTTON`), and multi-monitor DPI transitions.
  - Launch packaged Linux Gallery on real X11 window manager and native Wayland compositor (confirm `-2` backdrop fallback and compositor resize).
  - Capture updated Gallery screenshot(s) and add them to `README.md` / release notes.
- [ ] **Merge & Verify `main`**: Open PR or fast-forward merge `phase2-validation` into `main` and confirm CI on `main` completes green.
- [ ] **Create Annotated Tag (Human Action)**:
  ```bash
  git checkout main && git pull --ff-only
  git tag -a v0.1.0-alpha.1 -m "rinui-rs v0.1.0-alpha.1"
  git push origin v0.1.0-alpha.1
  ```
- [ ] **Draft / Publish GitHub Pre-Release (Human Action)**: Create GitHub Release for `v0.1.0-alpha.1` as **Pre-release** using [`docs/release-notes-v0.1.0-alpha.1.md`](release-notes-v0.1.0-alpha.1.md).
