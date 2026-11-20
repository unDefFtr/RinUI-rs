use crate::config::{format_config_path, GalleryConfig};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use rinui_host::{AppJsonStore, HostHandle, QmlSource};
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use tracing::{debug, error};

fn guard_ffi<R>(op: &'static str, fallback: R, f: impl FnOnce() -> R) -> R {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => {
            error!(
                operation = op,
                "Recovered from panic at GalleryBackend FFI boundary"
            );
            fallback
        }
    }
}

#[cxx_qt::bridge(namespace = "rinui_gallery")]
pub mod qobject {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = rinui_host::cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = rinui_host::cxx_qt_lib::QStringList;

        include!("rinui-gallery/gallery_backend_factory.h");
        pub fn rinui_gallery_backend_new() -> UniquePtr<GalleryBackend>;
    }

    extern "RustQt" {
        /// Gallery `Backend` QObject matching upstream `examples/gallery.py` plus Rust host demo controls.
        #[qobject]
        type GalleryBackend = super::GalleryBackendRust;

        // Upstream Gallery QML contract:
        #[qinvokable]
        #[cxx_name = "getVersion"]
        fn get_version(self: &GalleryBackend) -> QString;

        #[qinvokable]
        #[cxx_name = "copyToClipboard"]
        fn copy_to_clipboard(self: &GalleryBackend, text: QString);

        #[qinvokable]
        #[cxx_name = "getLanguage"]
        fn get_language(self: &GalleryBackend) -> QString;

        #[qinvokable]
        #[cxx_name = "getSystemLanguage"]
        fn get_system_language(self: &GalleryBackend) -> QString;

        #[qinvokable]
        #[cxx_name = "setLanguage"]
        fn set_language(self: Pin<&mut GalleryBackend>, lang: QString);

        #[qinvokable]
        #[cxx_name = "getFavorites"]
        fn get_favorites(self: &GalleryBackend) -> QStringList;

        #[qinvokable]
        #[cxx_name = "isFavorite"]
        fn is_favorite(self: &GalleryBackend, title: QString) -> bool;

        #[qinvokable]
        #[cxx_name = "toggleFavorite"]
        fn toggle_favorite(self: Pin<&mut GalleryBackend>, title: QString) -> bool;

        #[qinvokable]
        #[cxx_name = "getRecentlyViewed"]
        fn get_recently_viewed(self: &GalleryBackend) -> QStringList;

        #[qinvokable]
        #[cxx_name = "addRecentlyViewed"]
        fn add_recently_viewed(self: Pin<&mut GalleryBackend>, title: QString);

        #[qinvokable]
        #[cxx_name = "clearRecentlyViewed"]
        fn clear_recently_viewed(self: Pin<&mut GalleryBackend>);

        #[qinvokable]
        #[cxx_name = "clearFavorites"]
        fn clear_favorites(self: Pin<&mut GalleryBackend>);

        // Rust Host Demo QML contract:
        #[qinvokable]
        #[cxx_name = "getRustCounter"]
        fn get_rust_counter(self: &GalleryBackend) -> i32;

        #[qinvokable]
        #[cxx_name = "incrementCounter"]
        fn increment_counter(self: Pin<&mut GalleryBackend>);

        #[qinvokable]
        #[cxx_name = "resetCounter"]
        fn reset_counter(self: Pin<&mut GalleryBackend>);

        #[qinvokable]
        #[cxx_name = "openSecondaryWindow"]
        fn open_secondary_window(self: Pin<&mut GalleryBackend>) -> bool;

        #[qinvokable]
        #[cxx_name = "closeSecondaryWindows"]
        fn close_secondary_windows(self: Pin<&mut GalleryBackend>) -> i32;

        #[qinvokable]
        #[cxx_name = "getOpenWindowCount"]
        fn get_open_window_count(self: &GalleryBackend) -> i32;

        #[qinvokable]
        #[cxx_name = "getPlatformName"]
        fn get_platform_name(self: &GalleryBackend) -> QString;

        #[qinvokable]
        #[cxx_name = "getResourceMode"]
        fn get_resource_mode(self: &GalleryBackend) -> QString;

        #[qinvokable]
        #[cxx_name = "getConfigPath"]
        fn get_config_path(self: &GalleryBackend) -> QString;

        #[qinvokable]
        #[cxx_name = "isBackdropSupported"]
        fn is_backdrop_supported(self: &GalleryBackend) -> bool;

        #[qinvokable]
        #[cxx_name = "getHostSummary"]
        fn get_host_summary(self: &GalleryBackend) -> QString;
    }

    unsafe extern "RustQt" {
        #[qsignal]
        #[cxx_name = "favoritesChanged"]
        fn favorites_changed(self: Pin<&mut GalleryBackend>);

        #[qsignal]
        #[cxx_name = "recentlyViewedChanged"]
        fn recently_viewed_changed(self: Pin<&mut GalleryBackend>);

        #[qsignal]
        #[cxx_name = "rustCounterChanged"]
        fn rust_counter_changed(self: Pin<&mut GalleryBackend>);

        #[qsignal]
        #[cxx_name = "openWindowCountChanged"]
        fn open_window_count_changed(self: Pin<&mut GalleryBackend>);

        #[qsignal]
        #[cxx_name = "hostEventLogged"]
        fn host_event_logged(self: Pin<&mut GalleryBackend>, message: QString);
    }
}

pub use qobject::GalleryBackend;

#[derive(Default)]
pub struct GalleryBackendRust {
    host: Option<HostHandle>,
    config_store: Option<Rc<RefCell<AppJsonStore<GalleryConfig>>>>,
    secondary_window_source: Option<QmlSource>,
    rust_counter: i32,
}

impl GalleryBackend {
    pub fn attach_context(
        mut self: Pin<&mut Self>,
        host: HostHandle,
        config_store: Rc<RefCell<AppJsonStore<GalleryConfig>>>,
        secondary_window_source: QmlSource,
    ) {
        let mut rust = self.as_mut().rust_mut();
        rust.host = Some(host);
        rust.config_store = Some(config_store);
        rust.secondary_window_source = Some(secondary_window_source);
    }

    pub fn get_version(&self) -> QString {
        QString::from(concat!("0.5.0 (rinui-rs ", env!("CARGO_PKG_VERSION"), ")"))
    }

    pub fn copy_to_clipboard(&self, text: QString) {
        guard_ffi("copy_to_clipboard", (), || {
            let s = text.to_string();
            if let Some(host) = &self.rust().host {
                host.copy_to_clipboard(&s);
            }
            debug!(copied = %s, "Copied text to clipboard");
        });
    }

    pub fn get_language(&self) -> QString {
        guard_ffi("get_language", QString::from("en_US"), || {
            if let Some(host) = &self.rust().host {
                if let Ok(active) = host.current_locale() {
                    if !active.is_empty() {
                        return QString::from(active.as_str());
                    }
                }
            }
            if let Some(store) = &self.rust().config_store {
                if let Ok(guard) = store.try_borrow() {
                    return QString::from(guard.get().language.as_str());
                }
            }
            QString::from("en_US")
        })
    }

    pub fn get_system_language(&self) -> QString {
        guard_ffi("get_system_language", QString::from("en_US"), || {
            if let Some(host) = &self.rust().host {
                return QString::from(host.system_locale().as_str());
            }
            QString::from("en_US")
        })
    }

    pub fn set_language(mut self: Pin<&mut Self>, lang: QString) {
        guard_ffi("set_language", (), move || {
            let requested = lang.to_string();
            let Some(host) = self.rust().host.clone() else {
                return;
            };
            match host.set_language(&requested) {
                Ok(result) => {
                    let active = result.active_locale;
                    if let Some(store) = self.rust().config_store.clone() {
                        if let Ok(mut guard) = store.try_borrow_mut() {
                            if let Err(err) = guard.update(|cfg| cfg.language = active.clone()) {
                                error!(error = %err, "Failed to persist gallery language");
                            }
                        } else {
                            error!("Gallery config store borrow contention during set_language");
                        }
                    }
                    self.as_mut().host_event_logged(QString::from(
                        format!(
                            "Language switched to {active} (RinUI catalog: {})",
                            result.rinui_locale
                        )
                        .as_str(),
                    ));
                }
                Err(err) => {
                    error!(
                        error = %err,
                        requested = %requested,
                        "Failed to switch gallery language"
                    );
                }
            }
        });
    }

    pub fn get_favorites(&self) -> QStringList {
        guard_ffi("get_favorites", QStringList::default(), || {
            let mut list = QStringList::default();
            if let Some(store) = &self.rust().config_store {
                if let Ok(guard) = store.try_borrow() {
                    for item in &guard.get().favorites {
                        list.append(QString::from(item.as_str()));
                    }
                }
            }
            list
        })
    }

    pub fn is_favorite(&self, title: QString) -> bool {
        guard_ffi("is_favorite", false, || {
            let title_str = title.to_string();
            self.rust()
                .config_store
                .as_ref()
                .and_then(|s| s.try_borrow().ok())
                .is_some_and(|guard| guard.get().favorites.contains(&title_str))
        })
    }

    pub fn toggle_favorite(mut self: Pin<&mut Self>, title: QString) -> bool {
        guard_ffi("toggle_favorite", false, move || {
            let title_str = title.to_string();
            let mut favorited = false;
            if let Some(store) = self.rust().config_store.clone() {
                if let Ok(mut guard) = store.try_borrow_mut() {
                    let _ = guard.update(|cfg| {
                        if let Some(pos) = cfg.favorites.iter().position(|x| x == &title_str) {
                            cfg.favorites.remove(pos);
                            favorited = false;
                        } else {
                            cfg.favorites.push(title_str.clone());
                            favorited = true;
                        }
                    });
                }
            }
            self.as_mut().favorites_changed();
            favorited
        })
    }

    pub fn get_recently_viewed(&self) -> QStringList {
        guard_ffi("get_recently_viewed", QStringList::default(), || {
            let mut list = QStringList::default();
            if let Some(store) = &self.rust().config_store {
                if let Ok(guard) = store.try_borrow() {
                    for item in &guard.get().recently_viewed {
                        list.append(QString::from(item.as_str()));
                    }
                }
            }
            list
        })
    }

    pub fn add_recently_viewed(mut self: Pin<&mut Self>, title: QString) {
        guard_ffi("add_recently_viewed", (), move || {
            let title_str = title.to_string();
            if let Some(store) = self.rust().config_store.clone() {
                if let Ok(mut guard) = store.try_borrow_mut() {
                    let _ = guard.update(|cfg| {
                        cfg.recently_viewed.retain(|x| x != &title_str);
                        cfg.recently_viewed.insert(0, title_str.clone());
                        cfg.recently_viewed.truncate(7);
                    });
                }
            }
            self.as_mut().recently_viewed_changed();
        });
    }

    pub fn clear_recently_viewed(mut self: Pin<&mut Self>) {
        guard_ffi("clear_recently_viewed", (), move || {
            if let Some(store) = self.rust().config_store.clone() {
                if let Ok(mut guard) = store.try_borrow_mut() {
                    let _ = guard.update(|cfg| cfg.recently_viewed.clear());
                }
            }
            self.as_mut().recently_viewed_changed();
        });
    }

    pub fn clear_favorites(mut self: Pin<&mut Self>) {
        guard_ffi("clear_favorites", (), move || {
            if let Some(store) = self.rust().config_store.clone() {
                if let Ok(mut guard) = store.try_borrow_mut() {
                    let _ = guard.update(|cfg| cfg.favorites.clear());
                }
            }
            self.as_mut().favorites_changed();
        });
    }

    pub fn get_rust_counter(&self) -> i32 {
        guard_ffi("get_rust_counter", 0, || self.rust().rust_counter)
    }

    pub fn increment_counter(mut self: Pin<&mut Self>) {
        guard_ffi("increment_counter", (), move || {
            let next = self.rust().rust_counter + 1;
            self.as_mut().rust_mut().rust_counter = next;
            self.as_mut().rust_counter_changed();
            self.as_mut().host_event_logged(QString::from(
                format!("Counter incremented to {next}").as_str(),
            ));
        });
    }

    pub fn reset_counter(mut self: Pin<&mut Self>) {
        guard_ffi("reset_counter", (), move || {
            self.as_mut().rust_mut().rust_counter = 0;
            self.as_mut().rust_counter_changed();
            self.as_mut()
                .host_event_logged(QString::from("Counter reset to 0"));
        });
    }

    pub fn open_secondary_window(mut self: Pin<&mut Self>) -> bool {
        guard_ffi("open_secondary_window", false, move || {
            let Some(host) = self.rust().host.clone() else {
                return false;
            };
            let source = self
                .rust()
                .secondary_window_source
                .clone()
                .unwrap_or_else(|| QmlSource::Url("qrc:/gallery/MultiWindowDialog.qml".to_owned()));
            match host.load_secondary_window(&source) {
                Ok(loaded) => {
                    let count = host.open_window_count();
                    self.as_mut().open_window_count_changed();
                    self.as_mut().host_event_logged(QString::from(
                        format!(
                            "Opened secondary window (root #{}, total windows: {count})",
                            loaded.root_id.raw()
                        )
                        .as_str(),
                    ));
                    true
                }
                Err(err) => {
                    error!(error = %err, "Failed to open secondary window");
                    self.as_mut().host_event_logged(QString::from(
                        format!("Failed to open window: {err}").as_str(),
                    ));
                    false
                }
            }
        })
    }

    pub fn close_secondary_windows(mut self: Pin<&mut Self>) -> i32 {
        guard_ffi("close_secondary_windows", 0, move || {
            let Some(host) = self.rust().host.clone() else {
                return 0;
            };
            let closed = host.close_secondary_windows() as i32;
            self.as_mut().open_window_count_changed();
            self.as_mut().host_event_logged(QString::from(
                format!("Closed {closed} secondary window(s)").as_str(),
            ));
            closed
        })
    }

    pub fn get_open_window_count(&self) -> i32 {
        guard_ffi("get_open_window_count", 0, || {
            self.rust()
                .host
                .as_ref()
                .map(|h| h.open_window_count() as i32)
                .unwrap_or(0)
        })
    }

    pub fn get_platform_name(&self) -> QString {
        guard_ffi("get_platform_name", QString::from("unknown"), || {
            let name = self
                .rust()
                .host
                .as_ref()
                .map(|h| h.platform_name())
                .unwrap_or_else(|| "unknown".to_owned());
            QString::from(name.as_str())
        })
    }

    pub fn get_resource_mode(&self) -> QString {
        guard_ffi("get_resource_mode", QString::from("unknown"), || {
            let desc = self
                .rust()
                .host
                .as_ref()
                .and_then(|h| h.resource_mode_description().ok())
                .unwrap_or_else(|| "unknown".to_owned());
            QString::from(desc.as_str())
        })
    }

    pub fn get_config_path(&self) -> QString {
        guard_ffi("get_config_path", QString::default(), || {
            let rinui_cfg = self
                .rust()
                .host
                .as_ref()
                .and_then(|h| h.config_path().ok().flatten());
            let app_cfg = self
                .rust()
                .config_store
                .as_ref()
                .and_then(|s| s.try_borrow().ok())
                .and_then(|s| s.path().map(PathBuf::from));
            let summary = format!(
                "RinUI: {} | App: {}",
                format_config_path(rinui_cfg.as_deref()).display(),
                format_config_path(app_cfg.as_deref()).display()
            );
            QString::from(summary.as_str())
        })
    }

    pub fn is_backdrop_supported(&self) -> bool {
        guard_ffi("is_backdrop_supported", false, || {
            self.rust()
                .host
                .as_ref()
                .and_then(|h| h.is_backdrop_supported().ok())
                .unwrap_or(false)
        })
    }

    pub fn get_host_summary(&self) -> QString {
        guard_ffi(
            "get_host_summary",
            QString::from("Host uninitialized"),
            || {
                let Some(host) = &self.rust().host else {
                    return QString::from("Host uninitialized");
                };
                let theme = host.current_theme_name().unwrap_or_default();
                let actual = host.actual_theme().unwrap_or_default();
                let locale = host.current_locale().unwrap_or_default();
                QString::from(
                    format!("rinui-rs host active — theme={theme} ({actual}), locale={locale}")
                        .as_str(),
                )
            },
        )
    }
}
