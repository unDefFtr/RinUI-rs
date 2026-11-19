use rinui_platform::ConfigError;
use std::path::PathBuf;
use thiserror::Error;

/// Errors returned by `rinui-host` initialization, QML loading, and runtime operations.
#[derive(Debug, Error)]
pub enum HostError {
    /// Failed to create Qt's `QGuiApplication`.
    #[error("failed to create QGuiApplication")]
    ApplicationInitFailed,

    /// Another `RinUIHost` or `QGuiApplication` is already active in this process.
    #[error("a RinUIHost or QGuiApplication instance is already active in this process")]
    ApplicationAlreadyInitialized,

    /// Re-entrant host operation attempted while another mutation/load is in progress.
    #[error("re-entrant call to {0} is not allowed")]
    ReentrantCall(&'static str),

    /// Failed to create Qt's `QQmlApplicationEngine`.
    #[error("failed to create QQmlApplicationEngine")]
    EngineInitFailed,

    /// Failed to construct `ThemeManager` QObject.
    #[error("failed to construct ThemeManager QObject")]
    ThemeManagerInitFailed,

    /// Invalid or missing filesystem resource directory for RinUI.
    #[error("invalid RinUI resource directory '{path}': missing RinUI/qmldir")]
    InvalidResourcePath { path: PathBuf },

    /// Failed to load a QML file or URL.
    #[error("failed to load QML source '{source_desc}': {reason}")]
    QmlLoadFailed {
        source_desc: String,
        reason: String,
        warnings: Vec<String>,
    },

    /// Configuration persistence or validation error.
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Attempted to use a `HostHandle` after `RinUIHost` was dropped.
    #[error("RinUIHost has already been shut down")]
    HostDisposed,

    /// Attempted to expose a null `QObject` pointer or null `UniquePtr`.
    #[error("cannot register null QObject context property '{0}'")]
    NullQObject(String),

    /// Translation file could not be loaded (and no fallback succeeded).
    #[error("failed to load translation '{path}'")]
    TranslationLoadFailed { path: String },
}
