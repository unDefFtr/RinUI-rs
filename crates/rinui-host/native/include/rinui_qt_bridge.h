#pragma once

#include <cstddef>
#include <cstdint>
#include <memory>

#include "rust/cxx.h"
#include "cxx-qt-lib/qguiapplication.h"
#include "cxx-qt-lib/qobject.h"
#include "cxx-qt-lib/qqmlapplicationengine.h"
#include "cxx-qt-lib/qstring.h"
#include "cxx-qt-lib/qstringlist.h"
#include "cxx-qt-lib/qvariant.h"

/// Configure Qt 6 PassThrough HiDPI scaling policy before QGuiApplication creation.
void rinui_set_high_dpi_pass_through();

/// Return true if a QCoreApplication/QGuiApplication instance currently exists.
bool rinui_has_gui_application();

/// Return `static_cast<QObject*>(QCoreApplication::instance())` if an app exists (else nullptr).
QObject* rinui_active_qt_application_qobject();

/// Validate a borrowed `QGuiApplication&` reference (0=Ok, 1=NoApp, 2=NotGuiApp, 3=WrongThread, 4=Mismatch).
std::int32_t rinui_validate_existing_gui_application_ref(const QGuiApplication& app);

/// Validate a raw `QObject*` candidate representing an existing `QGuiApplication`/`QApplication`
/// (0=Ok, 1=NoApp, 2=NotGuiApp, 3=WrongThread, 4=Mismatch).
std::int32_t rinui_validate_existing_qobject_ptr(QObject* ptr);

/// Return true if there is no application or the caller is on the Qt application main thread.
bool rinui_is_on_app_thread();

/// Process pending events via `QCoreApplication::instance()`.
void rinui_app_process_events();

/// Flush pending QEvent::DeferredDelete events on the application thread (no-op if invoke depth > 0).
void rinui_flush_deferred_deletes();

/// Enter/leave QML execution scope (ref-counted on the Qt GUI thread) to defer destructive teardown.
void rinui_qml_exec_enter();
void rinui_qml_exec_leave();

/// Return true if Qt is currently running inside `exec()` or a nested `QEventLoop` (`loopLevel() > 0`).
bool rinui_in_event_loop();

/// Return true only while the active application is still a QGuiApplication.
bool rinui_has_live_gui_application();
/// Detach this runtime's roots/windows and deleteLater without synchronous window cleanup.
void rinui_runtime_release_windows_deferred(std::uint64_t runtime_id);
/// Register the drain callback; bool is true only during the application post routine.
void rinui_set_deferred_drain_callback(rust::Fn<void(bool)> cb);

/// Coalesce drain wakeups; only a delivered per-engine DeferredDelete fence permits normal drops.
/// aboutToQuit wakes the same checks; qAddPostRoutine is the final shutdown fallback.
void rinui_schedule_deferred_drain();
/// Qualify a single engine only after Qt delivers its associated DeferredDelete event.
void rinui_defer_engine_drop(std::uint64_t engine_id);
bool rinui_engine_drop_ready(std::uint64_t engine_id);

/// Discard the latest unregistered root object created during a load whose session was dropped mid-load.
void rinui_engine_discard_latest_unregistered_root(QQmlApplicationEngine& engine);

/// Configure Qt Quick Controls style safely.
/// Returns 0 on success/match, or 1 if Quick Controls is already initialized with an incompatible style.
std::int32_t rinui_configure_quick_controls_style(
    const QString& requested,
    bool is_borrowed,
    QString& out_current);

/// Snapshot process-global settings from a borrowed `QGuiApplication` before RinUI mutates them.
void rinui_capture_borrowed_app_state();

/// Restore snapshotted settings back onto the borrowed `QGuiApplication`.
void rinui_restore_borrowed_app_state(bool restore_identity);

/// Apply application identity fields to the active `QCoreApplication`.
void rinui_apply_active_app_identity(
    const QString& org_name,
    const QString& org_domain,
    const QString& app_name,
    const QString& app_version);

/// Query current `QCoreApplication::organizationName()`.
QString rinui_app_organization_name();

/// Query current `QCoreApplication::applicationName()`.
QString rinui_app_application_name();

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

/// Query QLocale().name() (current process default QLocale).
QString rinui_default_locale_name();

/// Canonicalize a locale tag using Qt's QLocale rules; returns empty QString if unrecognized.
QString rinui_canonicalize_locale_name(const QString& raw_tag);

/// Register platform-specific QML context objects on the engine.
void rinui_platform_register_engine_objects(QQmlApplicationEngine& engine, bool staged_filesystem);

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

/// Register a live `QQmlApplicationEngine` with its owning `(runtime_id, engine_id)`.
void rinui_engine_register(
    QQmlApplicationEngine& engine,
    std::uint64_t runtime_id,
    std::uint64_t engine_id);

/// Unregister an engine prior to destruction.
void rinui_engine_unregister(std::uint64_t engine_id);

/// Register the latest root object created by `engine` with owner IDs and discover its RinUI windows.
std::uint64_t rinui_engine_register_latest_root_owned(
    QQmlApplicationEngine& engine,
    std::uint64_t runtime_id,
    std::uint64_t engine_id,
    std::uint64_t session_id,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down);

/// Return tracked RinUI window handle IDs belonging to `(session_id, root_id)`.
rust::Vec<std::uint64_t> rinui_root_windows_owned(
    std::uint64_t session_id,
    std::uint64_t root_id);

/// Discover dynamic top-level RinUI windows belonging to `engine` and attributable to `session_id`.
/// On shared engines (`!is_private_engine`), ambiguous windows with no identifiable session owner
/// are left unregistered rather than being claimed by the first caller.
rust::Vec<std::uint64_t> rinui_sync_session_windows(
    QQmlApplicationEngine& engine,
    std::uint64_t runtime_id,
    std::uint64_t engine_id,
    std::uint64_t session_id,
    bool is_private_engine,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down);

/// Explicitly attribute and register a dynamic `QQuickWindow` stored on a session-owned root property.
/// Returns `true` (`out_status == 0`) with `out_window_id` on success.
/// Status: 0=Ok, 1=RootDisposed, 2=RootNotOwned, 3=MissingOrNull, 4=NotAWindow,
///         5=WindowReleased, 6=WindowOwnedByOtherSession, 7=EngineMismatch.
bool rinui_register_window_from_root_property(
    QQmlApplicationEngine& engine,
    std::uint64_t runtime_id,
    std::uint64_t engine_id,
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& property_name,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down,
    std::uint64_t& out_window_id,
    std::int32_t& out_status,
    QString& out_detail);

/// Return all currently alive RinUI window IDs owned by `session_id`.
rust::Vec<std::uint64_t> rinui_session_alive_windows(std::uint64_t session_id);

/// Return all currently alive RinUI window IDs owned by `runtime_id`.
rust::Vec<std::uint64_t> rinui_runtime_alive_windows(std::uint64_t runtime_id);

/// Return all currently alive RinUI window IDs across the process.
rust::Vec<std::uint64_t> rinui_all_alive_windows();

/// Release a session-owned root object tree, detach native hooks, and schedule QML object deletion.
bool rinui_root_release_owned(std::uint64_t session_id, std::uint64_t root_id);

/// Release all roots and windows owned by `session_id`.
void rinui_session_release_all(std::uint64_t session_id);

/// Release all remaining tracked roots, windows, and window/native filters for `runtime_id` (0 = all).
void rinui_runtime_release_windows_and_filters(std::uint64_t runtime_id);

/// Tear down runtime-lifetime platform services after engines have dropped.
void rinui_runtime_teardown_services(std::uint64_t runtime_id);

/// Legacy helper releasing all roots and platform services.
void rinui_release_all_roots_and_platform();

/// Check whether a session-owned root handle still points to a live QObject.
bool rinui_root_is_alive_owned(std::uint64_t session_id, std::uint64_t root_id);

/// Read a dynamic or Q_PROPERTY property from a session-owned root QObject.
QVariant rinui_root_get_property_owned(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name);

/// Write a property on a session-owned root QObject.
bool rinui_root_set_property_owned(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name,
    const QVariant& value);

/// Check whether a window handle belongs to `session_id` and points to a live QQuickWindow.
bool rinui_window_is_alive_owned(std::uint64_t session_id, std::uint64_t window_id);

/// Query native winId() for a session-owned window (0 if invalid or not owned).
std::size_t rinui_window_win_id_owned(std::uint64_t session_id, std::uint64_t window_id);

/// Read a property from a session-owned QQuickWindow.
QVariant rinui_window_get_property_owned(
    std::uint64_t session_id,
    std::uint64_t window_id,
    const QString& name);

/// Write a property on a session-owned QQuickWindow.
bool rinui_window_set_property_owned(
    std::uint64_t session_id,
    std::uint64_t window_id,
    const QString& name,
    const QVariant& value);

/// Show a session-owned QQuickWindow.
void rinui_window_show_owned(std::uint64_t session_id, std::uint64_t window_id);

/// Close a session-owned QQuickWindow.
void rinui_window_close_owned(std::uint64_t session_id, std::uint64_t window_id);

/// Set application icon (when `set_app_icon` is true) and optional session-owned root "icon" QUrl.
void rinui_set_app_icon_owned(
    const QString& icon_path,
    std::uint64_t session_id,
    std::uint64_t root_id,
    bool set_app_icon);

/// Read a property on a session-owned root QObject with strict ownership/existence status.
/// Status: 0=Ok, 1=RootDisposed, 2=NotOwned, 3=PropertyNotFound.
QVariant rinui_root_get_property_checked(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name,
    std::int32_t& out_status,
    QString& out_detail);

/// Write a meta-property on a session-owned root QObject with strict QMetaObject & type checks.
/// Never creates dynamic properties for unknown names.
/// Returns: 0=Ok, 1=RootDisposed, 2=NotOwned, 3=PropertyNotFound, 4=PropertyReadOnly,
///          5=TypeIncompatible, 6=WriteFailed.
std::int32_t rinui_root_set_property_checked(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name,
    const QVariant& value,
    QString& out_detail);

/// Synchronously invoke a QML method/slot on a session-owned root QObject via Qt meta-object dispatch.
/// Status: 0=Ok, 1=RootDisposed, 2=NotOwned, 3=MethodNotFound, 4=ArgumentCountMismatch,
///          5=UnsupportedSignature, 6=InvocationFailed, 7=TooManyArguments.
QVariant rinui_root_invoke_method(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& method_name,
    rust::Slice<const QVariant> args,
    std::int32_t& out_status,
    QString& out_detail);

/// Trigger QQmlEngine::retranslate().
void rinui_engine_retranslate(QQmlApplicationEngine& engine);

/// Set extra file selectors used by `QQmlFileSelector` inside `engine`.
void rinui_engine_set_extra_file_selectors(
    QQmlApplicationEngine& engine,
    const QStringList& selectors);

/// Clear engine component cache (only safe when dependent QML object trees are destroyed).
void rinui_engine_clear_component_cache(QQmlApplicationEngine& engine);

/// Run QML JS garbage collector.
void rinui_engine_collect_garbage(QQmlApplicationEngine& engine);

/// Atomically load and install RinUI + application translators and commit QLocale::setDefault().
/// On any load or install failure, restores prior translators and default QLocale without mutation.
bool rinui_translator_apply_transaction(
    const QString& active_locale,
    const QString& rinui_qm_path,
    const QStringList& app_catalog_ids,
    const QStringList& app_qm_paths,
    QString& out_failed_path);

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
