#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qguiapplication.h");
        type QGuiApplication = cxx_qt_lib::QGuiApplication;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qqmlapplicationengine.h");
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

        include!(<QtCore/QObject>);
        type QObject = cxx_qt::QObject;

        include!("rinui-host/rinui_qt_bridge.h");

        pub fn rinui_set_high_dpi_pass_through();
        pub fn rinui_has_gui_application() -> bool;
        pub fn rinui_active_qt_application_qobject() -> *mut QObject;
        pub fn rinui_validate_existing_gui_application_ref(app: &QGuiApplication) -> i32;
        pub unsafe fn rinui_validate_existing_qobject_ptr(ptr: *mut QObject) -> i32;
        pub fn rinui_is_on_app_thread() -> bool;
        pub fn rinui_app_process_events();
        pub fn rinui_flush_deferred_deletes();
        pub fn rinui_qml_exec_enter();
        pub fn rinui_qml_exec_leave();
        pub fn rinui_in_event_loop() -> bool;
        pub fn rinui_has_live_gui_application() -> bool;
        pub fn rinui_runtime_release_windows_deferred(runtime_id: u64);
        pub fn rinui_set_deferred_drain_callback(cb: fn(bool));
        pub fn rinui_schedule_deferred_drain();
        pub fn rinui_defer_engine_drop(engine_id: u64);
        pub fn rinui_engine_drop_ready(engine_id: u64) -> bool;
        pub fn rinui_engine_discard_latest_unregistered_root(
            engine: Pin<&mut QQmlApplicationEngine>,
        );
        pub fn rinui_configure_quick_controls_style(
            requested: &QString,
            is_borrowed: bool,
            out_current: &mut QString,
        ) -> i32;
        pub fn rinui_capture_borrowed_app_state();
        pub fn rinui_restore_borrowed_app_state(restore_identity: bool);
        pub fn rinui_apply_active_app_identity(
            org_name: &QString,
            org_domain: &QString,
            app_name: &QString,
            app_version: &QString,
        );
        pub fn rinui_standard_app_local_data_dir() -> QString;
        pub fn rinui_current_system_theme() -> QString;
        pub fn rinui_platform_name() -> QString;
        pub fn rinui_system_locale_name() -> QString;
        pub fn rinui_default_locale_name() -> QString;
        pub fn rinui_canonicalize_locale_name(raw_tag: &QString) -> QString;

        pub fn rinui_platform_register_engine_objects(engine: Pin<&mut QQmlApplicationEngine>);

        pub unsafe fn rinui_engine_set_context_property_object(
            engine: Pin<&mut QQmlApplicationEngine>,
            name: &QString,
            object: *mut QObject,
        );

        pub fn rinui_engine_set_context_property_variant(
            engine: Pin<&mut QQmlApplicationEngine>,
            name: &QString,
            value: &QVariant,
        );

        pub fn rinui_engine_install_warning_collector(engine: Pin<&mut QQmlApplicationEngine>);
        pub fn rinui_engine_warnings(engine: &QQmlApplicationEngine) -> QStringList;
        pub fn rinui_engine_clear_warnings(engine: &QQmlApplicationEngine);
        pub fn rinui_engine_uninstall_warning_collector(engine: &QQmlApplicationEngine);

        pub fn rinui_write_qml_failure_log(
            qml_source: &QString,
            rinui_origin: &QString,
            warnings: &QStringList,
            message: &QString,
        );

        pub fn rinui_engine_root_count(engine: &QQmlApplicationEngine) -> usize;

        pub fn rinui_engine_register(
            engine: Pin<&mut QQmlApplicationEngine>,
            runtime_id: u64,
            engine_id: u64,
        );
        pub fn rinui_engine_unregister(engine_id: u64);

        pub fn rinui_engine_register_latest_root_owned(
            engine: Pin<&mut QQmlApplicationEngine>,
            runtime_id: u64,
            engine_id: u64,
            session_id: u64,
            native_mac_frame: bool,
            mac_offset_x: i32,
            mac_offset_down: i32,
        ) -> u64;

        pub fn rinui_root_windows_owned(session_id: u64, root_id: u64) -> Vec<u64>;
        #[allow(clippy::too_many_arguments)]
        pub fn rinui_sync_session_windows(
            engine: Pin<&mut QQmlApplicationEngine>,
            runtime_id: u64,
            engine_id: u64,
            session_id: u64,
            is_private_engine: bool,
            native_mac_frame: bool,
            mac_offset_x: i32,
            mac_offset_down: i32,
        ) -> Vec<u64>;
        #[allow(clippy::too_many_arguments)]
        pub fn rinui_register_window_from_root_property(
            engine: Pin<&mut QQmlApplicationEngine>,
            runtime_id: u64,
            engine_id: u64,
            session_id: u64,
            root_id: u64,
            property_name: &QString,
            native_mac_frame: bool,
            mac_offset_x: i32,
            mac_offset_down: i32,
            out_window_id: &mut u64,
            out_status: &mut i32,
            out_detail: &mut QString,
        ) -> bool;
        pub fn rinui_session_alive_windows(session_id: u64) -> Vec<u64>;
        pub fn rinui_runtime_alive_windows(runtime_id: u64) -> Vec<u64>;
        pub fn rinui_all_alive_windows() -> Vec<u64>;

        pub fn rinui_root_release_owned(session_id: u64, root_id: u64) -> bool;
        pub fn rinui_session_release_all(session_id: u64);
        pub fn rinui_runtime_release_windows_and_filters(runtime_id: u64);
        pub fn rinui_runtime_teardown_services(runtime_id: u64);
        pub fn rinui_root_is_alive_owned(session_id: u64, root_id: u64) -> bool;
        pub fn rinui_root_get_property_owned(
            session_id: u64,
            root_id: u64,
            name: &QString,
        ) -> QVariant;
        pub fn rinui_root_set_property_owned(
            session_id: u64,
            root_id: u64,
            name: &QString,
            value: &QVariant,
        ) -> bool;
        pub fn rinui_root_get_property_checked(
            session_id: u64,
            root_id: u64,
            name: &QString,
            out_status: &mut i32,
            out_detail: &mut QString,
        ) -> QVariant;
        pub fn rinui_root_set_property_checked(
            session_id: u64,
            root_id: u64,
            name: &QString,
            value: &QVariant,
            out_detail: &mut QString,
        ) -> i32;
        pub fn rinui_root_invoke_method(
            session_id: u64,
            root_id: u64,
            method_name: &QString,
            args: &[QVariant],
            out_status: &mut i32,
            out_detail: &mut QString,
        ) -> QVariant;

        pub fn rinui_window_is_alive_owned(session_id: u64, window_id: u64) -> bool;
        pub fn rinui_window_win_id_owned(session_id: u64, window_id: u64) -> usize;
        pub fn rinui_window_get_property_owned(
            session_id: u64,
            window_id: u64,
            name: &QString,
        ) -> QVariant;
        pub fn rinui_window_set_property_owned(
            session_id: u64,
            window_id: u64,
            name: &QString,
            value: &QVariant,
        ) -> bool;
        pub fn rinui_window_show_owned(session_id: u64, window_id: u64);
        pub fn rinui_window_close_owned(session_id: u64, window_id: u64);

        pub fn rinui_set_app_icon_owned(
            icon_path: &QString,
            session_id: u64,
            root_id: u64,
            set_app_icon: bool,
        );

        pub fn rinui_engine_retranslate(engine: Pin<&mut QQmlApplicationEngine>);
        pub fn rinui_engine_set_extra_file_selectors(
            engine: Pin<&mut QQmlApplicationEngine>,
            selectors: &QStringList,
        );
        pub fn rinui_engine_clear_component_cache(engine: Pin<&mut QQmlApplicationEngine>);
        pub fn rinui_engine_collect_garbage(engine: Pin<&mut QQmlApplicationEngine>);

        pub fn rinui_translator_apply_transaction(
            active_locale: &QString,
            rinui_qm_path: &QString,
            app_catalog_ids: &QStringList,
            app_qm_paths: &QStringList,
            out_failed_path: &mut QString,
        ) -> bool;
        pub fn rinui_translator_clear_all();

        pub fn rinui_platform_apply_theme(actual_theme: &QString, configured_theme: &QString);
        pub fn rinui_platform_apply_backdrop(
            effect_type: &QString,
            is_dark: bool,
            win10_light: u32,
            win10_dark: u32,
        ) -> i32;

        pub fn rinui_qt_file_exists(path: &QString) -> bool;
        pub fn rinui_set_clipboard_text(text: &QString);
        pub fn rinui_schedule_quit_after_ms(delay_ms: i32);
    }
}
