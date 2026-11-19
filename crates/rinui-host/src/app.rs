use crate::bridge::ffi as qt_bridge;
use crate::error::HostError;
use crate::resources::{strip_windows_verbatim_prefix, ResolvedResources, ResourceMode};
use crate::theme::{qobject as theme_qobject, ThemeManager};
use crate::translator::{TranslationCatalog, TranslationLoadResult, TranslationManager};
use crate::window::{
    LoadedRoot, MacWindowOptions, NativeWindowRef, RootId, WindowCustomizer, WindowId,
};
use cxx::memory::UniquePtrTarget;
use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QStringList, QUrl, QVariant};
use rinui_platform::{
    AppIdentity, ConfigLoadReport, ConfigLocation, ConfigStore, PlatformCapabilities,
};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use tracing::{debug, info, warn};

static HOST_ACTIVE: AtomicBool = AtomicBool::new(false);
static INIT_CRATE_ONCE: Once = Once::new();

struct HostActiveGuard;

impl Drop for HostActiveGuard {
    fn drop(&mut self) {
        HOST_ACTIVE.store(false, Ordering::Release);
    }
}

struct InitCleanupGuard {
    armed: bool,
}

impl InitCleanupGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for InitCleanupGuard {
    fn drop(&mut self) {
        if self.armed {
            qt_bridge::rinui_release_all_roots_and_platform();
            qt_bridge::rinui_flush_deferred_deletes();
            qt_bridge::rinui_translator_clear_all();
        }
    }
}

struct LoadingGuard(Rc<Cell<bool>>);

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Configuration for creating a [`RinUIHost`].
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

/// Input QML document passed to [`RinUIHost::load`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QmlSource {
    /// Explicit QML URL (`"qrc:/..."`, `"file:///..."`, etc.).
    Url(String),
    /// Local filesystem `.qml` file path.
    File(PathBuf),
}

impl QmlSource {
    pub fn to_qurl(&self) -> QUrl {
        match self {
            Self::Url(u) => QUrl::from(u.as_str()),
            Self::File(path) => {
                let abs = strip_windows_verbatim_prefix(
                    path.canonicalize().unwrap_or_else(|_| path.clone()),
                );
                QUrl::from_local_file(&QString::from(abs.to_string_lossy().as_ref()))
            }
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Url(u) => u.clone(),
            Self::File(p) => p.display().to_string(),
        }
    }
}

struct HostSharedInner {
    engine_ptr: *mut QQmlApplicationEngine,
    theme_manager_ptr: *mut ThemeManager,
    store: Rc<RefCell<ConfigStore>>,
    translator: TranslationManager,
    resources: ResolvedResources,
    capabilities: PlatformCapabilities,
    mac_window: MacWindowOptions,
    secondary_roots: Vec<RootId>,
    loading_qml: Rc<Cell<bool>>,
}

/// Main-thread handle allowing Rust-owned `QObject` backends to interact safely with [`RinUIHost`].
///
/// `!Send` and `!Sync`: cannot be moved across threads. Once [`RinUIHost`] drops,
/// `HostHandle` marks itself disposed and returns [`HostError::HostDisposed`] instead of
/// dereferencing dangling Qt pointers.
#[derive(Clone)]
pub struct HostHandle {
    inner: Rc<RefCell<Option<HostSharedInner>>>,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl HostHandle {
    /// Switch active Qt locale across RinUI + consumer catalogs and retranslate QML live.
    pub fn set_language(&self, locale: &str) -> Result<TranslationLoadResult, HostError> {
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("set_language"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        let engine_ptr = state.engine_ptr;
        // Note: `retranslate()` can trigger QML bindings, so drop `guard` before calling retranslate.
        let result = state.translator.load_locale(locale, None)?;
        drop(guard);

        let alive = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("set_language"))?;
        if alive.is_none() {
            return Err(HostError::HostDisposed);
        }
        drop(alive);
        unsafe {
            let engine_pin = Pin::new_unchecked(&mut *engine_ptr);
            qt_bridge::rinui_engine_retranslate(engine_pin);
        }
        Ok(result)
    }

    /// Currently active translation locale string.
    pub fn current_locale(&self) -> Result<String, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("current_locale"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        Ok(state.translator.current_locale().to_owned())
    }

    /// System locale name detected by Qt.
    pub fn system_locale(&self) -> String {
        TranslationManager::system_locale()
    }

    /// Register a consumer translation catalog and immediately load it for the active locale.
    pub fn register_translation_catalog(
        &self,
        catalog: TranslationCatalog,
    ) -> Result<TranslationLoadResult, HostError> {
        let (res, engine_ptr) = {
            let mut guard = self
                .inner
                .try_borrow_mut()
                .map_err(|_| HostError::ReentrantCall("register_translation_catalog"))?;
            let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
            state.translator.register_catalog(catalog);
            let locale = state.translator.current_locale().to_owned();
            let res = state.translator.load_locale(&locale, None)?;
            (res, state.engine_ptr)
        };
        let alive = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("register_translation_catalog"))?;
        if alive.is_none() {
            return Err(HostError::HostDisposed);
        }
        drop(alive);
        unsafe {
            let engine_pin = Pin::new_unchecked(&mut *engine_ptr);
            qt_bridge::rinui_engine_retranslate(engine_pin);
        }
        Ok(res)
    }

    /// Load an additional QML root/window on the shared engine and track it as a secondary root.
    pub fn load_secondary_window(&self, source: &QmlSource) -> Result<LoadedRoot, HostError> {
        let (engine_ptr, mac_window, resources_desc, theme_ptr, loading_flag) = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("load_secondary_window"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            (
                state.engine_ptr,
                state.mac_window,
                state.resources.describe(),
                state.theme_manager_ptr,
                Rc::clone(&state.loading_qml),
            )
        };

        if loading_flag.get() {
            return Err(HostError::ReentrantCall("load_secondary_window"));
        }
        loading_flag.set(true);
        let _loading_guard = LoadingGuard(loading_flag);

        let loaded = load_qml_on_engine(
            unsafe { Pin::new_unchecked(&mut *engine_ptr) },
            source,
            &resources_desc,
            mac_window,
            unsafe { Pin::new_unchecked(&mut *theme_ptr) },
        )?;

        if let Ok(mut guard) = self.inner.try_borrow_mut() {
            if let Some(state) = guard.as_mut() {
                state.secondary_roots.push(loaded.root_id);
            }
        }
        Ok(loaded)
    }

    /// Release a specific root object tree (hides window, detaches native hooks, calls `deleteLater`).
    pub fn release_root(&self, root_id: RootId) -> Result<(), HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("release_root"))?;
        if guard.is_none() {
            return Err(HostError::HostDisposed);
        }
        drop(guard);

        qt_bridge::rinui_root_release(root_id.0);
        if let Ok(mut guard) = self.inner.try_borrow_mut() {
            if let Some(state) = guard.as_mut() {
                state.secondary_roots.retain(|id| *id != root_id);
            }
        }
        Ok(())
    }

    /// Close and release all secondary roots opened via [`Self::load_secondary_window`].
    pub fn close_secondary_windows(&self) -> usize {
        let ids = {
            let Ok(mut guard) = self.inner.try_borrow_mut() else {
                return 0;
            };
            let Some(state) = guard.as_mut() else {
                return 0;
            };
            std::mem::take(&mut state.secondary_roots)
        };
        let count = ids.len();
        for root_id in ids {
            qt_bridge::rinui_root_release(root_id.0);
        }
        count
    }

    /// Number of currently live RinUI windows tracked across the process.
    pub fn open_window_count(&self) -> usize {
        let Ok(guard) = self.inner.try_borrow() else {
            return 0;
        };
        if guard.is_none() {
            return 0;
        }
        qt_bridge::rinui_all_alive_windows().len()
    }

    /// Synchronize any top-level RinUI windows created dynamically inside QML.
    pub fn sync_windows(&self) -> Result<Vec<NativeWindowRef>, HostError> {
        let (mac_window, theme_ptr) = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("sync_windows"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            (state.mac_window, state.theme_manager_ptr)
        };

        let ids = qt_bridge::rinui_sync_top_level_windows(
            mac_window.enabled,
            mac_window.traffic_lights_offset_x,
            mac_window.traffic_lights_offset_down,
        );
        unsafe {
            (&*theme_ptr).sync_platform_appearance();
        }
        Ok(ids
            .into_iter()
            .map(|id| NativeWindowRef::new(WindowId(id)))
            .collect())
    }

    pub fn platform_name(&self) -> String {
        qt_bridge::rinui_platform_name().to_string()
    }

    pub fn resource_mode_description(&self) -> Result<String, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("resource_mode_description"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        Ok(state.resources.describe())
    }

    pub fn config_path(&self) -> Result<Option<PathBuf>, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("config_path"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        let store = state
            .store
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("config_path"))?;
        let path = store.path().map(Path::to_path_buf);
        Ok(path)
    }

    pub fn is_backdrop_supported(&self) -> Result<bool, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("is_backdrop_supported"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        Ok(state.capabilities.is_windows())
    }

    pub fn current_theme_name(&self) -> Result<String, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("current_theme_name"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        let store = state
            .store
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("current_theme_name"))?;
        let name = store.theme().as_str().to_owned();
        Ok(name)
    }

    pub fn actual_theme(&self) -> Result<String, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("actual_theme"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        let actual = unsafe { (&*state.theme_manager_ptr).resolved_theme() };
        Ok(actual.as_str().to_owned())
    }

    pub fn copy_to_clipboard(&self, text: &str) {
        qt_bridge::rinui_set_clipboard_text(&QString::from(text));
    }

    pub fn schedule_quit(&self, delay_ms: i32) {
        qt_bridge::rinui_schedule_quit_after_ms(delay_ms);
    }
}

/// Complete Qt 6 / QML application host for RinUI.
///
/// Field declaration order enforces safe C++/Qt teardown order on drop:
/// `roots` & platform hooks (in `Drop`) -> `engine` -> `owned_context_objects` -> `theme_manager` -> `store` -> `app` -> `_active_guard`.
/// `!Send` and `!Sync`: must live and be dropped on the Qt main thread.
pub struct RinUIHost {
    roots: Vec<RootId>,
    customizer: Option<Box<dyn WindowCustomizer>>,
    shared: Rc<RefCell<Option<HostSharedInner>>>,
    loading_qml: Rc<Cell<bool>>,
    // Dropped BEFORE owned_context_objects, theme_manager, and app:
    engine: cxx::UniquePtr<QQmlApplicationEngine>,
    owned_context_objects: Vec<Box<dyn Any>>,
    theme_manager: cxx::UniquePtr<ThemeManager>,
    store: Rc<RefCell<ConfigStore>>,
    app: cxx::UniquePtr<QGuiApplication>,
    _active_guard: HostActiveGuard,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl RinUIHost {
    /// Initialize `QGuiApplication`, HiDPI scaling, Qt Quick style, `ConfigStore`,
    /// `ThemeManager`, `TranslationManager`, and `QQmlApplicationEngine`.
    pub fn new(config: HostConfig) -> Result<Self, HostError> {
        // Validate resource paths before claiming the process-wide QGuiApplication slot.
        let resources = ResolvedResources::resolve(&config.resource_mode)?;

        if HOST_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(HostError::ApplicationAlreadyInitialized);
        }
        let active_guard = HostActiveGuard;

        if qt_bridge::rinui_has_gui_application() {
            return Err(HostError::ApplicationAlreadyInitialized);
        }

        // Ensure CXX-Qt internal QML/type registration for rinui-host is initialized once.
        INIT_CRATE_ONCE.call_once(|| {
            cxx_qt::init_crate!(rinui_host);
        });

        // Configure HiDPI PassThrough before QGuiApplication is instantiated.
        qt_bridge::rinui_set_high_dpi_pass_through();

        let mut app = QGuiApplication::new();
        if app.is_null() {
            return Err(HostError::ApplicationInitFailed);
        }

        let mut cleanup_guard = InitCleanupGuard { armed: true };

        if let Some(mut app_pin) = app.as_mut() {
            app_pin
                .as_mut()
                .set_organization_name(&QString::from(config.identity.organization_name.as_str()));
            if !config.identity.organization_domain.is_empty() {
                app_pin.as_mut().set_organization_domain(&QString::from(
                    config.identity.organization_domain.as_str(),
                ));
            }
            app_pin
                .as_mut()
                .set_application_name(&QString::from(config.identity.application_name.as_str()));
            if !config.identity.application_version.is_empty() {
                app_pin.as_mut().set_application_version(&QString::from(
                    config.identity.application_version.as_str(),
                ));
            }
        }

        if !config.qt_quick_controls_style.is_empty() {
            qt_bridge::rinui_set_quick_controls_style(&QString::from(
                config.qt_quick_controls_style.as_str(),
            ));
        }

        let capabilities = PlatformCapabilities::current();

        let qt_local_data = qt_bridge::rinui_standard_app_local_data_dir().to_string();
        let qt_local_path = (!qt_local_data.is_empty()).then(|| PathBuf::from(&qt_local_data));

        let (store_val, load_report) = ConfigStore::load_or_default(
            &config.config_location,
            &config.identity,
            qt_local_path.as_deref(),
            capabilities,
        )?;
        match &load_report {
            ConfigLoadReport::RecoveredFromCorruption {
                backup_path,
                reason,
            } => {
                warn!(
                    reason = %reason,
                    backup = ?backup_path,
                    "Recovered RinUI config from corrupted JSON"
                );
            }
            other => {
                debug!(report = ?other, path = ?store_val.path(), "RinUI config initialized");
            }
        }
        let store = Rc::new(RefCell::new(store_val));

        // Construct ThemeManager BEFORE QQmlApplicationEngine so reverse local drop
        // always destroys engine before ThemeManager if initialization fails later.
        let mut theme_manager = theme_qobject::rinui_theme_manager_new();
        if theme_manager.is_null() {
            return Err(HostError::ThemeManagerInitFailed);
        }

        let mut engine = QQmlApplicationEngine::new();
        if engine.is_null() {
            return Err(HostError::EngineInitFailed);
        }

        if let Some(mut eng_pin) = engine.as_mut() {
            qt_bridge::rinui_engine_install_warning_collector(eng_pin.as_mut());
            eng_pin
                .as_mut()
                .add_import_path(&QString::from(resources.qml_import_path().as_str()));
            qt_bridge::rinui_platform_register_engine_objects(eng_pin.as_mut());
        }

        if let Some(mut tm_pin) = theme_manager.as_mut() {
            tm_pin.as_mut().attach_store(Rc::clone(&store));
            theme_qobject::rinui_connect_color_scheme_changed(tm_pin.as_mut());

            let tm_qobj: Pin<&mut cxx_qt::QObject> = tm_pin.as_mut().upcast_pin();
            if let Some(eng_pin) = engine.as_mut() {
                unsafe {
                    qt_bridge::rinui_engine_set_context_property_object(
                        eng_pin,
                        &QString::from("ThemeManager"),
                        tm_qobj.get_unchecked_mut() as *mut cxx_qt::QObject,
                    );
                }
            }
        }

        let mut translator = TranslationManager::new(resources.clone());
        let initial_locale = config
            .locale
            .clone()
            .unwrap_or_else(TranslationManager::system_locale);
        let tr_res = translator.load_locale(&initial_locale, None)?;
        debug!(
            requested = %tr_res.requested_locale,
            effective = %tr_res.effective_locale,
            fallback = tr_res.used_fallback,
            "Initial RinUI translation loaded"
        );

        let engine_ptr = engine
            .as_mut()
            .map(|p| unsafe { p.get_unchecked_mut() as *mut QQmlApplicationEngine })
            .ok_or(HostError::EngineInitFailed)?;
        let theme_manager_ptr = theme_manager
            .as_mut()
            .map(|p| unsafe { p.get_unchecked_mut() as *mut ThemeManager })
            .ok_or(HostError::ThemeManagerInitFailed)?;

        let loading_qml = Rc::new(Cell::new(false));
        let shared = Rc::new(RefCell::new(Some(HostSharedInner {
            engine_ptr,
            theme_manager_ptr,
            store: Rc::clone(&store),
            translator,
            resources,
            capabilities,
            mac_window: config.mac_window,
            secondary_roots: Vec::new(),
            loading_qml: Rc::clone(&loading_qml),
        })));

        cleanup_guard.disarm();

        info!(
            org = %config.identity.organization_name,
            app = %config.identity.application_name,
            platform = %qt_bridge::rinui_platform_name().to_string(),
            "RinUIHost initialized"
        );

        Ok(Self {
            roots: Vec::new(),
            customizer: None,
            shared,
            loading_qml,
            engine,
            owned_context_objects: Vec::new(),
            theme_manager,
            store,
            app,
            _active_guard: active_guard,
            _not_send_sync: PhantomData,
        })
    }

    /// Obtain a cloneable main-thread [`HostHandle`] to pass into application `QObject` backends.
    pub fn handle(&self) -> HostHandle {
        HostHandle {
            inner: Rc::clone(&self.shared),
            _not_send_sync: PhantomData,
        }
    }

    /// Attach a custom [`WindowCustomizer`] notified whenever RinUI windows are discovered.
    pub fn set_window_customizer(&mut self, customizer: Box<dyn WindowCustomizer>) {
        self.customizer = Some(customizer);
    }

    /// Add an additional QML import path (e.g. an application's custom QML modules).
    pub fn add_import_path(&mut self, path: &str) {
        if let Some(eng) = self.engine.as_mut() {
            eng.add_import_path(&QString::from(path));
        }
    }

    /// Current QML engine import search paths.
    pub fn import_paths(&self) -> Vec<String> {
        let Some(eng) = self.engine.as_ref() else {
            return Vec::new();
        };
        let list = eng.import_path_list();
        list.iter().map(|s| s.to_string()).collect()
    }

    /// Register an application-owned Rust/CXX-Qt `QObject` as a named QML root context property.
    ///
    /// Ownership transfers to `RinUIHost`, ensuring the `QObject` outlives `QQmlApplicationEngine`
    /// and is destroyed safely on the GUI thread before `QGuiApplication`.
    pub fn expose_qobject<T>(
        &mut self,
        name: &str,
        mut object: cxx::UniquePtr<T>,
    ) -> Result<(), HostError>
    where
        T: Upcast<cxx_qt::QObject> + UniquePtrTarget + 'static,
    {
        let Some(obj_pin) = object.as_mut() else {
            return Err(HostError::NullQObject(name.to_owned()));
        };
        let qobj_pin: Pin<&mut cxx_qt::QObject> = obj_pin.upcast_pin();
        let eng_pin = self.engine.as_mut().ok_or(HostError::EngineInitFailed)?;
        unsafe {
            qt_bridge::rinui_engine_set_context_property_object(
                eng_pin,
                &QString::from(name),
                qobj_pin.get_unchecked_mut() as *mut cxx_qt::QObject,
            );
        }
        self.owned_context_objects.push(Box::new(object));
        Ok(())
    }

    /// Expose a string value directly on `QQmlApplicationEngine::rootContext()`.
    pub fn set_context_property_string(
        &mut self,
        name: &str,
        value: &str,
    ) -> Result<(), HostError> {
        let eng_pin = self.engine.as_mut().ok_or(HostError::EngineInitFailed)?;
        qt_bridge::rinui_engine_set_context_property_variant(
            eng_pin,
            &QString::from(name),
            &QVariant::from(&QString::from(value)),
        );
        Ok(())
    }

    /// Expose a `QVariant` directly on `QQmlApplicationEngine::rootContext()`.
    pub fn set_context_property_variant(
        &mut self,
        name: &str,
        value: &QVariant,
    ) -> Result<(), HostError> {
        let eng_pin = self.engine.as_mut().ok_or(HostError::EngineInitFailed)?;
        qt_bridge::rinui_engine_set_context_property_variant(eng_pin, &QString::from(name), value);
        Ok(())
    }

    /// Register a consumer application translation catalog and immediately apply the current locale.
    pub fn register_translation_catalog(
        &mut self,
        catalog: TranslationCatalog,
    ) -> Result<TranslationLoadResult, HostError> {
        self.handle().register_translation_catalog(catalog)
    }

    /// Switch language at runtime (`"en_US"`, `"zh_CN"`, etc.) and retranslate live QML bindings.
    pub fn set_language(&mut self, locale: &str) -> Result<TranslationLoadResult, HostError> {
        self.handle().set_language(locale)
    }

    /// Load a QML document, register discovered RinUI windows, and apply native platform hooks.
    pub fn load(&mut self, source: &QmlSource) -> Result<LoadedRoot, HostError> {
        if self.loading_qml.get() {
            return Err(HostError::ReentrantCall("load"));
        }
        self.loading_qml.set(true);
        let _loading_guard = LoadingGuard(Rc::clone(&self.loading_qml));

        let (mac_window, resources_desc) = {
            let guard = self
                .shared
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("load"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            (state.mac_window, state.resources.describe())
        };

        let eng_pin = self.engine.as_mut().ok_or(HostError::EngineInitFailed)?;
        let tm_pin = self
            .theme_manager
            .as_mut()
            .ok_or(HostError::ThemeManagerInitFailed)?;

        let loaded = load_qml_on_engine(eng_pin, source, &resources_desc, mac_window, tm_pin)?;
        self.roots.push(loaded.root_id);

        if let Some(customizer) = self.customizer.as_mut() {
            customizer.on_windows_discovered(&loaded.windows);
        }
        Ok(loaded)
    }

    /// Set the application window icon and update the given root window's `icon` property.
    pub fn set_icon(&mut self, icon_path: &str, root_id: Option<RootId>) {
        let target_root = root_id
            .or_else(|| self.roots.first().copied())
            .map(|r| r.0)
            .unwrap_or(0);
        qt_bridge::rinui_set_app_icon(&QString::from(icon_path), target_root);
    }

    /// Set a property on a loaded root QML object (e.g. `"title"`).
    pub fn set_root_string_property(&mut self, root_id: RootId, name: &str, value: &str) -> bool {
        qt_bridge::rinui_root_set_property(
            root_id.0,
            &QString::from(name),
            &QVariant::from(&QString::from(value)),
        )
    }

    /// Read a property from a loaded root QML object.
    pub fn root_property(&self, root_id: RootId, name: &str) -> QVariant {
        qt_bridge::rinui_root_get_property(root_id.0, &QString::from(name))
    }

    /// Release a loaded root object tree (for transient secondary windows).
    pub fn release_root(&mut self, root_id: RootId) {
        self.roots.retain(|id| *id != root_id);
        let _ = self.handle().release_root(root_id);
    }

    /// Check whether a root object handle still points to a live Qt object.
    pub fn is_root_alive(&self, root_id: RootId) -> bool {
        qt_bridge::rinui_root_is_alive(root_id.0)
    }

    /// Synchronize any top-level RinUI windows opened directly from QML.
    pub fn sync_windows(&mut self) -> Result<Vec<NativeWindowRef>, HostError> {
        let discovered = self.handle().sync_windows()?;
        if !discovered.is_empty() {
            if let Some(customizer) = self.customizer.as_mut() {
                customizer.on_windows_discovered(&discovered);
            }
        }
        Ok(discovered)
    }

    /// Borrow `ThemeManager` immutably.
    pub fn theme_manager(&self) -> &ThemeManager {
        self.theme_manager
            .as_ref()
            .expect("ThemeManager alive for RinUIHost lifetime")
    }

    /// Borrow `ThemeManager` pinned mutably.
    pub fn theme_manager_mut(&mut self) -> Pin<&mut ThemeManager> {
        self.theme_manager
            .as_mut()
            .expect("ThemeManager alive for RinUIHost lifetime")
    }

    /// Borrow shared RinUI `ConfigStore`.
    pub fn config_store(&self) -> &Rc<RefCell<ConfigStore>> {
        &self.store
    }

    /// Resolved RinUI resources (`Embedded` or `Filesystem`).
    pub fn resources(&self) -> ResolvedResources {
        self.shared
            .borrow()
            .as_ref()
            .expect("shared alive")
            .resources
            .clone()
    }

    /// QML warnings emitted since engine creation or last clear.
    pub fn qml_warnings(&self) -> Vec<String> {
        let Some(eng) = self.engine.as_ref() else {
            return Vec::new();
        };
        qt_bridge::rinui_engine_warnings(eng)
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Process pending Qt GUI events (useful in deterministic tests).
    pub fn process_events(&self) {
        if let Some(app) = self.app.as_ref() {
            app.process_events();
        }
    }

    /// Run Qt's main event loop until application exit and persist configuration before returning.
    pub fn exec(&mut self) -> i32 {
        let code = self.app.as_mut().expect("QGuiApplication alive").exec();
        let _ = self.store.borrow().save();
        code
    }
}

impl Drop for RinUIHost {
    fn drop(&mut self) {
        if let Ok(store) = self.store.try_borrow() {
            let _ = store.save();
        }
        if let Ok(mut shared) = self.shared.try_borrow_mut() {
            *shared = None;
        }
        self.customizer.take();
        qt_bridge::rinui_release_all_roots_and_platform();
        qt_bridge::rinui_flush_deferred_deletes();
        qt_bridge::rinui_translator_clear_all();
        if let Some(mut eng) = self.engine.as_mut() {
            qt_bridge::rinui_engine_clear_component_cache(eng.as_mut());
            qt_bridge::rinui_engine_collect_garbage(eng);
        }
        if let Some(eng) = self.engine.as_ref() {
            qt_bridge::rinui_engine_uninstall_warning_collector(eng);
        }
        // Remaining fields drop in struct declaration order:
        // engine -> owned_context_objects -> theme_manager -> store -> app -> _active_guard.
    }
}

fn load_qml_on_engine(
    mut engine: Pin<&mut QQmlApplicationEngine>,
    source: &QmlSource,
    resources_desc: &str,
    mac_window: MacWindowOptions,
    mut theme_manager: Pin<&mut ThemeManager>,
) -> Result<LoadedRoot, HostError> {
    let before_count = qt_bridge::rinui_engine_root_count(&engine.as_ref());
    qt_bridge::rinui_engine_clear_warnings(&engine.as_ref());

    let url = source.to_qurl();
    engine.as_mut().load(&url);

    let after_count = qt_bridge::rinui_engine_root_count(&engine.as_ref());
    let warnings_list: QStringList = qt_bridge::rinui_engine_warnings(&engine.as_ref());
    let warnings: Vec<String> = warnings_list.iter().map(|s| s.to_string()).collect();

    if after_count <= before_count {
        let source_desc = source.describe();
        let reason = if warnings.is_empty() {
            "QQmlApplicationEngine produced no root object".to_owned()
        } else {
            warnings.join(" | ")
        };
        qt_bridge::rinui_write_qml_failure_log(
            &QString::from(source_desc.as_str()),
            &QString::from(resources_desc),
            &warnings_list,
            &QString::from(reason.as_str()),
        );
        return Err(HostError::QmlLoadFailed {
            source_desc,
            reason,
            warnings,
        });
    }

    let root_id_raw = qt_bridge::rinui_engine_register_latest_root(
        engine,
        mac_window.enabled,
        mac_window.traffic_lights_offset_x,
        mac_window.traffic_lights_offset_down,
    );
    if root_id_raw == 0 {
        return Err(HostError::QmlLoadFailed {
            source_desc: source.describe(),
            reason: "failed to register created root object".to_owned(),
            warnings,
        });
    }

    let window_ids = qt_bridge::rinui_root_windows(root_id_raw);
    let windows: Vec<NativeWindowRef> = window_ids
        .into_iter()
        .map(|id| NativeWindowRef::new(WindowId(id)))
        .collect();

    theme_manager.sync_platform_appearance();
    if cfg!(target_os = "windows") && !windows.is_empty() {
        let effect = theme_manager.current_backdrop();
        let _ = theme_manager
            .as_mut()
            .apply_backdrop_effect(QString::from(effect.as_str()));
    }

    debug!(
        source = %source.describe(),
        root_id = root_id_raw,
        window_count = windows.len(),
        "Loaded QML root"
    );

    Ok(LoadedRoot {
        root_id: RootId(root_id_raw),
        windows,
    })
}
