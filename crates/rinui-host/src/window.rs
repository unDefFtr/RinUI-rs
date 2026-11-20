use crate::bridge::ffi as qt_bridge;
use cxx_qt_lib::{QString, QVariant};
use std::marker::PhantomData;

/// Opaque identifier for a root QML object tree created by `QQmlApplicationEngine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RootId(pub(crate) u64);

impl RootId {
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Opaque identifier for a tracked `QQuickWindow` managed by `RinUIHost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) u64);

impl WindowId {
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// Metadata for a loaded QML root and its discovered RinUI windows.
#[derive(Debug, Clone)]
pub struct LoadedRoot {
    pub root_id: RootId,
    pub windows: Vec<NativeWindowRef>,
}

/// Configuration for native macOS `NSWindow` titlebar integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacWindowOptions {
    /// Opt into Cocoa `NSWindowStyleMaskFullSizeContentView` + transparent titlebar integration.
    /// Defaults to `false`, using RinUI's QML titlebar and traffic-light controls.
    pub enabled: bool,
    /// Horizontal offset applied to native traffic lights (points).
    pub traffic_lights_offset_x: i32,
    /// Downward offset applied to native traffic lights (points).
    pub traffic_lights_offset_down: i32,
}

impl Default for MacWindowOptions {
    fn default() -> Self {
        Self::self_drawn()
    }
}

impl MacWindowOptions {
    /// Use RinUI's self-drawn QML titlebar and traffic-light controls on macOS.
    ///
    /// This is the default for `MacWindowOptions::default()` and avoids unexpected
    /// `NSWindow` frame/mask changes in host integrations unless explicitly opted in.
    pub const fn self_drawn() -> Self {
        Self {
            enabled: false,
            traffic_lights_offset_x: 10,
            traffic_lights_offset_down: 10,
        }
    }

    /// Explicitly opt into upstream RinUI QML's native macOS titlebar behavior
    /// (`useNativeMacFrame = true` on macOS, normal platform frame on other OSes).
    pub const fn upstream_compatible() -> Self {
        Self {
            enabled: cfg!(target_os = "macos"),
            traffic_lights_offset_x: 10,
            traffic_lights_offset_down: 10,
        }
    }

    /// Enable Cocoa native macOS titlebar integration with custom traffic-light offsets.
    pub const fn native(offset_x: i32, offset_down: i32) -> Self {
        Self {
            enabled: cfg!(target_os = "macos"),
            traffic_lights_offset_x: offset_x,
            traffic_lights_offset_down: offset_down,
        }
    }

    /// Whether native Cocoa macOS frame hooks should be activated for windows using this policy.
    pub const fn effective_native_frame(&self) -> bool {
        self.enabled && cfg!(target_os = "macos")
    }
}

/// Safe main-thread handle to a tracked `QQuickWindow`.
///
/// `!Send + !Sync`: must only be used on the Qt GUI thread.
/// Backed by a session-scoped Qt `QPointer<QQuickWindow>` table in C++, so dangling or cross-session window dereferences are impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWindowRef {
    id: WindowId,
    session_id: u64,
    _not_send_sync: PhantomData<*const ()>,
}

impl NativeWindowRef {
    pub(crate) fn new(session_id: u64, id: WindowId) -> Self {
        Self {
            id,
            session_id,
            _not_send_sync: PhantomData,
        }
    }

    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn is_alive(&self) -> bool {
        qt_bridge::rinui_is_on_app_thread()
            && qt_bridge::rinui_window_is_alive_owned(self.session_id, self.id.0)
    }

    /// Native platform handle (`NSView*` on macOS, `HWND` on Windows, XID on X11; `0` if unavailable or off-thread).
    pub fn win_id(&self) -> usize {
        if !qt_bridge::rinui_is_on_app_thread() {
            return 0;
        }
        qt_bridge::rinui_window_win_id_owned(self.session_id, self.id.0)
    }

    pub fn property(&self, name: &str) -> QVariant {
        if !qt_bridge::rinui_is_on_app_thread() {
            return QVariant::default();
        }
        crate::runtime::with_active_qml_exec_guard(|| {
            qt_bridge::rinui_window_get_property_owned(
                self.session_id,
                self.id.0,
                &QString::from(name),
            )
        })
        .unwrap_or_default()
    }

    pub fn set_property(&self, name: &str, value: &QVariant) -> bool {
        if !qt_bridge::rinui_is_on_app_thread() {
            return false;
        }
        crate::runtime::with_active_qml_exec_guard(|| {
            qt_bridge::rinui_window_set_property_owned(
                self.session_id,
                self.id.0,
                &QString::from(name),
                value,
            )
        })
        .unwrap_or(false)
    }

    pub fn set_bool_property(&self, name: &str, value: bool) -> bool {
        self.set_property(name, &QVariant::from(&value))
    }

    pub fn set_string_property(&self, name: &str, value: &str) -> bool {
        self.set_property(name, &QVariant::from(&QString::from(value)))
    }

    pub fn show(&self) {
        if qt_bridge::rinui_is_on_app_thread() {
            let _ = crate::runtime::with_active_qml_exec_guard(|| {
                qt_bridge::rinui_window_show_owned(self.session_id, self.id.0);
            });
        }
    }

    pub fn close(&self) {
        if qt_bridge::rinui_is_on_app_thread() {
            let _ = crate::runtime::with_active_qml_exec_guard(|| {
                qt_bridge::rinui_window_close_owned(self.session_id, self.id.0);
            });
        }
    }
}

/// Optional consumer hook invoked whenever RinUI windows are discovered or synchronized.
///
/// Applications such as Goosy can implement this trait to attach custom window-level native
/// behavior without coupling `rinui-host` to application specifics.
pub trait WindowCustomizer {
    /// Called on the Qt GUI thread after `windows` are registered and platform setup runs.
    fn on_windows_discovered(&mut self, windows: &[NativeWindowRef]);
}
