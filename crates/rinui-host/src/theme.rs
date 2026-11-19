use crate::bridge::ffi as qt_bridge;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use rinui_platform::{BackdropEffect, ConfigStore, PlatformCapabilities, ResolvedTheme, ThemeMode};
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::rc::Rc;
use tracing::{debug, error, warn};

fn guard_ffi<R>(op: &'static str, fallback: R, f: impl FnOnce() -> R) -> R {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => {
            error!(
                operation = op,
                "Recovered from panic at ThemeManager FFI boundary"
            );
            fallback
        }
    }
}

#[cxx_qt::bridge(namespace = "rinui")]
pub mod qobject {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("rinui-host/rinui_theme_bridge.h");

        pub fn rinui_theme_manager_new() -> UniquePtr<ThemeManager>;
        pub fn rinui_connect_color_scheme_changed(manager: Pin<&mut ThemeManager>);
    }

    extern "RustQt" {
        /// Upstream-compatible `ThemeManager` QObject exposed as the `"ThemeManager"` QML context property.
        #[qobject]
        type ThemeManager = super::ThemeManagerRust;

        #[qinvokable]
        fn toggle_theme(self: Pin<&mut ThemeManager>, theme: QString);

        #[qinvokable]
        fn get_theme(self: &ThemeManager) -> QString;

        #[qinvokable]
        fn get_theme_name(self: &ThemeManager) -> QString;

        #[qinvokable]
        fn apply_backdrop_effect(self: Pin<&mut ThemeManager>, effect_type: QString) -> i32;

        #[qinvokable]
        fn get_backdrop_effect(self: &ThemeManager) -> QString;

        #[qinvokable]
        fn set_theme_color(self: Pin<&mut ThemeManager>, color: QString);

        #[qinvokable]
        fn get_theme_color(self: &ThemeManager) -> QString;

        fn handle_system_theme_changed(self: Pin<&mut ThemeManager>);
    }

    unsafe extern "RustQt" {
        #[qsignal]
        #[cxx_name = "themeChanged"]
        fn theme_changed(self: Pin<&mut ThemeManager>, theme: QString);

        #[qsignal]
        #[cxx_name = "backdropChanged"]
        fn backdrop_changed(self: Pin<&mut ThemeManager>, effect: QString);
    }
}

pub use qobject::ThemeManager;

/// Backing Rust state for `rinui::ThemeManager`.
#[derive(Default)]
pub struct ThemeManagerRust {
    store: Option<Rc<RefCell<ConfigStore>>>,
    fallback_theme: ThemeMode,
    fallback_color: String,
    fallback_backdrop: BackdropEffect,
}

impl ThemeManager {
    /// Attach the shared `ConfigStore` owned by `RinUIHost`.
    pub(crate) fn attach_store(mut self: Pin<&mut Self>, store: Rc<RefCell<ConfigStore>>) {
        self.as_mut().rust_mut().store = Some(store);
    }

    /// Apply current theme and backdrop state to registered native windows without changing config.
    pub(crate) fn sync_platform_appearance(&self) {
        let actual = self.resolved_theme();
        let configured = self.configured_theme();
        qt_bridge::rinui_platform_apply_theme(
            &QString::from(actual.as_str()),
            &QString::from(configured.as_str()),
        );
    }

    pub(crate) fn configured_theme(&self) -> ThemeMode {
        if let Some(store) = &self.rust().store {
            if let Ok(guard) = store.try_borrow() {
                return guard.theme();
            }
            warn!("ConfigStore is currently borrowed; using fallback theme");
        }
        self.rust().fallback_theme
    }

    pub(crate) fn resolved_theme(&self) -> ResolvedTheme {
        match self.configured_theme() {
            ThemeMode::Dark => ResolvedTheme::Dark,
            ThemeMode::Light => ResolvedTheme::Light,
            ThemeMode::Auto => {
                if qt_bridge::rinui_current_system_theme().to_string() == "Dark" {
                    ResolvedTheme::Dark
                } else {
                    ResolvedTheme::Light
                }
            }
        }
    }

    pub(crate) fn current_backdrop(&self) -> BackdropEffect {
        if let Some(store) = &self.rust().store {
            if let Ok(guard) = store.try_borrow() {
                return guard.backdrop_effect();
            }
            warn!("ConfigStore is currently borrowed; using fallback backdrop");
        }
        self.rust().fallback_backdrop
    }

    pub(crate) fn current_color(&self) -> String {
        if let Some(store) = &self.rust().store {
            if let Ok(guard) = store.try_borrow() {
                return guard.theme_color().to_owned();
            }
            warn!("ConfigStore is currently borrowed; using fallback color");
        }
        if self.rust().fallback_color.is_empty() {
            "#605ed2".to_owned()
        } else {
            self.rust().fallback_color.clone()
        }
    }

    fn capabilities(&self) -> PlatformCapabilities {
        if let Some(store) = &self.rust().store {
            if let Ok(guard) = store.try_borrow() {
                return guard.capabilities();
            }
        }
        PlatformCapabilities::current()
    }

    /// Upstream slot: switch between `"Auto"`, `"Light"`, and `"Dark"`.
    pub fn toggle_theme(mut self: Pin<&mut Self>, theme: QString) {
        guard_ffi("toggle_theme", (), move || {
            let theme_str = theme.to_string();
            let Ok(mode) = ThemeMode::parse_str(&theme_str) else {
                warn!(theme = %theme_str, "Ignoring invalid RinUI theme mode");
                return;
            };

            if self.configured_theme() == mode {
                return;
            }

            debug!(theme = %mode.as_str(), "Switching RinUI theme mode");
            if let Some(store) = self.rust().store.clone() {
                match store.try_borrow_mut() {
                    Ok(mut guard) => {
                        if let Err(err) = guard.set_theme(mode) {
                            error!(error = %err, "Failed to persist theme mode");
                        }
                    }
                    Err(_) => {
                        error!("ConfigStore borrow contention during toggle_theme");
                        return;
                    }
                }
            } else {
                self.as_mut().rust_mut().fallback_theme = mode;
            }

            let actual = self.resolved_theme();
            qt_bridge::rinui_platform_apply_theme(
                &QString::from(actual.as_str()),
                &QString::from(mode.as_str()),
            );
            self.as_mut().theme_changed(QString::from(actual.as_str()));
        });
    }

    /// Upstream slot: return resolved theme (`"Light"` or `"Dark"`).
    pub fn get_theme(&self) -> QString {
        guard_ffi("get_theme", QString::from("Light"), || {
            QString::from(self.resolved_theme().as_str())
        })
    }

    /// Upstream slot: return configured theme (`"Auto"`, `"Light"`, or `"Dark"`).
    pub fn get_theme_name(&self) -> QString {
        guard_ffi("get_theme_name", QString::from("Auto"), || {
            QString::from(self.configured_theme().as_str())
        })
    }

    /// Upstream slot: apply platform backdrop effect (`"mica"`, `"acrylic"`, `"tabbed"`, `"none"`).
    ///
    /// Returns `0` on success, `-1` if the effect is unsupported, or `-2` if running on a
    /// non-Windows platform or when no RinUI windows are currently registered.
    pub fn apply_backdrop_effect(mut self: Pin<&mut Self>, effect_type: QString) -> i32 {
        guard_ffi("apply_backdrop_effect", -2, move || {
            self.sync_platform_appearance();

            let requested_str = effect_type.to_string();
            let caps = self.capabilities();
            let has_windows = !qt_bridge::rinui_all_alive_windows().is_empty();

            if !caps.is_windows() || !has_windows {
                debug!(
                    effect = %requested_str,
                    is_windows = caps.is_windows(),
                    has_windows,
                    "Backdrop effect unavailable on current platform or window state"
                );
                return -2;
            }

            self.as_mut().backdrop_changed(effect_type);

            let Ok(requested) = BackdropEffect::parse_str(&requested_str) else {
                warn!(effect = %requested_str, "Unknown backdrop effect requested");
                return -1;
            };

            let applied = match caps.resolve_backdrop(requested) {
                Ok(effect) => effect,
                Err(err) => {
                    warn!(effect = %requested_str, "Backdrop effect unsupported on this Windows version");
                    return err.status_code();
                }
            };

            if applied != requested {
                warn!(
                    requested = %requested,
                    fallback = %applied,
                    "Backdrop effect unsupported on Windows 11 21H2; falling back"
                );
            }

            let (win10_light, win10_dark) = if let Some(store) = &self.rust().store {
                if let Ok(borrowed) = store.try_borrow() {
                    let colors = borrowed.win10_backdrop_colors();
                    (colors.backdrop_light, colors.backdrop_dark)
                } else {
                    (0xA6FF_FFFF, 0xA600_0000)
                }
            } else {
                (0xA6FF_FFFF, 0xA600_0000)
            };

            let is_dark = self.resolved_theme().is_dark();
            let rc = qt_bridge::rinui_platform_apply_backdrop(
                &QString::from(applied.as_str()),
                is_dark,
                win10_light,
                win10_dark,
            );

            if rc == 0 {
                if let Some(store) = self.rust().store.clone() {
                    match store.try_borrow_mut() {
                        Ok(mut guard) => {
                            if let Err(err) = guard.set_backdrop_effect(requested) {
                                error!(error = %err, "Failed to persist backdrop effect");
                            }
                        }
                        Err(_) => {
                            error!("ConfigStore borrow contention during apply_backdrop_effect");
                        }
                    }
                } else {
                    self.as_mut().rust_mut().fallback_backdrop = requested;
                }
            }
            rc
        })
    }

    /// Upstream slot: return configured backdrop effect string.
    pub fn get_backdrop_effect(&self) -> QString {
        guard_ffi("get_backdrop_effect", QString::from("none"), || {
            QString::from(self.current_backdrop().as_str())
        })
    }

    /// Upstream slot: persist accent hex color.
    pub fn set_theme_color(mut self: Pin<&mut Self>, color: QString) {
        guard_ffi("set_theme_color", (), move || {
            let color_str = color.to_string();
            if let Some(store) = self.rust().store.clone() {
                match store.try_borrow_mut() {
                    Ok(mut guard) => {
                        if let Err(err) = guard.set_theme_color(&color_str) {
                            error!(error = %err, "Failed to persist theme color");
                        }
                    }
                    Err(_) => {
                        error!("ConfigStore borrow contention during set_theme_color");
                    }
                }
            } else {
                self.as_mut().rust_mut().fallback_color = color_str;
            }
        });
    }

    /// Upstream slot: return persisted accent hex color.
    pub fn get_theme_color(&self) -> QString {
        guard_ffi("get_theme_color", QString::from("#605ed2"), || {
            QString::from(self.current_color().as_str())
        })
    }

    /// Invoked via `QStyleHints::colorSchemeChanged` on the Qt GUI thread.
    pub fn handle_system_theme_changed(mut self: Pin<&mut Self>) {
        guard_ffi("handle_system_theme_changed", (), move || {
            self.sync_platform_appearance();
            if self.configured_theme() == ThemeMode::Auto {
                let actual = self.resolved_theme();
                debug!(theme = %actual.as_str(), "System color scheme changed under Auto theme");
                self.as_mut().theme_changed(QString::from(actual.as_str()));
            }
        });
    }
}
