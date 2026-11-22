#[path = "src/qmldir_sanitize.rs"]
mod qmldir_sanitize;

use cxx_qt_build::CxxQtBuilder;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rerun-if-env-changed=RINUI_UPSTREAM_DIR");
    println!("cargo::rerun-if-env-changed=QMAKE");
    println!("cargo::rerun-if-env-changed=QT_VERSION_MAJOR");
    println!("cargo::rerun-if-changed=src/qmldir_sanitize.rs");

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));

    let upstream_rinui = resolve_upstream_rinui(&manifest_dir);
    let native_include_dir = manifest_dir.join("native/include");
    let native_src_dir = manifest_dir.join("native/src");

    watch_dir(&native_include_dir);
    watch_dir(&native_src_dir);

    let (packaged_files, drops, qrc_path) =
        prepare_rinui_resources(&upstream_rinui, &out_dir.join("rinui_sanitized"), &out_dir);
    write_manifest_rs(&out_dir.join("rinui_manifest.rs"), &packaged_files, &drops);

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let qt_bridge_cpp = native_src_dir.join("rinui_qt_bridge.cpp");
    let theme_bridge_cpp = native_src_dir.join("rinui_theme_bridge.cpp");
    let test_support_cpp = native_src_dir.join("rinui_test_support.cpp");

    let mut builder = CxxQtBuilder::new()
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .include_dir(&native_include_dir)
        .file("src/theme.rs")
        .file("src/bridge.rs")
        .file("src/test_support.rs")
        .qrc(&qrc_path)
        .cpp_file(&qt_bridge_cpp)
        .cpp_file(&theme_bridge_cpp)
        .cpp_file(&test_support_cpp);

    if env::var_os("CARGO_FEATURE_QT_WIDGETS_TEST").is_some() {
        builder = builder
            .qt_module("Widgets")
            .cpp_file(native_src_dir.join("rinui_widgets_test_helper.cpp"));
    }

    let include_for_cc = native_include_dir.clone();
    // Safety: adds native/include directly so C++ translation units resolve local headers.
    unsafe {
        builder = builder.cc_builder(move |cc| {
            cc.include(&include_for_cc);
        });
    }

    match target_os.as_str() {
        "windows" => {
            let win_header = native_include_dir.join("rinui_win_bridge.h");
            let win_cpp = native_src_dir.join("rinui_win_bridge.cpp");
            builder = builder.cpp_file(&win_header).cpp_file(&win_cpp);
            println!("cargo::rustc-link-lib=dwmapi");
            println!("cargo::rustc-link-lib=user32");
            println!("cargo::rustc-link-lib=shell32");
        }
        "linux" => {
            let linux_cpp = native_src_dir.join("rinui_linux_window.cpp");
            builder = builder.cpp_file(&linux_cpp);
        }
        "macos" => {
            compile_macos_native(&native_include_dir, &native_src_dir);
        }
        other => {
            println!("cargo::warning=Unsupported target OS '{other}'; using Linux stub adapter");
            let linux_cpp = native_src_dir.join("rinui_linux_window.cpp");
            builder = builder.cpp_file(&linux_cpp);
        }
    }

    builder.build().reexport_dependency("cxx-qt-lib").export();
}

fn compile_macos_native(native_include_dir: &Path, native_src_dir: &Path) {
    let mac_mm = native_src_dir.join("rinui_mac_window.mm");
    println!("cargo::rerun-if-changed={}", mac_mm.display());

    let qt_modules = vec![
        "Core".to_owned(),
        "Gui".to_owned(),
        "Qml".to_owned(),
        "Quick".to_owned(),
    ];
    let qtbuild =
        qt_build_utils::QtBuild::new(qt_modules).expect("Could not locate Qt 6 for macOS adapter");

    let mut mm_build = cc::Build::new();
    mm_build
        .cpp(true)
        .std("c++17")
        .flag("-fobjc-arc")
        .include(native_include_dir)
        .file(&mac_mm);

    for inc in qtbuild.include_paths() {
        mm_build.include(inc);
    }
    for fw in qtbuild.framework_paths() {
        mm_build.flag(format!("-F{}", fw.display()));
    }

    mm_build.compile("rinui-mac-native");
    println!("cargo::rustc-link-lib=framework=Cocoa");
    println!("cargo::rustc-link-lib=framework=AppKit");
    println!("cargo::rustc-link-lib=framework=Foundation");
}

fn resolve_upstream_rinui(manifest_dir: &Path) -> PathBuf {
    let candidate = if let Ok(custom) = env::var("RINUI_UPSTREAM_DIR") {
        let p = PathBuf::from(custom);
        if p.join("RinUI").join("qmldir").is_file() {
            p.join("RinUI")
        } else {
            p
        }
    } else {
        manifest_dir.join("vendor/RinUI")
    };

    if !candidate.join("qmldir").is_file() {
        panic!(
            "RinUI qmldir not found at '{}'. Check the bundled sources or RINUI_UPSTREAM_DIR.",
            candidate.display()
        );
    }
    candidate
}

fn watch_dir(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                watch_dir(&path);
            } else {
                println!("cargo::rerun-if-changed={}", path.display());
            }
        }
    }
}

fn prepare_rinui_resources(
    upstream_rinui: &Path,
    sanitized_dir: &Path,
    out_dir: &Path,
) -> (Vec<String>, Vec<(String, String)>, PathBuf) {
    if sanitized_dir.exists() {
        let _ = fs::remove_dir_all(sanitized_dir);
    }
    fs::create_dir_all(sanitized_dir).expect("create rinui_sanitized directory");

    let files = qmldir_sanitize::collect_runtime_files_with_dirs(upstream_rinui, &mut |dir| {
        println!("cargo::rerun-if-changed={}", dir.display());
    })
    .expect("collect upstream RinUI runtime files");

    let mut drops = Vec::new();
    let mut qrc_entries: Vec<(String, PathBuf)> = Vec::with_capacity(files.len());

    for rel in &files {
        let src_path = upstream_rinui.join(rel);
        println!("cargo::rerun-if-changed={}", src_path.display());

        if Path::new(rel).file_name().and_then(|n| n.to_str()) == Some("qmldir") {
            let (sanitized_text, file_drops) =
                qmldir_sanitize::sanitize_qmldir_file(rel, &src_path)
                    .expect("sanitize upstream qmldir");
            drops.extend(file_drops);
            let dst_path = sanitized_dir.join(rel);
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).expect("create sanitized qmldir parent");
            }
            fs::write(&dst_path, sanitized_text).expect("write sanitized qmldir");
            qrc_entries.push((rel.clone(), dst_path));
        } else if let Some(patched) = qmldir_sanitize::sanitize_runtime_qml(rel, &src_path)
            .expect("sanitize upstream QML runtime file")
        {
            let dst_path = sanitized_dir.join(rel);
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).expect("create sanitized QML parent");
            }
            fs::write(&dst_path, patched).expect("write sanitized QML");
            qrc_entries.push((rel.clone(), dst_path));
        } else {
            qrc_entries.push((rel.clone(), src_path));
        }
    }

    let qrc_path = out_dir.join("rinui_resources.qrc");
    let mut qrc = fs::File::create(&qrc_path).expect("create rinui_resources.qrc");
    writeln!(qrc, "<RCC>").unwrap();
    writeln!(qrc, "  <qresource prefix=\"/rinui/RinUI\">").unwrap();
    for (alias, abs_path) in &qrc_entries {
        let canonical = normalize_qrc_path(abs_path);
        writeln!(
            qrc,
            "    <file alias=\"{}\">{}</file>",
            xml_escape(alias),
            xml_escape(&canonical)
        )
        .unwrap();
    }
    writeln!(qrc, "  </qresource>").unwrap();
    writeln!(qrc, "</RCC>").unwrap();

    (files, drops, qrc_path)
}

fn write_manifest_rs(out_path: &Path, files: &[String], drops: &[(String, String)]) {
    let mut f = fs::File::create(out_path).expect("create rinui_manifest.rs");
    writeln!(f, "pub const PACKAGED_RINUI_FILES: &[&str] = &[").unwrap();
    for file in files {
        writeln!(f, "    {:?},", file).unwrap();
    }
    writeln!(f, "];").unwrap();
    writeln!(f, "pub const SANITIZED_QMLDIR_DROPS: &[(&str, &str)] = &[").unwrap();
    for (qmldir, line) in drops {
        writeln!(f, "    ({:?}, {:?}),", qmldir, line).unwrap();
    }
    writeln!(f, "];").unwrap();
}

fn normalize_qrc_path(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = canonical.to_string_lossy();
    let stripped = if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        raw.into_owned()
    };
    stripped.replace('\\', "/")
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
