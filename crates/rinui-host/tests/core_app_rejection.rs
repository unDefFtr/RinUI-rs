use rinui_host::cxx_qt_lib::QCoreApplication;
use rinui_host::{
    active_qt_application_ptr, AppIdentity, ConfigLocation, HostError, ResourceMode, RinUIRuntime,
    RuntimeConfig,
};
use tempfile::TempDir;

fn main() {
    let tmp = TempDir::new().expect("temp dir");
    let core_app = QCoreApplication::new();
    assert!(!core_app.is_null());
    let app_ptr = active_qt_application_ptr();
    assert!(!app_ptr.is_null());

    let cfg = RuntimeConfig {
        identity: AppIdentity::new("RinUITest", "CoreAppRejection"),
        resource_mode: ResourceMode::Embedded,
        config_location: ConfigLocation::CustomDir(tmp.path().to_path_buf()),
        ..RuntimeConfig::default()
    };

    let res = unsafe { RinUIRuntime::attach_existing_raw(app_ptr, cfg) };
    assert!(
        matches!(res, Err(HostError::NotGuiApplication)),
        "attaching to bare QCoreApplication must return HostError::NotGuiApplication"
    );

    println!("core_app_rejection integration tests passed.");
}
