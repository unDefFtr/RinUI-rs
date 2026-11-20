use rinui_host::__test_support::{
    install_callback_handler, rinui_callback_relay_new, rinui_test_dispatch_deferred_deletes,
};
use rinui_host::cxx_qt_lib::{QGuiApplication, QString, QVariant};
use rinui_host::{
    AppIdentity, ConfigLocation, HostConfig, HostError, MacWindowOptions, QmlSource, ResourceMode,
    RinUIHost, RinUIRuntime, RuntimeConfig, SessionConfig,
};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const WORKER_MODES: &[&str] = &[
    "load-drop",
    "load-drop-shared",
    "property-drop",
    "translation-drop",
    "event-drop-process",
    "event-drop-exec",
    "event-nested-prop-drop",
    "theme-signal-drop",
    "ownership",
    "theme-alias",
    "locale-live-embedded",
    "locale-live-dev",
    "external-process-safe",
    "external-process-nested-prop-drop",
    "external-process-nested-pump",
    "external-process-nested-pump-session",
    "external-process-nested-app-pump",
    "external-process-nested-app-pump-last-private",
    "external-process-nested-app-pump-last-shared",
    "external-exec-last-private",
    "external-exec-last-reversed",
    "external-exec-last-shared",
    "external-process-last-private",
    "external-process-last-shared-reversed",
    "external-process-last-app-drop",
];

fn runtime_cfg(tmp: &TempDir, name: &str, mode: ResourceMode) -> RuntimeConfig {
    RuntimeConfig {
        identity: AppIdentity::new("RinUIParityReview", name),
        resource_mode: mode,
        config_location: ConfigLocation::CustomDir(tmp.path().join(name)),
        locale: Some("en_US".to_owned()),
        mac_window: MacWindowOptions::default(),
        qt_quick_controls_style: "Basic".to_owned(),
        apply_identity_to_borrowed_app: false,
    }
}

fn get_string_prop(
    session: &rinui_host::RinUISession<'_>,
    root_id: rinui_host::RootId,
    name: &str,
) -> String {
    session
        .get_root_property(root_id, name)
        .unwrap_or_else(|e| panic!("failed reading '{name}': {e}"))
        .value::<QString>()
        .unwrap_or_else(|| panic!("property '{name}' was not a QString"))
        .to_string()
}

fn run_worker(mode: &str) {
    let tmp = TempDir::new().expect("create tempdir");
    match mode {
        "load-drop" => worker_load_drop(&tmp),
        "load-drop-shared" => worker_load_drop_shared(&tmp),
        "property-drop" => worker_property_drop(&tmp),
        "translation-drop" => worker_translation_drop(&tmp),
        "event-drop-process" => worker_event_drop_process(&tmp),
        "event-drop-exec" => worker_event_drop_exec(&tmp),
        "event-nested-prop-drop" => worker_event_nested_prop_drop(&tmp),
        "theme-signal-drop" => worker_theme_signal_drop(&tmp),
        "ownership" => worker_ownership(&tmp),
        "theme-alias" => worker_theme_alias(&tmp),
        "locale-live-embedded" => worker_locale_live(&tmp, ResourceMode::Embedded),
        "locale-live-dev" => {
            let upstream = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("upstream")
                .join("Rin-UI");
            worker_locale_live(&tmp, ResourceMode::Filesystem(upstream));
        }
        mode if mode.starts_with("external-") => worker_external(&tmp, mode),
        other => panic!("unknown parity worker mode: {other}"),
    }
}

fn worker_load_drop(tmp: &TempDir) {
    let rt = RinUIRuntime::new(runtime_cfg(tmp, "LoadDrop", ResourceMode::Embedded))
        .expect("init runtime");
    let mut s = rt
        .create_session(SessionConfig::private())
        .expect("create private session");
    s.expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay");
    let qml_path = tmp.path().join("LoadDrop.qml");
    std::fs::write(
        &qml_path,
        "import QtQuick\nItem { Component.onCompleted: Relay.trigger(\"drop\") }\n",
    )
    .expect("write qml");

    let handle = s.handle();
    let cell = Rc::new(RefCell::new(Some(s)));
    let drop_target = Rc::clone(&cell);
    let _guard = install_callback_handler(move |tag| {
        if tag == "drop" {
            drop(drop_target.borrow_mut().take());
        }
    });

    let res = handle.load_secondary_window(&QmlSource::File(qml_path));
    assert!(
        matches!(res, Err(HostError::HostDisposed)),
        "load after self-drop must return HostDisposed, got {res:?}"
    );
    assert!(cell.borrow().is_none());
    assert!(matches!(
        handle.current_locale(),
        Err(HostError::HostDisposed)
    ));
    assert_eq!(rt.open_window_count(), 0);
}

fn worker_load_drop_shared(tmp: &TempDir) {
    let rt = RinUIRuntime::new(runtime_cfg(tmp, "LoadDropShared", ResourceMode::Embedded))
        .expect("init runtime");
    let mut keep = rt
        .create_session(SessionConfig::shared())
        .expect("create shared keep");
    let mut s_drop = rt
        .create_session(
            SessionConfig::shared()
                .with_conflict_policy(rinui_host::ContextPropertyConflict::Overwrite),
        )
        .expect("create shared drop");

    let keep_qml = tmp.path().join("Keep.qml");
    std::fs::write(
        &keep_qml,
        "import QtQuick\nItem { property int v: 42; function ping() { return v } }\n",
    )
    .expect("write keep qml");
    let drop_qml = tmp.path().join("DropShared.qml");
    std::fs::write(
        &drop_qml,
        "import QtQuick\nItem { Component.onCompleted: Relay.trigger(\"drop\") }\n",
    )
    .expect("write drop qml");

    let keep_loaded = keep.load(&QmlSource::File(keep_qml)).expect("load keep");

    s_drop
        .expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay");

    let handle_drop = s_drop.handle();
    let cell = Rc::new(RefCell::new(Some(s_drop)));
    let target = Rc::clone(&cell);
    let _guard = install_callback_handler(move |tag| {
        if tag == "drop" {
            drop(target.borrow_mut().take());
        }
    });

    let res = handle_drop.load_secondary_window(&QmlSource::File(drop_qml));
    assert!(
        matches!(res, Err(HostError::HostDisposed)),
        "expected HostDisposed, got {res:?}"
    );
    assert!(keep.is_root_alive(keep_loaded.root_id));
    let ping = keep
        .invoke_root_method(keep_loaded.root_id, "ping", &[])
        .expect("sibling shared session stays healthy");
    assert_eq!(ping.value::<i32>(), Some(42));
}

fn worker_property_drop(tmp: &TempDir) {
    let rt = RinUIRuntime::new(runtime_cfg(tmp, "PropertyDrop", ResourceMode::Embedded))
        .expect("init runtime");
    let mut s = rt
        .create_session(SessionConfig::private())
        .expect("create private session");
    s.expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay");
    let qml_path = tmp.path().join("PropDrop.qml");
    std::fs::write(
        &qml_path,
        "import QtQuick\nItem { property int n: 0; onNChanged: { if (n === 1) Relay.trigger(\"drop\") } }\n",
    )
    .expect("write qml");
    let rid = s
        .load(&QmlSource::File(qml_path))
        .expect("load qml")
        .root_id;

    let handle = s.handle();
    let cell = Rc::new(RefCell::new(Some(s)));
    let drop_target = Rc::clone(&cell);
    let _guard = install_callback_handler(move |tag| {
        if tag == "drop" {
            drop(drop_target.borrow_mut().take());
        }
    });

    handle
        .set_root_property(rid, "n", &QVariant::from(&1i32))
        .expect("set_root_property triggering self-drop unwinds cleanly");
    assert!(cell.borrow().is_none());
    assert!(matches!(
        handle.get_root_property(rid, "n"),
        Err(HostError::HostDisposed)
    ));
}

fn worker_translation_drop(tmp: &TempDir) {
    let rt = RinUIRuntime::new(runtime_cfg(tmp, "TranslationDrop", ResourceMode::Embedded))
        .expect("init runtime");
    let mut s1 = rt
        .create_session(SessionConfig::private())
        .expect("create private s1");
    let mut s2 = rt
        .create_session(SessionConfig::private())
        .expect("create private s2");
    let mut s3 = rt
        .create_session(SessionConfig::private())
        .expect("create private s3");

    s1.expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay on s1");
    s2.expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay on s2");

    let qml_drop = tmp.path().join("TransDrop.qml");
    std::fs::write(
        &qml_drop,
        "import QtQuick\nItem { property string label: qsTranslate(\"Calendar\", \"Today\"); onLabelChanged: { if (label !== \"Today\") Relay.trigger(\"drop\") } }\n",
    )
    .expect("write drop qml");
    let qml_keep = tmp.path().join("TransKeep.qml");
    std::fs::write(
        &qml_keep,
        "import QtQuick\nItem { property string label: qsTranslate(\"Calendar\", \"Today\") }\n",
    )
    .expect("write keep qml");

    s1.load(&QmlSource::File(qml_drop.clone()))
        .expect("load s1");
    s2.load(&QmlSource::File(qml_drop)).expect("load s2");
    let r3 = s3
        .load(&QmlSource::File(qml_keep))
        .expect("load s3")
        .root_id;

    let h1 = s1.handle();
    let h2 = s2.handle();
    let c1 = Rc::new(RefCell::new(Some(s1)));
    let c2 = Rc::new(RefCell::new(Some(s2)));
    let d1 = Rc::clone(&c1);
    let d2 = Rc::clone(&c2);
    let _guard = install_callback_handler(move |tag| {
        if tag == "drop" {
            // Dropping both private engines while retranslate_all_engines is active
            // must neither SIGABRT current engine nor UAF sibling snapshots.
            drop(d1.borrow_mut().take());
            drop(d2.borrow_mut().take());
        }
    });

    let res = rt
        .set_language("zh_CN")
        .expect("set_language unwinds cleanly");
    assert_eq!(res.active_locale, "zh_CN");
    assert!(c1.borrow().is_none());
    assert!(c2.borrow().is_none());
    assert!(matches!(h1.current_locale(), Err(HostError::HostDisposed)));
    assert!(matches!(h2.current_locale(), Err(HostError::HostDisposed)));
    assert_eq!(get_string_prop(&s3, r3, "label"), "今天");
}

fn worker_event_drop_process(tmp: &TempDir) {
    let rt = RinUIRuntime::new(runtime_cfg(tmp, "EventDropProcess", ResourceMode::Embedded))
        .expect("init runtime");
    let mut s = rt
        .create_session(SessionConfig::private())
        .expect("create private session");
    s.expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay");

    let qml = tmp.path().join("EventDrop.qml");
    std::fs::write(
        &qml,
        r#"import QtQuick
Item {
    Timer {
        interval: 1
        running: true
        repeat: false
        onTriggered: Relay.trigger("drop")
    }
}
"#,
    )
    .expect("write qml");
    s.load(&QmlSource::File(qml)).expect("load qml");

    let handle = s.handle();
    let cell = Rc::new(RefCell::new(Some(s)));
    let target = Rc::clone(&cell);
    let fired = Rc::new(RefCell::new(false));
    let fired_flag = Rc::clone(&fired);
    let _guard = install_callback_handler(move |tag| {
        if tag == "drop" {
            *fired_flag.borrow_mut() = true;
            drop(target.borrow_mut().take());
        }
    });

    for _ in 0..40 {
        rt.process_events();
        if *fired.borrow() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(*fired.borrow(), "Timer did not fire under process_events()");
    assert!(cell.borrow().is_none());
    assert!(matches!(
        handle.current_locale(),
        Err(HostError::HostDisposed)
    ));
    assert_eq!(rt.open_window_count(), 0);
}

fn worker_event_drop_exec(tmp: &TempDir) {
    let mut rt = RinUIRuntime::new(runtime_cfg(tmp, "EventDropExec", ResourceMode::Embedded))
        .expect("init runtime");
    let mut s = rt
        .create_session(SessionConfig::private())
        .expect("create private session");
    let mut s_keep = rt
        .create_session(SessionConfig::private())
        .expect("create keeper session");

    s.expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay");

    let qml = tmp.path().join("ExecDrop.qml");
    std::fs::write(
        &qml,
        r#"import QtQuick
Item {
    Timer {
        interval: 5
        running: true
        repeat: false
        onTriggered: Relay.trigger("drop")
    }
}
"#,
    )
    .expect("write drop qml");
    let keep_qml = tmp.path().join("ExecKeep.qml");
    std::fs::write(&keep_qml, "import QtQuick\nItem { property int ok: 1 }\n")
        .expect("write keep qml");

    s.load(&QmlSource::File(qml)).expect("load qml");
    let keep_root = s_keep
        .load(&QmlSource::File(keep_qml))
        .expect("load keep")
        .root_id;

    let handle = s.handle();
    let keep_handle = s_keep.handle();
    let cell = Rc::new(RefCell::new(Some(s)));
    let target = Rc::clone(&cell);
    let fired = Rc::new(RefCell::new(false));
    let fired_flag = Rc::clone(&fired);
    let _guard = install_callback_handler(move |tag| {
        if tag == "drop" {
            *fired_flag.borrow_mut() = true;
            drop(target.borrow_mut().take());
            keep_handle.schedule_quit(40);
        }
    });

    let code = rt.exec().expect("exec exits cleanly");
    assert_eq!(code, 0);
    assert!(*fired.borrow(), "Timer did not trigger inside exec()");
    assert!(cell.borrow().is_none());
    assert!(matches!(
        handle.current_locale(),
        Err(HostError::HostDisposed)
    ));
    assert!(s_keep.is_root_alive(keep_root));
}

fn worker_event_nested_prop_drop(tmp: &TempDir) {
    let mut rt = RinUIRuntime::new(runtime_cfg(tmp, "EventNestedDrop", ResourceMode::Embedded))
        .expect("init runtime");
    let mut s = rt
        .create_session(SessionConfig::private())
        .expect("create private session");
    let mut s_keep = rt
        .create_session(SessionConfig::private())
        .expect("create keeper session");

    s.expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay");

    let qml = tmp.path().join("ExecNestedDrop.qml");
    std::fs::write(
        &qml,
        r#"import QtQuick
Item {
    property int n: 0
    property int afterCount: 0
    onNChanged: {
        if (n === 1) {
            Relay.trigger("drop")
        }
    }
    Timer {
        interval: 5
        running: true
        repeat: false
        onTriggered: {
            Relay.trigger("nested")
            afterCount = n + 10
            Relay.trigger("after")
        }
    }
}
"#,
    )
    .expect("write nested drop qml");
    let keep_qml = tmp.path().join("ExecNestedKeep.qml");
    std::fs::write(&keep_qml, "import QtQuick\nItem { property int ok: 1 }\n")
        .expect("write keep qml");

    let rid = s.load(&QmlSource::File(qml)).expect("load qml").root_id;
    let keep_root = s_keep
        .load(&QmlSource::File(keep_qml))
        .expect("load keep")
        .root_id;

    let handle = s.handle();
    let keep_handle = s_keep.handle();
    let cell = Rc::new(RefCell::new(Some(s)));
    let target = Rc::clone(&cell);
    let drop_handle = handle.clone();
    let dropped = Rc::new(RefCell::new(false));
    let after_js = Rc::new(RefCell::new(false));
    let dropped_flag = Rc::clone(&dropped);
    let after_flag = Rc::clone(&after_js);

    let _guard = install_callback_handler(move |tag| match tag {
        "nested" => {
            drop_handle
                .set_root_property(rid, "n", &QVariant::from(&1i32))
                .expect("nested set_root_property succeeds");
        }
        "drop" => {
            *dropped_flag.borrow_mut() = true;
            drop(target.borrow_mut().take());
        }
        "after" => {
            *after_flag.borrow_mut() = true;
            keep_handle.schedule_quit(40);
        }
        other => panic!("unexpected tag: {other}"),
    });

    let code = rt.exec().expect("exec exits cleanly");
    assert_eq!(code, 0);
    assert!(*dropped.borrow(), "nested drop callback did not run");
    assert!(
        *after_js.borrow(),
        "JS frame after nested Rust callback did not continue cleanly"
    );
    assert!(cell.borrow().is_none());
    assert!(matches!(
        handle.current_locale(),
        Err(HostError::HostDisposed)
    ));
    assert!(s_keep.is_root_alive(keep_root));
}

fn worker_theme_signal_drop(tmp: &TempDir) {
    let rt = RinUIRuntime::new(runtime_cfg(tmp, "ThemeSignalDrop", ResourceMode::Embedded))
        .expect("init runtime");
    let mut s_drop = rt
        .create_session(SessionConfig::private())
        .expect("create drop session");
    let mut s_keep = rt
        .create_session(SessionConfig::private())
        .expect("create keep session");

    s_drop
        .expose_qobject("Relay", rinui_callback_relay_new())
        .expect("expose Relay");
    let drop_qml = tmp.path().join("ThemeDrop.qml");
    std::fs::write(
        &drop_qml,
        r#"import QtQuick
Item {
    Connections {
        target: ThemeManager
        function onThemeChanged(t) {
            Relay.trigger("theme-drop")
        }
    }
}
"#,
    )
    .expect("write drop qml");
    let keep_qml = tmp.path().join("ThemeKeep.qml");
    std::fs::write(
        &keep_qml,
        r#"import QtQuick
Item {
    property string seen: ThemeManager.get_theme()
    Connections {
        target: ThemeManager
        function onThemeChanged(t) { seen = t }
    }
}
"#,
    )
    .expect("write keep qml");

    s_drop.load(&QmlSource::File(drop_qml)).expect("load drop");
    let keep_root = s_keep
        .load(&QmlSource::File(keep_qml))
        .expect("load keep")
        .root_id;

    let cell = Rc::new(RefCell::new(Some(s_drop)));
    let h_keep = s_keep.handle();
    let ran = Rc::new(RefCell::new(false));
    let ran_flag = Rc::clone(&ran);
    let qml_reenter_blocked = Rc::new(RefCell::new(false));
    let qml_reenter_flag = Rc::clone(&qml_reenter_blocked);
    let drop_target = Rc::clone(&cell);
    let _guard = install_callback_handler(move |tag| {
        if tag == "theme-drop" {
            *ran_flag.borrow_mut() = true;
            // actual_theme() works cleanly during themeChanged, while entering QML/translator
            // APIs must detect active theme_borrow_state == -1 and return ReentrantCall.
            let _ = h_keep.actual_theme().expect("actual_theme inside signal");
            if matches!(
                h_keep.invoke_root_method(keep_root, "metaPing", &[]),
                Err(HostError::ReentrantCall("theme_manager_borrowed"))
            ) {
                *qml_reenter_flag.borrow_mut() = true;
            }
            drop(drop_target.borrow_mut().take());
        }
    });

    rt.with_theme_manager_mut(|tm| tm.toggle_theme(QString::from("Dark")))
        .expect("toggle_theme succeeds");
    assert!(*ran.borrow(), "onThemeChanged callback did not run");
    assert!(
        *qml_reenter_blocked.borrow(),
        "ThemeManager write lock must block nested QML host calls inside onThemeChanged"
    );
    assert!(cell.borrow().is_none());
    assert_eq!(get_string_prop(&s_keep, keep_root, "seen"), "Dark");
}

fn worker_ownership(tmp: &TempDir) {
    let rt = RinUIRuntime::new(runtime_cfg(tmp, "Ownership", ResourceMode::Embedded))
        .expect("init runtime");

    // Part 1: exact minimal repro from review (B creates Window via factory.createObject(null))
    {
        let a = rt
            .create_session(SessionConfig::shared())
            .expect("create shared a");
        let mut b = rt
            .create_session(SessionConfig::shared())
            .expect("create shared b");

        let qml_path = tmp.path().join("OwnershipRepro.qml");
        std::fs::write(
            &qml_path,
            r#"import QtQuick
import QtQuick.Window
Item {
    property var kept
    Component {
        id: factory
        Window { property bool isRinUIWindow: true; visible: false; objectName: "B-dynamic" }
    }
    function spawn() {
        kept = factory.createObject(null)
        return kept !== null
    }
}
"#,
        )
        .expect("write qml");
        let loaded = b.load(&QmlSource::File(qml_path)).expect("load on b");
        let ok = b
            .invoke_root_method(loaded.root_id, "spawn", &[])
            .expect("spawn");
        assert_eq!(ok.value::<bool>(), Some(true));

        let a_claimed = a.handle().sync_windows().expect("a sync_windows");
        assert!(
            a_claimed.is_empty(),
            "session A must NEVER claim dynamic window created by session B (claimed {})",
            a_claimed.len()
        );
        assert_eq!(a.open_window_count(), 0);

        let b_claimed = b.handle().sync_windows().expect("b sync_windows");
        assert_eq!(
            b_claimed.len(),
            1,
            "session B should discover its own dynamically created window"
        );
        let b_win = b_claimed[0];
        assert_eq!(
            b_win
                .property("objectName")
                .value::<QString>()
                .unwrap()
                .to_string(),
            "B-dynamic"
        );

        drop(a);
        assert!(
            b_win.is_alive(),
            "dropping session A must NOT destroy session B's window"
        );
        assert_eq!(b.open_window_count(), 1);
    }

    // Part 2: Truly unattributed dynamic window spawned from a Shared-engine QML singleton
    // has NO identifiable session owner (`session_id == 0`). Neither A nor B may auto-claim it
    // via `sync_windows()`, and B must explicitly claim it via `register_window_from_property()`.
    {
        let mut a = rt
            .create_session(SessionConfig::shared())
            .expect("create shared a2");
        let mut b = rt
            .create_session(SessionConfig::shared())
            .expect("create shared b2");

        let part2_dir = tmp.path().join("ownership_part2");
        std::fs::create_dir_all(&part2_dir).expect("create part2 dir");
        std::fs::write(
            part2_dir.join("qmldir"),
            "singleton SharedWinFactory 1.0 SharedWinFactory.qml\n",
        )
        .expect("write qmldir");
        std::fs::write(
            part2_dir.join("SharedWinFactory.qml"),
            r#"pragma Singleton
import QtQuick
import QtQuick.Window

QtObject {
    id: fac
    property Component comp: Component {
        Window { property bool isRinUIWindow: true; visible: false; objectName: "SingletonShared" }
    }
    function makeWin() {
        return comp.createObject(null)
    }
}
"#,
        )
        .expect("write SharedWinFactory.qml");
        let a_qml = part2_dir.join("AHost.qml");
        std::fs::write(&a_qml, "import QtQuick\nItem {}\n").expect("write AHost");
        let b_qml = part2_dir.join("DetachedDynamic.qml");
        std::fs::write(
            &b_qml,
            r#"import QtQuick
import "."
Item {
    property var detachedWin: null
    function spawnDetached() {
        detachedWin = SharedWinFactory.makeWin()
        return detachedWin !== null
    }
}
"#,
        )
        .expect("write DetachedDynamic");
        let _loaded_a = a.load(&QmlSource::File(a_qml)).expect("load a2");
        let loaded_b = b.load(&QmlSource::File(b_qml)).expect("load b2");
        let spawned = b
            .invoke_root_method(loaded_b.root_id, "spawnDetached", &[])
            .expect("spawnDetached");
        assert_eq!(spawned.value::<bool>(), Some(true));

        assert!(
            a.sync_windows().expect("a2 sync").is_empty(),
            "ambiguous singleton window on shared engine must not be auto-claimed by A"
        );
        assert!(
            b.sync_windows().expect("b2 sync").is_empty(),
            "ambiguous singleton window on shared engine must not be auto-claimed by B without explicit registration"
        );

        assert!(matches!(
            a.register_window_from_property(loaded_b.root_id, "detachedWin"),
            Err(HostError::RootNotOwned(_))
        ));

        let b_reg = b
            .register_window_from_property(loaded_b.root_id, "detachedWin")
            .expect("explicit registration by B succeeds");
        assert_eq!(
            b_reg
                .property("objectName")
                .value::<QString>()
                .unwrap()
                .to_string(),
            "SingletonShared"
        );
        assert_eq!(a.open_window_count(), 0);
        assert_eq!(b.open_window_count(), 1);

        drop(a);
        assert!(b_reg.is_alive());
        b.release_root(loaded_b.root_id).expect("release root on b");
        rt.process_events();
        assert!(
            !b_reg.is_alive(),
            "explicitly registered detached window must be destroyed on release_root"
        );
        drop(b);
    }
}

fn worker_theme_alias(tmp: &TempDir) {
    // Part A: Runtime / Session reader-writer borrow protection
    {
        let rt = RinUIRuntime::new(runtime_cfg(tmp, "ThemeAlias", ResourceMode::Embedded))
            .expect("init runtime");
        let s = rt
            .create_session(SessionConfig::shared())
            .expect("create shared");

        // 1. Overlapping shared reads are allowed; write inside read MUST return ReentrantCall.
        let nested_res = rt.with_theme_manager(|read| {
            let read_again = rt
                .with_theme_manager(|r2| r2.get_theme().to_string())
                .expect("concurrent shared ThemeManager read must succeed");
            assert_eq!(read.get_theme().to_string(), read_again);

            let write_err = rt.with_theme_manager_mut(|mut write| {
                write.as_mut().toggle_theme(QString::from("Dark"));
            });
            assert!(
                matches!(
                    write_err,
                    Err(HostError::ReentrantCall("theme_manager_mut"))
                ),
                "write borrow inside active read borrow must be rejected, got {write_err:?}"
            );

            let sess_write_err = s.with_theme_manager_mut(|mut write| {
                write.as_mut().toggle_theme(QString::from("Dark"));
            });
            assert!(matches!(
                sess_write_err,
                Err(HostError::ReentrantCall("theme_manager_mut"))
            ));

            // Dropping into QML while ThemeManager read guard is held must also be refused.
            assert!(matches!(
                rt.set_language("zh_CN"),
                Err(HostError::ReentrantCall("theme_manager_borrowed"))
            ));
            assert_eq!(read.get_theme().to_string(), read_again);
        });
        nested_res.expect("with_theme_manager returns cleanly");

        // 2. Inside active write borrow, BOTH nested read and nested write MUST fail.
        rt.with_theme_manager_mut(|mut write| {
            let r_err = rt.with_theme_manager(|r| r.get_theme().to_string());
            assert!(
                matches!(r_err, Err(HostError::ReentrantCall("theme_manager"))),
                "read inside active write must fail, got {r_err:?}"
            );
            let w_err = rt.with_theme_manager_mut(|_| ());
            assert!(matches!(
                w_err,
                Err(HostError::ReentrantCall("theme_manager_mut"))
            ));
            write.as_mut().toggle_theme(QString::from("Dark"));
        })
        .expect("with_theme_manager_mut succeeds after reader released");
        {
            let mut mut_guard = rt
                .theme_manager_mut()
                .expect("acquire mut guard from &Runtime after reader dropped");
            assert!(matches!(
                rt.theme_manager(),
                Err(HostError::ReentrantCall("theme_manager"))
            ));
            assert!(matches!(
                rt.theme_manager_mut(),
                Err(HostError::ReentrantCall("theme_manager_mut"))
            ));
            assert!(matches!(
                s.with_theme_manager(|_| ()),
                Err(HostError::ReentrantCall("theme_manager"))
            ));
            mut_guard.toggle_theme(QString::from("Light"));
        }
    }

    // Part B: RinUIHost::theme_manager() / theme_manager_mut() RAII guard enforcement
    {
        let mut host = RinUIHost::new(HostConfig {
            identity: AppIdentity::new("RinUIParityReview", "ThemeAliasHost"),
            resource_mode: ResourceMode::Embedded,
            config_location: ConfigLocation::CustomDir(tmp.path().join("host_theme")),
            locale: Some("en_US".to_owned()),
            mac_window: MacWindowOptions::default(),
            qt_quick_controls_style: "Basic".to_owned(),
        })
        .expect("create host");

        let read_guard = host.theme_manager().expect("acquire read guard");
        let cur_theme = read_guard.get_theme().to_string();
        assert!(matches!(
            host.runtime().with_theme_manager_mut(|_| ()),
            Err(HostError::ReentrantCall("theme_manager_mut"))
        ));
        assert!(matches!(
            host.session().with_theme_manager_mut(|_| ()),
            Err(HostError::ReentrantCall("theme_manager_mut"))
        ));
        assert!(matches!(
            host.with_theme_manager_mut(|_| ()),
            Err(HostError::ReentrantCall("theme_manager_mut"))
        ));
        assert_eq!(read_guard.get_theme().to_string(), cur_theme);
        drop(read_guard);

        // Dropping a Private session while a ThemeManagerRef is held must reject create_session
        // and defer engine teardown cleanly until the ThemeManager guard drops.
        let mut priv_sess = host
            .runtime()
            .create_session(SessionConfig::private())
            .expect("create private session before borrow");
        let priv_qml = tmp.path().join("ThemeBorrowDrop.qml");
        std::fs::write(&priv_qml, "import QtQuick\nItem { property int k: 7 }\n")
            .expect("write ThemeBorrowDrop.qml");
        priv_sess
            .load(&QmlSource::File(priv_qml))
            .expect("load priv_sess");

        let read_guard2 = host.theme_manager().expect("acquire second read guard");
        assert!(matches!(
            host.runtime().create_session(SessionConfig::private()),
            Err(HostError::ReentrantCall("create_session"))
        ));
        let priv_handle = priv_sess.handle();
        drop(priv_sess);
        assert!(matches!(
            priv_handle.current_locale(),
            Err(HostError::HostDisposed)
        ));
        assert_eq!(read_guard2.get_theme().to_string(), cur_theme);
        drop(read_guard2);
        host.process_events();

        {
            let mut mut_guard = host
                .theme_manager_mut()
                .expect("acquire mut guard after reader dropped");
            mut_guard.toggle_theme(QString::from("Dark"));
        }

        assert_eq!(
            host.theme_manager()
                .expect("read after mut dropped")
                .get_theme()
                .to_string(),
            "Dark"
        );
    }
}

fn worker_locale_live(tmp: &TempDir, mode: ResourceMode) {
    let label = match &mode {
        ResourceMode::Embedded => "LocaleLiveEmbedded",
        ResourceMode::Filesystem(_) => "LocaleLiveDev",
        ResourceMode::Auto => "LocaleLiveAuto",
    };
    let rt = RinUIRuntime::new(runtime_cfg(tmp, label, mode)).expect("init runtime");
    let mut s = rt
        .create_session(SessionConfig::private())
        .expect("create private session");

    let qml_path = tmp.path().join("LocaleLive.qml");
    std::fs::write(
        &qml_path,
        r#"import QtQuick
import QtQuick.Controls
import RinUI

Item {
    property string defCalLocale: calDef.locale.name
    property string defCalMonth: calDef.locale.monthName(0)
    property string expCalLocale: calExp.locale.name
    property string expCalMonth: calExp.locale.monthName(0)
    property string impCalLocale: calImp.locale.name
    property string impCalMonth: calImp.locale.monthName(0)

    property string defCdpLocale: cdpDef.calendarLocale.name
    property string defCdpMonth: cdpDef.calendarLocale.monthName(0)
    property string expCdpLocale: cdpExp.calendarLocale.name
    property string expCdpMonth: cdpExp.calendarLocale.monthName(0)

    property string defDpLocale: dpDef.locale.name
    property string defDpMonth0: String(dpDef.monthModel[0])
    property string defDpGetMonth: dpDef.getMonthName(1)
    property string defDpOrder: dpDef.dateOrder.join(",")
    property string expDpLocale: dpExp.locale.name
    property string expDpMonth0: String(dpExp.monthModel[0])
    property string expDpGetMonth: dpExp.getMonthName(1)
    property string expDpOrder: dpExp.dateOrder.join(",")

    property string defSpinLocale: spinDef.locale.name
    property string expSpinLocale: spinExp.locale.name

    property string translatedToday: qsTranslate("Calendar", "Today")

    function overrideImpCalLocale() {
        calImp.locale = Qt.locale("de_DE")
    }

    Calendar { id: calDef; visible: false }
    Calendar { id: calExp; locale: Qt.locale("en_US"); visible: false }
    Calendar { id: calImp; visible: false }

    CalendarDatePicker { id: cdpDef; visible: false }
    CalendarDatePicker { id: cdpExp; calendarLocale: Qt.locale("en_US"); visible: false }

    DatePicker { id: dpDef; visible: false }
    DatePicker { id: dpExp; locale: Qt.locale("en_US"); visible: false }

    SpinBox { id: spinDef; visible: false }
    SpinBox { id: spinExp; locale: Qt.locale("en_US"); visible: false }
}
"#,
    )
    .expect("write LocaleLive.qml");

    let rid = s
        .load(&QmlSource::File(qml_path))
        .expect("load qml")
        .root_id;
    rt.process_events();

    assert_eq!(get_string_prop(&s, rid, "defCalLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "defCalMonth"), "January");
    assert_eq!(get_string_prop(&s, rid, "defCdpLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "defDpLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "defDpMonth0"), "January");
    assert_eq!(get_string_prop(&s, rid, "defDpOrder"), "month,day,year");
    assert_eq!(get_string_prop(&s, rid, "expDpOrder"), "month,day,year");
    assert_eq!(get_string_prop(&s, rid, "defSpinLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "translatedToday"), "Today");

    s.invoke_root_method(rid, "overrideImpCalLocale", &[])
        .expect("override imperative locale");
    assert_eq!(get_string_prop(&s, rid, "impCalLocale"), "de_DE");
    assert_eq!(get_string_prop(&s, rid, "impCalMonth"), "Januar");

    // Switch live to zh_CN on existing controls!
    let zh = rt.set_language("zh_CN").expect("switch to zh_CN");
    assert_eq!(zh.active_locale, "zh_CN");
    rt.process_events();

    assert_eq!(get_string_prop(&s, rid, "defCalLocale"), "zh_CN");
    assert_eq!(get_string_prop(&s, rid, "defCalMonth"), "一月");
    assert_eq!(get_string_prop(&s, rid, "defCdpLocale"), "zh_CN");
    assert_eq!(get_string_prop(&s, rid, "defCdpMonth"), "一月");
    assert_eq!(get_string_prop(&s, rid, "defDpLocale"), "zh_CN");
    assert_eq!(get_string_prop(&s, rid, "defDpMonth0"), "一月");
    assert_eq!(get_string_prop(&s, rid, "defDpGetMonth"), "一月");
    assert_eq!(get_string_prop(&s, rid, "defDpOrder"), "year,month,day");
    assert_eq!(get_string_prop(&s, rid, "defSpinLocale"), "zh_CN");
    assert_eq!(get_string_prop(&s, rid, "translatedToday"), "今天");

    // Explicit declarative & imperative overrides must remain untouched!
    assert_eq!(get_string_prop(&s, rid, "expCalLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "expCalMonth"), "January");
    assert_eq!(get_string_prop(&s, rid, "expCdpLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "expCdpMonth"), "January");
    assert_eq!(get_string_prop(&s, rid, "expDpLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "expDpMonth0"), "January");
    assert_eq!(get_string_prop(&s, rid, "expDpGetMonth"), "January");
    assert_eq!(get_string_prop(&s, rid, "expDpOrder"), "month,day,year");
    assert_eq!(get_string_prop(&s, rid, "expSpinLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "impCalLocale"), "de_DE");
    assert_eq!(get_string_prop(&s, rid, "impCalMonth"), "Januar");

    // Switch live again to fr_FR (app locale active even though RinUI falls back to en_US.qm).
    let fr = rt.set_language("fr_FR").expect("switch to fr_FR");
    assert_eq!(fr.active_locale, "fr_FR");
    rt.process_events();

    assert_eq!(get_string_prop(&s, rid, "defCalLocale"), "fr_FR");
    assert_eq!(get_string_prop(&s, rid, "defCalMonth"), "janvier");
    assert_eq!(get_string_prop(&s, rid, "defCdpLocale"), "fr_FR");
    assert_eq!(get_string_prop(&s, rid, "defCdpMonth"), "janvier");
    assert_eq!(get_string_prop(&s, rid, "defDpLocale"), "fr_FR");
    assert_eq!(get_string_prop(&s, rid, "defDpMonth0"), "janvier");
    assert_eq!(get_string_prop(&s, rid, "defDpGetMonth"), "janvier");
    assert_eq!(get_string_prop(&s, rid, "defDpOrder"), "day,month,year");
    assert_eq!(get_string_prop(&s, rid, "defSpinLocale"), "fr_FR");
    assert_eq!(get_string_prop(&s, rid, "expCalLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "expCdpLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "expDpLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "expDpOrder"), "month,day,year");
    assert_eq!(get_string_prop(&s, rid, "expSpinLocale"), "en_US");
    assert_eq!(get_string_prop(&s, rid, "impCalLocale"), "de_DE");
    assert_eq!(get_string_prop(&s, rid, "impCalMonth"), "Januar");
}

fn copy_resource_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create resource copy directory");
    for entry in std::fs::read_dir(src).expect("read resource tree") {
        let entry = entry.expect("resource entry");
        let dest = dst.join(entry.file_name());
        if entry.file_type().expect("resource file type").is_dir() {
            copy_resource_tree(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).expect("copy resource file");
        }
    }
}

#[derive(Default)]
struct ExternalFlags {
    after: Cell<bool>,
    after_prop: Cell<bool>,
    dropped: Cell<bool>,
    sibling_ok: Cell<bool>,
    disposed: Cell<bool>,
    pending_resources: Cell<bool>,
    attach_blocked: Cell<bool>,
    nested_pump_safe: Cell<bool>,
}

fn pump_external(app: &QGuiApplication, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() && start.elapsed() < Duration::from_secs(5) {
        app.process_events();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(done(), "external event processing did not complete");
}

fn worker_external(tmp: &TempDir, mode: &str) {
    let upstream = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../upstream/Rin-UI/RinUI");
    let source = tmp.path().join("upstream-copy");
    copy_resource_tree(&upstream, &source.join("RinUI"));
    let cfg = runtime_cfg(tmp, mode, ResourceMode::Filesystem(source));
    if mode.starts_with("external-process-nested-app-pump")
        || matches!(
            mode,
            "external-process-safe"
                | "external-process-nested-prop-drop"
                | "external-process-nested-pump"
                | "external-process-nested-pump-session"
        )
    {
        // The relay helper requires a 'static closure. Keep the application's owner in a
        // Box until all callback captures, sessions, runtimes and handles have been dropped.
        let owner = Box::into_raw(Box::new(QGuiApplication::new()));
        let app: &'static QGuiApplication = unsafe { (*owner).as_ref().unwrap() };
        let rt = RinUIRuntime::attach_existing(app, cfg.clone()).expect("safe external attach");
        let roots = worker_external_body(tmp, mode, rt, Some(app), |flags, roots| {
            pump_external(app, || flags.after.get());
            if (mode.starts_with("external-process-nested-pump")
                || mode.starts_with("external-process-nested-app-pump"))
                && !mode.contains("last")
            {
                // Reclaim the dropped Private engine while its Runtime and sibling
                // are still alive: final Runtime teardown must not mask a stuck drain.
                pump_external(app, || {
                    // The nested external pump may leave DeferredDeletes pending.
                    // Dispatch them at the outer boundary, after the QML handler returned.
                    // SAFETY: the external app call returned and the outer JS
                    // continuation ran; this is the top-level Rust driver.
                    unsafe { rinui_test_dispatch_deferred_deletes() };
                    !roots[0].exists()
                });
                assert!(
                    roots[1].exists(),
                    "sibling engine was reclaimed by the drain"
                );
            }
        });
        // These releases happened outside exec()/Qt notification delivery. A manual
        // Qt driver explicitly dispatches DeferredDeletes after callbacks return.
        pump_external(app, || {
            // SAFETY: worker_external_body returned, including its outer Qt call.
            unsafe { rinui_test_dispatch_deferred_deletes() };
            roots.iter().all(|root| !root.exists())
        });
        let rt = RinUIRuntime::attach_existing(app, cfg).expect("reattach after external drain");
        drop(rt);
        // SAFETY: worker_external_body dropped the relay guard and all app-borrowing values;
        // the second runtime is also gone. No callback can access this application anymore.
        unsafe { drop(Box::from_raw(owner)) };
    } else {
        let mut app = QGuiApplication::new();
        // SAFETY: this application outlives every runtime/session/handle in the body.
        let rt = unsafe {
            RinUIRuntime::attach_existing_raw(rinui_host::active_qt_application_ptr(), cfg.clone())
        }
        .expect("raw external attach");
        let roots = worker_external_body(tmp, mode, rt, None, |flags, _roots| {
            if mode.starts_with("external-exec-") {
                assert_eq!(app.as_mut().unwrap().exec(), 0);
            } else {
                pump_external(app.as_ref().unwrap(), || flags.after.get());
            }
        });
        if mode == "external-process-last-app-drop" {
            assert!(
                roots.iter().all(|root| root.exists()),
                "post routine fixture drained early"
            );
            // Do not process any more events: qAddPostRoutine must finish the pending teardown.
            drop(app);
            assert!(
                roots.iter().all(|root| !root.exists()),
                "post routine leaked resource views"
            );
            let app = QGuiApplication::new();
            let rt = RinUIRuntime::attach_existing(app.as_ref().unwrap(), cfg)
                .expect("runtime reservation released by post routine");
            drop(rt);
            drop(app);
        } else {
            if mode.starts_with("external-exec-") {
                assert!(
                    roots.iter().all(|root| !root.exists()),
                    "exec/aboutToQuit failed to drain"
                );
            } else {
                pump_external(app.as_ref().unwrap(), || {
                    roots.iter().all(|root| !root.exists())
                });
            }
            let rt = RinUIRuntime::attach_existing(app.as_ref().unwrap(), cfg)
                .expect("reattach after external drain");
            drop(rt);
            drop(app);
        }
    }
}

fn worker_external_body(
    tmp: &TempDir,
    mode: &str,
    rt: RinUIRuntime<'static>,
    external_app: Option<&'static QGuiApplication>,
    drive: impl FnOnce(&ExternalFlags, &[PathBuf]),
) -> Vec<PathBuf> {
    let shared = mode.contains("shared");
    let last = mode.contains("last");
    let reversed = mode.contains("reversed");
    let nested = mode.contains("nested-prop");
    let exec = mode.starts_with("external-exec-");
    let pump_runtime = mode == "external-process-nested-pump";
    let pump_session = mode == "external-process-nested-pump-session";
    let pump_app = mode.starts_with("external-process-nested-app-pump");
    let config = if shared {
        SessionConfig::shared()
    } else {
        SessionConfig::private()
    };
    let mut keep = rt
        .create_session(config.clone())
        .expect("external sibling session");
    let mut target = rt.create_session(config).expect("external target session");
    target
        .expose_qobject("Relay", rinui_callback_relay_new())
        .expect("external Relay");
    let keep_qml = tmp.path().join("ExternalKeep.qml");
    std::fs::write(
        &keep_qml,
        "import QtQuick\nItem { function ping() { return 42 } }\n",
    )
    .expect("write external sibling");
    let qml = tmp.path().join("ExternalDrop.qml");
    std::fs::write(
        &qml,
        format!(
            r#"import QtQuick
import QtQuick.Window
Window {{
    visible: false
    property bool isRinUIWindow: true
    property int n: 0
    onNChanged: {{
        if (n === 1) {{ Relay.trigger("drop"); Relay.trigger("after-prop") }}
    }}
    Timer {{
        interval: 5; running: true; repeat: false
        onTriggered: {{ Relay.trigger("{}"); Relay.trigger("after") }}
    }}
}}
"#,
            if nested { "nested" } else { "drop" }
        ),
    )
    .expect("write external Timer QML");
    // Materialize both files before Qt caches their directory in a shared engine.
    let keep_root = keep
        .load(&QmlSource::File(keep_qml))
        .expect("load external sibling")
        .root_id;
    let root = target
        .load(&QmlSource::File(qml))
        .expect("load external Timer")
        .root_id;
    let roots = vec![
        target
            .resource_view()
            .unwrap()
            .load_root()
            .unwrap()
            .to_path_buf(),
        keep.resource_view()
            .unwrap()
            .load_root()
            .unwrap()
            .to_path_buf(),
    ];
    let handle = target.handle();
    let callback_handle = handle.clone();
    let sibling = keep.handle();
    let sessions = Rc::new(RefCell::new(Some(target)));
    let siblings = Rc::new(RefCell::new(Some(keep)));
    let runtimes = Rc::new(RefCell::new(Some(rt)));
    let flags = Rc::new(ExternalFlags::default());
    let cb_flags = Rc::clone(&flags);
    let cb_sessions = Rc::clone(&sessions);
    let cb_siblings = Rc::clone(&siblings);
    let cb_runtimes = Rc::clone(&runtimes);
    let cb_roots = roots.clone();
    let guard = install_callback_handler(move |tag| match tag {
        "nested" => callback_handle
            .set_root_property(root, "n", &QVariant::from(&1i32))
            .expect("nested property drop returns safely"),
        "drop" => {
            if exec {
                // Queue quit ahead of the drain for the reversed worker: aboutToQuit
                // must perform the cleanup even if the queued drain hasn't run yet.
                callback_handle.schedule_quit(if reversed { 0 } else { 30 });
            }
            if reversed {
                drop(cb_runtimes.borrow_mut().take());
            }
            drop(cb_sessions.borrow_mut().take());
            cb_flags.disposed.set(matches!(
                callback_handle.current_locale(),
                Err(HostError::HostDisposed)
            ));
            cb_flags.sibling_ok.set(
                sibling
                    .invoke_root_method(keep_root, "ping", &[])
                    .ok()
                    .and_then(|v| v.value::<i32>())
                    == Some(42),
            );
            if last {
                drop(cb_siblings.borrow_mut().take());
                drop(cb_runtimes.borrow_mut().take());
            }
            if pump_runtime || pump_session || pump_app {
                // Still on the Timer's outer QML stack, entered via external
                // app.process_events(). No outer host guard or exec() loop exists.
                for _ in 0..3 {
                    if pump_runtime {
                        cb_runtimes.borrow().as_ref().unwrap().process_events();
                    } else if pump_app {
                        external_app.unwrap().process_events();
                    } else {
                        cb_siblings.borrow().as_ref().unwrap().process_events();
                    }
                    assert!(
                        cb_roots.iter().all(|root| root.exists()),
                        "nested event processing destroyed an engine on the QML stack"
                    );
                }
                cb_flags.nested_pump_safe.set(true);
            }
            cb_flags
                .pending_resources
                .set(cb_roots.iter().all(|root| root.exists()));
            // Reattachment must be refused while either the runtime or its pending
            // teardown still owns RUNTIME_ACTIVE. The failing call creates no engine.
            let attach = unsafe {
                RinUIRuntime::attach_existing_raw(
                    rinui_host::active_qt_application_ptr(),
                    RuntimeConfig {
                        resource_mode: ResourceMode::Embedded,
                        config_location: ConfigLocation::InMemory,
                        ..RuntimeConfig::default()
                    },
                )
            };
            cb_flags.attach_blocked.set(matches!(
                attach,
                Err(HostError::ApplicationAlreadyInitialized)
            ));
            cb_flags.dropped.set(true);
        }
        "after-prop" => cb_flags.after_prop.set(true),
        "after" => cb_flags.after.set(true),
        other => panic!("unexpected external callback tag: {other}"),
    });
    drive(&flags, &roots);
    assert!(flags.after.get(), "JS did not resume after drop");
    assert!(flags.dropped.get(), "drop callback did not finish");
    assert!(
        flags.disposed.get(),
        "handle did not immediately report HostDisposed"
    );
    assert!(
        flags.sibling_ok.get(),
        "sibling was not callable after self-drop"
    );
    assert!(
        flags.pending_resources.get(),
        "engine/resources destroyed on the QML stack"
    );
    assert!(
        flags.attach_blocked.get(),
        "runtime reservation released before draining"
    );
    if nested {
        assert!(flags.after_prop.get(), "nested property JS did not resume");
    }
    if pump_runtime || pump_session || pump_app {
        assert!(
            flags.nested_pump_safe.get(),
            "nested event processing did not return safely"
        );
    }
    assert!(sessions.borrow().is_none());
    assert!(matches!(
        handle.current_locale(),
        Err(HostError::HostDisposed)
    ));
    if !last {
        let keep = siblings.borrow();
        assert_eq!(
            keep.as_ref()
                .unwrap()
                .invoke_root_method(keep_root, "ping", &[])
                .unwrap()
                .value::<i32>(),
            Some(42)
        );
    } else {
        assert!(siblings.borrow().is_none());
        assert!(runtimes.borrow().is_none());
    }
    drop(guard);
    drop(siblings.borrow_mut().take());
    drop(runtimes.borrow_mut().take());
    roots
}

fn run_subprocess_worker(mode: &str, timeout: Duration) {
    let exe = std::env::current_exe().expect("resolve current test executable");
    let mut child = Command::new(exe)
        .arg("--worker")
        .arg(mode)
        .env("QT_QPA_PLATFORM", "offscreen")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed spawning worker '{mode}': {e}"));

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll worker status") {
            let output = child
                .wait_with_output()
                .expect("collect finished worker output");
            assert!(
                status.success(),
                "worker '{mode}' failed with status {status}:\n--- stdout ---\n{}\n--- stderr ---\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            println!("parity_review_regressions worker '{mode}': PASSED");
            return;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let output = child.wait_with_output().ok();
            let (out_s, err_s) = output
                .map(|o| {
                    (
                        String::from_utf8_lossy(&o.stdout).into_owned(),
                        String::from_utf8_lossy(&o.stderr).into_owned(),
                    )
                })
                .unwrap_or_default();
            panic!(
                "worker '{mode}' timed out after {:?} (killed):\n--- stdout ---\n{out_s}\n--- stderr ---\n{err_s}",
                timeout
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--worker") {
        let mode = args.get(2).expect("missing worker mode");
        run_worker(mode);
        return;
    }

    for mode in WORKER_MODES {
        run_subprocess_worker(mode, Duration::from_secs(20));
    }
    println!(
        "parity_review_regressions: all {} isolated subprocess tests passed.",
        WORKER_MODES.len()
    );
}
