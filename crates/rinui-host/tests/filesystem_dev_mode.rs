use rinui_host::cxx_qt_lib::QString;
use rinui_host::{
    qt_file_exists, AppIdentity, ConfigLocation, HostConfig, HostError, MacWindowOptions,
    QmlSource, ResourceMode, RinUIHost, EMBEDDED_IMPORT_PREFIX,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create target dir");
    for entry in fs::read_dir(src).expect("read source dir").flatten() {
        let src_path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy() == ".git" {
            continue;
        }
        let dst_path = dst.join(name);
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

fn main() {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let upstream_root = workspace_root.join("upstream").join("Rin-UI");
    let tmp = TempDir::new().expect("create temp dir");
    let cfg_dir = tmp.path().join("cfg");

    // Copy upstream Rin-UI to a writable directory containing spaces to verify
    // runtime sanitization, path handling, and reload-on-restart without touching submodule.
    let copy_root = tmp.path().join("rinui dev copy");
    copy_dir_recursive(&upstream_root, &copy_root);

    let smoke_qml = workspace_root.join("qml").join("SmokeTest.qml");

    let staged_load_root: PathBuf;
    {
        let config = HostConfig {
            identity: AppIdentity::new("RinUITest", "FilesystemDevMode"),
            resource_mode: ResourceMode::Filesystem(copy_root.clone()),
            config_location: ConfigLocation::CustomDir(cfg_dir.clone()),
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

        let view = host.resource_view();
        let src_root = view
            .source_root()
            .expect("source_root present in filesystem mode");
        let load_root = view
            .load_root()
            .expect("load_root present in filesystem mode");
        assert_ne!(src_root, load_root);
        assert!(load_root.is_dir());
        staged_load_root = load_root.to_path_buf();

        let desc = host
            .handle()
            .resource_mode_description()
            .expect("resource_mode_description");
        assert!(desc.contains(&src_root.display().to_string()));
        assert!(desc.contains(&load_root.display().to_string()));

        // `qt_file_exists` against embedded prefix doesn't interfere with dev view.
        assert!(qt_file_exists(
            &load_root.join("RinUI/qmldir").to_string_lossy()
        ));

        let loaded = host
            .load(&QmlSource::File(smoke_qml.clone()))
            .expect("load SmokeTest.qml from filesystem RinUI checkout");
        host.process_events();

        let warnings = host.qml_warnings();
        assert!(
            warnings.is_empty(),
            "unexpected QML warnings in filesystem dev mode: {warnings:#?}"
        );

        assert!(
            host.root_property(loaded.root_id, "singletonsValid")
                .value::<bool>()
                .unwrap_or(false),
            "RinUI singletons should resolve cleanly in filesystem dev mode"
        );
        assert!(
            host.root_property(loaded.root_id, "scrollViewValid")
                .value::<bool>()
                .unwrap_or(false),
            "ScrollView must instantiate via `import RinUI` in filesystem dev mode"
        );
        assert!(
            host.root_property(loaded.root_id, "iconFontValid")
                .value::<bool>()
                .unwrap_or(false),
            "FluentSystemIcons font and index must load in filesystem dev mode"
        );
        assert!(
            host.root_property(loaded.root_id, "noiseImageReady")
                .value::<bool>()
                .unwrap_or(false),
            "AcrylicBrush noise.png texture must resolve in filesystem dev mode"
        );

        // Non-existent upstream components remain unavailable in dev mode too.
        for missing_type in [
            "DateField",
            "TeachingTip",
            "ScrollViewer",
            "HyperlinkButton",
        ] {
            let probe_dir = tmp.path().join(format!("fs_probe_{missing_type}"));
            fs::create_dir_all(&probe_dir).expect("create fs probe dir");
            let probe_path = probe_dir.join(format!("FsMissing{missing_type}.qml"));
            fs::write(
                &probe_path,
                format!("import QtQuick 2.15\nimport RinUI\n{missing_type} {{}}\n"),
            )
            .expect("write missing type probe");
            let res = host.load(&QmlSource::File(probe_path));
            match res {
                Err(HostError::QmlLoadFailed { reason, .. }) => {
                    assert!(
                        reason.contains("is not a type"),
                        "expected `{missing_type} is not a type`, got: {reason}"
                    );
                }
                other => {
                    panic!("non-existent component {missing_type} must not load in dev mode, got {other:?}")
                }
            }
        }

        // Dynamic NavigationView error-page fallback (`ErrorPage.qml`).
        let nav_dir = tmp.path().join("fs_nav_probe");
        fs::create_dir_all(&nav_dir).expect("create fs_nav_probe dir");
        let nav_probe = nav_dir.join("FsNavErrorProbe.qml");
        fs::write(
            &nav_probe,
            r#"import QtQuick 2.15
import RinUI
FluentWindow {
    visible: false
    navigationItems: [{ title: "Bad", page: "nonexistent/Page.qml", icon: "ic_fluent_home_20_regular" }]
}
"#,
        )
        .expect("write FsNavErrorProbe.qml");
        let nav_loaded = host
            .load(&QmlSource::File(nav_probe))
            .expect("NavigationView ErrorPage fallback must load in dev mode");
        host.process_events();
        host.release_root(nav_loaded.root_id);
        host.process_events();

        // Theme switching works identically in filesystem mode.
        host.theme_manager_mut()
            .unwrap()
            .toggle_theme(QString::from("Dark"));
        host.process_events();
        assert_eq!(
            host.theme_manager().unwrap().get_theme().to_string(),
            "Dark"
        );

        // Translation switching works from filesystem .qm files.
        let zh = host
            .set_language("zh_CN")
            .expect("load zh_CN.qm from filesystem");
        assert!(!zh.rinui_used_fallback);
        assert_eq!(zh.active_locale, "zh_CN");
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
    }

    // Staged dev directory must be cleaned up after RinUIHost drops.
    assert!(
        !staged_load_root.exists(),
        "staged dev view {} must be cleaned up on host drop",
        staged_load_root.display()
    );

    // Modify a RinUI QML control on disk AFTER compilation and verify a new host reads the edit.
    let scroll_qml_path = copy_root.join("RinUI/components/ScrollView.qml");
    let orig_scroll = fs::read_to_string(&scroll_qml_path).expect("read ScrollView.qml");
    let edited_scroll = orig_scroll.replacen(
        "ScrollView {",
        "ScrollView {\n    property string editedMarker: \"DEV_CONTROL_RELOADED\"",
        1,
    );
    assert_ne!(orig_scroll, edited_scroll);
    fs::write(&scroll_qml_path, edited_scroll).expect("write edited ScrollView.qml");

    let probe_scroll = tmp.path().join("EditedScrollProbe.qml");
    fs::write(
        &probe_scroll,
        "import QtQuick 2.15\nimport RinUI\nScrollView { visible: false }\n",
    )
    .expect("write EditedScrollProbe.qml");

    let mut host2 = RinUIHost::new(HostConfig {
        identity: AppIdentity::new("RinUITest", "FilesystemDevMode2"),
        resource_mode: ResourceMode::Filesystem(copy_root.clone()),
        config_location: ConfigLocation::CustomDir(cfg_dir),
        locale: Some("en_US".to_owned()),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
    })
    .expect("re-init filesystem dev host");

    let edited_loaded = host2
        .load(&QmlSource::File(probe_scroll))
        .expect("load edited ScrollView");
    assert_eq!(
        host2
            .root_property(edited_loaded.root_id, "editedMarker")
            .value::<QString>()
            .map(|s| s.to_string())
            .as_deref(),
        Some("DEV_CONTROL_RELOADED")
    );

    println!("filesystem_dev_mode integration tests passed.");
}
