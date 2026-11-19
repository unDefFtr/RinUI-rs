use crate::error::HostError;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/rinui_manifest.rs"));

/// Embedded QML import prefix for RinUI (intentionally distinct from `qrc:/qt/qml`
/// so development filesystem mode is never shadowed by embedded resources).
pub const EMBEDDED_IMPORT_PREFIX: &str = "qrc:/rinui";

/// Embedded QRC resource root for RinUI translation and asset lookups (`:/rinui/RinUI/...`).
pub const EMBEDDED_QRC_ROOT: &str = ":/rinui/RinUI";

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
        /// Parent directory added to `QQmlEngine::addImportPath` (contains `RinUI/`).
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

    /// Path string passed to `QQmlApplicationEngine::add_import_path`.
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
