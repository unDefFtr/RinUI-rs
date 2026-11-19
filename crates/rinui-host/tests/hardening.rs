use rinui_host::cxx_qt_lib::QString;
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, HostError, MacWindowOptions, QmlSource, ResourceMode,
    RinUIHost,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn test_config(tmp: &TempDir, app_name: &str, mode: ResourceMode) -> HostConfig {
    HostConfig {
        identity: AppIdentity::new("RinUIHardeningTest", app_name),
        resource_mode: mode,
        config_location: ConfigLocation::CustomDir(tmp.path().to_path_buf()),
        locale: Some("en_US".to_owned()),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    }
}

fn main() {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let smoke_qml = workspace_root.join("qml").join("SmokeTest.qml");
    let dialog_qml = workspace_root.join("qml").join("MultiWindowDialog.qml");

    // 1. Invalid ResourceMode must fail cleanly without poisoning subsequent RinUIHost::new().
    let tmp_invalid = TempDir::new().expect("temp dir for invalid resource test");
    let invalid_res = RinUIHost::new(test_config(
        &tmp_invalid,
        "InvalidRes",
        ResourceMode::Filesystem(PathBuf::from("/nonexistent/rinui-upstream")),
    ));
    assert!(matches!(
        invalid_res,
        Err(HostError::InvalidResourcePath { .. })
    ));

    // 2. First active host lifecycle: concurrent re-init, invalid QML load, FFI borrow-contention safety,
    // locale normalization, and multi-window cleanup.
    let tmp1 = TempDir::new().expect("temp dir 1");
    let handle_after_drop = {
        let mut host = RinUIHost::new(test_config(&tmp1, "Session1", ResourceMode::Embedded))
            .expect("first RinUIHost::new must succeed after failed invalid-path init");

        // Concurrent initialization while a host is active must fail safely without touching Qt state.
        let tmp_dup = TempDir::new().expect("temp dir dup");
        let dup_res = RinUIHost::new(test_config(&tmp_dup, "Duplicate", ResourceMode::Embedded));
        assert!(matches!(
            dup_res,
            Err(HostError::ApplicationAlreadyInitialized)
        ));

        // Loading a nonexistent QML file must return QmlLoadFailed without corrupting the engine.
        let missing_qml = tmp1.path().join("DoesNotExist.qml");
        let missing_res = host.load(&QmlSource::File(missing_qml));
        assert!(matches!(missing_res, Err(HostError::QmlLoadFailed { .. })));

        // Valid load still works afterwards.
        let primary = host
            .load(&QmlSource::File(smoke_qml.clone()))
            .expect("load SmokeTest.qml in session 1");
        host.process_events();
        assert!(host.is_root_alive(primary.root_id));

        // FFI panic boundary test: hold ConfigStore borrow_mut across ThemeManager slots.
        {
            let store_rc = std::rc::Rc::clone(host.config_store());
            let _borrow = store_rc.borrow_mut();
            let theme = host.theme_manager().get_theme().to_string();
            assert!(theme == "Light" || theme == "Dark");
            assert_eq!(host.theme_manager().get_theme_name().to_string(), "Auto");
            assert!(!host
                .theme_manager()
                .get_theme_color()
                .to_string()
                .is_empty());
            assert_eq!(
                host.theme_manager().get_backdrop_effect().to_string(),
                "none"
            );
            host.theme_manager_mut().toggle_theme(QString::from("Dark"));
            host.theme_manager_mut()
                .set_theme_color(QString::from("#abcdef"));
            let _ = host
                .theme_manager_mut()
                .apply_backdrop_effect(QString::from("none"));
        }

        // Locale normalization (`zh-CN`, `zh_Hans_CN`, `zh`) and fallback (`xx_NONEXISTENT`).
        let hyphen = host.set_language("zh-CN").expect("zh-CN normalization");
        assert!(!hyphen.used_fallback);
        assert_eq!(hyphen.effective_locale, "zh_CN");
        host.process_events();
        assert_eq!(
            host.root_property(primary.root_id, "translatedCalendarToday")
                .value::<QString>()
                .unwrap()
                .to_string(),
            "今天"
        );

        let script = host
            .set_language("zh_Hans_CN")
            .expect("zh_Hans_CN simplification");
        assert!(!script.used_fallback);
        assert_eq!(script.effective_locale, "zh_CN");

        let bare_zh = host.set_language("zh").expect("zh bare tag mapping");
        assert!(!bare_zh.used_fallback);
        assert_eq!(bare_zh.effective_locale, "zh_CN");

        let fb = host
            .set_language("xx_NONEXISTENT")
            .expect("unknown locale fallback");
        assert!(fb.used_fallback);
        assert_eq!(fb.effective_locale, "en_US");
        host.process_events();
        assert_eq!(
            host.root_property(primary.root_id, "translatedCalendarToday")
                .value::<QString>()
                .unwrap()
                .to_string(),
            "Today"
        );

        // Multi-window secondary root creation and controlled release.
        let handle = host.handle();
        let secondary = handle
            .load_secondary_window(&QmlSource::File(dialog_qml))
            .expect("load secondary window via HostHandle");
        host.process_events();
        assert_eq!(handle.open_window_count(), 2);
        assert_eq!(handle.close_secondary_windows(), 1);
        host.process_events();
        assert!(!host.is_root_alive(secondary.root_id));
        assert_eq!(handle.open_window_count(), 1);

        handle
    };

    // HostHandle after Drop must report HostDisposed cleanly.
    assert!(matches!(
        handle_after_drop.set_language("en_US"),
        Err(HostError::HostDisposed)
    ));
    assert_eq!(handle_after_drop.open_window_count(), 0);
    assert_eq!(handle_after_drop.close_secondary_windows(), 0);

    // 3. Sequential repeated initialization after previous RinUIHost dropped cleanly.
    let tmp2 = TempDir::new().expect("temp dir 2");
    {
        let mut host2 = RinUIHost::new(test_config(&tmp2, "Session2", ResourceMode::Embedded))
            .expect("second sequential RinUIHost::new must succeed");
        let root2 = host2
            .load(&QmlSource::File(smoke_qml))
            .expect("load SmokeTest.qml in session 2");
        host2.process_events();
        assert!(host2.is_root_alive(root2.root_id));
        let warnings = host2.qml_warnings();
        assert!(
            warnings.is_empty(),
            "unexpected QML warnings in second host session: {warnings:#?}"
        );
        let zh2 = host2
            .set_language("zh_CN")
            .expect("translation in session 2");
        assert_eq!(zh2.effective_locale, "zh_CN");
        host2.process_events();
        assert_eq!(
            host2
                .root_property(root2.root_id, "translatedCalendarToday")
                .value::<QString>()
                .unwrap()
                .to_string(),
            "今天"
        );
    }

    println!("hardening integration tests passed.");
}
