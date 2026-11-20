// Link crate dependencies whose C++ initializers are referenced by rinui-gallery's build.rs.
extern crate cxx_qt;
extern crate cxx_qt_lib;
extern crate rinui_host;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create target dir");
    for entry in fs::read_dir(src).expect("read source dir").flatten() {
        let src_path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy() == ".git" {
            continue;
        }
        let dst_path = dst.join(name);
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

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
        .env_remove("RINUI_EXAMPLES_DIR")
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

#[test]
fn gallery_dev_mode_reloads_modified_upstream_without_recompiling() {
    let built_bin = PathBuf::from(env!("CARGO_BIN_EXE_rinui-gallery"));
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let upstream_root = workspace_root.join("upstream").join("Rin-UI");

    let tmp = TempDir::new().expect("create temp dir");
    let isolated_run_dir = tmp.path().join("run dir");
    fs::create_dir_all(&isolated_run_dir).expect("create isolated run dir");

    // Copy upstream checkout to a writable path containing spaces.
    let dev_copy = tmp.path().join("rinui upstream copy");
    copy_dir_recursive(&upstream_root, &dev_copy);

    // Edit a RinUI control, upstream gallery root, and upstream Home page AFTER binary build.
    let fluent_win_path = dev_copy.join("RinUI/windows/FluentWindow.qml");
    let orig_fw = fs::read_to_string(&fluent_win_path).expect("read FluentWindow.qml");
    let edited_fw = orig_fw.replacen(
        "FluentWindowBase {",
        "FluentWindowBase {\n    property string rinuiDevProbe: \"RINUI_CONTROL_MODIFIED\"",
        1,
    );
    assert_ne!(orig_fw, edited_fw);
    fs::write(&fluent_win_path, edited_fw).expect("write edited FluentWindow.qml");

    let gallery_qml_path = dev_copy.join("examples/gallery.qml");
    let orig_gal = fs::read_to_string(&gallery_qml_path).expect("read gallery.qml");
    let edited_gal = orig_gal.replacen("minimumWidth: 550", "minimumWidth: 612", 1);
    assert_ne!(orig_gal, edited_gal);
    fs::write(&gallery_qml_path, edited_gal).expect("write edited gallery.qml");

    let home_qml_path = dev_copy.join("examples/pages/Home.qml");
    let orig_home = fs::read_to_string(&home_qml_path).expect("read Home.qml");
    let edited_home = orig_home.replacen(
        "FluentPage {",
        "FluentPage {\n    title: \"DEV_MODIFIED_HOME\"",
        1,
    );
    assert_ne!(orig_home, edited_home);
    fs::write(&home_qml_path, edited_home).expect("write edited Home.qml");

    let qpa = std::env::var("QT_QPA_PLATFORM").unwrap_or_else(|_| "offscreen".to_owned());

    // 1. `--dev <PATH>` loads modified RinUI control + modified upstream gallery/Home page.
    let cfg_cli = tmp.path().join("cfg_cli");
    let cli_out = Command::new(&built_bin)
        .current_dir(&isolated_run_dir)
        .env_remove("RINUI_DEV_PATH")
        .env_remove("RINUI_EXAMPLES_DIR")
        .env("QT_QPA_PLATFORM", &qpa)
        .arg("--dev")
        .arg(&dev_copy)
        .arg("--smoke-test")
        .arg("--config-dir")
        .arg(&cfg_cli)
        .output()
        .expect("run rinui-gallery --dev");
    let cli_stdout = String::from_utf8_lossy(&cli_out.stdout);
    let cli_stderr = String::from_utf8_lossy(&cli_out.stderr);
    assert!(
        cli_out.status.success(),
        "gallery --dev failed:\nstdout:\n{cli_stdout}\nstderr:\n{cli_stderr}"
    );
    assert!(
        cli_stdout.contains("minimumWidth=612")
            && cli_stdout.contains("currentPageObjectName=Home")
            && cli_stdout.contains("currentPageTitle=DEV_MODIFIED_HOME")
            && cli_stdout.contains("rinuiDevProbe=RINUI_CONTROL_MODIFIED"),
        "expected modified dev probes in stdout, got:\n{cli_stdout}"
    );

    // 2. `RINUI_DEV_PATH=<PATH>` (without `--dev`) follows the exact same resolution path.
    let cfg_env = tmp.path().join("cfg_env");
    let env_out = Command::new(&built_bin)
        .current_dir(&isolated_run_dir)
        .env("RINUI_DEV_PATH", &dev_copy)
        .env_remove("RINUI_EXAMPLES_DIR")
        .env("QT_QPA_PLATFORM", &qpa)
        .arg("--smoke-test")
        .arg("--config-dir")
        .arg(&cfg_env)
        .output()
        .expect("run rinui-gallery with RINUI_DEV_PATH");
    let env_stdout = String::from_utf8_lossy(&env_out.stdout);
    let env_stderr = String::from_utf8_lossy(&env_out.stderr);
    assert!(
        env_out.status.success(),
        "gallery RINUI_DEV_PATH failed:\nstdout:\n{env_stdout}\nstderr:\n{env_stderr}"
    );
    assert!(
        env_stdout.contains("minimumWidth=612")
            && env_stdout.contains("currentPageTitle=DEV_MODIFIED_HOME")
            && env_stdout.contains("rinuiDevProbe=RINUI_CONTROL_MODIFIED"),
        "expected modified dev probes under RINUI_DEV_PATH, got:\n{env_stdout}"
    );

    // 3. A RinUI-only directory without examples must error clearly rather than silently
    // mixing embedded QRC examples.
    let rinui_only_root = tmp.path().join("rinui_only_root");
    copy_dir_recursive(&dev_copy.join("RinUI"), &rinui_only_root.join("RinUI"));
    let cfg_only = tmp.path().join("cfg_only");
    let only_out = Command::new(&built_bin)
        .current_dir(&isolated_run_dir)
        .env_remove("RINUI_DEV_PATH")
        .env_remove("RINUI_EXAMPLES_DIR")
        .env("QT_QPA_PLATFORM", &qpa)
        .arg("--dev")
        .arg(rinui_only_root.join("RinUI"))
        .arg("--smoke-test")
        .arg("--config-dir")
        .arg(&cfg_only)
        .output()
        .expect("run rinui-gallery --dev on RinUI-only dir");
    assert!(
        !only_out.status.success(),
        "Gallery must fail clearly when examples/gallery.qml is missing"
    );
    let only_stderr = String::from_utf8_lossy(&only_out.stderr);
    assert!(
        only_stderr.contains("gallery.qml"),
        "expected error mentioning missing gallery.qml, got:\n{only_stderr}"
    );
}
