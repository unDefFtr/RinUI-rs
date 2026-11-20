# Windows 10 Gallery border investigation

## Windows Qt 6.10.3 baseline: initial visual check passed

The Windows CI/package toolchain now uses Qt 6.10.3 instead of 6.9.3.
The user confirmed on 2026-11-21 that the upgraded test window has no white
strips along its left and top edges. This validates the observed window,
not every resize, maximize/restore or backdrop transition. No upstream QML
or native window geometry workaround was needed for this result.

Local environment (2026-11-21): Windows 10 Pro for Workstations 22H2,
build 19045, x64 MSVC 2022, Rust 1.98.0, 125% display scaling.
Qt was installed with `qt5compat` and `qtshadertools`.

The originally reported Rust distribution loads Qt 6.9.3, whereas the Python
distribution used for visual comparison loads Qt 6.10.0. Their Qt versions
and host revisions differ, so the comparison does not establish a Qt bug.
The supplied screenshot shows partial white strips several pixels wide,
unlike the uniform one-pixel QML theme border. Qt 6.9.3 startup diagnostics
show frame margins transitioning through eight pixels before returning to
zero, followed by a D3D11/DirectComposition alpha swapchain. Native frame
changes, redraw and transparent composition remain the suspected area.

## Local validation

- `cargo build -p rinui-gallery --locked`: passed with Qt 6.10.3.
- `scripts/package_windows.ps1` on that executable: passed.
- Packaged `--smoke-test` with `windows` QPA and an isolated configuration:
  passed (exit 0), including navigation/error recovery, theme, i18n and
  multi-window assertions.
- `scripts/audit_upstream.py` and `cargo fmt --all -- --check`: passed;
  upstream remains unchanged.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked` with `offscreen` QPA: standalone
  packaging, resource checks, core-app rejection, embedded/filesystem
  runtime, hardening, i18n, object interop and all 25 isolated parity
  regression scenarios passed. The run stopped at
  `runtime_sessions.rs`'s assertion that the staged development resource
  directory has already been removed after final-session teardown.
  Re-running that test with `QML_DISABLE_DISK_CACHE=1` failed at the same
  assertion; a disk-cache explanation is therefore unconfirmed. This is
  an outstanding validation failure, not a passed full suite.
- Desktop capture repeatedly timed out for both original and upgraded
  Gallery windows. The initial visual result therefore comes from the
  user's explicit confirmation that the Qt 6.10.3 test window has no white
  borders, rather than an automated screenshot assertion.
- The upgraded native window has matching 1750x1000 window/client rectangles
  and a client origin at the window origin. Its extended style is `0x100`,
  matching the Python reference window; the original Rust process reported
  `0xC0110`. These observations are useful diagnostics, not a pixel test.

The initial visual check has passed. Remaining regression checks are
resizing, maximize/restore, minimize/restore, and theme/backdrop changes.
Compare at the same window size and scaling. If strips reappear,
compare D3D11 with OpenGL using a process-local `QSG_RHI_BACKEND=opengl`
and then investigate native frame/redraw initialization; do not infer
success from an offscreen or smoke-test pass.
