use crate::error::HostError;
use crate::qmldir_sanitize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

include!(concat!(env!("OUT_DIR"), "/rinui_manifest.rs"));

/// Embedded QML import prefix for RinUI (intentionally distinct from `qrc:/qt/qml`
/// so development filesystem mode is never shadowed by embedded resources).
pub const EMBEDDED_IMPORT_PREFIX: &str = "qrc:/rinui";

/// Embedded QRC resource root for RinUI translation and asset lookups (`:/rinui/RinUI/...`).
pub const EMBEDDED_QRC_ROOT: &str = ":/rinui/RinUI";

static TEMP_VIEW_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Controls where the RinUI QML module, assets, and translations are loaded from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ResourceMode {
    /// Load bundled resources compiled into the binary via Qt RCC (`qrc:/rinui`).
    Embedded,
    /// Load QML, assets, and translations directly from a filesystem checkout containing `RinUI/qmldir`.
    Filesystem(PathBuf),
    /// Use `RINUI_DEV_PATH` if set and non-empty; otherwise fall back to `Embedded`.
    #[default]
    Auto,
}

/// Concrete resolved RinUI resource origin for an initialized host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedResources {
    Embedded,
    Filesystem {
        /// Parent directory of the source `RinUI/` checkout.
        import_root: PathBuf,
    },
}

impl ResolvedResources {
    /// Resolve `ResourceMode` into either `Embedded` or validated `Filesystem`.
    pub fn resolve(mode: &ResourceMode) -> Result<Self, HostError> {
        match mode {
            ResourceMode::Embedded => Ok(Self::Embedded),
            ResourceMode::Filesystem(path) => {
                let import_root = normalize_rinui_import_root(path)?;
                Ok(Self::Filesystem { import_root })
            }
            ResourceMode::Auto => {
                if let Some(dev_path) = std::env::var_os("RINUI_DEV_PATH") {
                    if !dev_path.is_empty() {
                        let path = PathBuf::from(dev_path);
                        let import_root = normalize_rinui_import_root(&path)?;
                        return Ok(Self::Filesystem { import_root });
                    }
                }
                Ok(Self::Embedded)
            }
        }
    }

    /// Path string passed to `QQmlApplicationEngine::add_import_path` for raw `ResolvedResources`.
    pub fn qml_import_path(&self) -> String {
        match self {
            Self::Embedded => EMBEDDED_IMPORT_PREFIX.to_owned(),
            Self::Filesystem { import_root } => import_root.to_string_lossy().into_owned(),
        }
    }

    /// Resolve a RinUI `.qm` file path for `locale` (`en_US`, `zh_CN`, etc.).
    pub fn rinui_qm_path(&self, locale: &str) -> String {
        match self {
            Self::Embedded => format!("{EMBEDDED_QRC_ROOT}/languages/{locale}.qm"),
            Self::Filesystem { import_root } => import_root
                .join("RinUI")
                .join("languages")
                .join(format!("{locale}.qm"))
                .to_string_lossy()
                .into_owned(),
        }
    }

    /// Human-readable description for diagnostics and logs.
    pub fn describe(&self) -> String {
        match self {
            Self::Embedded => format!("embedded ({EMBEDDED_IMPORT_PREFIX})"),
            Self::Filesystem { import_root } => {
                format!("filesystem ({})", import_root.display())
            }
        }
    }
}

#[derive(Debug)]
struct TempViewDir {
    path: PathBuf,
}

impl TempViewDir {
    fn create(prefix: &str) -> Result<Self, std::io::Error> {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..64 {
            let seq = TEMP_VIEW_COUNTER.fetch_add(1, Ordering::Relaxed);
            let candidate = base.join(format!("{prefix}-{pid}-{seq}"));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    let canonical = strip_windows_verbatim_prefix(
                        candidate.canonicalize().unwrap_or(candidate),
                    );
                    return Ok(Self { path: canonical });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "exhausted unique temporary directory names",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempViewDir {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %self.path.display(), error = %err, "Could not remove staged RinUI resources");
            }
        }
    }
}

/// Prepared runtime view of RinUI resources held for the lifetime of an engine/session.
///
/// In `Embedded` mode, points to compiled QRC resources (`qrc:/rinui`).
/// In `Filesystem` mode, materializes a fresh sanitized copy on disk (applying the exact
/// same `qmldir` sanitization rules as `build.rs` without touching the source checkout)
/// so QML singletons and canonical URLs resolve exclusively from a single unified load root.
/// The temporary directory stays alive until the last `ResourceView` clone drops after the
/// associated `QQmlApplicationEngine` and QML roots are destroyed.
#[derive(Debug, Clone)]
pub struct ResourceView {
    resolved: ResolvedResources,
    staged_dir: Option<Arc<TempViewDir>>,
    sanitized_drops: Vec<(String, String)>,
}

impl ResourceView {
    /// Resolve `mode` and prepare the runtime view (`Embedded` or freshly staged `Filesystem`).
    pub fn prepare(mode: &ResourceMode) -> Result<Self, HostError> {
        let resolved = ResolvedResources::resolve(mode)?;
        match &resolved {
            ResolvedResources::Embedded => Ok(Self {
                resolved,
                staged_dir: None,
                sanitized_drops: SANITIZED_QMLDIR_DROPS
                    .iter()
                    .map(|(q, l)| ((*q).to_owned(), (*l).to_owned()))
                    .collect(),
            }),
            ResolvedResources::Filesystem { import_root } => {
                let temp_dir = TempViewDir::create("rinui-dev-view").map_err(|err| {
                    HostError::ResourceStagingFailed {
                        path: import_root.clone(),
                        reason: err.to_string(),
                    }
                })?;
                let src_rinui = import_root.join("RinUI");
                let dst_rinui = temp_dir.path().join("RinUI");
                let (_files, drops) =
                    qmldir_sanitize::materialize_sanitized_rinui_tree(&src_rinui, &dst_rinui)
                        .map_err(|err| HostError::ResourceStagingFailed {
                            path: src_rinui,
                            reason: err.to_string(),
                        })?;

                Ok(Self {
                    resolved,
                    staged_dir: Some(Arc::new(temp_dir)),
                    sanitized_drops: drops,
                })
            }
        }
    }

    /// Underlying resolved resource origin (`Embedded` or `Filesystem` source checkout).
    pub fn resolved(&self) -> &ResolvedResources {
        &self.resolved
    }

    /// User-specified filesystem source root (parent of `RinUI/`), or `None` in `Embedded` mode.
    pub fn source_root(&self) -> Option<&Path> {
        match &self.resolved {
            ResolvedResources::Embedded => None,
            ResolvedResources::Filesystem { import_root } => Some(import_root.as_path()),
        }
    }

    /// Actual filesystem root added to `QQmlApplicationEngine`, or `None` in `Embedded` mode.
    pub fn load_root(&self) -> Option<&Path> {
        self.staged_dir.as_deref().map(TempViewDir::path)
    }

    /// Import path string passed to `QQmlApplicationEngine::add_import_path`.
    pub fn qml_import_path(&self) -> String {
        if let Some(staged) = &self.staged_dir {
            staged.path().to_string_lossy().into_owned()
        } else {
            EMBEDDED_IMPORT_PREFIX.to_owned()
        }
    }

    /// Resolve a RinUI `.qm` translation file path from the active runtime view.
    pub fn rinui_qm_path(&self, locale: &str) -> String {
        if let Some(staged) = &self.staged_dir {
            staged
                .path()
                .join("RinUI")
                .join("languages")
                .join(format!("{locale}.qm"))
                .to_string_lossy()
                .into_owned()
        } else {
            format!("{EMBEDDED_QRC_ROOT}/languages/{locale}.qm")
        }
    }

    /// Missing `qmldir` declarations removed during sanitization.
    pub fn sanitized_drops(&self) -> &[(String, String)] {
        &self.sanitized_drops
    }

    /// Human-readable description showing both source and actual load root in filesystem mode.
    pub fn describe(&self) -> String {
        match (&self.resolved, &self.staged_dir) {
            (ResolvedResources::Embedded, _) => format!("embedded ({EMBEDDED_IMPORT_PREFIX})"),
            (ResolvedResources::Filesystem { import_root }, Some(staged)) => {
                format!(
                    "filesystem (source: {}, load: {})",
                    import_root.display(),
                    staged.path().display()
                )
            }
            (ResolvedResources::Filesystem { import_root }, None) => {
                format!("filesystem ({})", import_root.display())
            }
        }
    }
}

pub(crate) fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
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

fn normalize_rinui_import_root(path: &Path) -> Result<PathBuf, HostError> {
    if path.join("RinUI").join("qmldir").is_file() {
        let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        return Ok(strip_windows_verbatim_prefix(resolved));
    }
    if path.file_name().and_then(|n| n.to_str()) == Some("RinUI") && path.join("qmldir").is_file() {
        if let Some(parent) = path.parent() {
            let resolved = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            return Ok(strip_windows_verbatim_prefix(resolved));
        }
    }
    Err(HostError::InvalidResourcePath {
        path: path.to_path_buf(),
    })
}
