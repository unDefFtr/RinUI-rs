// Link crate dependencies whose C++ initializers are referenced by rinui-gallery's build.rs.
extern crate cxx_qt;
extern crate cxx_qt_lib;
extern crate rinui_host;

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn gallery_runs_standalone_outside_source_checkout() {
    let built_bin = PathBuf::from(env!("CARGO_BIN_EXE_rinui-gallery"));
    assert!(
        built_bin.is_file(),
        "expected compiled rinui-gallery binary at {}",
        built_bin.display()
    );

    // Copy the compiled binary into an isolated directory outside the repository
    // so relative paths to upstream/Rin-UI cannot satisfy QML loading.
    let isolated_dir = TempDir::new().expect("create isolated temp dir");
    let cfg_dir = isolated_dir.path().join("app_config");
    let bin_name = built_bin
        .file_name()
        .expect("rinui-gallery binary file name");
    let isolated_bin = isolated_dir.path().join(bin_name);
    fs::copy(&built_bin, &isolated_bin).expect("copy rinui-gallery to isolated dir");

    let mut cmd = Command::new(&isolated_bin);
    cmd.current_dir(isolated_dir.path())
        .env_remove("RINUI_DEV_PATH")
        .env_remove("RINUI_UPSTREAM_DIR")
        .arg("--smoke-test")
        .arg("--config-dir")
        .arg(&cfg_dir);

    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        cmd.env("QT_QPA_PLATFORM", "offscreen");
    }

    let output = cmd.output().expect("execute isolated rinui-gallery");
    assert!(
        output.status.success(),
        "standalone rinui-gallery failed (status: {:?})\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rinui_cfg = cfg_dir.join("RinUI").join("config").join("rin_ui.json");
    let app_cfg = cfg_dir.join("config").join("config.json");
    assert!(
        rinui_cfg.is_file(),
        "expected persisted rin_ui.json at {}",
        rinui_cfg.display()
    );
    assert!(
        app_cfg.is_file(),
        "expected persisted gallery config.json at {}",
        app_cfg.display()
    );
}
