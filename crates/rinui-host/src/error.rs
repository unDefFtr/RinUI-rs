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

    /// Failed to prepare sanitized runtime resource view for filesystem mode.
    #[error("failed to prepare RinUI resource view from '{path}': {reason}")]
    ResourceStagingFailed { path: PathBuf, reason: String },

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

    /// Attempted to attach to an existing Qt application when none exists in the process.
    #[error("no Qt application instance exists in the current process")]
    NoGuiApplication,

    /// Active Qt application is a headless `QCoreApplication`, not a `QGuiApplication`/`QApplication`.
    #[error("active Qt application is not a QGuiApplication (cannot host QML GUI windows on QCoreApplication)")]
    NotGuiApplication,

    /// Caller invoked `rinui-host` from a non-GUI thread.
    #[error(
        "Qt operation attempted from a non-GUI thread; RinUI runtime/sessions are main-thread only"
    )]
    WrongThread,

    /// Supplied Qt application pointer or reference does not match `QCoreApplication::instance()`.
    #[error("supplied application instance does not match active QCoreApplication::instance()")]
    ApplicationMismatch,

    /// Nested `exec()` is not allowed when borrowing an externally owned `QGuiApplication`.
    #[error(
        "cannot call exec() on a borrowed Qt application; drive the owner's event loop instead"
    )]
    BorrowedApplicationExecNotAllowed,

    /// Qt Quick Controls style was already initialized with a conflicting style.
    #[error("incompatible Qt Quick Controls style requested '{requested}': active style '{active}' cannot be replaced after Quick Controls initialization")]
    IncompatibleQuickControlsStyle { requested: String, active: String },

    /// Context property collision rejected on a shared (or configured) engine.
    #[error("context property '{name}' is already registered on this engine")]
    ContextPropertyConflict { name: String },

    /// Root handle is stale or belongs to another session.
    #[error("root object {0} is disposed or does not belong to this session")]
    RootNotOwned(u64),

    /// Window handle or target window belongs to another session.
    #[error("window {0} is owned by another session")]
    WindowNotOwned(u64),

    /// Explicit window registration failed (missing/null property, non-window object, or wrong engine).
    #[error("failed to register window from property '{property}': {detail}")]
    WindowRegistrationFailed { property: String, detail: String },

    /// Target root object does not declare the requested meta-property.
    #[error("property '{property}' was not found on QML root object")]
    PropertyNotFound { property: String },

    /// Target root meta-property is read-only.
    #[error("property '{property}' is read-only on QML root object")]
    PropertyReadOnly { property: String },

    /// Value variant type is incompatible with target root meta-property.
    #[error("type mismatch assigning property '{property}': {detail}")]
    PropertyTypeMismatch { property: String, detail: String },

    /// Requested method/slot was not found on root object.
    #[error("method '{method}' was not found on QML root object")]
    MethodNotFound { method: String },

    /// Invocation argument count does not match QML method/slot arity.
    #[error("argument count mismatch calling method '{method}': {detail}")]
    MethodArgumentCountMismatch { method: String, detail: String },

    /// Method parameter or return type is not supported by checked QVariant meta-object invocation.
    #[error("unsupported signature for method '{method}': {detail}")]
    UnsupportedMethodSignature { method: String, detail: String },

    /// `QMetaMethod::invoke` reported failure.
    #[error("failed to invoke method '{method}': {detail}")]
    MethodInvocationFailed { method: String, detail: String },

    /// Exceeded maximum supported meta-object invocation arguments (10).
    #[error("too many arguments calling method '{method}': got {count}, maximum supported is 10")]
    TooManyMethodArguments { method: String, count: usize },
}
