#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
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
        pub fn rinui_is_on_app_thread() -> bool;
        pub fn rinui_flush_deferred_deletes();
        pub fn rinui_set_quick_controls_style(style: &QString);
        pub fn rinui_standard_app_local_data_dir() -> QString;
        pub fn rinui_current_system_theme() -> QString;
        pub fn rinui_platform_name() -> QString;
        pub fn rinui_system_locale_name() -> QString;

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

        pub fn rinui_engine_register_latest_root(
            engine: Pin<&mut QQmlApplicationEngine>,
            native_mac_frame: bool,
            mac_offset_x: i32,
            mac_offset_down: i32,
        ) -> u64;

        pub fn rinui_root_windows(root_id: u64) -> Vec<u64>;
        pub fn rinui_sync_top_level_windows(
            native_mac_frame: bool,
            mac_offset_x: i32,
            mac_offset_down: i32,
        ) -> Vec<u64>;
        pub fn rinui_all_alive_windows() -> Vec<u64>;

        pub fn rinui_root_release(root_id: u64);
        pub fn rinui_release_all_roots_and_platform();
        pub fn rinui_root_is_alive(root_id: u64) -> bool;
        pub fn rinui_root_get_property(root_id: u64, name: &QString) -> QVariant;
        pub fn rinui_root_set_property(root_id: u64, name: &QString, value: &QVariant) -> bool;

        pub fn rinui_window_is_alive(window_id: u64) -> bool;
        pub fn rinui_window_win_id(window_id: u64) -> usize;
        pub fn rinui_window_get_property(window_id: u64, name: &QString) -> QVariant;
        pub fn rinui_window_set_property(window_id: u64, name: &QString, value: &QVariant) -> bool;
        pub fn rinui_window_show(window_id: u64);
        pub fn rinui_window_close(window_id: u64);

        pub fn rinui_set_app_icon(icon_path: &QString, root_id: u64);

        pub fn rinui_engine_retranslate(engine: Pin<&mut QQmlApplicationEngine>);
        pub fn rinui_engine_clear_component_cache(engine: Pin<&mut QQmlApplicationEngine>);
        pub fn rinui_engine_collect_garbage(engine: Pin<&mut QQmlApplicationEngine>);

        pub fn rinui_translator_load_rinui(qm_path: &QString, locale_name: &QString) -> bool;
        pub fn rinui_translator_load_app(catalog_id: &QString, qm_path: &QString) -> bool;
        pub fn rinui_translator_unload_app(catalog_id: &QString);
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
