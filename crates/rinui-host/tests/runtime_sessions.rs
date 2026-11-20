use rinui_host::cxx_qt::casting::Upcast;
use rinui_host::cxx_qt_lib::{
    QByteArray, QGuiApplication, QQmlApplicationEngine, QQmlEngine, QString, QUrl, QVariant,
};
use rinui_host::{
    active_qt_application_ptr, AppIdentity, ConfigLocation, ContextPropertyConflict, HostError,
    MacWindowOptions, QmlSource, ResourceMode, RinUIRuntime, RuntimeConfig, SessionConfig,
    TranslationCatalog, TranslationManager,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// Local pure-Rust Qt .qm binary writer for self-contained multi-engine i18n checks.
fn qt_elf_hash(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for &b in bytes {
        h = (h << 4).wrapping_add(b as u32);
        let g = h & 0xf000_0000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    if h == 0 {
        1
    } else {
        h
    }
}

fn push_u32_be(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_be_bytes());
}

fn push_qm_utf8_field(buf: &mut Vec<u8>, tag: u8, s: &str) {
    buf.push(tag);
    push_u32_be(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

fn push_qm_utf16be_field(buf: &mut Vec<u8>, tag: u8, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    buf.push(tag);
    push_u32_be(buf, (units.len() * 2) as u32);
    for u in units {
        buf.extend_from_slice(&u.to_be_bytes());
    }
}

fn build_qt_qm(entries: &[(&str, &str, &str)]) -> Vec<u8> {
    const QM_MAGIC: [u8; 16] = [
        0x3c, 0xb8, 0x64, 0x18, 0xca, 0xef, 0x9c, 0x95, 0xcd, 0x21, 0x1c, 0xbf, 0x60, 0xa1, 0xbd,
        0xdd,
    ];
    let mut messages = Vec::new();
    let mut hashes: Vec<(u32, u32)> = Vec::with_capacity(entries.len());
    for &(context, source, translation) in entries {
        let offset = messages.len() as u32;
        hashes.push((qt_elf_hash(source.as_bytes()), offset));
        push_qm_utf16be_field(&mut messages, 0x03, translation);
        push_qm_utf8_field(&mut messages, 0x08, "");
        push_qm_utf8_field(&mut messages, 0x06, source);
        push_qm_utf8_field(&mut messages, 0x07, context);
        messages.push(0x01);
    }
    hashes.sort_by_key(|&(h, o)| (h, o));

    let mut hash_section = Vec::with_capacity(hashes.len() * 8);
    for (h, offset) in hashes {
        push_u32_be(&mut hash_section, h);
        push_u32_be(&mut hash_section, offset);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&QM_MAGIC);
    out.push(0x42);
    push_u32_be(&mut out, hash_section.len() as u32);
    out.extend_from_slice(&hash_section);
    out.push(0x69);
    push_u32_be(&mut out, messages.len() as u32);
    out.extend_from_slice(&messages);
    out
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create target dir");
    for entry in std::fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let src_path = entry.path();
        let dst_path = dst.join(name);
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else if src_path.is_file() {
            std::fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

fn runtime_cfg(cfg_dir: &Path, app_name: &str, mode: ResourceMode) -> RuntimeConfig {
    RuntimeConfig {
        identity: AppIdentity::new("RinUIRuntimeTest", app_name),
        resource_mode: mode,
        config_location: ConfigLocation::CustomDir(cfg_dir.to_path_buf()),
        locale: Some("en_US".to_owned()),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
        apply_identity_to_borrowed_app: false,
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
    let i18n_qml = workspace_root.join("qml").join("I18nTest.qml");
    let upstream_rinui = workspace_root.join("upstream").join("Rin-UI").join("RinUI");

    let tmp = TempDir::new().expect("create temp test dir");
    let dev_upstream_copy = tmp.path().join("dev_upstream");
    copy_dir_recursive(&upstream_rinui, &dev_upstream_copy.join("RinUI"));

    // Prepare app fr_FR catalog (RinUI itself lacks fr_FR and falls back to en_US).
    let app_i18n_dir = tmp.path().join("app_i18n");
    std::fs::create_dir_all(&app_i18n_dir).expect("create app_i18n dir");
    std::fs::write(
        app_i18n_dir.join("fr_FR.qm"),
        build_qt_qm(&[
            ("AppI18n", "Hello", "Bonjour Application"),
            ("I18nTest", "Hello App", "Bonjour Application"),
        ]),
    )
    .expect("write fr_FR.qm");

    // -------------------------------------------------------------------------
    // Phase 1: Failed init rollback before active runtime must not poison state.
    // -------------------------------------------------------------------------
    assert!(matches!(
        RinUIRuntime::new(runtime_cfg(
            tmp.path(),
            "BadRes",
            ResourceMode::Filesystem(tmp.path().join("missing-rinui")),
        )),
        Err(HostError::InvalidResourcePath { .. })
    ));

    // Attach before any QGuiApplication exists must return NoGuiApplication.
    let fake_ptr: *mut rinui_host::cxx_qt::QObject = std::ptr::null_mut();
    assert!(matches!(
        unsafe {
            RinUIRuntime::attach_existing_raw(
                fake_ptr,
                runtime_cfg(tmp.path(), "NoApp", ResourceMode::Embedded),
            )
        },
        Err(HostError::NoGuiApplication)
    ));

    // -------------------------------------------------------------------------
    // Phase 2: Multi-session (Shared + Private), multi-engine i18n, dev ResourceView,
    // cross-session ownership isolation, dynamic windows, and reversed drop order.
    // -------------------------------------------------------------------------
    let isolated_qml_dir = tmp.path().join("session_qml");
    std::fs::create_dir_all(&isolated_qml_dir).expect("create session_qml dir");
    let ctx_probe_qml = isolated_qml_dir.join("CtxProbe.qml");
    std::fs::write(
        &ctx_probe_qml,
        r#"import QtQuick 2.15
import RinUI
FluentWindow {
    visible: false
    property string seenLabel: SessionLabel
    property string sharedVal: typeof SharedProp !== "undefined" ? SharedProp : ""
    property bool isDarkNow: Theme.isDark()
    property string i18nGreeting: qsTranslate("I18nTest", "Hello App")
    property int dynamicCount: 0
    function openDynamicChild() {
        var win = Qt.createQmlObject(
            'import QtQuick 2.15; import RinUI; FluentWindow { visible: false; objectName: "dynFromPrivate" }',
            this,
            "dynChild"
        );
        dynamicCount += 1;
        return win !== null;
    }
}
"#,
    )
    .expect("write CtxProbe.qml");

    let dev_load_root_to_verify: PathBuf;
    {
        let rt = RinUIRuntime::new(runtime_cfg(
            &tmp.path().join("phase2_cfg"),
            "MultiSession",
            ResourceMode::Embedded,
        ))
        .expect("create owned RinUIRuntime");

        assert!(!rt.is_borrowed_application());
        assert_eq!(rt.open_window_count(), 0);

        let mut shared1 = rt
            .create_session(SessionConfig::shared())
            .expect("create shared session 1");
        let mut shared2 = rt
            .create_session(SessionConfig::shared())
            .expect("create shared session 2");
        let mut priv1 = rt
            .create_session(SessionConfig::private())
            .expect("create private session 1");
        let mut priv2_dev = rt
            .create_session(
                SessionConfig::private()
                    .with_resource_mode(ResourceMode::Filesystem(dev_upstream_copy.clone())),
            )
            .expect("create private session 2 in filesystem dev mode");

        dev_load_root_to_verify = priv2_dev
            .resource_view()
            .expect("dev resource view")
            .load_root()
            .expect("dev mode staged load root")
            .to_path_buf();
        assert!(dev_load_root_to_verify.is_dir());

        // Shared conflict policy: Reject blocks overwrite; explicit Overwrite updates both.
        shared1
            .set_context_property_string("SessionLabel", "SharedInitial")
            .expect("set shared SessionLabel");
        shared1
            .set_context_property_string("SharedProp", "FirstVal")
            .expect("set shared SharedProp");
        assert!(matches!(
            shared2.set_context_property_string("SharedProp", "BlockedOverwrite"),
            Err(HostError::ContextPropertyConflict { ref name }) if name == "SharedProp"
        ));
        shared2
            .set_context_property_variant_with_policy(
                "SharedProp",
                &QVariant::from(&QString::from("UpdatedSharedVal")),
                ContextPropertyConflict::Overwrite,
            )
            .expect("explicit overwrite succeeds on shared engine");

        // Private sessions keep isolated context values for identical keys.
        priv1
            .set_context_property_string("SessionLabel", "PrivateOne")
            .expect("set priv1 SessionLabel");
        priv2_dev
            .set_context_property_string("SessionLabel", "PrivateTwoDev")
            .expect("set priv2 SessionLabel");

        let s1_root = shared1
            .load(&QmlSource::File(ctx_probe_qml.clone()))
            .expect("load CtxProbe on shared1");
        let s2_root = shared2
            .load(&QmlSource::File(ctx_probe_qml.clone()))
            .expect("load CtxProbe on shared2");
        let p1_root = priv1
            .load(&QmlSource::File(ctx_probe_qml.clone()))
            .expect("load CtxProbe on priv1");
        let p2_root = priv2_dev
            .load(&QmlSource::File(ctx_probe_qml.clone()))
            .expect("load CtxProbe on priv2_dev");
        let p1_i18n = priv1
            .load(&QmlSource::File(i18n_qml.clone()))
            .expect("load I18nTest on priv1");
        let p2_i18n = priv2_dev
            .load(&QmlSource::File(i18n_qml.clone()))
            .expect("load I18nTest on priv2_dev");
        rt.process_events();

        assert_eq!(
            shared1
                .root_property(s1_root.root_id, "seenLabel")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "SharedInitial"
        );
        assert_eq!(
            shared2
                .root_property(s2_root.root_id, "sharedVal")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "UpdatedSharedVal"
        );
        assert_eq!(
            priv1
                .root_property(p1_root.root_id, "seenLabel")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "PrivateOne"
        );
        assert_eq!(
            priv2_dev
                .root_property(p2_root.root_id, "seenLabel")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "PrivateTwoDev"
        );
        // Priv1 did not inherit Shared's `SharedProp`.
        assert_eq!(
            priv1
                .root_property(p1_root.root_id, "sharedVal")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            ""
        );

        // Cross-session isolation: shared2 must not release or mutate priv1's root!
        assert!(matches!(
            shared2.release_root(p1_root.root_id),
            Err(HostError::RootNotOwned(_))
        ));
        assert!(priv1.is_root_alive(p1_root.root_id));
        assert!(!shared2.is_root_alive(p1_root.root_id));
        assert_eq!(shared1.open_window_count(), 1);
        assert_eq!(shared2.open_window_count(), 1);
        assert_eq!(priv1.open_window_count(), 2);
        assert_eq!(priv2_dev.open_window_count(), 2);
        assert_eq!(rt.open_window_count(), 6);

        // Multi-engine i18n: Stage A semantics propagate to Shared AND all Private engines.
        let fr_res = rt
            .register_translation_catalog(TranslationCatalog::Directory {
                id: "app".to_owned(),
                dir: app_i18n_dir.clone(),
            })
            .expect("register app catalog");
        assert_eq!(fr_res.active_locale, "en_US");

        let sw = rt
            .set_language("fr_FR")
            .expect("switch all engines to fr_FR");
        assert_eq!(sw.active_locale, "fr_FR");
        assert_eq!(sw.rinui_locale, "en_US");
        assert!(sw.rinui_used_fallback);
        assert!(!sw.active_locale_fallback);
        assert_eq!(TranslationManager::default_qt_locale(), "fr_FR");
        rt.process_events();

        for (sess, root_id) in [(&priv1, p1_i18n.root_id), (&priv2_dev, p2_i18n.root_id)] {
            assert_eq!(
                sess.root_property(root_id, "appGreeting")
                    .unwrap()
                    .value::<QString>()
                    .unwrap()
                    .to_string(),
                "Bonjour Application"
            );
        }
        // Newly created Calendar component after runtime switch to fr_FR uses French locale months
        // even though RinUI core catalog fell back to en_US.
        let p1_i18n_fr = priv1
            .load(&QmlSource::File(i18n_qml.clone()))
            .expect("load fresh I18nTest after fr_FR switch");
        rt.process_events();
        assert_eq!(
            priv1
                .root_property(p1_i18n_fr.root_id, "calendarMonthZero")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string()
                .to_lowercase(),
            "janvier"
        );
        priv1
            .release_root(p1_i18n_fr.root_id)
            .expect("release p1_i18n_fr");
        rt.process_events();
        assert_eq!(
            shared1
                .root_property(s1_root.root_id, "i18nGreeting")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "Bonjour Application"
        );

        // Dynamic top-level window opened inside priv1 QML must belong ONLY to priv1!
        priv1
            .load(&QmlSource::File(dialog_qml.clone()))
            .expect("load secondary dialog in priv1");
        // Clear any pending and create a dynamic window via QML secondary root on priv1 using load_secondary_window vs QML component.
        let dyn_qml_dir = tmp.path().join("dyn_qml");
        std::fs::create_dir_all(&dyn_qml_dir).expect("create dyn_qml dir");
        let dyn_opener_qml = dyn_qml_dir.join("DynOpener.qml");
        std::fs::write(
            &dyn_opener_qml,
            r#"import QtQuick 2.15
import RinUI
FluentWindow {
    id: openerRoot
    visible: false
    Component {
        id: subComp
        FluentWindow { visible: false; objectName: "dynCompWin" }
    }
    Component.onCompleted: {
        Qt.callLater(function() {
            subComp.createObject(null, { transientParent: openerRoot })
        })
    }
}
"#,
        )
        .expect("write DynOpener.qml");
        priv1
            .load(&QmlSource::File(dyn_opener_qml))
            .expect("load DynOpener on priv1");
        rt.process_events();
        // Second dynamic window created post-load must belong ONLY to priv1 (`sync_windows` isolates sessions/engines).
        let s2_synced = shared2.sync_windows().expect("shared2 sync_windows");
        assert!(s2_synced.is_empty());
        let p2_synced = priv2_dev.sync_windows().expect("priv2 sync_windows");
        assert!(p2_synced.is_empty());
        let p1_synced = priv1.sync_windows().expect("priv1 sync_windows");
        assert_eq!(p1_synced.len(), 1);
        assert_eq!(
            p1_synced[0]
                .property("objectName")
                .value::<QString>()
                .unwrap()
                .to_string(),
            "dynCompWin"
        );

        // Drop shared1 and verify shared1's handle is disposed while shared2 & priv sessions stay live.
        let shared1_handle = shared1.handle();
        drop(shared1);
        assert!(matches!(
            shared1_handle.current_locale(),
            Err(HostError::HostDisposed)
        ));
        assert_eq!(shared2.open_window_count(), 1);
        rt.with_theme_manager_mut(|tm| tm.toggle_theme(QString::from("Dark")))
            .expect("toggle theme to Dark");
        rt.process_events();
        assert_eq!(
            shared2
                .root_property(s2_root.root_id, "isDarkNow")
                .unwrap()
                .value::<bool>(),
            Some(true)
        );
        assert_eq!(
            priv2_dev
                .root_property(p2_root.root_id, "isDarkNow")
                .unwrap()
                .value::<bool>(),
            Some(true)
        );

        // Reversed drop order: drop RinUIRuntime BEFORE shared2, priv1, priv2_dev!
        drop(rt);
        assert_eq!(
            priv2_dev
                .current_locale()
                .expect("session survives runtime drop"),
            "fr_FR"
        );
        assert!(priv2_dev.is_root_alive(p2_root.root_id));
        shared2
            .set_language("en_US")
            .expect("session can still retranslate after runtime drop");
        priv2_dev.process_events();
        assert_eq!(
            shared2
                .root_property(s2_root.root_id, "i18nGreeting")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "Hello App"
        );
        drop(shared2);
        drop(priv1);
        // Temporary dev view directory stays alive while priv2_dev is alive...
        assert!(dev_load_root_to_verify.is_dir());
        drop(priv2_dev);
    }
    // ...and is cleaned up cleanly once final session/runtime finishes dropping.
    assert!(!dev_load_root_to_verify.exists());

    // -------------------------------------------------------------------------
    // Phase 3: External QGuiApplication borrowing, preservation, restoration,
    // wrong-thread/mismatch/style-conflict guards, and sequential re-attach.
    // -------------------------------------------------------------------------
    let mut external_app = QGuiApplication::new();
    {
        let mut app_pin = external_app
            .as_mut()
            .expect("create external QGuiApplication");
        app_pin
            .as_mut()
            .set_organization_name(&QString::from("CallerOriginalOrg"));
        app_pin
            .as_mut()
            .set_application_name(&QString::from("CallerOriginalApp"));
    }

    // Create caller's own external non-RinUI window via a separate QQmlApplicationEngine.
    let mut caller_engine = QQmlApplicationEngine::new();
    {
        let caller_qml = QByteArray::from(
            b"import QtQuick 2.15\nimport QtQuick.Window 2.15\nWindow { visible: false; objectName: \"callerExternalWin\" }\n"
                .as_slice(),
        );
        caller_engine
            .as_mut()
            .unwrap()
            .load_data(&caller_qml, &QUrl::from("qrc:/caller/ExternalWindow.qml"));
    }

    // Wrong thread test using raw pointer usize passed to worker thread.
    let app_qobj_addr = active_qt_application_ptr() as usize;
    let wrong_thread_dir = tmp.path().join("wrong_thread_cfg");
    let thread_res = std::thread::spawn(move || {
        let ptr = app_qobj_addr as *mut rinui_host::cxx_qt::QObject;
        unsafe {
            RinUIRuntime::attach_existing_raw(
                ptr,
                runtime_cfg(&wrong_thread_dir, "WrongThread", ResourceMode::Embedded),
            )
        }
        .map(|_| ())
    })
    .join()
    .expect("join wrong-thread test thread");
    assert!(matches!(thread_res, Err(HostError::WrongThread)));

    // Mismatched QObject* (an engine pointer instead of QGuiApplication*) must fail cleanly.
    let eng_qobj_ptr = {
        let eng_pin = caller_engine.as_mut().unwrap();
        let qml_eng_pin: std::pin::Pin<&mut QQmlEngine> = eng_pin.upcast_pin();
        let qobj_pin: std::pin::Pin<&mut rinui_host::cxx_qt::QObject> = qml_eng_pin.upcast_pin();
        unsafe { qobj_pin.get_unchecked_mut() as *mut rinui_host::cxx_qt::QObject }
    };
    assert!(matches!(
        unsafe {
            RinUIRuntime::attach_existing_raw(
                eng_qobj_ptr,
                runtime_cfg(tmp.path(), "MismatchPtr", ResourceMode::Embedded),
            )
        },
        Err(HostError::ApplicationMismatch)
    ));

    // Incompatible Quick Controls style after Basic was already initialized must be rejected.
    let mut bad_style_cfg = runtime_cfg(tmp.path(), "BadStyle", ResourceMode::Embedded);
    bad_style_cfg.qt_quick_controls_style = "Fusion".to_owned();
    assert!(matches!(
        RinUIRuntime::attach_existing(external_app.as_ref().unwrap(), bad_style_cfg),
        Err(HostError::IncompatibleQuickControlsStyle { .. })
    ));

    let locale_before_attach = TranslationManager::default_qt_locale();

    // Safe lifetime-bound attach #1 (with identity override temporarily applied & restored).
    {
        let mut attach_cfg = runtime_cfg(
            &tmp.path().join("borrowed1_cfg"),
            "RinUIOverrideApp",
            ResourceMode::Embedded,
        );
        attach_cfg.apply_identity_to_borrowed_app = true;
        attach_cfg.identity.organization_name = "RinUITempOrg".to_owned();

        let mut rt_borrowed =
            RinUIRuntime::attach_existing(external_app.as_ref().unwrap(), attach_cfg.clone())
                .expect("attach_existing to QGuiApplication");
        assert!(rt_borrowed.is_borrowed_application());
        assert_eq!(
            external_app
                .as_ref()
                .unwrap()
                .organization_name()
                .to_string(),
            "RinUITempOrg"
        );
        assert!(matches!(
            rt_borrowed.exec(),
            Err(HostError::BorrowedApplicationExecNotAllowed)
        ));

        // Second concurrent attach must be rejected cleanly.
        assert!(matches!(
            RinUIRuntime::attach_existing(external_app.as_ref().unwrap(), attach_cfg),
            Err(HostError::ApplicationAlreadyInitialized)
        ));

        let mut session = rt_borrowed
            .create_session(SessionConfig::shared())
            .expect("create session on borrowed runtime");
        let loaded = session
            .load(&QmlSource::File(smoke_qml.clone()))
            .expect("load SmokeTest on borrowed app");
        assert_eq!(loaded.windows.len(), 1);

        // sync_windows must ignore caller's external Window!
        let synced = session.sync_windows().expect("sync windows");
        assert!(synced.is_empty());
        assert_eq!(session.open_window_count(), 1);

        rt_borrowed
            .set_language("zh_CN")
            .expect("switch borrowed locale");
        rt_borrowed.process_events();
        assert_eq!(TranslationManager::default_qt_locale(), "zh_CN");
    }

    // After borrowed runtime drops: QGuiApplication, identity, QLocale, and caller window survive!
    assert_eq!(
        external_app
            .as_ref()
            .unwrap()
            .organization_name()
            .to_string(),
        "CallerOriginalOrg"
    );
    assert_eq!(
        external_app
            .as_ref()
            .unwrap()
            .application_name()
            .to_string(),
        "CallerOriginalApp"
    );
    assert_eq!(
        TranslationManager::default_qt_locale(),
        locale_before_attach
    );
    external_app.as_ref().unwrap().process_events();
    // A manual Qt driver must dispatch DeferredDeletes posted outside exec().
    // SAFETY: this is the outer Rust driver; no Qt/QML callback is on the stack.
    unsafe { rinui_host::__test_support::rinui_test_dispatch_deferred_deletes() };

    // Sequential re-attach #2 via unsafe raw pointer escape hatch.
    {
        let mut raw_cfg = runtime_cfg(
            &tmp.path().join("borrowed2_cfg"),
            "RawReattachApp",
            ResourceMode::Embedded,
        );
        // Caller did not request identity override this time: Qt identity stays untouched!
        raw_cfg.apply_identity_to_borrowed_app = false;
        let ptr = app_qobj_addr as *mut rinui_host::cxx_qt::QObject;
        let rt2 = unsafe { RinUIRuntime::attach_existing_raw(ptr, raw_cfg) }
            .expect("sequential re-attach via attach_existing_raw must succeed");
        assert_eq!(
            external_app
                .as_ref()
                .unwrap()
                .organization_name()
                .to_string(),
            "CallerOriginalOrg"
        );
        let mut s2 = rt2
            .create_session(SessionConfig::private())
            .expect("session after re-attach");
        s2.load(&QmlSource::File(smoke_qml))
            .expect("load SmokeTest after sequential re-attach");
        rt2.process_events();
    }

    external_app.as_ref().unwrap().process_events();
    // A manual Qt driver must dispatch DeferredDeletes posted outside exec().
    // SAFETY: this is the outer Rust driver; no Qt/QML callback is on the stack.
    unsafe { rinui_host::__test_support::rinui_test_dispatch_deferred_deletes() };
    drop(caller_engine);
    drop(external_app);
    println!("runtime_sessions integration tests passed.");
}
