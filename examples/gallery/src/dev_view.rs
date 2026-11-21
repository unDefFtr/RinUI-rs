use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const ALLOWED_EXTENSIONS: &[&str] = &["qml", "js", "png", "svg", "jpg", "ttf", "otf", "qm"];
const EMBEDDED_GALLERY_MAIN_QML: &str = include_str!("../qml/GalleryMain.qml");
const EMBEDDED_DEMO_PAGE_QML: &str = include_str!("../qml/RustHostDemoPage.qml");
const EMBEDDED_MULTI_WINDOW_QML: &str =
    include_str!("../../../crates/rinui-host/tests/fixtures/MultiWindowDialog.qml");

static GALLERY_DEV_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Ephemeral filesystem dev view for RinUI Gallery (`examples/` + host wrapper QMLs).
///
/// Rebuilt on every launch from the latest disk sources so edits to upstream `examples/`
/// and RinUI QML take effect on next launch without recompiling the Rust binary.
pub struct GalleryDevView {
    source_examples_dir: PathBuf,
    staged_dir: PathBuf,
}

impl GalleryDevView {
    pub fn prepare(examples_source_dir: &Path) -> io::Result<Self> {
        let canonical_src = strip_windows_verbatim_prefix(examples_source_dir.canonicalize()?);
        let upstream_gallery = canonical_src.join("gallery.qml");
        if !upstream_gallery.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "missing upstream gallery.qml at '{}'",
                    upstream_gallery.display()
                ),
            ));
        }

        let staged_dir = create_unique_temp_dir("rinui-gallery-dev")?;
        let copy_res = (|| -> io::Result<()> {
            copy_example_tree(&canonical_src, &canonical_src, &staged_dir)?;
            fs::copy(&upstream_gallery, staged_dir.join("UpstreamGallery.qml"))?;

            write_wrapper_qml(
                &staged_dir.join("GalleryMain.qml"),
                &[Path::new("examples/gallery/qml/GalleryMain.qml")],
                EMBEDDED_GALLERY_MAIN_QML,
            )?;
            write_wrapper_qml(
                &staged_dir.join("RustHostDemoPage.qml"),
                &[Path::new("examples/gallery/qml/RustHostDemoPage.qml")],
                EMBEDDED_DEMO_PAGE_QML,
            )?;
            write_wrapper_qml(
                &staged_dir.join("MultiWindowDialog.qml"),
                &[Path::new(
                    "crates/rinui-host/tests/fixtures/MultiWindowDialog.qml",
                )],
                EMBEDDED_MULTI_WINDOW_QML,
            )?;
            Ok(())
        })();

        if let Err(err) = copy_res {
            let _ = fs::remove_dir_all(&staged_dir);
            return Err(err);
        }

        Ok(Self {
            source_examples_dir: canonical_src,
            staged_dir,
        })
    }

    pub fn source_examples_dir(&self) -> &Path {
        &self.source_examples_dir
    }

    pub fn staged_dir(&self) -> &Path {
        &self.staged_dir
    }

    pub fn main_qml(&self) -> PathBuf {
        self.staged_dir.join("GalleryMain.qml")
    }

    pub fn secondary_dialog_qml(&self) -> PathBuf {
        self.staged_dir.join("MultiWindowDialog.qml")
    }

    pub fn languages_dir(&self) -> PathBuf {
        self.staged_dir.join("languages")
    }

    pub fn icon_path(&self) -> PathBuf {
        self.staged_dir
            .join("assets")
            .join("icons")
            .join("app")
            .join("gallery.png")
    }
}

impl Drop for GalleryDevView {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.staged_dir);
    }
}

fn write_wrapper_qml(
    dest: &Path,
    cwd_candidates: &[&Path],
    fallback_contents: &str,
) -> io::Result<()> {
    for rel in cwd_candidates {
        if rel.is_file() {
            fs::copy(rel, dest)?;
            return Ok(());
        }
    }
    fs::write(dest, fallback_contents)
}

fn copy_example_tree(root: &Path, current: &Path, stage_root: &Path) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(current)?.flatten().collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if path.is_dir() {
            if matches!(name_str.as_ref(), "scripts" | "__pycache__") || name_str.starts_with('.') {
                continue;
            }
            copy_example_tree(root, &path, stage_root)?;
        } else if is_allowed_example_file(&path) {
            let rel = path.strip_prefix(root).map_err(io::Error::other)?;
            let dst = stage_root.join(rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &dst)?;
        }
    }
    Ok(())
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

fn create_unique_temp_dir(prefix: &str) -> io::Result<PathBuf> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    for _ in 0..64 {
        let seq = GALLERY_DEV_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = base.join(format!("{prefix}-{pid}-{seq}"));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                return Ok(strip_windows_verbatim_prefix(
                    candidate.canonicalize().unwrap_or(candidate),
                ));
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "exhausted unique gallery dev view directory names",
    ))
}

fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}
