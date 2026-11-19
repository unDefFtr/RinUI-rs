use cxx_qt_build::CxxQtBuilder;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const ALLOWED_EXTENSIONS: &[&str] = &["qml", "js", "png", "svg", "jpg", "ttf", "otf", "qm"];

fn main() {
    println!("cargo::rerun-if-env-changed=RINUI_UPSTREAM_DIR");
    println!("cargo::rerun-if-env-changed=QMAKE");
    println!("cargo::rerun-if-env-changed=QT_VERSION_MAJOR");

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("examples/gallery must live two levels under workspace root")
        .to_path_buf();
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));

    let upstream_examples = resolve_upstream_examples(&workspace_root);
    let gallery_qml_dir = manifest_dir.join("qml");
    let shared_qml_dir = workspace_root.join("qml");
    let native_dir = manifest_dir.join("native");

    watch_dir(&gallery_qml_dir);
    watch_dir(&native_dir);

    let mut qrc_entries: Vec<(String, PathBuf)> = Vec::new();
    collect_upstream_example_files(&upstream_examples, &upstream_examples, &mut qrc_entries);

    let upstream_gallery_qml = upstream_examples.join("gallery.qml");
    if upstream_gallery_qml.is_file() {
        qrc_entries.push(("UpstreamGallery.qml".to_owned(), upstream_gallery_qml));
    }

    for (alias, path) in [
        ("GalleryMain.qml", gallery_qml_dir.join("GalleryMain.qml")),
        (
            "RustHostDemoPage.qml",
            gallery_qml_dir.join("RustHostDemoPage.qml"),
        ),
        (
            "MultiWindowDialog.qml",
            shared_qml_dir.join("MultiWindowDialog.qml"),
        ),
    ] {
        println!("cargo::rerun-if-changed={}", path.display());
        qrc_entries.push((alias.to_owned(), path));
    }

    let qrc_path = out_dir.join("gallery_resources.qrc");
    write_qrc(&qrc_path, "/gallery", &qrc_entries);

    CxxQtBuilder::new()
        .include_dir(&native_dir)
        .file("src/backend.rs")
        .cpp_file("native/gallery_backend_factory.cpp")
        .qrc(&qrc_path)
        .build();
}

fn resolve_upstream_examples(workspace_root: &Path) -> PathBuf {
    let candidate = if let Ok(custom) = env::var("RINUI_UPSTREAM_DIR") {
        let p = PathBuf::from(custom);
        if p.join("examples").join("gallery.qml").is_file() {
            p.join("examples")
        } else {
            workspace_root
                .join("upstream")
                .join("Rin-UI")
                .join("examples")
        }
    } else {
        workspace_root
            .join("upstream")
            .join("Rin-UI")
            .join("examples")
    };

    if !candidate.join("gallery.qml").is_file() {
        panic!(
            "Upstream RinUI examples not found at '{}'. Run `git submodule update --init --recursive`.",
            candidate.display()
        );
    }
    candidate
}

fn collect_upstream_example_files(root: &Path, current: &Path, out: &mut Vec<(String, PathBuf)>) {
    println!("cargo::rerun-if-changed={}", current.display());
    let mut entries: Vec<_> = fs::read_dir(current)
        .expect("read upstream examples directory")
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if path.is_dir() {
            if matches!(name_str.as_ref(), "scripts" | "__pycache__") || name_str.starts_with('.') {
                continue;
            }
            collect_upstream_example_files(root, &path, out);
        } else if is_allowed_example_file(&path) {
            println!("cargo::rerun-if-changed={}", path.display());
            let rel = path
                .strip_prefix(root)
                .expect("strip upstream examples root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn is_allowed_example_file(path: &Path) -> bool {
    if path.file_name().and_then(|n| n.to_str()) == Some("qmldir") {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            ALLOWED_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
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

fn write_qrc(qrc_path: &Path, prefix: &str, entries: &[(String, PathBuf)]) {
    let mut qrc = fs::File::create(qrc_path).expect("create gallery_resources.qrc");
    writeln!(qrc, "<RCC>").unwrap();
    writeln!(qrc, "  <qresource prefix=\"{}\">", xml_escape(prefix)).unwrap();
    for (alias, abs_path) in entries {
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
