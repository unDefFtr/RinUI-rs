use rinui_host::__test_support::{install_callback_handler, rinui_callback_relay_new};
use rinui_host::cxx_qt_lib::{QList, QString, QStringList, QVariant};
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, HostError, MacWindowOptions, QmlSource, ResourceMode,
    RinUIHost, RinUIRuntime, RuntimeConfig, SessionConfig,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use tempfile::TempDir;

fn main() {
    if std::env::var_os("QT_QPA_PLATFORM").is_none() {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let fixture_qml =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ObjectInteropFixture.qml");

    let tmp = TempDir::new().expect("temp dir");

    // -------------------------------------------------------------------------
    // Part 1: RinUIHost + configure_engine + checked properties & methods
    // -------------------------------------------------------------------------
    {
        let offline_dir = tmp.path().join("offline_storage");
        let extra_import_dir = tmp.path().join("custom_imports");
        let selector_dir = tmp.path().join("selector_qml");
        let selector_custom_dir = selector_dir.join("+custom");
        std::fs::create_dir_all(&offline_dir).unwrap();
        std::fs::create_dir_all(&extra_import_dir).unwrap();
        std::fs::create_dir_all(&selector_custom_dir).unwrap();

        std::fs::write(
            selector_dir.join("SelectedBadge.qml"),
            "import QtQuick 2.15\nQtObject { property string origin: \"base\" }\n",
        )
        .unwrap();
        std::fs::write(
            selector_custom_dir.join("SelectedBadge.qml"),
            "import QtQuick 2.15\nQtObject { property string origin: \"custom-selector\" }\n",
        )
        .unwrap();
        let selector_host_qml = selector_dir.join("SelectorHost.qml");
        std::fs::write(
            &selector_host_qml,
            "import QtQuick 2.15\nimport RinUI\nFluentWindow { visible: false; SelectedBadge { id: b } property string selectedOrigin: b.origin }\n",
        )
        .unwrap();

        let mut host = RinUIHost::new(HostConfig {
            identity: AppIdentity::new("RinUIInteropTest", "HostInterop"),
            resource_mode: ResourceMode::Embedded,
            config_location: ConfigLocation::CustomDir(tmp.path().join("host_cfg")),
            locale: Some("en_US".to_owned()),
            mac_window: MacWindowOptions::default(),
            qt_quick_controls_style: "Basic".to_owned(),
        })
        .expect("init RinUIHost");

        let configured_offline = host
            .configure_engine(|cfg| {
                cfg.set_offline_storage_path(&offline_dir);
                cfg.add_import_path(&extra_import_dir);
                cfg.set_extra_file_selectors(&["custom"]);
                unsafe {
                    cfg.with_raw_engine_mut(|_raw| ());
                }
                assert!(cfg
                    .import_paths()
                    .iter()
                    .any(|p| p.contains("custom_imports")));
                cfg.offline_storage_path()
            })
            .expect("configure_engine succeeds");
        assert!(configured_offline.contains("offline_storage"));

        // Verify extra file selector `+custom` actually changed QML file selection.
        let sel_root = host
            .load(&QmlSource::File(selector_host_qml))
            .expect("load SelectorHost.qml");
        assert_eq!(
            host.get_root_property(sel_root.root_id, "selectedOrigin")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "custom-selector"
        );

        let loaded = host
            .load(&QmlSource::File(fixture_qml.clone()))
            .expect("load ObjectInteropFixture.qml");
        host.process_events();

        // Write & read bool, int, double, string, var list, QStringList, QList<int>.
        host.set_root_property(loaded.root_id, "flagProp", &QVariant::from(&true))
            .expect("set bool prop");
        assert_eq!(
            host.get_root_property(loaded.root_id, "flagProp")
                .unwrap()
                .value::<bool>(),
            Some(true)
        );

        host.set_root_property(loaded.root_id, "countProp", &QVariant::from(&99i32))
            .expect("set int prop");
        assert_eq!(
            host.get_root_property(loaded.root_id, "countProp")
                .unwrap()
                .value::<i32>(),
            Some(99)
        );

        host.set_root_property(loaded.root_id, "ratioProp", &QVariant::from(&3.25f64))
            .expect("set double prop");
        assert_eq!(
            host.get_root_property(loaded.root_id, "ratioProp")
                .unwrap()
                .value::<f64>(),
            Some(3.25)
        );

        host.set_root_property(
            loaded.root_id,
            "titleProp",
            &QVariant::from(&QString::from("updated-from-rust")),
        )
        .expect("set string prop");
        assert_eq!(
            host.get_root_property(loaded.root_id, "titleProp")
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "updated-from-rust"
        );

        let mut var_list = QList::<QVariant>::default();
        var_list.append(QVariant::from(&QString::from("x")));
        var_list.append(QVariant::from(&QString::from("y")));
        host.set_root_property(loaded.root_id, "listProp", &QVariant::from(&var_list))
            .expect("set var list prop");
        let read_var_list = host
            .get_root_property(loaded.root_id, "listProp")
            .unwrap()
            .value::<QList<QVariant>>()
            .unwrap();
        assert_eq!(read_var_list.len(), 2);

        let mut str_list = QStringList::default();
        str_list.append(QString::from("s1"));
        str_list.append(QString::from("s2"));
        str_list.append(QString::from("s3"));
        host.set_root_property(loaded.root_id, "strListProp", &QVariant::from(&str_list))
            .expect("set list<string> prop");
        assert_eq!(
            host.get_root_property(loaded.root_id, "strListProp")
                .unwrap()
                .value::<QStringList>()
                .unwrap()
                .len(),
            3
        );

        let int_list = QList::<i32>::from_iter(&[4, 5, 6]);
        host.set_root_property(loaded.root_id, "intListProp", &QVariant::from(&int_list))
            .expect("set list<int> prop");
        assert_eq!(
            host.get_root_property(loaded.root_id, "intListProp")
                .unwrap()
                .value::<QList<i32>>()
                .unwrap()
                .len(),
            3
        );

        // Read-only rejection.
        assert!(matches!(
            host.set_root_property(loaded.root_id, "readOnlyNum", &QVariant::from(&100i32)),
            Err(HostError::PropertyReadOnly { ref property }) if property == "readOnlyNum"
        ));

        // Unknown property / typo must return PropertyNotFound and NEVER create a dynamic property!
        assert!(matches!(
            host.set_root_property(loaded.root_id, "typoNonExistent", &QVariant::from(&123i32)),
            Err(HostError::PropertyNotFound { ref property }) if property == "typoNonExistent"
        ));
        assert!(matches!(
            host.get_root_property(loaded.root_id, "typoNonExistent"),
            Err(HostError::PropertyNotFound { ref property }) if property == "typoNonExistent"
        ));

        // Strict type mismatch checks.
        assert!(matches!(
            host.set_root_property(
                loaded.root_id,
                "countProp",
                &QVariant::from(&QString::from("not-an-int"))
            ),
            Err(HostError::PropertyTypeMismatch { .. })
        ));
        assert!(matches!(
            host.set_root_property(loaded.root_id, "titleProp", &QVariant::from(&123i32)),
            Err(HostError::PropertyTypeMismatch { .. })
        ));
        assert!(matches!(
            host.set_root_property(loaded.root_id, "flagProp", &QVariant::from(&1i32)),
            Err(HostError::PropertyTypeMismatch { .. })
        ));
        assert!(matches!(
            host.set_root_property(loaded.root_id, "countProp", &QVariant::from(&i64::MAX)),
            Err(HostError::PropertyTypeMismatch { .. })
        ));

        // Invoking 0-arg, multi-arg, typed, list-returning methods on RinUIHost AND HostHandle.
        let p1 = host
            .invoke_root_method(loaded.root_id, "zeroArgPing", &[])
            .expect("zeroArgPing via host");
        assert_eq!(p1.value::<QString>().unwrap().to_string(), "pong:1");

        let handle = host.handle();
        let p2 = handle
            .invoke_root_method(loaded.root_id, "zeroArgPing", &[])
            .expect("zeroArgPing via handle");
        assert_eq!(p2.value::<QString>().unwrap().to_string(), "pong:2");

        let sum = host
            .invoke_root_method(
                loaded.root_id,
                "addInts",
                &[QVariant::from(&19i32), QVariant::from(&23i32)],
            )
            .expect("addInts");
        assert_eq!(sum.value::<i32>(), Some(42));

        let sum5 = handle
            .invoke_root_method(
                loaded.root_id,
                "sumFive",
                &[
                    QVariant::from(&1i32),
                    QVariant::from(&2i32),
                    QVariant::from(&3i32),
                    QVariant::from(&4i32),
                    QVariant::from(&5i32),
                ],
            )
            .expect("sumFive");
        assert_eq!(sum5.value::<i32>(), Some(15));

        let list_ret = host
            .invoke_root_method(
                loaded.root_id,
                "makeSummaryList",
                &[QVariant::from(&QString::from("tag")), QVariant::from(&3i32)],
            )
            .expect("makeSummaryList");
        let list_items = list_ret
            .value::<QList<QVariant>>()
            .expect("QList<QVariant> return");
        assert_eq!(list_items.len(), 3);
        assert_eq!(
            list_items
                .get(0)
                .unwrap()
                .value::<QString>()
                .unwrap()
                .to_string(),
            "tag"
        );
        assert_eq!(list_items.get(1).unwrap().value::<i32>(), Some(3));
        assert_eq!(list_items.get(2).unwrap().value::<bool>(), Some(true));

        let typed_ret = host
            .invoke_root_method(
                loaded.root_id,
                "typedRepeat",
                &[QVariant::from(&QString::from("ha")), QVariant::from(&3i32)],
            )
            .expect("typedRepeat typed QML method");
        assert_eq!(typed_ret.value::<QString>().unwrap().to_string(), "hahaha");

        // Error paths: missing method, arity mismatch, typed arg mismatch, > 10 args.
        assert!(matches!(
            host.invoke_root_method(loaded.root_id, "missingMethod", &[]),
            Err(HostError::MethodNotFound { ref method }) if method == "missingMethod"
        ));
        assert!(matches!(
            host.invoke_root_method(loaded.root_id, "addInts", &[QVariant::from(&1i32)]),
            Err(HostError::MethodArgumentCountMismatch { ref method, .. }) if method == "addInts"
        ));
        assert!(matches!(
            host.invoke_root_method(
                loaded.root_id,
                "typedRepeat",
                &[QVariant::from(&123i32), QVariant::from(&2i32)],
            ),
            Err(HostError::UnsupportedMethodSignature { ref method, .. }) if method == "typedRepeat"
        ));
        let eleven: Vec<QVariant> = (0..11).map(|i| QVariant::from(&i)).collect();
        assert!(matches!(
            host.invoke_root_method(loaded.root_id, "addInts", &eleven),
            Err(HostError::TooManyMethodArguments { count: 11, .. })
        ));

        // Releasing root makes subsequent property/method operations return HostDisposed.
        host.release_root(loaded.root_id);
        host.process_events();
        assert!(matches!(
            host.set_root_property(loaded.root_id, "flagProp", &QVariant::from(&false)),
            Err(HostError::HostDisposed)
        ));
        assert!(matches!(
            host.get_root_property(loaded.root_id, "flagProp"),
            Err(HostError::HostDisposed)
        ));
        assert!(matches!(
            host.invoke_root_method(loaded.root_id, "zeroArgPing", &[]),
            Err(HostError::HostDisposed)
        ));
    }

    // -------------------------------------------------------------------------
    // Part 2: Cross-session ownership, QML->Rust re-entrancy & mid-call session drop
    // -------------------------------------------------------------------------
    {
        let rt = RinUIRuntime::new(RuntimeConfig {
            identity: AppIdentity::new("RinUIInteropTest", "SessionCallback"),
            resource_mode: ResourceMode::Embedded,
            config_location: ConfigLocation::CustomDir(tmp.path().join("sess_cfg")),
            locale: Some("en_US".to_owned()),
            mac_window: MacWindowOptions::default(),
            qt_quick_controls_style: "Basic".to_owned(),
            apply_identity_to_borrowed_app: false,
        })
        .expect("create runtime");

        let mut sess_a = rt
            .create_session(SessionConfig::private())
            .expect("create priv sess_a");
        let mut sess_b = rt
            .create_session(SessionConfig::private())
            .expect("create priv sess_b");

        let relay = rinui_callback_relay_new();
        sess_a
            .expose_qobject("HostCallback", relay)
            .expect("expose HostCallback");

        let root_a = sess_a
            .load(&QmlSource::File(fixture_qml.clone()))
            .expect("load fixture on sess_a");
        let root_b = sess_b
            .load(&QmlSource::File(fixture_qml))
            .expect("load fixture on sess_b");
        rt.process_events();

        // Cross-session access must be rejected with RootNotOwned.
        assert!(matches!(
            sess_b.set_root_property(root_a.root_id, "countProp", &QVariant::from(&5i32)),
            Err(HostError::RootNotOwned(_))
        ));
        assert!(matches!(
            sess_b.get_root_property(root_a.root_id, "countProp"),
            Err(HostError::RootNotOwned(_))
        ));
        assert!(matches!(
            sess_b.invoke_root_method(root_a.root_id, "zeroArgPing", &[]),
            Err(HostError::RootNotOwned(_))
        ));

        // Case 2a: QML -> Rust callback attempting re-entrant invoke_root_method gets ReentrantCall.
        let handle_a = sess_a.handle();
        let reentrant_seen = Rc::new(RefCell::new(false));
        let reentrant_flag = Rc::clone(&reentrant_seen);
        let handle_for_reenter = handle_a.clone();
        let root_a_id = root_a.root_id;
        let _guard_reenter = install_callback_handler(move |tag| {
            if tag == "reenter" {
                let res = handle_for_reenter.invoke_root_method(root_a_id, "zeroArgPing", &[]);
                if matches!(res, Err(HostError::ReentrantCall("invoke_root_method"))) {
                    *reentrant_flag.borrow_mut() = true;
                }
            }
        });

        let cb_reenter = handle_a
            .invoke_root_method(
                root_a.root_id,
                "invokeHostCallback",
                &[QVariant::from(&QString::from("reenter"))],
            )
            .expect("invokeHostCallback reenter returns cleanly");
        assert_eq!(
            cb_reenter.value::<QString>().unwrap().to_string(),
            "callback-done:reenter"
        );
        assert!(
            *reentrant_seen.borrow(),
            "Re-entrant QML->Rust callback must observe HostError::ReentrantCall"
        );

        // Case 2b: QML -> Rust callback drops its own RinUISession mid-invocation without UAF.
        let sess_a_cell = Rc::new(RefCell::new(Some(sess_a)));
        let sess_a_drop_target = Rc::clone(&sess_a_cell);
        let handle_check_disposed = handle_a.clone();
        let self_dropped_ok = Rc::new(RefCell::new(false));
        let self_dropped_flag = Rc::clone(&self_dropped_ok);
        let _guard_drop = install_callback_handler(move |tag| {
            if tag == "drop-self" {
                // Drop RinUISession (and private engine) while C++ QMetaMethod::invoke is still on stack!
                drop(sess_a_drop_target.borrow_mut().take());
                if matches!(
                    handle_check_disposed.current_locale(),
                    Err(HostError::HostDisposed)
                ) {
                    *self_dropped_flag.borrow_mut() = true;
                }
            }
        });

        let cb_drop = handle_a
            .invoke_root_method(
                root_a.root_id,
                "invokeHostCallback",
                &[QVariant::from(&QString::from("drop-self"))],
            )
            .expect("invokeHostCallback self-drop must unwind cleanly without UAF");
        assert_eq!(
            cb_drop.value::<QString>().unwrap().to_string(),
            "callback-done:drop-self"
        );
        assert!(*self_dropped_ok.borrow());
        assert!(sess_a_cell.borrow().is_none());
        assert!(matches!(
            handle_a.current_locale(),
            Err(HostError::HostDisposed)
        ));

        // Sibling session sess_b remains completely unaffected!
        assert!(sess_b.is_root_alive(root_b.root_id));
        let b_ping = sess_b
            .invoke_root_method(root_b.root_id, "zeroArgPing", &[])
            .expect("sibling session unaffected after private session mid-callback drop");
        assert_eq!(b_ping.value::<QString>().unwrap().to_string(), "pong:1");
    }

    println!("object_interop integration tests passed.");
}
