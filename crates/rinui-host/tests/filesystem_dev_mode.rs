use rinui_host::cxx_qt_lib::QString;
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, MacWindowOptions, QmlSource, ResourceMode, RinUIHost,
    EMBEDDED_IMPORT_PREFIX,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn main() {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let upstream_root = workspace_root.join("upstream").join("Rin-UI");
    let tmp = TempDir::new().expect("create temp config dir");

    let config = HostConfig {
        identity: AppIdentity::new("RinUITest", "FilesystemDevMode"),
        resource_mode: ResourceMode::Filesystem(upstream_root.clone()),
        config_location: ConfigLocation::CustomDir(tmp.path().to_path_buf()),
        locale: Some("en_US".to_owned()),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    };

    let mut host = RinUIHost::new(config).expect("initialize filesystem dev-mode RinUIHost");

    // Ensure embedded QRC import path is NOT registered in filesystem mode (no shadowing).
    let import_paths = host.import_paths();
    assert!(
        !import_paths.iter().any(|p| p == EMBEDDED_IMPORT_PREFIX),
        "embedded QRC import path must not be active in filesystem mode: {import_paths:#?}"
    );

    let smoke_qml = workspace_root.join("qml").join("SmokeTest.qml");
    let loaded = host
        .load(&QmlSource::File(smoke_qml))
        .expect("load SmokeTest.qml from filesystem RinUI checkout");
    host.process_events();

    let warnings = host.qml_warnings();
    assert!(
        warnings.is_empty(),
        "unexpected QML warnings in filesystem dev mode: {warnings:#?}"
    );

    let singletons_ok = host
        .root_property(loaded.root_id, "singletonsValid")
        .value::<bool>()
        .unwrap_or(false);
    assert!(
        singletons_ok,
        "RinUI singletons should resolve cleanly in filesystem dev mode"
    );

    // Theme switching works identically in filesystem mode.
    host.theme_manager_mut().toggle_theme(QString::from("Dark"));
    host.process_events();
    assert_eq!(host.theme_manager().get_theme().to_string(), "Dark");

    // Translation switching works from filesystem .qm files.
    let zh = host
        .set_language("zh_CN")
        .expect("load zh_CN.qm from filesystem");
    assert!(!zh.used_fallback);
    host.process_events();
    assert_eq!(
        host.root_property(loaded.root_id, "translatedCalendarToday")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "今天"
    );
    assert_eq!(
        host.root_property(loaded.root_id, "datePickerPlaceholder")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "选择日期"
    );

    println!("filesystem_dev_mode integration tests passed.");
}
