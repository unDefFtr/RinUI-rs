use cxx_qt_build::CxxQtBuilder;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const RUNTIME_EXTENSIONS: &[&str] = &["qml", "js", "ttf", "otf", "png", "svg", "jpg", "qm"];

fn main() {
    println!("cargo::rerun-if-env-changed=RINUI_UPSTREAM_DIR");
    println!("cargo::rerun-if-env-changed=QMAKE");
    println!("cargo::rerun-if-env-changed=QT_VERSION_MAJOR");

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("crates/rinui-host must live two levels under workspace root")
        .to_path_buf();
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));

    let upstream_rinui = resolve_upstream_rinui(&workspace_root);
    let native_include_dir = workspace_root.join("native").join("include");
    let native_src_dir = workspace_root.join("native").join("src");

    watch_dir(&native_include_dir);
    watch_dir(&native_src_dir);

    let (packaged_files, drops, qrc_path) =
        prepare_rinui_resources(&upstream_rinui, &out_dir.join("rinui_sanitized"), &out_dir);
    write_manifest_rs(&out_dir.join("rinui_manifest.rs"), &packaged_files, &drops);

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let qt_bridge_cpp = native_src_dir.join("rinui_qt_bridge.cpp");
    let theme_bridge_cpp = native_src_dir.join("rinui_theme_bridge.cpp");

    let mut builder = CxxQtBuilder::new()
        .qt_module("Gui")
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .include_dir(&native_include_dir)
        .file("src/theme.rs")
        .file("src/bridge.rs")
        .qrc(&qrc_path)
        .cpp_file(&qt_bridge_cpp)
        .cpp_file(&theme_bridge_cpp);

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

fn resolve_upstream_rinui(workspace_root: &Path) -> PathBuf {
    let candidate = if let Ok(custom) = env::var("RINUI_UPSTREAM_DIR") {
        let p = PathBuf::from(custom);
        if p.join("RinUI").join("qmldir").is_file() {
            p.join("RinUI")
        } else {
            p
        }
    } else {
        workspace_root.join("upstream").join("Rin-UI").join("RinUI")
    };

    if !candidate.join("qmldir").is_file() {
        panic!(
            "Upstream RinUI not found at '{}'. Run `git submodule update --init --recursive`.",
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

    let mut files = Vec::new();
    collect_runtime_files(upstream_rinui, upstream_rinui, &mut files);
    files.sort();

    let mut drops = Vec::new();
    let mut qrc_entries: Vec<(String, PathBuf)> = Vec::with_capacity(files.len());

    for rel in &files {
        let src_path = upstream_rinui.join(rel);
        println!("cargo::rerun-if-changed={}", src_path.display());

        if Path::new(rel).file_name().and_then(|n| n.to_str()) == Some("qmldir") {
            let (sanitized_text, file_drops) = sanitize_qmldir(rel, &src_path);
            drops.extend(file_drops);
            let dst_path = sanitized_dir.join(rel);
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).expect("create sanitized qmldir parent");
            }
            fs::write(&dst_path, sanitized_text).expect("write sanitized qmldir");
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

fn collect_runtime_files(root: &Path, current: &Path, out: &mut Vec<String>) {
    println!("cargo::rerun-if-changed={}", current.display());
    let mut entries: Vec<_> = fs::read_dir(current)
        .expect("read upstream RinUI directory")
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if name_str == "__pycache__" || name_str.starts_with('.') {
                continue;
            }
            collect_runtime_files(root, &path, out);
        } else if is_runtime_file(&path) {
            let rel = path
                .strip_prefix(root)
                .expect("strip RinUI root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
}

fn is_runtime_file(path: &Path) -> bool {
    if path.file_name().and_then(|n| n.to_str()) == Some("qmldir") {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            RUNTIME_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

fn sanitize_qmldir(rel_qmldir: &str, src_path: &Path) -> (String, Vec<(String, String)>) {
    let qmldir_dir = src_path.parent().expect("qmldir must have parent");
    let raw = fs::read_to_string(src_path).expect("read upstream qmldir");

    let mut out_lines = Vec::new();
    let mut drops = Vec::new();
    let mut declared_types = BTreeSet::new();

    for raw_line in raw.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("module ") {
            out_lines.push(raw_line.to_owned());
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(target) = parts.last() else {
            out_lines.push(raw_line.to_owned());
            continue;
        };

        let target_path = qmldir_dir.join(target);
        if !target_path.is_file() {
            drops.push((rel_qmldir.to_owned(), trimmed.to_owned()));
            out_lines.push(format!("# [rinui-rs sanitized missing target] {trimmed}"));
            continue;
        }

        // Normalize versionless singleton declarations in themes/qmldir (e.g. `singleton Colors Colors.qml`).
        if parts[0] == "singleton" && parts.len() == 3 {
            let normalized = format!("singleton {} 1.0 {}", parts[1], parts[2]);
            declared_types.insert(parts[1].to_owned());
            out_lines.push(normalized);
        } else if parts[0] != "singleton" && parts.len() == 2 {
            let normalized = format!("{} 1.0 {}", parts[0], parts[1]);
            declared_types.insert(parts[0].to_owned());
            out_lines.push(normalized);
        } else {
            let type_name = if parts[0] == "singleton" {
                parts.get(1).copied().unwrap_or("")
            } else {
                parts[0]
            };
            declared_types.insert(type_name.to_owned());
            out_lines.push(raw_line.to_owned());
        }
    }

    if rel_qmldir == "qmldir"
        && !declared_types.contains("ScrollView")
        && qmldir_dir.join("components/ScrollView.qml").is_file()
    {
        out_lines.push(String::new());
        out_lines.push("# [rinui-rs added missing existing component]".to_owned());
        out_lines.push("ScrollView 1.0 components/ScrollView.qml".to_owned());
    }

    let mut result = out_lines.join("\n");
    result.push('\n');
    (result, drops)
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
