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
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes across **macOS arm64**, **Windows x64 (MSVC)**, and **Linux x64 (GCC + `lld`)**.
- [x] `cargo test --workspace` passes across all 3 CI targets (`Qt 6.9.3`, `rustc 1.100.0`) and locally on **macOS arm64** (`Qt 6.12.0` + `Qt 6.9.3`, `rustc 1.97.1`):
  - `rinui-platform`: `10 passed; 0 failed`
  - `rinui-host` unit tests: `2 passed; 0 failed`
  - `rinui-host` integration suites: `embedded_runtime`, `filesystem_dev_mode`, `hardening` (`harness = false`) passed
  - `rinui-gallery` `standalone_packaging`: `1 passed; 0 failed`
- [x] GitHub Actions CI passed all 4 jobs on `60ca934` ([run #44518777691](https://github.com/unDefFtr/rinui-rs/actions/runs/44518777691)) — final documentation commit CI run verified green before handoff.

## D. Packaging & Self-Containment

- [x] **macOS (`scripts/package_macos.sh`)**: Verified on CI (`Qt 6.9.3`) and locally (`Qt 6.12.0` & `Qt 6.9.3`). Bundle includes `Info.plist` (`plutil -lint` OK), `qt.conf`, `QtCore.framework`, `libqcocoa.dylib`, `Qt5Compat/GraphicalEffects`, and license notices; `otool -L` and `LC_RPATH` contain zero `/opt/homebrew`, `/usr/local`, `/Users/`, or unresolved `@rpath` references; isolated Cocoa `.app` `--smoke-test` and `--auto-close-ms 1500` exit `0` outside checkout; zero `RinUI` dev checkout paths embedded in release binary.
- [x] **Windows (`scripts/package_windows.ps1`)**: Verified in GitHub Actions `windows-latest` (`Qt 6.9.3` MSVC); `windeployqt` deploys `Qt6ShaderTools.dll`, `Qt5Compat/GraphicalEffects`, `platforms/qwindows.dll`, and license notices; isolated raw binary (`offscreen`) and isolated `windeployqt` package (`windows` QPA) `--smoke-test` pass.
- [x] **Linux (`scripts/package_linux.sh`)**: Verified in GitHub Actions `ubuntu-latest` (`Qt 6.9.3` + `lld` + `qtshadertools` + `xvfb`); raw binary and packaged `run-gallery.sh` `--smoke-test` pass under both `QT_QPA_PLATFORM=offscreen` and `xvfb-run` `QT_QPA_PLATFORM=xcb`.
- [ ] **Apple Developer ID Code-Signing & Notarization**: Deferred post-alpha (`scripts/package_macos.sh` performs local ad-hoc Mach-O signing only).
- [ ] **Windows Authenticode Signing / MSIX / Linux AppImage**: Deferred post-alpha (directory distribution validated for `v0.1.0-alpha.1`).

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
