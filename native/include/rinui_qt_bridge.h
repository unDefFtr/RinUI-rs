#pragma once

#include <cstddef>
#include <cstdint>
#include <memory>

#include "rust/cxx.h"
#include "cxx-qt-lib/qobject.h"
#include "cxx-qt-lib/qqmlapplicationengine.h"
#include "cxx-qt-lib/qstring.h"
#include "cxx-qt-lib/qstringlist.h"
#include "cxx-qt-lib/qvariant.h"

/// Configure Qt 6 PassThrough HiDPI scaling policy before QGuiApplication creation.
void rinui_set_high_dpi_pass_through();

/// Return true if a QCoreApplication/QGuiApplication instance currently exists.
bool rinui_has_gui_application();

/// Return true if there is no application or the caller is on the Qt application main thread.
bool rinui_is_on_app_thread();

/// Flush pending QEvent::DeferredDelete events on the application thread.
void rinui_flush_deferred_deletes();

/// Configure Qt Quick Controls style (must be called before engine loads QML).
void rinui_set_quick_controls_style(const QString& style);

/// Query Qt's QStandardPaths::AppLocalDataLocation.
QString rinui_standard_app_local_data_dir();

/// Query Qt 6 styleHints()->colorScheme() ("Dark" or "Light").
QString rinui_current_system_theme();

/// Query QGuiApplication::platformName().
QString rinui_platform_name();

/// Query QLocale::system().name().
QString rinui_system_locale_name();

/// Register platform-specific QML context objects on the engine.
void rinui_platform_register_engine_objects(QQmlApplicationEngine& engine);

/// Set a QObject* context property on engine->rootContext().
/// Safety: `object` must remain alive on the GUI thread at least as long as `engine`.
void rinui_engine_set_context_property_object(
    QQmlApplicationEngine& engine,
    const QString& name,
    QObject* object);

/// Set a QVariant context property on engine->rootContext().
void rinui_engine_set_context_property_variant(
    QQmlApplicationEngine& engine,
    const QString& name,
    const QVariant& value);

/// Connect QQmlEngine::warnings so QML diagnostics are captured per engine.
void rinui_engine_install_warning_collector(QQmlApplicationEngine& engine);

/// Retrieve captured QML warnings for an engine.
QStringList rinui_engine_warnings(const QQmlApplicationEngine& engine);

/// Clear captured QML warnings for an engine.
void rinui_engine_clear_warnings(const QQmlApplicationEngine& engine);

/// Remove warning tracking entry for an engine prior to teardown.
void rinui_engine_uninstall_warning_collector(const QQmlApplicationEngine& engine);

/// Write QML load failure diagnostics to <AppLocalDataLocation>/logs/qml-runtime.log.
void rinui_write_qml_failure_log(
    const QString& qml_source,
    const QString& rinui_origin,
    const QStringList& warnings,
    const QString& message);

/// Return current number of root objects owned by the engine.
std::size_t rinui_engine_root_count(const QQmlApplicationEngine& engine);

/// Register the latest root object created by `engine` and discover its RinUI windows.
/// Returns 0 if no root object was added or index exceeds expectations.
std::uint64_t rinui_engine_register_latest_root(
    QQmlApplicationEngine& engine,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down);

/// Return tracked RinUI window handle IDs belonging to `root_id`.
rust::Vec<std::uint64_t> rinui_root_windows(std::uint64_t root_id);

/// Discover any top-level RinUI windows not yet registered (e.g. dynamically created QML windows).
rust::Vec<std::uint64_t> rinui_sync_top_level_windows(
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down);

/// Return all currently alive RinUI window IDs.
rust::Vec<std::uint64_t> rinui_all_alive_windows();

/// Release a loaded root object tree, detach native hooks, and schedule QML object deletion.
void rinui_root_release(std::uint64_t root_id);

/// Release all tracked roots and platform hooks (called during host Drop before engine teardown).
void rinui_release_all_roots_and_platform();

/// Check whether a root handle still points to a live QObject.
bool rinui_root_is_alive(std::uint64_t root_id);

/// Read a dynamic or Q_PROPERTY property from a root QObject.
QVariant rinui_root_get_property(std::uint64_t root_id, const QString& name);

/// Write a property on a root QObject.
bool rinui_root_set_property(std::uint64_t root_id, const QString& name, const QVariant& value);

/// Check whether a window handle still points to a live QQuickWindow.
bool rinui_window_is_alive(std::uint64_t window_id);

/// Query native winId() for a window (0 if invalid or unavailable).
std::size_t rinui_window_win_id(std::uint64_t window_id);

/// Read a property from a tracked QQuickWindow.
QVariant rinui_window_get_property(std::uint64_t window_id, const QString& name);

/// Write a property on a tracked QQuickWindow.
bool rinui_window_set_property(std::uint64_t window_id, const QString& name, const QVariant& value);

/// Show a tracked QQuickWindow.
void rinui_window_show(std::uint64_t window_id);

/// Close a tracked QQuickWindow.
void rinui_window_close(std::uint64_t window_id);

/// Set application icon and optional root window "icon" QUrl property.
void rinui_set_app_icon(const QString& icon_path, std::uint64_t root_id);

/// Trigger QQmlEngine::retranslate().
void rinui_engine_retranslate(QQmlApplicationEngine& engine);

/// Clear engine component cache (only safe when dependent QML object trees are destroyed).
void rinui_engine_clear_component_cache(QQmlApplicationEngine& engine);

/// Run QML JS garbage collector.
void rinui_engine_collect_garbage(QQmlApplicationEngine& engine);

/// Load or replace the RinUI core QTranslator and set QLocale::setDefault().
bool rinui_translator_load_rinui(const QString& qm_path, const QString& locale_name);

/// Load or replace an application translation catalog identified by `catalog_id`.
bool rinui_translator_load_app(const QString& catalog_id, const QString& qm_path);

/// Unload and destroy a single application translation catalog if present.
void rinui_translator_unload_app(const QString& catalog_id);

/// Remove and destroy all active translators before application shutdown.
void rinui_translator_clear_all();

/// Notify native platform layers of theme update.
void rinui_platform_apply_theme(const QString& actual_theme, const QString& configured_theme);

/// Apply platform backdrop effect across registered windows.
std::int32_t rinui_platform_apply_backdrop(
    const QString& effect_type,
    bool is_dark,
    std::uint32_t win10_light,
    std::uint32_t win10_dark);

/// Check if a file exists in Qt resources (":/...") or filesystem.
bool rinui_qt_file_exists(const QString& path);

/// Copy text to the system clipboard via QGuiApplication::clipboard().
void rinui_set_clipboard_text(const QString& text);

/// Schedule QCoreApplication::quit() after `delay_ms` milliseconds.
void rinui_schedule_quit_after_ms(std::int32_t delay_ms);
