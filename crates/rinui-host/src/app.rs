use crate::error::HostError;
use crate::resources::{ResolvedResources, ResourceMode, ResourceView};
use crate::runtime::{
    ContextPropertyConflict, EngineConfigurator, EngineMode, RinUIRuntime, RinUISession,
    RuntimeConfig, SessionConfig, ThemeManagerMut, ThemeManagerRef,
};
pub use crate::runtime::{QmlSource, SessionHandle};
use crate::theme::ThemeManager;
use crate::translator::{TranslationCatalog, TranslationLoadResult};
use crate::window::{LoadedRoot, MacWindowOptions, NativeWindowRef, RootId, WindowCustomizer};
use cxx::memory::UniquePtrTarget;
use cxx_qt::casting::Upcast;
use cxx_qt_lib::QVariant;
use rinui_platform::{AppIdentity, ConfigLocation, ConfigStore};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

/// Configuration for creating a standalone [`RinUIHost`].
#[derive(Debug, Clone)]
pub struct HostConfig {
    pub identity: AppIdentity,
    pub resource_mode: ResourceMode,
    pub config_location: ConfigLocation,
    pub locale: Option<String>,
    pub mac_window: MacWindowOptions,
    pub qt_quick_controls_style: String,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            identity: AppIdentity::new("RinUI", "Application"),
            resource_mode: ResourceMode::Auto,
            config_location: ConfigLocation::Standard,
            locale: None,
            mac_window: MacWindowOptions::default(),
            qt_quick_controls_style: "Basic".to_owned(),
        }
    }
}

impl HostConfig {
    /// Override macOS titlebar integration policy (chainable builder helper).
    pub fn with_mac_window(mut self, mac_window: MacWindowOptions) -> Self {
        self.mac_window = mac_window;
        self
    }

    /// Opt into upstream QML's native macOS titlebar framing on macOS.
    pub fn with_upstream_mac_frame(self) -> Self {
        self.with_mac_window(MacWindowOptions::upstream_compatible())
    }
}

impl From<HostConfig> for RuntimeConfig {
    fn from(config: HostConfig) -> Self {
        Self {
            identity: config.identity,
            resource_mode: config.resource_mode,
            config_location: config.config_location,
            locale: config.locale,
            mac_window: config.mac_window,
            qt_quick_controls_style: config.qt_quick_controls_style,
            apply_identity_to_borrowed_app: false,
        }
    }
}

/// Main-thread handle allowing Rust-owned `QObject` backends to interact safely with [`RinUIHost`].
///
/// `!Send` and `!Sync`: cannot be moved across threads. Once [`RinUIHost`] drops,
/// `HostHandle` marks itself disposed and returns [`HostError::HostDisposed`] instead of
/// dereferencing dangling Qt pointers.
pub type HostHandle = SessionHandle<'static>;

/// Complete standalone Qt 6 / QML application host for RinUI.
///
/// Internally composes a default [`RinUISession<'static>`] over an owned process-level
/// [`RinUIRuntime<'static>`]. Field declaration order (`session` -> `runtime`) guarantees the
/// QML session, roots, and engine drop BEFORE process-wide translators, `ThemeManager`, and
/// `QGuiApplication`.
///
/// `!Send` and `!Sync`: must live and be dropped on the Qt main thread.
pub struct RinUIHost {
    session: RinUISession<'static>,
    runtime: RinUIRuntime<'static>,
    resource_view: ResourceView,
}

impl RinUIHost {
    /// Initialize standalone `QGuiApplication`, HiDPI scaling, Qt Quick style, `ConfigStore`,
    /// `ThemeManager`, `TranslationManager`, and `QQmlApplicationEngine`.
    pub fn new(config: HostConfig) -> Result<Self, HostError> {
        let runtime = RinUIRuntime::new(config.into())?;
        let session = runtime.create_session(SessionConfig {
            engine_mode: EngineMode::Shared,
            resource_mode: None,
            mac_window: None,
            context_property_conflict: ContextPropertyConflict::Overwrite,
        })?;
        let resource_view = session.resource_view()?;
        Ok(Self {
            session,
            runtime,
            resource_view,
        })
    }

    /// Borrow the underlying process-level [`RinUIRuntime`].
    pub fn runtime(&self) -> &RinUIRuntime<'static> {
        &self.runtime
    }

    /// Borrow the default [`RinUISession`].
    pub fn session(&self) -> &RinUISession<'static> {
        &self.session
    }

    /// Mutably borrow the default [`RinUISession`].
    pub fn session_mut(&mut self) -> &mut RinUISession<'static> {
        &mut self.session
    }

    /// Obtain a cloneable main-thread [`HostHandle`] to pass into application `QObject` backends.
    pub fn handle(&self) -> HostHandle {
        self.session.handle()
    }

    /// Attach a custom [`WindowCustomizer`] notified whenever RinUI windows are discovered.
    pub fn set_window_customizer(&mut self, customizer: Box<dyn WindowCustomizer>) {
        self.session.set_window_customizer(customizer);
    }

    /// Add an additional QML import path (e.g. an application's custom QML modules).
    pub fn add_import_path(&mut self, path: &str) {
        let _ = self.session.add_import_path(path);
    }

    /// Current QML engine import search paths.
    pub fn import_paths(&self) -> Vec<String> {
        self.session.import_paths().unwrap_or_default()
    }

    /// Register an application-owned Rust/CXX-Qt `QObject` as a named QML root context property.
    ///
    /// Ownership transfers to the underlying engine, ensuring the `QObject` outlives
    /// `QQmlApplicationEngine` and is destroyed safely on the GUI thread before `QGuiApplication`.
    pub fn expose_qobject<T>(
        &mut self,
        name: &str,
        object: cxx::UniquePtr<T>,
    ) -> Result<(), HostError>
    where
        T: Upcast<cxx_qt::QObject> + UniquePtrTarget + 'static,
    {
        self.session.expose_qobject(name, object)
    }

    /// Expose a string value directly on `QQmlApplicationEngine::rootContext()`.
    pub fn set_context_property_string(
        &mut self,
        name: &str,
        value: &str,
    ) -> Result<(), HostError> {
        self.session.set_context_property_string(name, value)
    }

    /// Expose a `QVariant` directly on `QQmlApplicationEngine::rootContext()`.
    pub fn set_context_property_variant(
        &mut self,
        name: &str,
        value: &QVariant,
    ) -> Result<(), HostError> {
        self.session.set_context_property_variant(name, value)
    }

    /// Register a consumer application translation catalog and immediately apply the current locale.
    pub fn register_translation_catalog(
        &mut self,
        catalog: TranslationCatalog,
    ) -> Result<TranslationLoadResult, HostError> {
        self.session.register_translation_catalog(catalog)
    }

    /// Switch language at runtime (`"en_US"`, `"zh_CN"`, etc.) and retranslate live QML bindings.
    pub fn set_language(&mut self, locale: &str) -> Result<TranslationLoadResult, HostError> {
        self.session.set_language(locale)
    }

    /// Load a QML document, register discovered RinUI windows, and apply native platform hooks.
    pub fn load(&mut self, source: &QmlSource) -> Result<LoadedRoot, HostError> {
        self.session.load(source)
    }

    /// Set the application window icon and update the given root window's `icon` property.
    pub fn set_icon(&mut self, icon_path: &str, root_id: Option<RootId>) {
        let _ = self.session.set_icon(icon_path, root_id);
    }

    /// Set a string property on a loaded root QML object (e.g. `"title"`).
    pub fn set_root_string_property(&mut self, root_id: RootId, name: &str, value: &str) -> bool {
        self.session
            .set_root_string_property(root_id, name, value)
            .unwrap_or(false)
    }

    /// Write a declared meta-property on a loaded root QML object with strict type checking.
    pub fn set_root_property(
        &self,
        root_id: RootId,
        name: &str,
        value: &QVariant,
    ) -> Result<(), HostError> {
        self.session.set_root_property(root_id, name, value)
    }

    /// Read a property from a loaded root QML object (returns default `QVariant` if missing).
    pub fn root_property(&self, root_id: RootId, name: &str) -> QVariant {
        self.session
            .root_property(root_id, name)
            .unwrap_or_default()
    }

    /// Read a property from a loaded root QML object with strict existence & ownership checks.
    pub fn get_root_property(&self, root_id: RootId, name: &str) -> Result<QVariant, HostError> {
        self.session.get_root_property(root_id, name)
    }

    /// Synchronously invoke a QML method or slot on a loaded root object via Qt's meta-object system.
    pub fn invoke_root_method(
        &self,
        root_id: RootId,
        method_name: &str,
        args: &[QVariant],
    ) -> Result<QVariant, HostError> {
        self.session.invoke_root_method(root_id, method_name, args)
    }

    /// Scoped main-thread configuration access to the underlying `QQmlApplicationEngine`.
    pub fn configure_engine<R>(
        &mut self,
        f: impl FnOnce(&mut EngineConfigurator<'_>) -> R,
    ) -> Result<R, HostError> {
        self.session.configure_engine(f)
    }

    /// Release a loaded root object tree (for transient secondary windows).
    pub fn release_root(&mut self, root_id: RootId) {
        let _ = self.session.release_root(root_id);
    }

    /// Check whether a root object handle still points to a live Qt object.
    pub fn is_root_alive(&self, root_id: RootId) -> bool {
        self.session.is_root_alive(root_id)
    }

    /// Synchronize any top-level RinUI windows opened directly from QML.
    pub fn sync_windows(&mut self) -> Result<Vec<NativeWindowRef>, HostError> {
        self.session.sync_windows()
    }

    /// Explicitly attribute and register a dynamic `QQuickWindow` referenced by `property_name`
    /// on a loaded root object.
    pub fn register_window_from_property(
        &mut self,
        root_id: RootId,
        property_name: &str,
    ) -> Result<NativeWindowRef, HostError> {
        self.session
            .register_window_from_property(root_id, property_name)
    }

    /// Acquire a dynamically tracked shared read guard to [`ThemeManager`].
    ///
    /// Returns [`HostError::ReentrantCall`] if a mutable borrow or FFI mutation is active.
    pub fn theme_manager(&self) -> Result<ThemeManagerRef<'_>, HostError> {
        self.runtime.theme_manager()
    }

    /// Acquire a dynamically tracked exclusive write guard to [`ThemeManager`].
    ///
    /// Returns [`HostError::ReentrantCall`] if any read or write borrow of `ThemeManager` is active.
    pub fn theme_manager_mut(&mut self) -> Result<ThemeManagerMut<'_>, HostError> {
        self.runtime.theme_manager_mut()
    }

    /// Execute a closure with a dynamically checked shared reference to `ThemeManager`.
    pub fn with_theme_manager<R>(
        &self,
        f: impl FnOnce(&ThemeManager) -> R,
    ) -> Result<R, HostError> {
        self.runtime.with_theme_manager(f)
    }

    /// Execute a closure with a dynamically checked exclusive pinned mutable reference to `ThemeManager`.
    pub fn with_theme_manager_mut<R>(
        &self,
        f: impl FnOnce(Pin<&mut ThemeManager>) -> R,
    ) -> Result<R, HostError> {
        self.runtime.with_theme_manager_mut(f)
    }

    /// Borrow shared RinUI `ConfigStore`.
    pub fn config_store(&self) -> &Rc<RefCell<ConfigStore>> {
        self.session.config_store()
    }

    /// Resolved RinUI resources (`Embedded` or `Filesystem`).
    pub fn resources(&self) -> ResolvedResources {
        self.resource_view.resolved().clone()
    }

    /// Prepared RinUI resource view (distinguishing source root from actual load root).
    pub fn resource_view(&self) -> &ResourceView {
        &self.resource_view
    }

    /// QML warnings emitted since engine creation or last clear.
    pub fn qml_warnings(&self) -> Vec<String> {
        self.session.qml_warnings()
    }

    /// Process pending Qt GUI events (useful in deterministic tests).
    pub fn process_events(&self) {
        self.runtime.process_events();
    }

    /// Run Qt's main event loop until application exit and persist configuration before returning.
    pub fn exec(&mut self) -> i32 {
        self.runtime.exec().unwrap_or(-1)
    }
}
