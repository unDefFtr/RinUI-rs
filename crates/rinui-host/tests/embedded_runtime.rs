use rinui_host::cxx_qt_lib::QString;
use rinui_host::{
    qt_file_exists, AppIdentity, ConfigLocation, ConfigStore, HostConfig, MacWindowOptions,
    NativeWindowRef, PlatformCapabilities, QmlSource, ResourceMode, RinUIHost, WindowCustomizer,
    PACKAGED_RINUI_FILES, SANITIZED_QMLDIR_DROPS,
};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use tempfile::TempDir;

struct RecordingCustomizer {
    callbacks: Rc<Cell<usize>>,
}

impl WindowCustomizer for RecordingCustomizer {
    fn on_windows_discovered(&mut self, windows: &[NativeWindowRef]) {
        assert!(!windows.is_empty());
        self.callbacks.set(self.callbacks.get() + 1);
    }
}

fn main() {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let tmp = TempDir::new().expect("create temp config dir");
    let identity = AppIdentity::new("RinUITest", "EmbeddedRuntime");
    let config = HostConfig {
        identity: identity.clone(),
        resource_mode: ResourceMode::Embedded,
        config_location: ConfigLocation::CustomDir(tmp.path().to_path_buf()),
        locale: Some("en_US".to_owned()),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    };

    let mut host = RinUIHost::new(config).expect("initialize embedded RinUIHost");

    // 1. Verify all RinUI runtime files are present in compiled QRC resources.
    for rel in PACKAGED_RINUI_FILES {
        let qrc_path = format!(":/rinui/RinUI/{rel}");
        assert!(
            qt_file_exists(&qrc_path),
            "missing embedded QRC resource: {qrc_path}"
        );
    }
    assert!(
        SANITIZED_QMLDIR_DROPS.len() >= 5,
        "expected build-time qmldir sanitization drops"
    );

    // 2. Register WindowCustomizer and custom context property before loading QML.
    let callbacks = Rc::new(Cell::new(0usize));
    host.set_window_customizer(Box::new(RecordingCustomizer {
        callbacks: Rc::clone(&callbacks),
    }));
    host.set_context_property_string("CustomGreeting", "HelloFromRust")
        .expect("set custom context property");

    let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let smoke_qml = fixtures_dir.join("SmokeTest.qml");

    let loaded = host
        .load(&QmlSource::File(smoke_qml))
        .expect("load SmokeTest.qml in embedded mode");
    host.process_events();

    let warnings = host.qml_warnings();
    assert!(
        warnings.is_empty(),
        "unexpected QML warnings in embedded mode: {warnings:#?}"
    );
    assert_eq!(loaded.windows.len(), 1);
    assert_eq!(callbacks.get(), 1);
    assert_eq!(host.handle().open_window_count(), 1);

    if cfg!(target_os = "macos") {
        assert_eq!(
            host.root_property(loaded.root_id, "useNativeMacFrame")
                .value::<bool>(),
            Some(false),
            "The default host must use the upstream QML titlebar"
        );
        assert_eq!(
            host.root_property(loaded.root_id, "titleBarHeight")
                .value::<i32>(),
            Some(48)
        );
    }

    let singletons_ok = host
        .root_property(loaded.root_id, "singletonsValid")
        .value::<bool>()
        .unwrap_or(false);
    assert!(
        singletons_ok,
        "RinUI singletons should resolve without duplicates"
    );
    assert!(
        host.root_property(loaded.root_id, "scrollViewValid")
            .value::<bool>()
            .unwrap_or(false),
        "ScrollView must instantiate via `import RinUI` in embedded mode"
    );
    assert!(
        host.root_property(loaded.root_id, "iconFontValid")
            .value::<bool>()
            .unwrap_or(false),
        "FluentSystemIcons font and index must load in embedded mode"
    );
    assert!(
        host.root_property(loaded.root_id, "noiseImageReady")
            .value::<bool>()
            .unwrap_or(false),
        "AcrylicBrush noise.png texture must resolve in embedded mode"
    );

    // Non-existent upstream components remain unavailable after qmldir sanitization.
    for missing_type in [
        "DateField",
        "TeachingTip",
        "ScrollViewer",
        "HyperlinkButton",
    ] {
        let probe_dir = tmp.path().join(format!("probe_{missing_type}"));
        std::fs::create_dir_all(&probe_dir).expect("create probe dir");
        let probe_path = probe_dir.join(format!("Missing{missing_type}.qml"));
        std::fs::write(
            &probe_path,
            format!("import QtQuick 2.15\nimport RinUI\n{missing_type} {{}}\n"),
        )
        .expect("write missing type probe");
        let res = host.load(&QmlSource::File(probe_path));
        match res {
            Err(rinui_host::HostError::QmlLoadFailed { reason, .. }) => {
                assert!(
                    reason.contains("is not a type"),
                    "expected `{missing_type} is not a type`, got: {reason}"
                );
            }
            other => {
                panic!("non-existent component {missing_type} must fail to load, got {other:?}")
            }
        }
    }

    // Dynamic NavigationView error-page fallback (`ErrorPage.qml` in Navigation/).
    let nav_dir = tmp.path().join("nav_probe");
    std::fs::create_dir_all(&nav_dir).expect("create nav_probe dir");
    let nav_probe = nav_dir.join("NavErrorProbe.qml");
    std::fs::write(
        &nav_probe,
        r#"import QtQuick 2.15
import RinUI
FluentWindow {
    visible: false
    navigationItems: [{ title: "Bad", page: "nonexistent/Page.qml", icon: "ic_fluent_home_20_regular" }]
}
"#,
    )
    .expect("write NavErrorProbe.qml");
    let nav_loaded = host
        .load(&QmlSource::File(nav_probe))
        .expect("NavigationView ErrorPage fallback must load in embedded mode");
    host.process_events();
    host.release_root(nav_loaded.root_id);
    host.process_events();

    let greeting = host
        .root_property(loaded.root_id, "customGreeting")
        .value::<QString>()
        .map(|s| s.to_string())
        .unwrap_or_default();
    assert_eq!(greeting, "HelloFromRust");

    // 3. Exercise ThemeManager contract, signals, accent color, and persistence.
    assert_eq!(
        host.theme_manager().unwrap().get_theme_name().to_string(),
        "Auto"
    );
    let initial_actual = host.theme_manager().unwrap().get_theme().to_string();
    assert!(initial_actual == "Light" || initial_actual == "Dark");

    host.theme_manager_mut()
        .unwrap()
        .toggle_theme(QString::from("Dark"));
    host.process_events();
    assert_eq!(
        host.theme_manager().unwrap().get_theme_name().to_string(),
        "Dark"
    );
    assert_eq!(
        host.theme_manager().unwrap().get_theme().to_string(),
        "Dark"
    );
    assert_eq!(
        host.root_property(loaded.root_id, "actualThemeName")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "Dark"
    );
    let signal_count = host
        .root_property(loaded.root_id, "themeChangedSignalCount")
        .value::<i32>()
        .unwrap_or(0);
    assert!(signal_count >= 1);

    host.theme_manager_mut()
        .unwrap()
        .toggle_theme(QString::from("Light"));
    host.process_events();
    assert_eq!(
        host.theme_manager().unwrap().get_theme().to_string(),
        "Light"
    );

    host.theme_manager_mut()
        .unwrap()
        .toggle_theme(QString::from("Auto"));
    host.process_events();
    assert_eq!(
        host.theme_manager().unwrap().get_theme_name().to_string(),
        "Auto"
    );
    let resolved_after_auto = host.theme_manager().unwrap().get_theme().to_string();
    assert!(resolved_after_auto == "Light" || resolved_after_auto == "Dark");

    host.theme_manager_mut()
        .unwrap()
        .set_theme_color(QString::from("#12AB34"));
    assert_eq!(
        host.theme_manager().unwrap().get_theme_color().to_string(),
        "#12ab34"
    );

    // Reload rin_ui.json independently to confirm disk persistence.
    let (reloaded_cfg, _) = ConfigStore::load_or_default(
        &ConfigLocation::CustomDir(tmp.path().to_path_buf()),
        &identity,
        None,
        PlatformCapabilities::current(),
    )
    .expect("reload persisted config");
    assert_eq!(reloaded_cfg.theme_color(), "#12ab34");

    // Backdrop return code contract on non-Windows platforms.
    if !PlatformCapabilities::current().is_windows() {
        let rc = host
            .theme_manager_mut()
            .unwrap()
            .apply_backdrop_effect(QString::from("mica"));
        assert_eq!(rc, -2);
    }

    // 4. Exercise TranslationManager live switching and en_US fallback.
    assert_eq!(
        host.root_property(loaded.root_id, "translatedCalendarToday")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "Today"
    );
    assert_eq!(
        host.root_property(loaded.root_id, "datePickerPlaceholder")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "Pick a date"
    );

    let zh_res = host
        .set_language("zh_CN")
        .expect("switch translation to zh_CN");
    assert!(!zh_res.used_fallback);
    assert_eq!(zh_res.effective_locale, "zh_CN");
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

    let ja_res = host
        .set_language("ja_JP")
        .expect("valid locale without RinUI .qm should preserve active locale ja_JP");
    assert_eq!(ja_res.active_locale, "ja_JP");
    assert_eq!(ja_res.effective_locale, "ja_JP");
    assert_eq!(ja_res.rinui_locale, "en_US");
    assert!(ja_res.rinui_used_fallback);
    assert!(!ja_res.active_locale_fallback);
    assert!(ja_res.used_fallback);

    let fallback_res = host
        .set_language("xx_NONEXISTENT")
        .expect("unrecognized locale should fall back to en_US");
    assert!(fallback_res.used_fallback);
    assert!(fallback_res.active_locale_fallback);
    assert_eq!(fallback_res.active_locale, "en_US");
    assert_eq!(fallback_res.effective_locale, "en_US");
    host.process_events();
    assert_eq!(
        host.root_property(loaded.root_id, "translatedCalendarToday")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "Today"
    );

    // 5. Exercise multi-window theme sync and controlled release cleanup.
    let callbacks_before_dialog = callbacks.get();
    assert_eq!(callbacks_before_dialog, 2);
    let dialog_qml = fixtures_dir.join("MultiWindowDialog.qml");
    let secondary = host
        .load(&QmlSource::File(dialog_qml))
        .expect("load secondary MultiWindowDialog.qml");
    host.process_events();
    assert_eq!(callbacks.get(), callbacks_before_dialog + 1);
    assert_eq!(host.handle().open_window_count(), 2);

    if cfg!(target_os = "macos") {
        assert_eq!(
            host.root_property(secondary.root_id, "useNativeMacFrame")
                .value::<bool>(),
            Some(false),
            "Secondary windows must inherit the default QML titlebar mode"
        );
    }

    host.theme_manager_mut()
        .unwrap()
        .toggle_theme(QString::from("Dark"));
    host.process_events();
    assert_eq!(
        host.root_property(secondary.root_id, "actualThemeName")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "Dark"
    );
    assert_eq!(
        host.root_property(loaded.root_id, "actualThemeName")
            .value::<QString>()
            .unwrap()
            .to_string(),
        "Dark"
    );

    host.release_root(secondary.root_id);
    host.process_events();
    assert!(!host.is_root_alive(secondary.root_id));
    assert_eq!(host.handle().open_window_count(), 1);

    let handle_after = host.handle();
    drop(host);
    assert!(handle_after.set_language("en_US").is_err());

    println!("embedded_runtime integration tests passed.");
}
