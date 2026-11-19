use crate::bridge::ffi as qt_bridge;
use cxx_qt_lib::{QString, QVariant};

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
    /// Enable Cocoa `NSWindowStyleMaskFullSizeContentView` + transparent titlebar integration.
    pub enabled: bool,
    /// Horizontal offset applied to native traffic lights (points).
    pub traffic_lights_offset_x: i32,
    /// Downward offset applied to native traffic lights (points).
    pub traffic_lights_offset_down: i32,
}

impl Default for MacWindowOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            traffic_lights_offset_x: 10,
            traffic_lights_offset_down: 10,
        }
    }
}

/// Safe main-thread handle to a tracked `QQuickWindow`.
///
/// Backed by a Qt `QPointer<QQuickWindow>` table in C++, so dangling window dereferences are impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWindowRef {
    id: WindowId,
}

impl NativeWindowRef {
    pub(crate) fn new(id: WindowId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn is_alive(&self) -> bool {
        qt_bridge::rinui_window_is_alive(self.id.0)
    }

    /// Native platform handle (`NSView*` on macOS, `HWND` on Windows, XID on X11; `0` if unavailable).
    pub fn win_id(&self) -> usize {
        qt_bridge::rinui_window_win_id(self.id.0)
    }

    pub fn property(&self, name: &str) -> QVariant {
        qt_bridge::rinui_window_get_property(self.id.0, &QString::from(name))
    }

    pub fn set_property(&self, name: &str, value: &QVariant) -> bool {
        qt_bridge::rinui_window_set_property(self.id.0, &QString::from(name), value)
    }

    pub fn set_bool_property(&self, name: &str, value: bool) -> bool {
        self.set_property(name, &QVariant::from(&value))
    }

    pub fn set_string_property(&self, name: &str, value: &str) -> bool {
        self.set_property(name, &QVariant::from(&QString::from(value)))
    }

    pub fn show(&self) {
        qt_bridge::rinui_window_show(self.id.0);
    }

    pub fn close(&self) {
        qt_bridge::rinui_window_close(self.id.0);
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
