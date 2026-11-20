use rinui_host::cxx_qt_lib::QString;
use rinui_host::{
    AppIdentity, ConfigLocation, QmlSource, ResourceMode, RinUIRuntime, RuntimeConfig,
    SessionConfig, TranslationManager,
};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::PathBuf;
use tempfile::TempDir;

extern "C" {
    fn rinui_widgets_test_app_create(label_text: *const c_char) -> *mut c_void;
    fn rinui_widgets_test_app_qobject(handle: *mut c_void) -> *mut rinui_host::cxx_qt::QObject;
    fn rinui_widgets_test_widget_alive(
        handle: *mut c_void,
        out_buf: *mut c_char,
        out_cap: usize,
    ) -> bool;
    fn rinui_widgets_test_app_process_events(handle: *mut c_void);
    fn rinui_widgets_test_app_destroy(handle: *mut c_void);
}

struct OwnedWidgetsFixture {
    raw: *mut c_void,
}

impl Drop for OwnedWidgetsFixture {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { rinui_widgets_test_app_destroy(self.raw) };
            self.raw = std::ptr::null_mut();
        }
    }
}

fn main() {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let initial_label = CString::new("HostWidgetAlive").expect("CString");
    let fixture = OwnedWidgetsFixture {
        raw: unsafe { rinui_widgets_test_app_create(initial_label.as_ptr()) },
    };
    assert!(!fixture.raw.is_null());

    let app_qobject = unsafe { rinui_widgets_test_app_qobject(fixture.raw) };
    assert!(!app_qobject.is_null());

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let smoke_qml = workspace_root.join("qml").join("SmokeTest.qml");
    let tmp = TempDir::new().expect("temp dir");

    // 1. Attach RinUIRuntime to Qt Widgets QApplication, run RinUI session, then drop runtime.
    {
        let cfg = RuntimeConfig {
            identity: AppIdentity::new("RinUIWidgetsHost", "ExternalQApplication"),
            resource_mode: ResourceMode::Embedded,
            config_location: ConfigLocation::CustomDir(tmp.path().to_path_buf()),
            locale: Some("en_US".to_owned()),
            ..RuntimeConfig::default()
        };

        let rt = unsafe { RinUIRuntime::attach_existing_raw(app_qobject, cfg) }
            .expect("attach RinUIRuntime to external Qt Widgets QApplication");
        assert!(rt.is_borrowed_application());

        let mut session = rt
            .create_session(SessionConfig::private())
            .expect("create RinUI session over QApplication");
        let loaded = session
            .load(&QmlSource::File(smoke_qml))
            .expect("load SmokeTest.qml inside Qt Widgets host");
        rt.process_events();

        assert_eq!(loaded.windows.len(), 1);
        assert!(session.is_root_alive(loaded.root_id));

        rt.with_theme_manager_mut(|tm| tm.toggle_theme(QString::from("Dark")))
            .expect("toggle theme");
        let tr = rt
            .set_language("zh_CN")
            .expect("switch translation under QApplication host");
        assert_eq!(tr.active_locale, "zh_CN");
        rt.process_events();
        assert_eq!(
            session
                .root_property(loaded.root_id, "translatedCalendarToday")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "今天"
        );
    }

    // 2. Verify QApplication and host QWidget/QLabel remain alive and responsive after RinUI drops.
    unsafe { rinui_widgets_test_app_process_events(fixture.raw) };
    let mut buf = [0u8; 64];
    let alive = unsafe {
        rinui_widgets_test_widget_alive(fixture.raw, buf.as_mut_ptr() as *mut c_char, buf.len())
    };
    assert!(
        alive,
        "Host QWidget and QApplication must remain alive after RinUIRuntime drops"
    );
    let label_text = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
        .to_str()
        .expect("utf8 label");
    assert_eq!(label_text, "HostWidgetAlive");

    // 3. Re-check default locale restoration.
    assert!(!TranslationManager::default_qt_locale().is_empty());

    println!("external_qapplication integration tests passed.");
}
