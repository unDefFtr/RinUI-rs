use crate::bridge::ffi as qt_bridge;
use crate::error::HostError;
use crate::resources::{
    strip_windows_verbatim_prefix, ResolvedResources, ResourceMode, ResourceView,
};
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
use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Once;
use tracing::{debug, error, info, warn};

static RUNTIME_ACTIVE: AtomicBool = AtomicBool::new(false);
static INIT_CRATE_ONCE: Once = Once::new();
static NEXT_RUNTIME_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_ENGINE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static LIVE_RUNTIMES: RefCell<HashMap<u64, Weak<RuntimeCore>>> = RefCell::new(HashMap::new());
    static ACTIVE_RUNTIME_ID: Cell<Option<u64>> = const { Cell::new(None) };
    static GLOBAL_QML_EXEC_DEPTH: Cell<usize> = const { Cell::new(0) };
    static GLOBAL_THEME_BORROW_ACTIVE: Cell<usize> = const { Cell::new(0) };
    static PENDING_RUNTIME_TEARDOWNS: RefCell<Vec<DeferredRuntimeTeardown>> = const { RefCell::new(Vec::new()) };
}

// These counters also cover guards whose RuntimeCore is no longer in LIVE_RUNTIMES.
fn global_qml_or_theme_busy() -> bool {
    GLOBAL_QML_EXEC_DEPTH.with(|depth| depth.get() != 0)
        || GLOBAL_THEME_BORROW_ACTIVE.with(|count| count.get() != 0)
}

fn on_deferred_drain_callback(is_post_routine: bool) {
    let cores: Vec<_> =
        LIVE_RUNTIMES.with(|map| map.borrow().values().filter_map(Weak::upgrade).collect());
    let mut busy = false;
    for core in cores {
        if core.qml_exec_depth.get() == 0 && core.theme_borrow_state.get() == 0 {
            core.flush_deferred_and_engines();
        } else {
            busy = true;
        }
    }
    if !global_qml_or_theme_busy() || is_post_routine {
        drain_pending_runtime_teardowns(is_post_routine);
    } else {
        busy = true;
    }
    if busy && !is_post_routine {
        qt_bridge::rinui_schedule_deferred_drain();
    }
}

struct DeferredRuntimeTeardown {
    runtime_id: u64,
    shared_engine: Option<EngineState>,
    deferred_engine_drops: Vec<EngineState>,
    theme_manager: cxx::UniquePtr<ThemeManager>,
    _active_guard: Option<RuntimeActiveGuard>,
}

fn flush_if_gui_alive() {
    if qt_bridge::rinui_has_live_gui_application() {
        qt_bridge::rinui_flush_deferred_deletes();
    }
}

fn drain_pending_runtime_teardowns(is_post_routine: bool) {
    if global_qml_or_theme_busy() && !is_post_routine {
        qt_bridge::rinui_schedule_deferred_drain();
        return;
    }
    // Release the RefCell borrow before any Qt destruction can reenter Rust.
    let pending = PENDING_RUNTIME_TEARDOWNS.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    for mut teardown in pending {
        let engines_ready = teardown
            .shared_engine
            .iter()
            .chain(teardown.deferred_engine_drops.iter())
            .all(|engine| qt_bridge::rinui_engine_drop_ready(engine.engine_id));
        if !is_post_routine && !engines_ready {
            PENDING_RUNTIME_TEARDOWNS.with(|queue| queue.borrow_mut().push(teardown));
            continue;
        }
        if is_post_routine {
            flush_if_gui_alive();
        }
        teardown.deferred_engine_drops.clear();
        teardown.shared_engine = None;
        if is_post_routine {
            flush_if_gui_alive();
        }
        qt_bridge::rinui_runtime_release_windows_and_filters(teardown.runtime_id);
        qt_bridge::rinui_runtime_teardown_services(teardown.runtime_id);
        teardown.theme_manager = cxx::UniquePtr::null();
        if is_post_routine {
            flush_if_gui_alive();
        }
        drop(teardown._active_guard.take());
    }
}

pub(crate) fn with_active_qml_exec_guard<R>(f: impl FnOnce() -> R) -> Option<R> {
    let active_core = ACTIVE_RUNTIME_ID.with(|cell| {
        cell.get().and_then(|id| {
            LIVE_RUNTIMES.with(|map| map.borrow().get(&id).and_then(|w| w.upgrade()))
        })
    });
    if let Some(core) = active_core {
        let Ok(_guard) = QmlExecGuard::enter_host_call(core) else {
            return None;
        };
        Some(f())
    } else {
        Some(f())
    }
}

pub(crate) fn enter_qml_exec_from_theme_slot(core: Rc<RuntimeCore>) -> QmlExecGuard {
    QmlExecGuard::enter_from_theme_slot(core)
}

struct RuntimeActiveGuard;

impl Drop for RuntimeActiveGuard {
    fn drop(&mut self) {
        RUNTIME_ACTIVE.store(false, Ordering::Release);
    }
}

struct LoadingGuard(Rc<Cell<bool>>);

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// RAII fence keeping deferred Qt deletes and deferred engine drops suspended while QML/JS
/// frames are active on the stack.
pub(crate) struct QmlExecGuard {
    core: Rc<RuntimeCore>,
}

impl QmlExecGuard {
    fn enter_host_call(core: Rc<RuntimeCore>) -> Result<Self, HostError> {
        core.ensure_main_thread()?;
        if core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("theme_manager_borrowed"));
        }
        Ok(Self::push(core))
    }

    fn enter_from_theme_slot(core: Rc<RuntimeCore>) -> Self {
        Self::push(core)
    }

    fn push(core: Rc<RuntimeCore>) -> Self {
        let cur = core.qml_exec_depth.get();
        core.qml_exec_depth.set(cur + 1);
        GLOBAL_QML_EXEC_DEPTH.with(|depth| depth.set(depth.get() + 1));
        qt_bridge::rinui_qml_exec_enter();
        Self { core }
    }
}

impl Drop for QmlExecGuard {
    fn drop(&mut self) {
        let prev = self.core.qml_exec_depth.get();
        let next = prev.saturating_sub(1);
        self.core.qml_exec_depth.set(next);
        GLOBAL_QML_EXEC_DEPTH.with(|depth| depth.set(depth.get() - 1));
        qt_bridge::rinui_qml_exec_leave();
        if next == 0 {
            self.core.finish_deferred_work_or_schedule();
        }
    }
}

struct InvokeMethodGuard {
    core: Rc<RuntimeCore>,
}

impl InvokeMethodGuard {
    fn enter(core: Rc<RuntimeCore>) -> Result<Self, HostError> {
        if core.invoke_method_active.get() {
            return Err(HostError::ReentrantCall("invoke_root_method"));
        }
        core.invoke_method_active.set(true);
        Ok(Self { core })
    }
}

impl Drop for InvokeMethodGuard {
    fn drop(&mut self) {
        self.core.invoke_method_active.set(false);
    }
}

struct RuntimeInitRollbackGuard {
    armed: bool,
    runtime_id: u64,
    is_borrowed: bool,
    captured_borrowed: bool,
    applied_identity: bool,
}

impl RuntimeInitRollbackGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RuntimeInitRollbackGuard {
    fn drop(&mut self) {
        if self.armed {
            qt_bridge::rinui_runtime_release_windows_and_filters(self.runtime_id);
            qt_bridge::rinui_flush_deferred_deletes();
            qt_bridge::rinui_translator_clear_all();
            theme_qobject::rinui_disconnect_color_scheme_changed();
            qt_bridge::rinui_runtime_teardown_services(self.runtime_id);
            if self.is_borrowed && self.captured_borrowed {
                qt_bridge::rinui_restore_borrowed_app_state(self.applied_identity);
            }
        }
    }
}

/// Shared RAII read guard to [`ThemeManager`].
///
/// `!Send + !Sync`: while alive, attempting an exclusive write via
/// [`RinUIRuntime::with_theme_manager_mut`], [`RinUISession::with_theme_manager_mut`],
/// [`crate::RinUIHost::with_theme_manager_mut`], or QML/FFI execution returns
/// `Err(HostError::ReentrantCall(...))`.
pub struct ThemeManagerRef<'a> {
    core: &'a RuntimeCore,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl Deref for ThemeManagerRef<'_> {
    type Target = ThemeManager;

    fn deref(&self) -> &ThemeManager {
        unsafe { &*self.core.theme_manager_ptr }
    }
}

impl Drop for ThemeManagerRef<'_> {
    fn drop(&mut self) {
        let cur = self.core.theme_borrow_state.get();
        debug_assert!(cur > 0);
        let next = cur.saturating_sub(1);
        self.core.theme_borrow_state.set(next);
        GLOBAL_THEME_BORROW_ACTIVE.with(|count| count.set(count.get() - 1));
        if next == 0 {
            self.core.finish_deferred_work_or_schedule();
        }
    }
}

/// Exclusive RAII write guard to [`ThemeManager`].
///
/// `!Send + !Sync`: while alive, any overlapping read or write borrow via
/// [`RinUIRuntime`], [`RinUISession`], or [`crate::RinUIHost`] returns
/// `Err(HostError::ReentrantCall(...))`.
pub struct ThemeManagerMut<'a> {
    core: &'a RuntimeCore,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl Deref for ThemeManagerMut<'_> {
    type Target = ThemeManager;

    fn deref(&self) -> &ThemeManager {
        unsafe { &*self.core.theme_manager_ptr }
    }
}

impl ThemeManagerMut<'_> {
    /// Obtain a pinned mutable reference to `ThemeManager` for calling CXX-Qt mutators.
    pub fn as_mut(&mut self) -> Pin<&mut ThemeManager> {
        unsafe { Pin::new_unchecked(&mut *self.core.theme_manager_ptr) }
    }

    pub fn toggle_theme(&mut self, theme: QString) {
        self.as_mut().toggle_theme(theme);
    }

    pub fn apply_backdrop_effect(&mut self, effect_type: QString) -> i32 {
        self.as_mut().apply_backdrop_effect(effect_type)
    }

    pub fn set_theme_color(&mut self, color: QString) {
        self.as_mut().set_theme_color(color);
    }
}

impl Drop for ThemeManagerMut<'_> {
    fn drop(&mut self) {
        debug_assert_eq!(self.core.theme_borrow_state.get(), -1);
        self.core.theme_borrow_state.set(0);
        GLOBAL_THEME_BORROW_ACTIVE.with(|count| count.set(count.get() - 1));
        self.core.finish_deferred_work_or_schedule();
    }
}

/// Configuration for initializing or attaching a process-level [`RinUIRuntime`].
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub identity: AppIdentity,
    pub resource_mode: ResourceMode,
    pub config_location: ConfigLocation,
    pub locale: Option<String>,
    pub mac_window: MacWindowOptions,
    pub qt_quick_controls_style: String,
    /// When borrowing an existing `QGuiApplication`/`QApplication`, leave its
    /// `organizationName`/`applicationName` untouched by default (`false`) and use `identity`
    /// only for RinUI config/log paths. Set to `true` to also apply `identity` onto Qt and
    /// restore the caller's original identity when `RinUIRuntime` drops.
    pub apply_identity_to_borrowed_app: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            identity: AppIdentity::new("RinUI", "Application"),
            resource_mode: ResourceMode::Auto,
            config_location: ConfigLocation::Standard,
            locale: None,
            mac_window: MacWindowOptions::default(),
            qt_quick_controls_style: "Basic".to_owned(),
            apply_identity_to_borrowed_app: false,
        }
    }
}

impl RuntimeConfig {
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

/// Engine ownership model for a [`RinUISession`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineMode {
    /// Reuse the runtime's shared `QQmlApplicationEngine` (shared singletons, import paths, and context properties).
    #[default]
    Shared,
    /// Dedicated `QQmlApplicationEngine` isolated per session (separate QML singletons, cache, and root context).
    Private,
}

/// Policy when setting a context property or exposing a `QObject` whose name is already registered on an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPropertyConflict {
    /// Return [`HostError::ContextPropertyConflict`] if the key already exists on this engine.
    Reject,
    /// Replace an existing context property value on this engine.
    Overwrite,
}

/// Configuration for creating a [`RinUISession`] from an active [`RinUIRuntime`].
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub engine_mode: EngineMode,
    /// Optional resource mode override (effective for [`EngineMode::Private`] sessions, or when initializing the shared engine).
    pub resource_mode: Option<ResourceMode>,
    /// Optional macOS titlebar option override (defaults to the runtime's `mac_window`).
    pub mac_window: Option<MacWindowOptions>,
    /// Conflict rule for `set_context_property_*` and `expose_qobject`.
    pub context_property_conflict: ContextPropertyConflict,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            engine_mode: EngineMode::Shared,
            resource_mode: None,
            mac_window: None,
            context_property_conflict: ContextPropertyConflict::Reject,
        }
    }
}

impl SessionConfig {
    /// Preset for a session backed by the runtime's shared `QQmlApplicationEngine`
    /// with strict context-property collision checking (`Reject`).
    pub fn shared() -> Self {
        Self::default()
    }

    /// Preset for an isolated private `QQmlApplicationEngine` session.
    pub fn private() -> Self {
        Self {
            engine_mode: EngineMode::Private,
            resource_mode: None,
            mac_window: None,
            context_property_conflict: ContextPropertyConflict::Overwrite,
        }
    }

    /// Override resource mode for this session.
    pub fn with_resource_mode(mut self, mode: ResourceMode) -> Self {
        self.resource_mode = Some(mode);
        self
    }

    /// Override macOS titlebar options for this session.
    pub fn with_mac_window(mut self, mac_window: MacWindowOptions) -> Self {
        self.mac_window = Some(mac_window);
        self
    }

    /// Override context-property conflict handling for this session.
    pub fn with_conflict_policy(mut self, policy: ContextPropertyConflict) -> Self {
        self.context_property_conflict = policy;
        self
    }
}

/// Input QML document passed to [`RinUISession::load`] / [`crate::RinUIHost::load`].
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

/// Engine-owned state guaranteeing `QQmlApplicationEngine` drops BEFORE any exposed
/// context `QObject`s and BEFORE its temporary filesystem `ResourceView`.
pub(crate) struct EngineState {
    engine_id: u64,
    // Drop order matters: engine -> owned_context_objects -> resource_view.
    engine: cxx::UniquePtr<QQmlApplicationEngine>,
    owned_context_objects: Vec<Box<dyn Any>>,
    registered_context_names: HashSet<String>,
    resource_view: ResourceView,
    engine_ptr: *mut QQmlApplicationEngine,
}

impl EngineState {
    fn create(
        runtime_id: u64,
        resource_mode: &ResourceMode,
        mac_window: MacWindowOptions,
        theme_manager_qobject_ptr: *mut cxx_qt::QObject,
    ) -> Result<Self, HostError> {
        let resource_view = ResourceView::prepare(resource_mode)?;
        let engine_id = NEXT_ENGINE_ID.fetch_add(1, Ordering::Relaxed);

        let mut engine = QQmlApplicationEngine::new();
        if engine.is_null() {
            return Err(HostError::EngineInitFailed);
        }

        let engine_ptr = {
            let mut eng_pin = engine.as_mut().ok_or(HostError::EngineInitFailed)?;
            qt_bridge::rinui_engine_install_warning_collector(eng_pin.as_mut());
            qt_bridge::rinui_engine_register(eng_pin.as_mut(), runtime_id, engine_id);
            eng_pin
                .as_mut()
                .add_import_path(&QString::from(resource_view.qml_import_path().as_str()));
            qt_bridge::rinui_platform_register_engine_objects(eng_pin.as_mut());

            // Pre-first-frame native/self-drawn macOS titlebar signal on QML context.
            qt_bridge::rinui_engine_set_context_property_variant(
                eng_pin.as_mut(),
                &QString::from("RinUINativeMacFrame"),
                &QVariant::from(&mac_window.effective_native_frame()),
            );

            // Expose shared process-level ThemeManager (lives cleanly past all engines).
            unsafe {
                qt_bridge::rinui_engine_set_context_property_object(
                    eng_pin.as_mut(),
                    &QString::from("ThemeManager"),
                    theme_manager_qobject_ptr,
                );
                eng_pin.get_unchecked_mut() as *mut QQmlApplicationEngine
            }
        };

        let mut registered_context_names = HashSet::new();
        registered_context_names.insert("ThemeManager".to_owned());
        registered_context_names.insert("RinUINativeMacFrame".to_owned());

        Ok(Self {
            engine_id,
            engine,
            owned_context_objects: Vec::new(),
            registered_context_names,
            resource_view,
            engine_ptr,
        })
    }

    fn set_context_property_variant(
        &mut self,
        name: &str,
        value: &QVariant,
        policy: ContextPropertyConflict,
    ) -> Result<(), HostError> {
        if self.registered_context_names.contains(name) && policy == ContextPropertyConflict::Reject
        {
            return Err(HostError::ContextPropertyConflict {
                name: name.to_owned(),
            });
        }
        let eng_pin = self.engine.as_mut().ok_or(HostError::EngineInitFailed)?;
        qt_bridge::rinui_engine_set_context_property_variant(eng_pin, &QString::from(name), value);
        self.registered_context_names.insert(name.to_owned());
        Ok(())
    }

    fn expose_qobject<T>(
        &mut self,
        name: &str,
        mut object: cxx::UniquePtr<T>,
        policy: ContextPropertyConflict,
    ) -> Result<(), HostError>
    where
        T: Upcast<cxx_qt::QObject> + UniquePtrTarget + 'static,
    {
        if self.registered_context_names.contains(name) && policy == ContextPropertyConflict::Reject
        {
            return Err(HostError::ContextPropertyConflict {
                name: name.to_owned(),
            });
        }
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
        self.registered_context_names.insert(name.to_owned());
        self.owned_context_objects.push(Box::new(object));
        Ok(())
    }

    fn add_import_path(&mut self, path: &str) {
        if let Some(eng) = self.engine.as_mut() {
            eng.add_import_path(&QString::from(path));
        }
    }

    fn import_paths(&self) -> Vec<String> {
        let Some(eng) = self.engine.as_ref() else {
            return Vec::new();
        };
        eng.import_path_list()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn qml_warnings(&self) -> Vec<String> {
        let Some(eng) = self.engine.as_ref() else {
            return Vec::new();
        };
        qt_bridge::rinui_engine_warnings(eng)
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn clear_component_cache(&mut self) {
        if let Some(eng) = self.engine.as_mut() {
            qt_bridge::rinui_engine_clear_component_cache(eng);
        }
    }

    fn collect_garbage(&mut self) {
        if let Some(eng) = self.engine.as_mut() {
            qt_bridge::rinui_engine_collect_garbage(eng);
        }
    }
}

impl Drop for EngineState {
    fn drop(&mut self) {
        if qt_bridge::rinui_has_live_gui_application() {
            if let Some(mut eng) = self.engine.as_mut() {
                qt_bridge::rinui_engine_clear_component_cache(eng.as_mut());
                qt_bridge::rinui_engine_collect_garbage(eng);
            }
        }
        if let Some(eng) = self.engine.as_ref() {
            qt_bridge::rinui_engine_uninstall_warning_collector(eng);
        }
        qt_bridge::rinui_engine_unregister(self.engine_id);
        // After this block finishes:
        // 1. `engine` drops first
        // 2. `owned_context_objects` drops second (shared context QObjects outlive engine)
        // 3. `resource_view` drops last (temp directory outlives engine & roots)
    }
}

pub(crate) struct RuntimeCore {
    runtime_id: u64,
    is_borrowed: bool,
    applied_identity_to_borrowed: bool,
    default_resource_mode: ResourceMode,
    default_mac_window: MacWindowOptions,
    capabilities: PlatformCapabilities,
    config_load_report: ConfigLoadReport,
    shared_engine: RefCell<Option<EngineState>>,
    deferred_engine_drops: RefCell<Vec<EngineState>>,
    live_engines: RefCell<HashMap<u64, *mut QQmlApplicationEngine>>,
    qml_exec_depth: Cell<usize>,
    invoke_method_active: Cell<bool>,
    translator: RefCell<TranslationManager>,
    store: Rc<RefCell<ConfigStore>>,
    theme_manager: UnsafeCell<cxx::UniquePtr<ThemeManager>>,
    theme_manager_ptr: *mut ThemeManager,
    theme_manager_qobject_ptr: *mut cxx_qt::QObject,
    theme_borrow_state: Rc<Cell<isize>>,
    owned_app: Option<cxx::UniquePtr<QGuiApplication>>,
    _active_guard: Option<RuntimeActiveGuard>,
}

impl RuntimeCore {
    fn ensure_main_thread(&self) -> Result<(), HostError> {
        if qt_bridge::rinui_is_on_app_thread() {
            Ok(())
        } else {
            Err(HostError::WrongThread)
        }
    }

    fn register_live_engine(&self, state: &EngineState) {
        self.live_engines
            .borrow_mut()
            .insert(state.engine_id, state.engine_ptr);
    }

    fn remove_live_engine(&self, engine_id: u64) {
        self.live_engines.borrow_mut().remove(&engine_id);
    }

    fn defer_engine_drop(&self, state: EngineState) {
        if self.is_borrowed {
            qt_bridge::rinui_defer_engine_drop(state.engine_id);
        }
        self.deferred_engine_drops.borrow_mut().push(state);
    }

    fn schedule_deferred_drain(&self) {
        qt_bridge::rinui_schedule_deferred_drain();
    }

    fn flush_deferred_and_engines(&self) {
        if self.qml_exec_depth.get() > 0 || self.theme_borrow_state.get() != 0 {
            return;
        }
        // Never force global DeferredDeletes from a borrowed-app callback: even an
        // older ready fence must not flush a newer, still executing QML object.
        if !self.is_borrowed {
            flush_if_gui_alive();
        }
        self.drain_deferred_engine_drops();
        if !self.is_borrowed {
            flush_if_gui_alive();
        }
    }

    fn finish_deferred_work_or_schedule(&self) {
        if self.qml_exec_depth.get() > 0 || self.theme_borrow_state.get() != 0 {
            return;
        }
        if self.is_borrowed || qt_bridge::rinui_in_event_loop() {
            self.schedule_deferred_drain();
        } else {
            self.flush_deferred_and_engines();
        }
    }

    fn drain_deferred_engine_drops(&self) {
        loop {
            let drained = {
                let Ok(mut vec) = self.deferred_engine_drops.try_borrow_mut() else {
                    return;
                };
                if vec.is_empty() {
                    return;
                }
                let (ready, waiting): (Vec<_>, Vec<_>) =
                    std::mem::take(&mut *vec).into_iter().partition(|engine| {
                        !self.is_borrowed || qt_bridge::rinui_engine_drop_ready(engine.engine_id)
                    });
                *vec = waiting;
                ready
            };
            if drained.is_empty() {
                return;
            }
            drop(drained);
        }
    }

    fn retranslate_all_engines(&self) {
        // Snapshot engine IDs (not raw pointers!) and re-check live_engines on each iteration
        // so QML translation callbacks dropping this or sibling Private sessions never use UAF pointers.
        let engine_ids: Vec<u64> = self.live_engines.borrow().keys().copied().collect();
        for engine_id in engine_ids {
            let ptr = self.live_engines.borrow().get(&engine_id).copied();
            if let Some(ptr) = ptr {
                if !ptr.is_null() {
                    unsafe {
                        qt_bridge::rinui_engine_retranslate(Pin::new_unchecked(&mut *ptr));
                    }
                }
            }
        }
    }

    pub(crate) fn set_language(
        core: &Rc<Self>,
        locale: &str,
    ) -> Result<TranslationLoadResult, HostError> {
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(core))?;
        let res = {
            let mut tr = core
                .translator
                .try_borrow_mut()
                .map_err(|_| HostError::ReentrantCall("set_language"))?;
            tr.load_locale(locale, None)?
        };
        core.retranslate_all_engines();
        Ok(res)
    }

    pub(crate) fn register_translation_catalog(
        core: &Rc<Self>,
        catalog: TranslationCatalog,
    ) -> Result<TranslationLoadResult, HostError> {
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(core))?;
        let res = {
            let mut tr = core
                .translator
                .try_borrow_mut()
                .map_err(|_| HostError::ReentrantCall("register_translation_catalog"))?;
            tr.register_and_apply(catalog, None)?
        };
        core.retranslate_all_engines();
        Ok(res)
    }

    pub(crate) fn current_locale(&self) -> Result<String, HostError> {
        self.ensure_main_thread()?;
        let tr = self
            .translator
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("current_locale"))?;
        Ok(tr.current_locale().to_owned())
    }

    pub(crate) fn borrow_theme_manager(&self) -> Result<ThemeManagerRef<'_>, HostError> {
        self.ensure_main_thread()?;
        let cur = self.theme_borrow_state.get();
        if cur < 0 {
            return Err(HostError::ReentrantCall("theme_manager"));
        }
        self.theme_borrow_state.set(cur + 1);
        GLOBAL_THEME_BORROW_ACTIVE.with(|count| count.set(count.get() + 1));
        Ok(ThemeManagerRef {
            core: self,
            _not_send_sync: PhantomData,
        })
    }

    pub(crate) fn borrow_theme_manager_mut(&self) -> Result<ThemeManagerMut<'_>, HostError> {
        self.ensure_main_thread()?;
        if self.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("theme_manager_mut"));
        }
        self.theme_borrow_state.set(-1);
        GLOBAL_THEME_BORROW_ACTIVE.with(|count| count.set(count.get() + 1));
        Ok(ThemeManagerMut {
            core: self,
            _not_send_sync: PhantomData,
        })
    }

    pub(crate) fn with_theme_manager<R>(
        &self,
        f: impl FnOnce(&ThemeManager) -> R,
    ) -> Result<R, HostError> {
        let guard = self.borrow_theme_manager()?;
        Ok(f(&guard))
    }

    pub(crate) fn with_theme_manager_mut<R>(
        &self,
        f: impl FnOnce(Pin<&mut ThemeManager>) -> R,
    ) -> Result<R, HostError> {
        let mut guard = self.borrow_theme_manager_mut()?;
        Ok(f(guard.as_mut()))
    }

    fn resolved_theme_str(&self) -> String {
        let configured = self
            .store
            .try_borrow()
            .map(|s| s.theme())
            .unwrap_or_default();
        let actual = match configured {
            rinui_platform::ThemeMode::Dark => rinui_platform::ResolvedTheme::Dark,
            rinui_platform::ThemeMode::Light => rinui_platform::ResolvedTheme::Light,
            rinui_platform::ThemeMode::Auto => {
                if qt_bridge::rinui_current_system_theme().to_string() == "Dark" {
                    rinui_platform::ResolvedTheme::Dark
                } else {
                    rinui_platform::ResolvedTheme::Light
                }
            }
        };
        actual.as_str().to_owned()
    }

    fn current_backdrop(&self) -> rinui_platform::BackdropEffect {
        self.store
            .try_borrow()
            .map(|s| s.backdrop_effect())
            .unwrap_or_default()
    }

    fn sync_platform_appearance(&self) {
        let configured = self
            .store
            .try_borrow()
            .map(|s| s.theme())
            .unwrap_or_default();
        let actual = self.resolved_theme_str();
        qt_bridge::rinui_platform_apply_theme(
            &QString::from(actual.as_str()),
            &QString::from(configured.as_str()),
        );
    }

    fn init_common(
        active_guard: RuntimeActiveGuard,
        config: RuntimeConfig,
        owned_app: Option<cxx::UniquePtr<QGuiApplication>>,
        is_borrowed: bool,
        captured_borrowed: bool,
    ) -> Result<Rc<Self>, HostError> {
        // Local drop order on any `?` error path:
        // theme_manager -> store -> rollback guard -> owned_app -> active_guard.
        let _active_guard = active_guard;
        let _owned_app = owned_app;
        let runtime_id = NEXT_RUNTIME_ID.fetch_add(1, Ordering::Relaxed);
        let applied_identity = is_borrowed && config.apply_identity_to_borrowed_app;

        let mut rollback = RuntimeInitRollbackGuard {
            armed: true,
            runtime_id,
            is_borrowed,
            captured_borrowed,
            applied_identity,
        };

        let resources = ResourceView::prepare(&config.resource_mode)?;

        if !is_borrowed || config.apply_identity_to_borrowed_app {
            qt_bridge::rinui_apply_active_app_identity(
                &QString::from(config.identity.organization_name.as_str()),
                &QString::from(config.identity.organization_domain.as_str()),
                &QString::from(config.identity.application_name.as_str()),
                &QString::from(config.identity.application_version.as_str()),
            );
        }

        let mut current_style = QString::default();
        let style_rc = qt_bridge::rinui_configure_quick_controls_style(
            &QString::from(config.qt_quick_controls_style.as_str()),
            is_borrowed,
            &mut current_style,
        );
        if style_rc != 0 {
            let active = {
                let s = current_style.to_string();
                if s.is_empty() {
                    "<platform-default>".to_owned()
                } else {
                    s
                }
            };
            return Err(HostError::IncompatibleQuickControlsStyle {
                requested: config.qt_quick_controls_style,
                active,
            });
        }

        let capabilities = PlatformCapabilities::current();
        let qt_local_data = qt_bridge::rinui_standard_app_local_data_dir().to_string();
        let qt_local_path = (!qt_local_data.is_empty()).then(|| PathBuf::from(&qt_local_data));

        let (store_val, config_load_report) = ConfigStore::load_or_default(
            &config.config_location,
            &config.identity,
            qt_local_path.as_deref(),
            capabilities,
        )?;
        match &config_load_report {
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

        let mut theme_manager = theme_qobject::rinui_theme_manager_new();
        if theme_manager.is_null() {
            return Err(HostError::ThemeManagerInitFailed);
        }
        if let Some(mut tm_pin) = theme_manager.as_mut() {
            tm_pin.as_mut().attach_store(Rc::clone(&store));
            theme_qobject::rinui_connect_color_scheme_changed(tm_pin.as_mut());
        }
        let (theme_manager_ptr, theme_manager_qobject_ptr) = theme_manager
            .as_mut()
            .map(|mut p| unsafe {
                let tm_raw = p.as_mut().get_unchecked_mut() as *mut ThemeManager;
                let qobj: Pin<&mut cxx_qt::QObject> = p.upcast_pin();
                (tm_raw, qobj.get_unchecked_mut() as *mut cxx_qt::QObject)
            })
            .ok_or(HostError::ThemeManagerInitFailed)?;

        let mut translator = TranslationManager::new(resources.clone());
        let initial_locale = config
            .locale
            .clone()
            .unwrap_or_else(TranslationManager::system_locale);
        let tr_res = translator.load_locale(&initial_locale, None)?;
        debug!(
            requested = %tr_res.requested_locale,
            active = %tr_res.active_locale,
            rinui = %tr_res.rinui_locale,
            rinui_fallback = tr_res.rinui_used_fallback,
            "Initial RinUI runtime translation loaded"
        );

        rollback.disarm();
        qt_bridge::rinui_set_deferred_drain_callback(on_deferred_drain_callback);

        info!(
            runtime_id,
            is_borrowed,
            org = %config.identity.organization_name,
            app = %config.identity.application_name,
            platform = %qt_bridge::rinui_platform_name().to_string(),
            resources = %resources.describe(),
            "RinUIRuntime initialized"
        );

        let theme_borrow_state = Rc::new(Cell::new(0isize));
        let core = Rc::new(Self {
            runtime_id,
            is_borrowed,
            applied_identity_to_borrowed: applied_identity,
            default_resource_mode: config.resource_mode,
            default_mac_window: config.mac_window,
            capabilities,
            config_load_report,
            shared_engine: RefCell::new(None),
            deferred_engine_drops: RefCell::new(Vec::new()),
            live_engines: RefCell::new(HashMap::new()),
            qml_exec_depth: Cell::new(0),
            invoke_method_active: Cell::new(false),
            translator: RefCell::new(translator),
            store,
            theme_manager: UnsafeCell::new(theme_manager),
            theme_manager_ptr,
            theme_manager_qobject_ptr,
            theme_borrow_state: Rc::clone(&theme_borrow_state),
            owned_app: _owned_app,
            _active_guard: Some(_active_guard),
        });

        unsafe {
            let mut tm_pin = Pin::new_unchecked(&mut *core.theme_manager_ptr);
            tm_pin
                .as_mut()
                .attach_runtime_guards(theme_borrow_state, Rc::downgrade(&core));
        }
        LIVE_RUNTIMES.with(|map| {
            map.borrow_mut().insert(runtime_id, Rc::downgrade(&core));
        });
        ACTIVE_RUNTIME_ID.with(|cell| {
            cell.set(Some(runtime_id));
        });

        Ok(core)
    }
}

impl Drop for RuntimeCore {
    fn drop(&mut self) {
        if !qt_bridge::rinui_is_on_app_thread() {
            error!(
                runtime_id = self.runtime_id,
                "RuntimeCore dropped off Qt main thread"
            );
        }

        LIVE_RUNTIMES.with(|map| {
            map.borrow_mut().remove(&self.runtime_id);
        });
        ACTIVE_RUNTIME_ID.with(|cell| {
            if cell.get() == Some(self.runtime_id) {
                cell.set(None);
            }
        });

        if let Ok(store) = self.store.try_borrow() {
            let _ = store.save();
        }

        self.live_engines.get_mut().clear();
        let has_engine = self.shared_engine.get_mut().is_some()
            || !self.deferred_engine_drops.get_mut().is_empty();
        if self.is_borrowed && has_engine && qt_bridge::rinui_has_live_gui_application() {
            // In an externally driven callback no Rust guard or Qt loopLevel need be active.
            // Detach ownership now, but retain engines, resources and the runtime reservation
            // until Qt delivers their DeferredDelete fences after the QML frame returns,
            // or application shutdown completes the pending teardown.
            qt_bridge::rinui_runtime_release_windows_deferred(self.runtime_id);
            qt_bridge::rinui_translator_clear_all();
            theme_qobject::rinui_disconnect_color_scheme_changed();
            qt_bridge::rinui_restore_borrowed_app_state(self.applied_identity_to_borrowed);
            if let Some(engine) = self.shared_engine.get_mut().as_ref() {
                qt_bridge::rinui_defer_engine_drop(engine.engine_id);
            }
            let teardown = DeferredRuntimeTeardown {
                runtime_id: self.runtime_id,
                shared_engine: self.shared_engine.get_mut().take(),
                deferred_engine_drops: std::mem::take(self.deferred_engine_drops.get_mut()),
                theme_manager: std::mem::replace(
                    self.theme_manager.get_mut(),
                    cxx::UniquePtr::null(),
                ),
                _active_guard: self._active_guard.take(),
            };
            PENDING_RUNTIME_TEARDOWNS.with(|queue| queue.borrow_mut().push(teardown));
            qt_bridge::rinui_schedule_deferred_drain();
            return;
        }

        // 1. Release any lingering windows and native filters while engines still exist.
        qt_bridge::rinui_runtime_release_windows_and_filters(self.runtime_id);
        qt_bridge::rinui_flush_deferred_deletes();

        // 2. Drop all shared & deferred engines BEFORE clearing translators or services.
        if let Ok(mut deferred) = self.deferred_engine_drops.try_borrow_mut() {
            deferred.clear();
        }
        if let Ok(mut shared) = self.shared_engine.try_borrow_mut() {
            *shared = None;
        }
        qt_bridge::rinui_flush_deferred_deletes();

        // 3. Remove QTranslator instances and disconnect QStyleHints signal before ThemeManager drops.
        qt_bridge::rinui_translator_clear_all();
        theme_qobject::rinui_disconnect_color_scheme_changed();

        // 4. Destroy runtime-lifetime native services (e.g., Windows WinEventManager) AFTER all engines drop.
        qt_bridge::rinui_runtime_teardown_services(self.runtime_id);

        // 5. Restore snapshotted caller state when borrowing an existing QGuiApplication.
        if self.is_borrowed {
            qt_bridge::rinui_restore_borrowed_app_state(self.applied_identity_to_borrowed);
        }

        // 6. Drop ThemeManager explicitly before owned QGuiApplication drops.
        unsafe {
            *self.theme_manager.get() = cxx::UniquePtr::null();
        }
        qt_bridge::rinui_flush_deferred_deletes();

        // Remaining field drops: `owned_app` (if Some) -> `_active_guard` (releases RUNTIME_ACTIVE).
    }
}

fn map_app_validation_status(code: i32) -> Result<(), HostError> {
    match code {
        0 => Ok(()),
        1 => Err(HostError::NoGuiApplication),
        2 => Err(HostError::NotGuiApplication),
        3 => Err(HostError::WrongThread),
        _ => Err(HostError::ApplicationMismatch),
    }
}

/// Process-level RinUI runtime coordinator owning or borrowing Qt's single GUI application.
///
/// `!Send + !Sync`: must live on the Qt GUI main thread. Multiple [`RinUISession`] instances
/// created via [`RinUIRuntime::create_session`] can coexist under one `RinUIRuntime` using either
/// shared or isolated private `QQmlApplicationEngine`s.
pub struct RinUIRuntime<'app> {
    core: Rc<RuntimeCore>,
    _app_lifetime: PhantomData<&'app QGuiApplication>,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl RinUIRuntime<'static> {
    /// Initialize a new standalone Qt `QGuiApplication` and process-level RinUI runtime.
    pub fn new(config: RuntimeConfig) -> Result<Self, HostError> {
        let _ = ResourceView::prepare(&config.resource_mode)?;

        if RUNTIME_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(HostError::ApplicationAlreadyInitialized);
        }

        let core_guard = RuntimeActiveGuard;
        if qt_bridge::rinui_has_gui_application() {
            return Err(HostError::ApplicationAlreadyInitialized);
        }

        INIT_CRATE_ONCE.call_once(|| {
            cxx_qt::init_crate!(rinui_host);
        });
        qt_bridge::rinui_set_high_dpi_pass_through();

        let app = QGuiApplication::new();
        if app.is_null() {
            return Err(HostError::ApplicationInitFailed);
        }

        let core = RuntimeCore::init_common(core_guard, config, Some(app), false, false)?;
        Ok(Self {
            core,
            _app_lifetime: PhantomData,
            _not_send_sync: PhantomData,
        })
    }
}

impl<'app> RinUIRuntime<'app> {
    /// Run Qt's main event loop until exit and persist configuration before returning.
    ///
    /// Returns [`HostError::BorrowedApplicationExecNotAllowed`] when borrowing an externally
    /// owned `QGuiApplication`/`QApplication` (drive the owner's event loop instead).
    pub fn exec(&mut self) -> Result<i32, HostError> {
        self.core.ensure_main_thread()?;
        if self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("exec"));
        }
        let code = self
            .core
            .owned_app
            .as_ref()
            .map(|app_ref| {
                // QGuiApplication::exec on cxx_qt_lib::QGuiApplication requires Pin<&mut Self>.
                let raw = app_ref
                    .as_ref()
                    .map(|r| r as *const QGuiApplication as *mut QGuiApplication)
                    .expect("owned QGuiApplication pointer valid");
                unsafe { Pin::new_unchecked(&mut *raw).exec() }
            })
            .ok_or(HostError::BorrowedApplicationExecNotAllowed)?;
        self.core.flush_deferred_and_engines();
        let _ = self.core.store.borrow().save();
        Ok(code)
    }

    /// Borrow an existing Qt `QGuiApplication` (or subclass such as Qt Widgets `QApplication`)
    /// without taking ownership or destroying it on drop.
    pub fn attach_existing(
        app: &'app QGuiApplication,
        config: RuntimeConfig,
    ) -> Result<Self, HostError> {
        map_app_validation_status(qt_bridge::rinui_validate_existing_gui_application_ref(app))?;
        Self::attach_validated(config)
    }

    /// Attach to an externally owned Qt `QGuiApplication` / `QApplication` via raw `QObject*`.
    ///
    /// # Safety
    ///
    /// - `app_ptr` must be a valid pointer to the process's live `QGuiApplication` (or `QApplication`)
    ///   instance on the current Qt GUI thread.
    /// - The underlying Qt application MUST outlive `'app` (i.e., outlive this `RinUIRuntime<'app>` and
    ///   all [`RinUISession<'app>`] / [`SessionHandle<'app>`] values derived from it).
    pub unsafe fn attach_existing_raw(
        app_ptr: *mut cxx_qt::QObject,
        config: RuntimeConfig,
    ) -> Result<Self, HostError> {
        map_app_validation_status(qt_bridge::rinui_validate_existing_qobject_ptr(app_ptr))?;
        Self::attach_validated(config)
    }

    fn attach_validated(config: RuntimeConfig) -> Result<Self, HostError> {
        let _ = ResourceView::prepare(&config.resource_mode)?;

        if RUNTIME_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(HostError::ApplicationAlreadyInitialized);
        }
        let guard = RuntimeActiveGuard;

        INIT_CRATE_ONCE.call_once(|| {
            cxx_qt::init_crate!(rinui_host);
        });

        qt_bridge::rinui_capture_borrowed_app_state();

        let core = RuntimeCore::init_common(guard, config, None, true, true)?;
        Ok(Self {
            core,
            _app_lifetime: PhantomData,
            _not_send_sync: PhantomData,
        })
    }

    /// Create a [`RinUISession<'app>`] backed by either a shared or isolated private engine.
    pub fn create_session(&self, config: SessionConfig) -> Result<RinUISession<'app>, HostError> {
        self.core.ensure_main_thread()?;
        if self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("create_session"));
        }
        let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
        let mac_window = config.mac_window.unwrap_or(self.core.default_mac_window);

        let private_engine = match config.engine_mode {
            EngineMode::Shared => {
                let mut shared_guard = self
                    .core
                    .shared_engine
                    .try_borrow_mut()
                    .map_err(|_| HostError::ReentrantCall("create_session"))?;
                if shared_guard.is_none() {
                    let res_mode = config
                        .resource_mode
                        .as_ref()
                        .unwrap_or(&self.core.default_resource_mode);
                    let state = EngineState::create(
                        self.core.runtime_id,
                        res_mode,
                        mac_window,
                        self.core.theme_manager_qobject_ptr,
                    )?;
                    self.core.register_live_engine(&state);
                    *shared_guard = Some(state);
                }
                None
            }
            EngineMode::Private => {
                let res_mode = config
                    .resource_mode
                    .as_ref()
                    .unwrap_or(&self.core.default_resource_mode);
                let state = EngineState::create(
                    self.core.runtime_id,
                    res_mode,
                    mac_window,
                    self.core.theme_manager_qobject_ptr,
                )?;
                self.core.register_live_engine(&state);
                Some(state)
            }
        };

        let loading_qml = Rc::new(Cell::new(false));
        let inner = Rc::new(RefCell::new(Some(SessionInner {
            session_id,
            engine_mode: config.engine_mode,
            default_conflict_policy: config.context_property_conflict,
            mac_window,
            private_engine,
            roots: Vec::new(),
            secondary_roots: Vec::new(),
            customizer: None,
            loading_qml,
        })));

        Ok(RinUISession {
            core: Rc::clone(&self.core),
            inner,
            _app_lifetime: PhantomData,
            _not_send_sync: PhantomData,
        })
    }

    pub fn is_borrowed_application(&self) -> bool {
        self.core.is_borrowed
    }

    pub fn config_load_report(&self) -> &ConfigLoadReport {
        &self.core.config_load_report
    }

    pub fn capabilities(&self) -> PlatformCapabilities {
        self.core.capabilities
    }

    pub fn config_store(&self) -> &Rc<RefCell<ConfigStore>> {
        &self.core.store
    }

    pub fn set_language(&self, locale: &str) -> Result<TranslationLoadResult, HostError> {
        RuntimeCore::set_language(&self.core, locale)
    }

    pub fn register_translation_catalog(
        &self,
        catalog: TranslationCatalog,
    ) -> Result<TranslationLoadResult, HostError> {
        RuntimeCore::register_translation_catalog(&self.core, catalog)
    }

    pub fn current_locale(&self) -> Result<String, HostError> {
        self.core.current_locale()
    }

    pub fn system_locale(&self) -> String {
        TranslationManager::system_locale()
    }

    pub fn theme_manager(&self) -> Result<ThemeManagerRef<'_>, HostError> {
        self.core.borrow_theme_manager()
    }

    pub fn theme_manager_mut(&self) -> Result<ThemeManagerMut<'_>, HostError> {
        self.core.borrow_theme_manager_mut()
    }

    pub fn with_theme_manager<R>(
        &self,
        f: impl FnOnce(&ThemeManager) -> R,
    ) -> Result<R, HostError> {
        self.core.with_theme_manager(f)
    }

    pub fn with_theme_manager_mut<R>(
        &self,
        f: impl FnOnce(Pin<&mut ThemeManager>) -> R,
    ) -> Result<R, HostError> {
        self.core.with_theme_manager_mut(f)
    }

    pub fn process_events(&self) {
        if qt_bridge::rinui_is_on_app_thread() {
            if self.core.theme_borrow_state.get() != 0 {
                warn!("Skipping Qt process_events() while ThemeManager borrow guard is active");
                return;
            }
            if let Ok(_exec_guard) = QmlExecGuard::enter_host_call(Rc::clone(&self.core)) {
                qt_bridge::rinui_app_process_events();
            }
            // QmlExecGuard finishes or schedules cleanup. A borrowed application's
            // outer QML callback may still be active even when loopLevel() is zero.
        }
    }

    pub fn open_window_count(&self) -> usize {
        if !qt_bridge::rinui_is_on_app_thread() {
            return 0;
        }
        qt_bridge::rinui_runtime_alive_windows(self.core.runtime_id).len()
    }
}

struct SessionInner {
    session_id: u64,
    engine_mode: EngineMode,
    default_conflict_policy: ContextPropertyConflict,
    mac_window: MacWindowOptions,
    private_engine: Option<EngineState>,
    roots: Vec<RootId>,
    secondary_roots: Vec<RootId>,
    customizer: Option<Box<dyn WindowCustomizer>>,
    loading_qml: Rc<Cell<bool>>,
}

impl SessionInner {
    fn with_engine_mut<R>(
        &mut self,
        core: &RuntimeCore,
        f: impl FnOnce(&mut EngineState) -> Result<R, HostError>,
    ) -> Result<R, HostError> {
        if let Some(eng) = self.private_engine.as_mut() {
            return f(eng);
        }
        let mut guard = core
            .shared_engine
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("engine_mut"))?;
        let eng = guard.as_mut().ok_or(HostError::HostDisposed)?;
        f(eng)
    }

    fn with_engine_ref<R>(
        &self,
        core: &RuntimeCore,
        f: impl FnOnce(&EngineState) -> Result<R, HostError>,
    ) -> Result<R, HostError> {
        if let Some(eng) = self.private_engine.as_ref() {
            return f(eng);
        }
        let guard = core
            .shared_engine
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("engine_ref"))?;
        let eng = guard.as_ref().ok_or(HostError::HostDisposed)?;
        f(eng)
    }
}

/// Session representing an owning consumer QML slice under [`RinUIRuntime`].
///
/// Releasing a `RinUISession` disposes its [`SessionHandle`] clones, destroys only this
/// session's QML roots/windows, and tears down its private engine (when [`EngineMode::Private`])
/// without disturbing sibling sessions or an externally owned Qt application.
pub struct RinUISession<'app> {
    core: Rc<RuntimeCore>,
    inner: Rc<RefCell<Option<SessionInner>>>,
    _app_lifetime: PhantomData<&'app QGuiApplication>,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl<'app> Drop for RinUISession<'app> {
    fn drop(&mut self) {
        if !qt_bridge::rinui_is_on_app_thread() {
            error!("RinUISession dropped off Qt main thread");
        }
        let taken = self.inner.try_borrow_mut().ok().and_then(|mut g| g.take());
        if let Some(mut inner) = taken {
            inner.customizer.take();

            qt_bridge::rinui_session_release_all(inner.session_id);
            if let Some(eng_state) = inner.private_engine.take() {
                self.core.remove_live_engine(eng_state.engine_id);
                self.core.defer_engine_drop(eng_state);
            }
            if self.core.qml_exec_depth.get() == 0 && self.core.theme_borrow_state.get() == 0 {
                self.core.schedule_deferred_drain();
            }
        }
    }
}

impl<'app> RinUISession<'app> {
    pub fn engine_mode(&self) -> Result<EngineMode, HostError> {
        self.core.ensure_main_thread()?;
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("engine_mode"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        Ok(state.engine_mode)
    }

    pub fn handle(&self) -> SessionHandle<'app> {
        SessionHandle {
            core: Rc::downgrade(&self.core),
            inner: Rc::clone(&self.inner),
            _app_lifetime: PhantomData,
            _not_send_sync: PhantomData,
        }
    }

    pub fn set_window_customizer(&mut self, customizer: Box<dyn WindowCustomizer>) {
        if let Ok(mut guard) = self.inner.try_borrow_mut() {
            if let Some(state) = guard.as_mut() {
                state.customizer = Some(customizer);
            }
        }
    }

    pub fn add_import_path(&mut self, path: &str) -> Result<(), HostError> {
        self.core.ensure_main_thread()?;
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("add_import_path"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        state.with_engine_mut(&self.core, |eng| {
            eng.add_import_path(path);
            Ok(())
        })
    }

    pub fn import_paths(&self) -> Result<Vec<String>, HostError> {
        self.core.ensure_main_thread()?;
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("import_paths"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        state.with_engine_ref(&self.core, |eng| Ok(eng.import_paths()))
    }

    pub fn expose_qobject<T>(
        &mut self,
        name: &str,
        object: cxx::UniquePtr<T>,
    ) -> Result<(), HostError>
    where
        T: Upcast<cxx_qt::QObject> + UniquePtrTarget + 'static,
    {
        self.core.ensure_main_thread()?;
        if self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("expose_qobject"));
        }
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("expose_qobject"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        let policy = state.default_conflict_policy;
        state.with_engine_mut(&self.core, |eng| eng.expose_qobject(name, object, policy))
    }

    pub fn expose_qobject_with_policy<T>(
        &mut self,
        name: &str,
        object: cxx::UniquePtr<T>,
        policy: ContextPropertyConflict,
    ) -> Result<(), HostError>
    where
        T: Upcast<cxx_qt::QObject> + UniquePtrTarget + 'static,
    {
        self.core.ensure_main_thread()?;
        if self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("expose_qobject_with_policy"));
        }
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("expose_qobject_with_policy"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        state.with_engine_mut(&self.core, |eng| eng.expose_qobject(name, object, policy))
    }

    pub fn set_context_property_string(
        &mut self,
        name: &str,
        value: &str,
    ) -> Result<(), HostError> {
        self.set_context_property_variant(name, &QVariant::from(&QString::from(value)))
    }

    pub fn set_context_property_variant(
        &mut self,
        name: &str,
        value: &QVariant,
    ) -> Result<(), HostError> {
        self.core.ensure_main_thread()?;
        if self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("set_context_property_variant"));
        }
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("set_context_property_variant"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        let policy = state.default_conflict_policy;
        state.with_engine_mut(&self.core, |eng| {
            eng.set_context_property_variant(name, value, policy)
        })
    }

    pub fn set_context_property_variant_with_policy(
        &mut self,
        name: &str,
        value: &QVariant,
        policy: ContextPropertyConflict,
    ) -> Result<(), HostError> {
        self.core.ensure_main_thread()?;
        if self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall(
                "set_context_property_variant_with_policy",
            ));
        }
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("set_context_property_variant_with_policy"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        state.with_engine_mut(&self.core, |eng| {
            eng.set_context_property_variant(name, value, policy)
        })
    }

    pub fn register_translation_catalog(
        &mut self,
        catalog: TranslationCatalog,
    ) -> Result<TranslationLoadResult, HostError> {
        RuntimeCore::register_translation_catalog(&self.core, catalog)
    }

    pub fn set_language(&mut self, locale: &str) -> Result<TranslationLoadResult, HostError> {
        RuntimeCore::set_language(&self.core, locale)
    }

    pub fn current_locale(&self) -> Result<String, HostError> {
        self.core.current_locale()
    }

    pub fn load(&mut self, source: &QmlSource) -> Result<LoadedRoot, HostError> {
        let loaded = self.handle().load_internal(source, false)?;
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("load"))?;
        if let Some(state) = guard.as_mut() {
            if let Some(customizer) = state.customizer.as_mut() {
                customizer.on_windows_discovered(&loaded.windows);
            }
        }
        Ok(loaded)
    }

    pub fn set_icon(&mut self, icon_path: &str, root_id: Option<RootId>) -> Result<(), HostError> {
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&self.core))?;
        let (session_id, target_root) = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("set_icon"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            let target_root = root_id
                .or_else(|| state.roots.first().copied())
                .map(|r| r.0)
                .unwrap_or(0);
            (state.session_id, target_root)
        };
        qt_bridge::rinui_set_app_icon_owned(
            &QString::from(icon_path),
            session_id,
            target_root,
            true,
        );
        Ok(())
    }

    pub fn set_root_string_property(
        &mut self,
        root_id: RootId,
        name: &str,
        value: &str,
    ) -> Result<bool, HostError> {
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&self.core))?;
        let session_id = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("set_root_string_property"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            state.session_id
        };
        Ok(qt_bridge::rinui_root_set_property_owned(
            session_id,
            root_id.0,
            &QString::from(name),
            &QVariant::from(&QString::from(value)),
        ))
    }

    pub fn root_property(&self, root_id: RootId, name: &str) -> Result<QVariant, HostError> {
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&self.core))?;
        let session_id = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("root_property"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            state.session_id
        };
        Ok(qt_bridge::rinui_root_get_property_owned(
            session_id,
            root_id.0,
            &QString::from(name),
        ))
    }

    pub fn release_root(&mut self, root_id: RootId) -> Result<(), HostError> {
        self.handle().release_root(root_id)
    }

    pub fn is_root_alive(&self, root_id: RootId) -> bool {
        self.handle().is_root_alive(root_id)
    }

    pub fn close_secondary_windows(&mut self) -> usize {
        self.handle().close_secondary_windows()
    }

    pub fn open_window_count(&self) -> usize {
        self.handle().open_window_count()
    }

    pub fn sync_windows(&mut self) -> Result<Vec<NativeWindowRef>, HostError> {
        let discovered = self.handle().sync_windows()?;
        if !discovered.is_empty() {
            if let Ok(mut guard) = self.inner.try_borrow_mut() {
                if let Some(state) = guard.as_mut() {
                    if let Some(customizer) = state.customizer.as_mut() {
                        customizer.on_windows_discovered(&discovered);
                    }
                }
            }
        }
        Ok(discovered)
    }

    /// Explicitly attribute and register a dynamic `QQuickWindow` stored on `property_name`
    /// of a session-owned root object (use this when QML creates windows via singleton/detached
    /// factories with no identifiable parent/context owner).
    pub fn register_window_from_property(
        &mut self,
        root_id: RootId,
        property_name: &str,
    ) -> Result<NativeWindowRef, HostError> {
        let win = self
            .handle()
            .register_window_from_property(root_id, property_name)?;
        if let Ok(mut guard) = self.inner.try_borrow_mut() {
            if let Some(state) = guard.as_mut() {
                if let Some(customizer) = state.customizer.as_mut() {
                    customizer.on_windows_discovered(&[win]);
                }
            }
        }
        Ok(win)
    }

    pub fn theme_manager(&self) -> Result<ThemeManagerRef<'_>, HostError> {
        self.core.borrow_theme_manager()
    }

    pub fn theme_manager_mut(&mut self) -> Result<ThemeManagerMut<'_>, HostError> {
        self.core.borrow_theme_manager_mut()
    }

    pub fn with_theme_manager<R>(
        &self,
        f: impl FnOnce(&ThemeManager) -> R,
    ) -> Result<R, HostError> {
        self.core.with_theme_manager(f)
    }

    pub fn with_theme_manager_mut<R>(
        &self,
        f: impl FnOnce(Pin<&mut ThemeManager>) -> R,
    ) -> Result<R, HostError> {
        self.core.with_theme_manager_mut(f)
    }

    pub fn config_store(&self) -> &Rc<RefCell<ConfigStore>> {
        &self.core.store
    }

    pub fn resolved_resources(&self) -> Result<ResolvedResources, HostError> {
        self.core.ensure_main_thread()?;
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("resolved_resources"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        state.with_engine_ref(&self.core, |eng| Ok(eng.resource_view.resolved().clone()))
    }

    pub fn resource_view_describe(&self) -> Result<String, HostError> {
        self.core.ensure_main_thread()?;
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("resource_view_describe"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        state.with_engine_ref(&self.core, |eng| Ok(eng.resource_view.describe()))
    }

    pub fn qml_warnings(&self) -> Vec<String> {
        if !qt_bridge::rinui_is_on_app_thread() {
            return Vec::new();
        }
        let Ok(guard) = self.inner.try_borrow() else {
            return Vec::new();
        };
        let Some(state) = guard.as_ref() else {
            return Vec::new();
        };
        state
            .with_engine_ref(&self.core, |eng| Ok(eng.qml_warnings()))
            .unwrap_or_default()
    }

    pub fn process_events(&self) {
        if qt_bridge::rinui_is_on_app_thread() {
            if self.core.theme_borrow_state.get() != 0 {
                warn!("Skipping Qt process_events() while ThemeManager borrow guard is active");
                return;
            }
            if let Ok(_exec_guard) = QmlExecGuard::enter_host_call(Rc::clone(&self.core)) {
                qt_bridge::rinui_app_process_events();
            }
            // QmlExecGuard finishes or schedules cleanup. A borrowed application's
            // outer QML callback may still be active even when loopLevel() is zero.
        }
    }

    pub fn clear_component_cache(&mut self) -> Result<(), HostError> {
        self.core.ensure_main_thread()?;
        if self.core.qml_exec_depth.get() > 0 || self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("clear_component_cache"));
        }
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("clear_component_cache"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        state.with_engine_mut(&self.core, |eng| {
            eng.clear_component_cache();
            Ok(())
        })
    }

    pub fn collect_garbage(&mut self) -> Result<(), HostError> {
        self.core.ensure_main_thread()?;
        if self.core.qml_exec_depth.get() > 0 || self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("collect_garbage"));
        }
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("collect_garbage"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        state.with_engine_mut(&self.core, |eng| {
            eng.collect_garbage();
            Ok(())
        })
    }

    /// Read a root object property with strict existence & session-ownership checks.
    pub fn get_root_property(&self, root_id: RootId, name: &str) -> Result<QVariant, HostError> {
        self.handle().get_root_property(root_id, name)
    }

    /// Write a declared meta-property on a session-owned root QML object with strict type checking.
    ///
    /// Returns [`HostError::PropertyNotFound`] for unknown property names (never creates dynamic
    /// properties on typos), [`HostError::PropertyReadOnly`] for `readonly` properties, and
    /// [`HostError::PropertyTypeMismatch`] on incompatible `QVariant` types.
    pub fn set_root_property(
        &self,
        root_id: RootId,
        name: &str,
        value: &QVariant,
    ) -> Result<(), HostError> {
        self.handle().set_root_property(root_id, name, value)
    }

    /// Synchronously invoke a QML method or Qt slot on a session-owned root object via
    /// Qt's meta-object system (`QMetaMethod::invoke`).
    pub fn invoke_root_method(
        &self,
        root_id: RootId,
        method_name: &str,
        args: &[QVariant],
    ) -> Result<QVariant, HostError> {
        self.handle().invoke_root_method(root_id, method_name, args)
    }

    /// Scoped, main-thread configuration access to this session's underlying `QQmlApplicationEngine`.
    pub fn configure_engine<R>(
        &mut self,
        f: impl FnOnce(&mut EngineConfigurator<'_>) -> R,
    ) -> Result<R, HostError> {
        self.core.ensure_main_thread()?;
        if self.core.qml_exec_depth.get() > 0 || self.core.theme_borrow_state.get() != 0 {
            return Err(HostError::ReentrantCall("configure_engine"));
        }
        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("configure_engine"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        if state.loading_qml.get() {
            return Err(HostError::ReentrantCall("configure_engine"));
        }
        state.with_engine_mut(&self.core, |eng| {
            let eng_pin = eng.engine.as_mut().ok_or(HostError::EngineInitFailed)?;
            let mut configurator = EngineConfigurator {
                engine: eng_pin,
                _not_send_sync: PhantomData,
            };
            Ok(f(&mut configurator))
        })
    }

    pub fn resource_view(&self) -> Result<ResourceView, HostError> {
        self.core.ensure_main_thread()?;
        self.resource_view_cloned()
    }

    pub(crate) fn resource_view_cloned(&self) -> Result<ResourceView, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("resource_view"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        state.with_engine_ref(&self.core, |eng| Ok(eng.resource_view.clone()))
    }
}

/// Scoped configurator providing main-thread configuration access to `QQmlApplicationEngine`
/// inside [`RinUISession::configure_engine`] and [`crate::RinUIHost::configure_engine`].
///
/// `!Send + !Sync`: cannot escape the configuration closure or outlive the engine borrow.
pub struct EngineConfigurator<'eng> {
    engine: Pin<&'eng mut QQmlApplicationEngine>,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl<'eng> EngineConfigurator<'eng> {
    pub fn add_import_path(&mut self, path: &Path) {
        let clean = strip_windows_verbatim_prefix(path.to_path_buf());
        self.engine
            .as_mut()
            .add_import_path(&QString::from(clean.to_string_lossy().as_ref()));
    }

    pub fn import_paths(&self) -> Vec<String> {
        self.engine
            .as_ref()
            .import_path_list()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn add_plugin_path(&mut self, path: &Path) {
        let clean = strip_windows_verbatim_prefix(path.to_path_buf());
        self.engine
            .as_mut()
            .add_plugin_path(&QString::from(clean.to_string_lossy().as_ref()));
    }

    pub fn plugin_paths(&self) -> Vec<String> {
        self.engine
            .as_ref()
            .plugin_path_list()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn base_url(&self) -> String {
        self.engine.as_ref().base_url().to_string()
    }

    pub fn set_base_url(&mut self, url: &str) {
        self.engine.as_mut().set_base_url(&QUrl::from(url));
    }

    pub fn offline_storage_path(&self) -> String {
        self.engine.as_ref().offline_storage_path().to_string()
    }

    pub fn set_offline_storage_path(&mut self, path: &Path) {
        let clean = strip_windows_verbatim_prefix(path.to_path_buf());
        self.engine
            .as_mut()
            .set_offline_storage_path(&QString::from(clean.to_string_lossy().as_ref()));
    }

    pub fn set_extra_file_selectors(&mut self, selectors: &[&str]) {
        let mut list = QStringList::default();
        for selector in selectors {
            list.append(QString::from(*selector));
        }
        qt_bridge::rinui_engine_set_extra_file_selectors(self.engine.as_mut(), &list);
    }

    pub fn collect_garbage(&mut self) {
        qt_bridge::rinui_engine_collect_garbage(self.engine.as_mut());
    }

    /// Controlled escape hatch for advanced Qt engine configuration not yet wrapped directly.
    ///
    /// # Safety
    ///
    /// Callers must NOT destroy the engine, override `RinUISession` ownership tracking, persist
    /// references or raw pointers to `QQmlApplicationEngine` past the closure lifetime `'eng`,
    /// or move it off the Qt main thread.
    pub unsafe fn with_raw_engine_mut<R>(
        &mut self,
        f: impl FnOnce(Pin<&mut QQmlApplicationEngine>) -> R,
    ) -> R {
        f(self.engine.as_mut())
    }
}

/// Cloneable main-thread handle tied to a [`RinUISession`] and [`RinUIRuntime`].
///
/// `!Send + !Sync`: becomes disposed (`HostError::HostDisposed`) immediately when its owning
/// [`RinUISession`] drops, preventing dangling Qt access and avoiding extending `RuntimeCore`
/// lifetime past session/runtime teardown.
#[derive(Clone)]
pub struct SessionHandle<'app> {
    core: Weak<RuntimeCore>,
    inner: Rc<RefCell<Option<SessionInner>>>,
    _app_lifetime: PhantomData<&'app QGuiApplication>,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl<'app> SessionHandle<'app> {
    fn core(&self) -> Result<Rc<RuntimeCore>, HostError> {
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("session_handle"))?;
        if guard.is_none() {
            return Err(HostError::HostDisposed);
        }
        drop(guard);
        self.core.upgrade().ok_or(HostError::HostDisposed)
    }

    pub fn set_language(&self, locale: &str) -> Result<TranslationLoadResult, HostError> {
        let core = self.core()?;
        RuntimeCore::set_language(&core, locale)
    }

    pub fn current_locale(&self) -> Result<String, HostError> {
        let core = self.core()?;
        core.current_locale()
    }

    pub fn system_locale(&self) -> String {
        TranslationManager::system_locale()
    }

    pub fn register_translation_catalog(
        &self,
        catalog: TranslationCatalog,
    ) -> Result<TranslationLoadResult, HostError> {
        let core = self.core()?;
        RuntimeCore::register_translation_catalog(&core, catalog)
    }

    pub fn load_secondary_window(&self, source: &QmlSource) -> Result<LoadedRoot, HostError> {
        self.load_internal(source, true)
    }

    fn load_internal(
        &self,
        source: &QmlSource,
        record_as_secondary: bool,
    ) -> Result<LoadedRoot, HostError> {
        let core = self.core()?;
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&core))?;
        let (session_id, mac_window, engine_ptr, engine_id, resources_desc, loading_flag) = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("load"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            let (ptr, eid, desc) = state.with_engine_ref(&core, |eng| {
                Ok((eng.engine_ptr, eng.engine_id, eng.resource_view.describe()))
            })?;
            (
                state.session_id,
                state.mac_window,
                ptr,
                eid,
                desc,
                Rc::clone(&state.loading_qml),
            )
        };

        if loading_flag.get() {
            return Err(HostError::ReentrantCall(if record_as_secondary {
                "load_secondary_window"
            } else {
                "load"
            }));
        }
        loading_flag.set(true);
        let _loading_guard = LoadingGuard(loading_flag);

        let eng_pin = unsafe { Pin::new_unchecked(&mut *engine_ptr) };
        let loaded = load_qml_on_engine(
            eng_pin,
            core.runtime_id,
            engine_id,
            session_id,
            &self.inner,
            source,
            &resources_desc,
            mac_window,
            &core,
        )?;

        let mut guard = self
            .inner
            .try_borrow_mut()
            .map_err(|_| HostError::ReentrantCall("load"))?;
        let state = guard.as_mut().ok_or(HostError::HostDisposed)?;
        state.roots.push(loaded.root_id);
        if record_as_secondary {
            state.secondary_roots.push(loaded.root_id);
        }
        Ok(loaded)
    }

    pub fn release_root(&self, root_id: RootId) -> Result<(), HostError> {
        let core = self.core()?;
        let _exec_guard = QmlExecGuard::enter_host_call(core)?;
        let session_id = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("release_root"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            state.session_id
        };

        let released = qt_bridge::rinui_root_release_owned(session_id, root_id.0);
        if !released {
            return Err(HostError::RootNotOwned(root_id.0));
        }

        if let Ok(mut guard) = self.inner.try_borrow_mut() {
            if let Some(state) = guard.as_mut() {
                state.roots.retain(|id| *id != root_id);
                state.secondary_roots.retain(|id| *id != root_id);
            }
        }
        Ok(())
    }

    pub fn is_root_alive(&self, root_id: RootId) -> bool {
        if !qt_bridge::rinui_is_on_app_thread() {
            return false;
        }
        let Ok(guard) = self.inner.try_borrow() else {
            return false;
        };
        let Some(state) = guard.as_ref() else {
            return false;
        };
        qt_bridge::rinui_root_is_alive_owned(state.session_id, root_id.0)
    }

    pub fn close_secondary_windows(&self) -> usize {
        let Ok(core) = self.core() else {
            return 0;
        };
        let Ok(_exec_guard) = QmlExecGuard::enter_host_call(core) else {
            return 0;
        };
        let (session_id, ids) = {
            let Ok(mut guard) = self.inner.try_borrow_mut() else {
                return 0;
            };
            let Some(state) = guard.as_mut() else {
                return 0;
            };
            let ids = std::mem::take(&mut state.secondary_roots);
            state.roots.retain(|id| !ids.contains(id));
            (state.session_id, ids)
        };
        let mut count = 0usize;
        for root_id in ids {
            if qt_bridge::rinui_root_release_owned(session_id, root_id.0) {
                count += 1;
            }
        }
        count
    }

    pub fn open_window_count(&self) -> usize {
        if !qt_bridge::rinui_is_on_app_thread() {
            return 0;
        }
        let Ok(guard) = self.inner.try_borrow() else {
            return 0;
        };
        let Some(state) = guard.as_ref() else {
            return 0;
        };
        qt_bridge::rinui_session_alive_windows(state.session_id).len()
    }

    pub fn sync_windows(&self) -> Result<Vec<NativeWindowRef>, HostError> {
        let core = self.core()?;
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&core))?;
        let (session_id, is_private_engine, mac_window, engine_ptr, engine_id) = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("sync_windows"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            let (ptr, eid) =
                state.with_engine_ref(&core, |eng| Ok((eng.engine_ptr, eng.engine_id)))?;
            (
                state.session_id,
                state.engine_mode == EngineMode::Private,
                state.mac_window,
                ptr,
                eid,
            )
        };

        let eng_pin = unsafe { Pin::new_unchecked(&mut *engine_ptr) };
        let ids = qt_bridge::rinui_sync_session_windows(
            eng_pin,
            core.runtime_id,
            engine_id,
            session_id,
            is_private_engine,
            mac_window.effective_native_frame(),
            mac_window.traffic_lights_offset_x,
            mac_window.traffic_lights_offset_down,
        );
        core.sync_platform_appearance();
        Ok(ids
            .into_iter()
            .map(|id| NativeWindowRef::new(session_id, WindowId(id)))
            .collect())
    }

    /// Explicitly attribute and register a dynamic `QQuickWindow` referenced by `property_name`
    /// on a session-owned root object.
    pub fn register_window_from_property(
        &self,
        root_id: RootId,
        property_name: &str,
    ) -> Result<NativeWindowRef, HostError> {
        let core = self.core()?;
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&core))?;
        let (session_id, mac_window, engine_ptr, engine_id) = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("register_window_from_property"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            let (ptr, eid) =
                state.with_engine_ref(&core, |eng| Ok((eng.engine_ptr, eng.engine_id)))?;
            (state.session_id, state.mac_window, ptr, eid)
        };

        let eng_pin = unsafe { Pin::new_unchecked(&mut *engine_ptr) };
        let mut window_id = 0u64;
        let mut status = 0i32;
        let mut detail = QString::default();
        let ok = qt_bridge::rinui_register_window_from_root_property(
            eng_pin,
            core.runtime_id,
            engine_id,
            session_id,
            root_id.0,
            &QString::from(property_name),
            mac_window.effective_native_frame(),
            mac_window.traffic_lights_offset_x,
            mac_window.traffic_lights_offset_down,
            &mut window_id,
            &mut status,
            &mut detail,
        );
        if ok && status == 0 && window_id != 0 {
            core.sync_platform_appearance();
            return Ok(NativeWindowRef::new(session_id, WindowId(window_id)));
        }
        Err(match status {
            1 => HostError::HostDisposed,
            2 => HostError::RootNotOwned(root_id.0),
            6 => HostError::WindowNotOwned(window_id),
            _ => HostError::WindowRegistrationFailed {
                property: property_name.to_owned(),
                detail: detail.to_string(),
            },
        })
    }

    pub fn platform_name(&self) -> String {
        qt_bridge::rinui_platform_name().to_string()
    }

    pub fn resource_mode_description(&self) -> Result<String, HostError> {
        let core = self.core()?;
        core.ensure_main_thread()?;
        let guard = self
            .inner
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("resource_mode_description"))?;
        let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
        state.with_engine_ref(&core, |eng| Ok(eng.resource_view.describe()))
    }

    pub fn config_path(&self) -> Result<Option<PathBuf>, HostError> {
        let core = self.core()?;
        core.ensure_main_thread()?;
        let store = core
            .store
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("config_path"))?;
        Ok(store.path().map(Path::to_path_buf))
    }

    pub fn is_backdrop_supported(&self) -> Result<bool, HostError> {
        let core = self.core()?;
        core.ensure_main_thread()?;
        Ok(core.capabilities.is_windows())
    }

    pub fn current_theme_name(&self) -> Result<String, HostError> {
        let core = self.core()?;
        core.ensure_main_thread()?;
        let store = core
            .store
            .try_borrow()
            .map_err(|_| HostError::ReentrantCall("current_theme_name"))?;
        Ok(store.theme().as_str().to_owned())
    }

    pub fn actual_theme(&self) -> Result<String, HostError> {
        let core = self.core()?;
        core.ensure_main_thread()?;
        Ok(core.resolved_theme_str())
    }

    /// Read a property on a session-owned root QObject with strict existence & ownership checks.
    pub fn get_root_property(&self, root_id: RootId, name: &str) -> Result<QVariant, HostError> {
        let core = self.core()?;
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&core))?;
        let session_id = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("get_root_property"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            state.session_id
        };

        let mut status = 0i32;
        let mut detail = QString::default();
        let val = qt_bridge::rinui_root_get_property_checked(
            session_id,
            root_id.0,
            &QString::from(name),
            &mut status,
            &mut detail,
        );
        match status {
            0 => Ok(val),
            1 => Err(HostError::HostDisposed),
            2 => Err(HostError::RootNotOwned(root_id.0)),
            3 => Err(HostError::PropertyNotFound {
                property: name.to_owned(),
            }),
            _ => Err(HostError::PropertyNotFound {
                property: format!("{name}: {}", detail),
            }),
        }
    }

    /// Write a declared meta-property on a session-owned root QML object with strict type checking.
    pub fn set_root_property(
        &self,
        root_id: RootId,
        name: &str,
        value: &QVariant,
    ) -> Result<(), HostError> {
        let core = self.core()?;
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&core))?;
        let session_id = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("set_root_property"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            state.session_id
        };

        let mut detail = QString::default();
        let status = qt_bridge::rinui_root_set_property_checked(
            session_id,
            root_id.0,
            &QString::from(name),
            value,
            &mut detail,
        );
        match status {
            0 => Ok(()),
            1 => Err(HostError::HostDisposed),
            2 => Err(HostError::RootNotOwned(root_id.0)),
            3 => Err(HostError::PropertyNotFound {
                property: name.to_owned(),
            }),
            4 => Err(HostError::PropertyReadOnly {
                property: name.to_owned(),
            }),
            _ => Err(HostError::PropertyTypeMismatch {
                property: name.to_owned(),
                detail: detail.to_string(),
            }),
        }
    }

    /// Synchronously invoke a QML method or Qt slot on a session-owned root object via
    /// Qt meta-object dispatch (`QMetaMethod::invoke`).
    pub fn invoke_root_method(
        &self,
        root_id: RootId,
        method_name: &str,
        args: &[QVariant],
    ) -> Result<QVariant, HostError> {
        let core = self.core()?;
        let _exec_guard = QmlExecGuard::enter_host_call(Rc::clone(&core))?;
        let _invoke_guard = InvokeMethodGuard::enter(Rc::clone(&core))?;
        let session_id = {
            let guard = self
                .inner
                .try_borrow()
                .map_err(|_| HostError::ReentrantCall("invoke_root_method"))?;
            let state = guard.as_ref().ok_or(HostError::HostDisposed)?;
            state.session_id
        };

        let mut status = 0i32;
        let mut detail = QString::default();
        let ret = qt_bridge::rinui_root_invoke_method(
            session_id,
            root_id.0,
            &QString::from(method_name),
            args,
            &mut status,
            &mut detail,
        );
        match status {
            0 => Ok(ret),
            1 => Err(HostError::HostDisposed),
            2 => Err(HostError::RootNotOwned(root_id.0)),
            3 => Err(HostError::MethodNotFound {
                method: method_name.to_owned(),
            }),
            4 => Err(HostError::MethodArgumentCountMismatch {
                method: method_name.to_owned(),
                detail: detail.to_string(),
            }),
            5 => Err(HostError::UnsupportedMethodSignature {
                method: method_name.to_owned(),
                detail: detail.to_string(),
            }),
            7 => Err(HostError::TooManyMethodArguments {
                method: method_name.to_owned(),
                count: args.len(),
            }),
            _ => Err(HostError::MethodInvocationFailed {
                method: method_name.to_owned(),
                detail: detail.to_string(),
            }),
        }
    }

    pub fn copy_to_clipboard(&self, text: &str) {
        if qt_bridge::rinui_is_on_app_thread() {
            qt_bridge::rinui_set_clipboard_text(&QString::from(text));
        }
    }

    pub fn schedule_quit(&self, delay_ms: i32) {
        if qt_bridge::rinui_is_on_app_thread() {
            qt_bridge::rinui_schedule_quit_after_ms(delay_ms);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn load_qml_on_engine(
    mut engine: Pin<&mut QQmlApplicationEngine>,
    runtime_id: u64,
    engine_id: u64,
    session_id: u64,
    session_inner: &RefCell<Option<SessionInner>>,
    source: &QmlSource,
    resources_desc: &str,
    mac_window: MacWindowOptions,
    core: &RuntimeCore,
) -> Result<LoadedRoot, HostError> {
    let before_count = qt_bridge::rinui_engine_root_count(&engine.as_ref());
    qt_bridge::rinui_engine_clear_warnings(&engine.as_ref());

    let url = source.to_qurl();
    engine.as_mut().load(&url);

    let after_count = qt_bridge::rinui_engine_root_count(&engine.as_ref());
    let session_disposed = session_inner
        .try_borrow()
        .map(|g| g.is_none())
        .unwrap_or(false);
    if session_disposed {
        if after_count > before_count {
            qt_bridge::rinui_engine_discard_latest_unregistered_root(engine);
        }
        return Err(HostError::HostDisposed);
    }

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

    let root_id_raw = qt_bridge::rinui_engine_register_latest_root_owned(
        engine,
        runtime_id,
        engine_id,
        session_id,
        mac_window.effective_native_frame(),
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

    let window_ids = qt_bridge::rinui_root_windows_owned(session_id, root_id_raw);
    let windows: Vec<NativeWindowRef> = window_ids
        .into_iter()
        .map(|id| NativeWindowRef::new(session_id, WindowId(id)))
        .collect();

    core.sync_platform_appearance();
    if cfg!(target_os = "windows") && !windows.is_empty() {
        let effect = core.current_backdrop();
        let _ = core.with_theme_manager_mut(|tm_pin| {
            tm_pin.apply_backdrop_effect(QString::from(effect.as_str()))
        });
    }

    debug!(
        source = %source.describe(),
        session_id,
        engine_id,
        root_id = root_id_raw,
        window_count = windows.len(),
        "Loaded QML root"
    );

    Ok(LoadedRoot {
        root_id: RootId(root_id_raw),
        windows,
    })
}
