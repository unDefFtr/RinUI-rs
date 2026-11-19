use std::path::PathBuf;
use thiserror::Error;

/// Errors produced by RinUI configuration loading, validation, and persistence.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Underlying filesystem I/O failure.
    #[error("configuration I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON serialization failure.
    #[error("failed to serialize configuration: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid or non-writable configuration path.
    #[error("invalid configuration path: {0}")]
    InvalidPath(String),

    /// Invalid theme string provided.
    #[error("invalid theme mode '{0}'; expected 'Auto', 'Light', or 'Dark'")]
    InvalidTheme(String),

    /// Invalid backdrop string provided.
    #[error("invalid backdrop effect '{0}'; expected 'none', 'acrylic', 'mica', or 'tabbed'")]
    InvalidBackdrop(String),
}
