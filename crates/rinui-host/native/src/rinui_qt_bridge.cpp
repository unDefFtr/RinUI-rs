#include "rinui_qt_bridge.h"
#include "rinui_platform_hooks.h"

#include <QClipboard>
#include <QCoreApplication>
#include <QDir>
#include <QEvent>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QHash>
#include <QIcon>
#include <QLocale>
#include <QMetaMethod>
#include <QMetaObject>
#include <QMetaProperty>
#include <QMetaType>
#include <QPointer>
#include <QJSValue>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QtQml/qqml.h>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QSet>
#include <QStandardPaths>
#include <QStyleHints>
#include <QTextStream>
#include <QThread>
#include <QTimer>
#include <QTranslator>
#include <QUrl>

#ifdef Q_OS_WIN
#include "rinui_win_bridge.h"
#endif

#include <limits>
#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace {

struct RootRecord {
    std::uint64_t runtime_id = 0;
    std::uint64_t engine_id = 0;
    std::uint64_t session_id = 0;
    QPointer<QObject> root;
    QList<std::uint64_t> window_ids;
};

struct WindowRecord {
    std::uint64_t runtime_id = 0;
    std::uint64_t engine_id = 0;
    std::uint64_t session_id = 0;
    QPointer<QQuickWindow> window;
};

struct BorrowedAppState {
    bool captured = false;
    QLocale locale;
    QIcon window_icon;
    QString organization_name;
    QString organization_domain;
    QString application_name;
    QString application_display_name;
    QString application_version;
    QString desktop_file_name;
};

std::uint64_t s_next_id = 1;
QHash<std::uint64_t, RootRecord> s_roots;
QHash<std::uint64_t, WindowRecord> s_windows;
QHash<std::uint64_t, const QQmlEngine*> s_engine_by_id;
QHash<const QQmlEngine*, std::uint64_t> s_id_by_engine;
QHash<const QQmlApplicationEngine*, QStringList> s_engine_warnings;
std::unique_ptr<QTranslator> s_rinui_translator;
std::unordered_map<std::string, std::unique_ptr<QTranslator>> s_app_translators;
BorrowedAppState s_borrowed_app_state;
bool s_quick_controls_initialized = false;
thread_local int s_active_invoke_depth = 0;
std::optional<rust::Fn<void(bool)>> s_deferred_drain_cb;
bool s_deferred_drain_scheduled = false;
QMetaObject::Connection s_about_to_quit_conn;
QMetaObject::Connection s_app_destroyed_conn;
bool s_post_routine_registered = false;
QHash<std::uint64_t, QPointer<QObject>> s_engine_drop_fences;
QSet<std::uint64_t> s_ready_engine_drops;

// Only Qt delivery of this object's DeferredDelete event qualifies its engine.
// A timer firing (possibly in a recursive external processEvents) is insufficient.
class EngineDropFence final : public QObject {
public:
    explicit EngineDropFence(std::uint64_t engine_id) : m_engine_id(engine_id) {}

protected:
    bool event(QEvent* event) override
    {
        if (event->type() != QEvent::DeferredDelete) {
            return QObject::event(event);
        }
        s_engine_drop_fences.remove(m_engine_id);
        s_ready_engine_drops.insert(m_engine_id);
        const bool result = QObject::event(event); // deletes this fence
        if (s_deferred_drain_cb.has_value()) {
            (*s_deferred_drain_cb)(false);
        }
        return result;
    }

private:
    std::uint64_t m_engine_id;
};

bool is_deferred_context()
{
    if (s_active_invoke_depth > 0) {
        return true;
    }
    if (QThread* cur = QThread::currentThread()) {
        return cur->loopLevel() > 0;
    }
    return false;
}

void append_qml_log_line(const QString& line)
{
    QString base = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
    if (base.isEmpty()) {
        base = QDir::homePath() + QStringLiteral("/.RinUI");
    }
    const QString log_dir_path = base + QStringLiteral("/logs");
    QDir log_dir(log_dir_path);
    if (!log_dir.exists()) {
        log_dir.mkpath(QStringLiteral("."));
    }
    QFile file(log_dir.filePath(QStringLiteral("qml-runtime.log")));
    if (file.open(QIODevice::WriteOnly | QIODevice::Append | QIODevice::Text)) {
        QTextStream out(&file);
        out << line << '\n';
    }
}

std::uint64_t find_window_id(const QQuickWindow* target)
{
    if (!target) {
        return 0;
    }
    for (auto it = s_windows.cbegin(); it != s_windows.cend(); ++it) {
        if (it.value().window.data() == target) {
            return it.key();
        }
    }
    return 0;
}

bool is_root_already_registered(const QObject* target)
{
    if (!target) {
        return false;
    }
    for (auto it = s_roots.cbegin(); it != s_roots.cend(); ++it) {
        if (it.value().root.data() == target) {
            return true;
        }
    }
    return false;
}

std::uint64_t register_window_internal(
    QQuickWindow* win,
    std::uint64_t runtime_id,
    std::uint64_t engine_id,
    std::uint64_t session_id,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down)
{
    if (!win) {
        return 0;
    }
    if (const auto existing = find_window_id(win); existing != 0) {
        return existing;
    }

    const std::uint64_t wid = s_next_id++;
    s_windows.insert(
        wid,
        WindowRecord{runtime_id, engine_id, session_id, QPointer<QQuickWindow>(win)});

    QObject::connect(win, &QObject::destroyed, [wid, win]() {
        rinui::platform::teardown_window(win);
        s_windows.remove(wid);
        for (auto it = s_roots.begin(); it != s_roots.end(); ++it) {
            it->window_ids.removeAll(wid);
        }
    });

    rinui::platform::setup_window(win, native_mac_frame, mac_offset_x, mac_offset_down);
    return wid;
}

struct ResolvedWindowOwner {
    std::uint64_t engine_id = 0;
    std::uint64_t session_id = 0;
};

std::uint64_t find_object_engine_id(const QObject* obj)
{
    if (!obj) {
        return 0;
    }
    if (const QQmlEngine* eng = qmlEngine(obj)) {
        return s_id_by_engine.value(eng, 0);
    }
    if (const QQmlContext* ctx = qmlContext(obj)) {
        if (const QQmlEngine* eng = ctx->engine()) {
            return s_id_by_engine.value(eng, 0);
        }
    }
    return 0;
}

bool find_root_owner(const QObject* target, ResolvedWindowOwner& out)
{
    if (!target) {
        return false;
    }
    for (auto it = s_roots.cbegin(); it != s_roots.cend(); ++it) {
        if (it->root.data() == target) {
            out.engine_id = it->engine_id;
            out.session_id = it->session_id;
            return true;
        }
    }
    return false;
}

bool resolve_object_chain_owner(const QObject* start, ResolvedWindowOwner& out)
{
    for (const QObject* p = start; p != nullptr; p = p->parent()) {
        if (const auto* pw = qobject_cast<const QQuickWindow*>(p)) {
            const std::uint64_t wid = find_window_id(pw);
            if (wid != 0) {
                const auto it = s_windows.constFind(wid);
                if (it != s_windows.cend()) {
                    out.engine_id = it->engine_id;
                    out.session_id = it->session_id;
                    return true;
                }
            }
        }
        if (find_root_owner(p, out)) {
            return true;
        }
        if (out.engine_id == 0) {
            out.engine_id = find_object_engine_id(p);
        }
    }
    return false;
}

ResolvedWindowOwner resolve_window_owner(QQuickWindow* win)
{
    ResolvedWindowOwner out;
    if (!win) {
        return out;
    }

    // 1. Explicit owner stamped via explicit host registration API or prior claim.
    const QVariant explicit_owner = win->property("_rinuiOwnerSessionId");
    if (explicit_owner.isValid() && explicit_owner.toULongLong() != 0) {
        out.session_id = explicit_owner.toULongLong();
        out.engine_id = find_object_engine_id(win);
        if (out.engine_id == 0 && win->contentItem()) {
            out.engine_id = find_object_engine_id(win->contentItem());
        }
        if (out.engine_id == 0) {
            for (auto it = s_roots.cbegin(); it != s_roots.cend(); ++it) {
                if (it->session_id == out.session_id) {
                    out.engine_id = it->engine_id;
                    break;
                }
            }
        }
        return out;
    }

    // 2. QObject parent chain (matching tracked windows OR non-window QML roots).
    if (resolve_object_chain_owner(win->parent(), out)) {
        return out;
    }

    // 3. QWindow transientParent chain (matching tracked windows OR QML roots).
    for (const QWindow* tp = win->transientParent(); tp != nullptr; tp = tp->transientParent()) {
        if (resolve_object_chain_owner(tp, out)) {
            return out;
        }
    }

    // 4. QML creation-context chain (`Component.createObject(null)` created inside a session's root QML).
    auto check_context_chain = [&](const QQmlContext* start_ctx) -> bool {
        for (const QQmlContext* ctx = start_ctx; ctx != nullptr; ctx = ctx->parentContext()) {
            if (out.engine_id == 0 && ctx->engine()) {
                out.engine_id = s_id_by_engine.value(ctx->engine(), 0);
            }
            if (const QObject* ctx_obj = ctx->contextObject()) {
                if (ctx_obj != static_cast<const QObject*>(win)
                    && resolve_object_chain_owner(ctx_obj, out)) {
                    return true;
                }
            }
        }
        return false;
    };
    if (check_context_chain(qmlContext(win))) {
        return out;
    }
    if (win->contentItem() && check_context_chain(qmlContext(win->contentItem()))) {
        return out;
    }

    // 5. Fallback engine-only attribution (session_id remains 0 = ambiguous on shared engines).
    if (out.engine_id == 0) {
        out.engine_id = find_object_engine_id(win);
    }
    if (out.engine_id == 0 && win->contentItem()) {
        out.engine_id = find_object_engine_id(win->contentItem());
    }
    if (out.engine_id == 0) {
        for (auto it = s_engine_by_id.cbegin(); it != s_engine_by_id.cend(); ++it) {
            if (const auto* app_eng = qobject_cast<const QQmlApplicationEngine*>(it.value())) {
                if (app_eng->rootObjects().contains(win)) {
                    out.engine_id = it.key();
                    break;
                }
            }
        }
    }
    return out;
}

bool is_object_descendant(const QObject* child, const QObject* ancestor)
{
    if (!child || !ancestor) {
        return false;
    }
    for (const QObject* p = child->parent(); p != nullptr; p = p->parent()) {
        if (p == ancestor) {
            return true;
        }
    }
    return false;
}

void release_root_record(RootRecord& rec, bool force_deferred = false)
{
    bool root_already_torn_down = false;
    QObject* root_ptr = rec.root.data();
    const bool defer_window_teardown = force_deferred || is_deferred_context();

    for (std::uint64_t wid : rec.window_ids) {
        const auto wrec = s_windows.take(wid);
        if (!wrec.window.isNull()) {
            const bool is_root = static_cast<QObject*>(wrec.window.data()) == root_ptr;
            if (is_root) {
                root_already_torn_down = true;
            }
            wrec.window->setProperty("_rinuiReleased", true);
            rinui::platform::teardown_window(wrec.window.data());
            if (!defer_window_teardown) {
                wrec.window->hide();
                wrec.window->releaseResources();
            }
            if (!is_root && !is_object_descendant(wrec.window.data(), root_ptr)) {
                wrec.window->deleteLater();
            }
        }
    }

    if (!rec.root.isNull()) {
        QObject* root = rec.root.data();
        if (!root_already_torn_down) {
            if (auto* root_win = qobject_cast<QQuickWindow*>(root)) {
                root_win->setProperty("_rinuiReleased", true);
                rinui::platform::teardown_window(root_win);
                if (!defer_window_teardown) {
                    root_win->hide();
                    root_win->releaseResources();
                }
            }
        }
        root->deleteLater();
    }
}

} // namespace

void rinui_set_high_dpi_pass_through()
{
    QGuiApplication::setHighDpiScaleFactorRoundingPolicy(
        Qt::HighDpiScaleFactorRoundingPolicy::PassThrough);
}

bool rinui_has_gui_application()
{
    return QCoreApplication::instance() != nullptr;
}

QObject* rinui_active_qt_application_qobject()
{
    return static_cast<QObject*>(QCoreApplication::instance());
}

std::int32_t rinui_validate_existing_gui_application_ref(const QGuiApplication& app)
{
    QCoreApplication* instance = QCoreApplication::instance();
    if (!instance) {
        return 1; // NoApplication
    }
    auto* gui = qobject_cast<QGuiApplication*>(instance);
    if (!gui) {
        return 2; // NotGuiApplication
    }
    if (gui != &app) {
        return 4; // ApplicationMismatch
    }
    if (QThread::currentThread() != instance->thread()) {
        return 3; // WrongThread
    }
    return 0;
}

std::int32_t rinui_validate_existing_qobject_ptr(QObject* ptr)
{
    QCoreApplication* instance = QCoreApplication::instance();
    if (!instance) {
        return 1; // NoApplication
    }
    auto* gui = qobject_cast<QGuiApplication*>(instance);
    if (!gui) {
        return 2; // NotGuiApplication
    }
    if (!ptr || ptr != static_cast<QObject*>(instance)) {
        return 4; // ApplicationMismatch
    }
    if (QThread::currentThread() != instance->thread()) {
        return 3; // WrongThread
    }
    return 0;
}

bool rinui_is_on_app_thread()
{
    const QCoreApplication* app = QCoreApplication::instance();
    return !app || QThread::currentThread() == app->thread();
}

void rinui_app_process_events()
{
    if (QCoreApplication::instance() && rinui_is_on_app_thread()) {
        QCoreApplication::processEvents();
    }
}

void rinui_flush_deferred_deletes()
{
    if (s_active_invoke_depth > 0) {
        return;
    }
    if (QCoreApplication::instance() && rinui_is_on_app_thread()) {
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }
}

void rinui_qml_exec_enter()
{
    ++s_active_invoke_depth;
}

void rinui_qml_exec_leave()
{
    if (s_active_invoke_depth > 0) {
        --s_active_invoke_depth;
    }
}

bool rinui_in_event_loop()
{
    if (!QCoreApplication::instance()) {
        return false;
    }
    if (QThread* cur = QThread::currentThread()) {
        return cur->loopLevel() > 0;
    }
    return false;
}

void rinui_set_deferred_drain_callback(rust::Fn<void(bool)> cb)
{
    s_deferred_drain_cb = cb;
}

namespace {

void rinui_post_routine_drain()
{
    s_post_routine_registered = false;
    s_deferred_drain_scheduled = false;
    s_about_to_quit_conn = QMetaObject::Connection{};
    s_app_destroyed_conn = QMetaObject::Connection{};
    if (s_deferred_drain_cb.has_value()) {
        (*s_deferred_drain_cb)(true);
    }
}

void rinui_ensure_app_drain_hooks()
{
    QCoreApplication* app = QCoreApplication::instance();
    if (!app) {
        return;
    }
    if (!s_about_to_quit_conn) {
        s_about_to_quit_conn = QObject::connect(app, &QCoreApplication::aboutToQuit, app, []() {
            s_deferred_drain_scheduled = false;
            if (s_deferred_drain_cb.has_value()) {
                (*s_deferred_drain_cb)(false);
            }
        });
    }
    if (!s_app_destroyed_conn) {
        s_app_destroyed_conn = QObject::connect(app, &QObject::destroyed, app, []() {
            s_about_to_quit_conn = QMetaObject::Connection{};
            s_app_destroyed_conn = QMetaObject::Connection{};
            s_deferred_drain_scheduled = false;
        });
    }
    if (!s_post_routine_registered) {
        qAddPostRoutine(rinui_post_routine_drain);
        s_post_routine_registered = true;
    }
}

} // namespace

void rinui_schedule_deferred_drain()
{
    QCoreApplication* app = QCoreApplication::instance();
    if (!app) {
        return;
    }
    rinui_ensure_app_drain_hooks();
    if (s_deferred_drain_scheduled) {
        return;
    }
    s_deferred_drain_scheduled = true;
    QTimer::singleShot(0, app, []() {
        s_deferred_drain_scheduled = false;
        if (s_deferred_drain_cb.has_value()) {
            (*s_deferred_drain_cb)(false);
        }
    });
}

void rinui_defer_engine_drop(std::uint64_t engine_id)
{
    if (s_engine_drop_fences.contains(engine_id) || s_ready_engine_drops.contains(engine_id)) {
        return;
    }
    auto* fence = new EngineDropFence(engine_id);
    s_engine_drop_fences.insert(engine_id, QPointer<QObject>(fence));
    fence->deleteLater();
}

bool rinui_engine_drop_ready(std::uint64_t engine_id)
{
    return s_ready_engine_drops.contains(engine_id);
}

bool rinui_has_live_gui_application()
{
    QCoreApplication* inst = QCoreApplication::instance();
    return inst != nullptr && qobject_cast<QGuiApplication*>(inst) != nullptr;
}

std::int32_t rinui_configure_quick_controls_style(
    const QString& requested,
    bool is_borrowed,
    QString& out_current)
{
    const QString current = QQuickStyle::name();
    out_current = current;

    if (requested.isEmpty()) {
        return 0;
    }
    if (!current.isEmpty() && current.compare(requested, Qt::CaseInsensitive) == 0) {
        return 0;
    }

    bool controls_already_active = s_quick_controls_initialized;
    if (!controls_already_active && QCoreApplication::instance()) {
        const auto top_windows = QGuiApplication::topLevelWindows();
        for (const QWindow* qw : top_windows) {
            if (qobject_cast<const QQuickWindow*>(qw) != nullptr) {
                controls_already_active = true;
                break;
            }
        }
    }

    if (controls_already_active) {
        return 1;
    }

    if (is_borrowed) {
        const QString env_style = qEnvironmentVariable("QT_QUICK_CONTROLS_STYLE").trimmed();
        if (!env_style.isEmpty() && env_style.compare(requested, Qt::CaseInsensitive) != 0) {
            out_current = env_style;
            return 1;
        }
    }

    QQuickStyle::setStyle(requested);
    const QString after = QQuickStyle::name();
    out_current = after;
    if (!after.isEmpty() && after.compare(requested, Qt::CaseInsensitive) != 0) {
        return 1;
    }
    return 0;
}

void rinui_capture_borrowed_app_state()
{
    s_borrowed_app_state.captured = true;
    s_borrowed_app_state.locale = QLocale();
    s_borrowed_app_state.window_icon = QGuiApplication::windowIcon();
    s_borrowed_app_state.organization_name = QCoreApplication::organizationName();
    s_borrowed_app_state.organization_domain = QCoreApplication::organizationDomain();
    s_borrowed_app_state.application_name = QCoreApplication::applicationName();
    s_borrowed_app_state.application_display_name = QGuiApplication::applicationDisplayName();
    s_borrowed_app_state.application_version = QCoreApplication::applicationVersion();
    s_borrowed_app_state.desktop_file_name = QGuiApplication::desktopFileName();
}

void rinui_restore_borrowed_app_state(bool restore_identity)
{
    if (!s_borrowed_app_state.captured) {
        return;
    }
    QLocale::setDefault(s_borrowed_app_state.locale);
    if (QCoreApplication::instance() && qobject_cast<QGuiApplication*>(QCoreApplication::instance())) {
        QGuiApplication::setWindowIcon(s_borrowed_app_state.window_icon);
        if (restore_identity) {
            QCoreApplication::setOrganizationName(s_borrowed_app_state.organization_name);
            QCoreApplication::setOrganizationDomain(s_borrowed_app_state.organization_domain);
            QCoreApplication::setApplicationName(s_borrowed_app_state.application_name);
            QGuiApplication::setApplicationDisplayName(s_borrowed_app_state.application_display_name);
            QCoreApplication::setApplicationVersion(s_borrowed_app_state.application_version);
            QGuiApplication::setDesktopFileName(s_borrowed_app_state.desktop_file_name);
        }
    }
    s_borrowed_app_state = BorrowedAppState{};
}

void rinui_apply_active_app_identity(
    const QString& org_name,
    const QString& org_domain,
    const QString& app_name,
    const QString& app_version)
{
    if (!QCoreApplication::instance()) {
        return;
    }
    QCoreApplication::setOrganizationName(org_name);
    if (!org_domain.isEmpty()) {
        QCoreApplication::setOrganizationDomain(org_domain);
    }
    QCoreApplication::setApplicationName(app_name);
    if (!app_version.isEmpty()) {
        QCoreApplication::setApplicationVersion(app_version);
    }
}

QString rinui_app_organization_name()
{
    return QCoreApplication::organizationName();
}

QString rinui_app_application_name()
{
    return QCoreApplication::applicationName();
}

void rinui_set_quick_controls_style(const QString& style)
{
    if (QQuickStyle::name() != style) {
        QQuickStyle::setStyle(style);
    }
}

QString rinui_standard_app_local_data_dir()
{
    return QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
}

QString rinui_current_system_theme()
{
    if (const auto* hints = QGuiApplication::styleHints()) {
        if (hints->colorScheme() == Qt::ColorScheme::Dark) {
            return QStringLiteral("Dark");
        }
    }
    return QStringLiteral("Light");
}

QString rinui_platform_name()
{
    return QGuiApplication::platformName();
}

QString rinui_system_locale_name()
{
    return QLocale::system().name();
}

QString rinui_default_locale_name()
{
    return QLocale().name();
}

QString rinui_canonicalize_locale_name(const QString& raw_tag)
{
    QString cleaned = raw_tag.trimmed();
    const qsizetype dot_pos = cleaned.indexOf(u'.');
    if (dot_pos >= 0) {
        cleaned.truncate(dot_pos);
    }
    const qsizetype at_pos = cleaned.indexOf(u'@');
    if (at_pos >= 0) {
        cleaned.truncate(at_pos);
    }
    cleaned = cleaned.trimmed();
    if (cleaned.isEmpty()) {
        return QString();
    }
    if (cleaned.compare(QStringLiteral("C"), Qt::CaseInsensitive) == 0
        || cleaned.compare(QStringLiteral("POSIX"), Qt::CaseInsensitive) == 0) {
        return QStringLiteral("C");
    }
    cleaned.replace(u'-', u'_');
    const QLocale loc(cleaned);
    if (loc.language() == QLocale::C) {
        return QString();
    }
    return loc.name();
}

void rinui_platform_register_engine_objects(QQmlApplicationEngine& engine, bool staged_filesystem)
{
    rinui::platform::register_engine_objects(engine);
#ifdef Q_OS_WIN
    if (staged_filesystem) {
        engine.rootContext()->setContextProperty(QStringLiteral("RinUIStagedFonts"),
                                                 new rinui::win::StagedFonts(&engine));
    }
#else
    Q_UNUSED(staged_filesystem);
#endif
}

void rinui_engine_set_context_property_object(
    QQmlApplicationEngine& engine,
    const QString& name,
    QObject* object)
{
    if (auto* ctx = engine.rootContext()) {
        ctx->setContextProperty(name, object);
    }
}

void rinui_engine_set_context_property_variant(
    QQmlApplicationEngine& engine,
    const QString& name,
    const QVariant& value)
{
    if (auto* ctx = engine.rootContext()) {
        ctx->setContextProperty(name, value);
    }
}

void rinui_engine_install_warning_collector(QQmlApplicationEngine& engine)
{
    const QQmlApplicationEngine* key = &engine;
    s_engine_warnings.insert(key, QStringList{});

    QObject::connect(&engine, &QObject::destroyed, [key]() {
        s_engine_warnings.remove(key);
    });

    QObject::connect(
        &engine,
        &QQmlEngine::warnings,
        &engine,
        [key](const QList<QQmlError>& warnings) {
            auto& list = s_engine_warnings[key];
            const bool write_log = qEnvironmentVariableIsSet("RINUI_QML_LOG")
                && !qEnvironmentVariableIsEmpty("RINUI_QML_LOG");
            for (const QQmlError& err : warnings) {
                const QString text = err.toString();
                list.append(text);
                if (write_log) {
                    append_qml_log_line(QStringLiteral("[qml-warning] ") + text);
                }
            }
        });
}

QStringList rinui_engine_warnings(const QQmlApplicationEngine& engine)
{
    return s_engine_warnings.value(&engine);
}

void rinui_engine_clear_warnings(const QQmlApplicationEngine& engine)
{
    auto it = s_engine_warnings.find(&engine);
    if (it != s_engine_warnings.end()) {
        it->clear();
    }
}

void rinui_engine_uninstall_warning_collector(const QQmlApplicationEngine& engine)
{
    s_engine_warnings.remove(&engine);
}

void rinui_write_qml_failure_log(
    const QString& qml_source,
    const QString& rinui_origin,
    const QStringList& warnings,
    const QString& message)
{
    append_qml_log_line(
        QStringLiteral("[load] qml=%1 rinui=%2").arg(qml_source, rinui_origin));
    for (const QString& warning : warnings) {
        append_qml_log_line(QStringLiteral("[qml-warning] ") + warning);
    }
    append_qml_log_line(QStringLiteral("[load-failed] ") + message);
}

std::size_t rinui_engine_root_count(const QQmlApplicationEngine& engine)
{
    return static_cast<std::size_t>(engine.rootObjects().size());
}

void rinui_engine_register(
    QQmlApplicationEngine& engine,
    std::uint64_t /*runtime_id*/,
    std::uint64_t engine_id)
{
    const QQmlEngine* key = static_cast<const QQmlEngine*>(&engine);
    s_engine_by_id.insert(engine_id, key);
    s_id_by_engine.insert(key, engine_id);
    QObject::connect(&engine, &QObject::destroyed, [engine_id, key]() {
        if (s_engine_by_id.value(engine_id, nullptr) == key) {
            s_engine_by_id.remove(engine_id);
        }
        if (s_id_by_engine.value(key, 0) == engine_id) {
            s_id_by_engine.remove(key);
        }
    });
}

void rinui_engine_unregister(std::uint64_t engine_id)
{
    if (const QQmlEngine* eng = s_engine_by_id.take(engine_id)) {
        if (s_id_by_engine.value(eng, 0) == engine_id) {
            s_id_by_engine.remove(eng);
        }
    }
    s_ready_engine_drops.remove(engine_id);
    delete s_engine_drop_fences.take(engine_id).data();
}

void rinui_engine_discard_latest_unregistered_root(QQmlApplicationEngine& engine)
{
    const auto roots = engine.rootObjects();
    QObject* root = nullptr;
    for (qsizetype i = roots.size(); i > 0; --i) {
        QObject* candidate = roots.at(i - 1);
        if (candidate && !is_root_already_registered(candidate)) {
            root = candidate;
            break;
        }
    }
    if (!root) {
        return;
    }

    const bool defer_window_teardown = is_deferred_context();
    auto detach_win = [&](QQuickWindow* win) {
        if (!win) {
            return;
        }
        win->setProperty("_rinuiReleased", true);
        rinui::platform::teardown_window(win);
        if (!defer_window_teardown) {
            win->hide();
            win->releaseResources();
        }
    };

    if (auto* root_win = qobject_cast<QQuickWindow*>(root)) {
        detach_win(root_win);
    }
    const auto child_windows = root->findChildren<QQuickWindow*>();
    for (QQuickWindow* child : child_windows) {
        detach_win(child);
    }
    root->deleteLater();
}

std::uint64_t rinui_engine_register_latest_root_owned(
    QQmlApplicationEngine& engine,
    std::uint64_t runtime_id,
    std::uint64_t engine_id,
    std::uint64_t session_id,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down)
{
    const auto roots = engine.rootObjects();
    if (roots.isEmpty()) {
        return 0;
    }

    QObject* root = roots.last();
    if (!root || is_root_already_registered(root)) {
        return 0;
    }

    s_quick_controls_initialized = true;
    const std::uint64_t root_id = s_next_id++;
    RootRecord rec;
    rec.runtime_id = runtime_id;
    rec.engine_id = engine_id;
    rec.session_id = session_id;
    rec.root = QPointer<QObject>(root);

    QList<QQuickWindow*> candidates;
    if (auto* root_win = qobject_cast<QQuickWindow*>(root)) {
        candidates.append(root_win);
    }
    const auto child_windows = root->findChildren<QQuickWindow*>();
    for (QQuickWindow* child : child_windows) {
        if (child && !candidates.contains(child)) {
            candidates.append(child);
        }
    }

    for (QQuickWindow* win : candidates) {
        if (win && win->property("isRinUIWindow").toBool()) {
            const auto wid = register_window_internal(
                win,
                runtime_id,
                engine_id,
                session_id,
                native_mac_frame,
                mac_offset_x,
                mac_offset_down);
            if (wid != 0) {
                rec.window_ids.append(wid);
            }
        }
    }

    s_roots.insert(root_id, std::move(rec));
    QObject::connect(root, &QObject::destroyed, [root_id]() {
        s_roots.remove(root_id);
    });
    return root_id;
}

rust::Vec<std::uint64_t> rinui_root_windows_owned(
    std::uint64_t session_id,
    std::uint64_t root_id)
{
    rust::Vec<std::uint64_t> out;
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->session_id != session_id) {
        return out;
    }
    for (std::uint64_t wid : it->window_ids) {
        if (rinui_window_is_alive_owned(session_id, wid)) {
            out.push_back(wid);
        }
    }
    return out;
}

rust::Vec<std::uint64_t> rinui_sync_session_windows(
    QQmlApplicationEngine& engine,
    std::uint64_t runtime_id,
    std::uint64_t engine_id,
    std::uint64_t session_id,
    bool is_private_engine,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down)
{
    rust::Vec<std::uint64_t> discovered;
    Q_UNUSED(engine);
    if (!QCoreApplication::instance()) {
        return discovered;
    }
    const auto top_windows = QGuiApplication::topLevelWindows();
    for (QWindow* qw : top_windows) {
        auto* quick_win = qobject_cast<QQuickWindow*>(qw);
        if (!quick_win || !quick_win->property("isRinUIWindow").toBool()
            || quick_win->property("_rinuiReleased").toBool()) {
            continue;
        }
        if (find_window_id(quick_win) != 0) {
            continue;
        }
        const auto owner = resolve_window_owner(quick_win);
        if (owner.engine_id != engine_id) {
            continue;
        }
        if (owner.session_id != 0) {
            if (owner.session_id != session_id) {
                continue;
            }
        } else if (!is_private_engine) {
            // Do not auto-claim unattributed windows on a shared engine.
            continue;
        }

        const auto wid = register_window_internal(
            quick_win,
            runtime_id,
            engine_id,
            session_id,
            native_mac_frame,
            mac_offset_x,
            mac_offset_down);
        if (wid != 0) {
            quick_win->setProperty("_rinuiOwnerSessionId", QVariant::fromValue<qulonglong>(session_id));
            discovered.push_back(wid);
        }
    }
    return discovered;
}

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
    QString& out_detail)
{
    Q_UNUSED(engine);
    out_window_id = 0;
    out_detail.clear();

    const auto root_it = s_roots.find(root_id);
    if (root_it == s_roots.end() || root_it->root.isNull()) {
        out_status = 1; // RootDisposed
        return false;
    }
    if (root_it->session_id != session_id) {
        out_status = 2; // RootNotOwned
        return false;
    }

    QObject* root = root_it->root.data();
    const QByteArray prop_utf8 = property_name.toUtf8();
    const QVariant raw_val = root->property(prop_utf8.constData());
    if (!raw_val.isValid()) {
        out_status = 3; // MissingOrNull
        out_detail = QStringLiteral("root property '%1' is missing or invalid").arg(property_name);
        return false;
    }

    QObject* candidate_obj = raw_val.value<QObject*>();
    if (!candidate_obj && raw_val.userType() == qMetaTypeId<QJSValue>()) {
        const QJSValue js_val = raw_val.value<QJSValue>();
        candidate_obj = js_val.toQObject();
        if (!candidate_obj) {
            candidate_obj = js_val.toVariant().value<QObject*>();
        }
    }
    if (!candidate_obj) {
        out_status = 3; // MissingOrNull
        out_detail = QStringLiteral("root property '%1' is null or not a QObject").arg(property_name);
        return false;
    }

    auto* quick_win = qobject_cast<QQuickWindow*>(candidate_obj);
    if (!quick_win) {
        out_status = 4; // NotAWindow
        out_detail = QStringLiteral("root property '%1' is not a QQuickWindow").arg(property_name);
        return false;
    }
    if (quick_win->property("_rinuiReleased").toBool()) {
        out_status = 5; // WindowReleased
        out_detail = QStringLiteral("window from property '%1' has already been released").arg(property_name);
        return false;
    }

    if (const auto existing = find_window_id(quick_win); existing != 0) {
        const auto wit = s_windows.constFind(existing);
        if (wit != s_windows.cend()) {
            if (wit->session_id != session_id) {
                out_status = 6; // WindowOwnedByOtherSession
                out_detail = QStringLiteral("window is already owned by session %1").arg(wit->session_id);
                return false;
            }
            if (!root_it->window_ids.contains(existing)) {
                root_it->window_ids.append(existing);
            }
            out_window_id = existing;
            out_status = 0;
            return true;
        }
    }

    const auto owner = resolve_window_owner(quick_win);
    if (owner.session_id != 0 && owner.session_id != session_id) {
        out_status = 6; // WindowOwnedByOtherSession
        out_detail = QStringLiteral("window belongs to session %1").arg(owner.session_id);
        return false;
    }
    if (owner.engine_id != engine_id) {
        out_status = 7; // EngineMismatch
        out_detail = QStringLiteral("window belongs to engine %1, expected %2").arg(owner.engine_id).arg(engine_id);
        return false;
    }

    const auto wid = register_window_internal(
        quick_win,
        runtime_id,
        engine_id,
        session_id,
        native_mac_frame,
        mac_offset_x,
        mac_offset_down);
    if (wid == 0) {
        out_status = 4;
        out_detail = QStringLiteral("failed to register window from '%1'").arg(property_name);
        return false;
    }

    quick_win->setProperty("_rinuiOwnerSessionId", QVariant::fromValue<qulonglong>(session_id));
    if (!root_it->window_ids.contains(wid)) {
        root_it->window_ids.append(wid);
    }
    out_window_id = wid;
    out_status = 0;
    return true;
}

rust::Vec<std::uint64_t> rinui_session_alive_windows(std::uint64_t session_id)
{
    rust::Vec<std::uint64_t> out;
    for (auto it = s_windows.cbegin(); it != s_windows.cend(); ++it) {
        if (it.value().session_id == session_id && !it.value().window.isNull()) {
            out.push_back(it.key());
        }
    }
    return out;
}

rust::Vec<std::uint64_t> rinui_runtime_alive_windows(std::uint64_t runtime_id)
{
    rust::Vec<std::uint64_t> out;
    for (auto it = s_windows.cbegin(); it != s_windows.cend(); ++it) {
        if (it.value().runtime_id == runtime_id && !it.value().window.isNull()) {
            out.push_back(it.key());
        }
    }
    return out;
}

rust::Vec<std::uint64_t> rinui_all_alive_windows()
{
    rust::Vec<std::uint64_t> out;
    for (auto it = s_windows.cbegin(); it != s_windows.cend(); ++it) {
        if (!it.value().window.isNull()) {
            out.push_back(it.key());
        }
    }
    return out;
}

bool rinui_root_release_owned(std::uint64_t session_id, std::uint64_t root_id)
{
    const auto it = s_roots.find(root_id);
    if (it == s_roots.end() || it->session_id != session_id) {
        return false;
    }
    RootRecord rec = std::move(it.value());
    s_roots.erase(it);
    release_root_record(rec);
    return true;
}

void rinui_session_release_all(std::uint64_t session_id)
{
    // Collect any un-synced dynamic windows attributable specifically to `session_id`
    // before removing this session's roots from `s_roots`.
    QList<QPointer<QQuickWindow>> owned_unsynced_windows;
    if (QCoreApplication::instance()) {
        const auto top_windows = QGuiApplication::topLevelWindows();
        for (QWindow* qw : top_windows) {
            auto* quick_win = qobject_cast<QQuickWindow*>(qw);
            if (!quick_win || quick_win->property("_rinuiReleased").toBool()
                || find_window_id(quick_win) != 0) {
                continue;
            }
            const auto owner = resolve_window_owner(quick_win);
            if (owner.session_id == session_id) {
                owned_unsynced_windows.append(QPointer<QQuickWindow>(quick_win));
            }
        }
    }

    const auto keys = s_roots.keys();
    for (std::uint64_t root_id : keys) {
        const auto it = s_roots.find(root_id);
        if (it != s_roots.end() && it->session_id == session_id) {
            RootRecord rec = std::move(it.value());
            s_roots.erase(it);
            release_root_record(rec, true);
        }
    }

    const auto win_keys = s_windows.keys();
    for (std::uint64_t wid : win_keys) {
        const auto it = s_windows.find(wid);
        if (it != s_windows.end() && it->session_id == session_id) {
            const auto rec = std::move(it.value());
            s_windows.erase(it);
            if (!rec.window.isNull()) {
                rec.window->setProperty("_rinuiReleased", true);
                rinui::platform::teardown_window(rec.window.data());
                rec.window->deleteLater();
            }
        }
    }

    for (const auto& win_ptr : owned_unsynced_windows) {
        if (!win_ptr.isNull()) {
            win_ptr->setProperty("_rinuiReleased", true);
            rinui::platform::teardown_window(win_ptr.data());
            win_ptr->deleteLater();
        }
    }
}

void rinui_runtime_release_windows_deferred(std::uint64_t runtime_id)
{
    const auto root_ids = s_roots.keys();
    for (std::uint64_t root_id : root_ids) {
        const auto it = s_roots.find(root_id);
        if (it != s_roots.end() && it->runtime_id == runtime_id) {
            RootRecord rec = std::move(it.value());
            s_roots.erase(it);
            release_root_record(rec, true);
        }
    }
    const auto window_ids = s_windows.keys();
    for (std::uint64_t wid : window_ids) {
        const auto it = s_windows.find(wid);
        if (it != s_windows.end() && it->runtime_id == runtime_id) {
            const auto rec = std::move(it.value());
            s_windows.erase(it);
            if (!rec.window.isNull()) {
                rec.window->setProperty("_rinuiReleased", true);
                rinui::platform::teardown_window(rec.window.data());
                rec.window->deleteLater();
            }
        }
    }
}

void rinui_runtime_release_windows_and_filters(std::uint64_t runtime_id)
{
    const auto keys = s_roots.keys();
    for (std::uint64_t root_id : keys) {
        const auto it = s_roots.find(root_id);
        if (it != s_roots.end() && (runtime_id == 0 || it->runtime_id == runtime_id)) {
            RootRecord rec = std::move(it.value());
            s_roots.erase(it);
            release_root_record(rec);
        }
    }

    const auto win_keys = s_windows.keys();
    for (std::uint64_t wid : win_keys) {
        const auto it = s_windows.find(wid);
        if (it != s_windows.end() && (runtime_id == 0 || it->runtime_id == runtime_id)) {
            const auto rec = std::move(it.value());
            s_windows.erase(it);
            if (!rec.window.isNull()) {
                rinui::platform::teardown_window(rec.window.data());
                rec.window->hide();
                rec.window->releaseResources();
            }
        }
    }

    if (runtime_id == 0 || (s_roots.isEmpty() && s_windows.isEmpty())) {
        rinui::platform::teardown_windows_and_filters();
    }
}

void rinui_runtime_teardown_services(std::uint64_t /*runtime_id*/)
{
    rinui::platform::teardown_runtime_services();
    rinui::platform::set_theme_manager(nullptr);
}

void rinui_release_all_roots_and_platform()
{
    rinui_runtime_release_windows_and_filters(0);
    rinui_runtime_teardown_services(0);
}

bool rinui_root_is_alive_owned(std::uint64_t session_id, std::uint64_t root_id)
{
    const auto it = s_roots.constFind(root_id);
    return it != s_roots.cend() && it->session_id == session_id && !it->root.isNull();
}

QVariant rinui_root_get_property_owned(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name)
{
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->session_id != session_id || it->root.isNull()) {
        return QVariant();
    }
    const QByteArray prop = name.toUtf8();
    return it->root->property(prop.constData());
}

bool rinui_root_set_property_owned(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name,
    const QVariant& value)
{
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->session_id != session_id || it->root.isNull()) {
        return false;
    }
    const QByteArray prop = name.toUtf8();
    return it->root->setProperty(prop.constData(), value);
}

namespace {

bool is_signed_integral_type(int type_id)
{
    switch (type_id) {
    case QMetaType::SChar:
    case QMetaType::Short:
    case QMetaType::Int:
    case QMetaType::Long:
    case QMetaType::LongLong:
        return true;
    default:
        return false;
    }
}

bool is_unsigned_integral_type(int type_id)
{
    switch (type_id) {
    case QMetaType::UChar:
    case QMetaType::UShort:
    case QMetaType::UInt:
    case QMetaType::ULong:
    case QMetaType::ULongLong:
        return true;
    default:
        return false;
    }
}

bool is_integral_type(int type_id)
{
    return is_signed_integral_type(type_id) || is_unsigned_integral_type(type_id);
}

bool is_floating_type(int type_id)
{
    return type_id == QMetaType::Double || type_id == QMetaType::Float;
}

bool convert_integral_checked(
    const QVariant& src,
    const QMetaType& target,
    QVariant& out,
    QString& out_detail)
{
    const int target_id = target.id();
    const int src_id = src.typeId();
    if (!is_integral_type(src_id)) {
        return false;
    }

    bool ok = false;
    const bool src_unsigned = is_unsigned_integral_type(src_id);
    const std::uint64_t uval = src_unsigned ? src.toULongLong(&ok) : 0;
    const std::int64_t sval = src_unsigned ? 0 : src.toLongLong(&ok);
    if (!ok) {
        out_detail = QStringLiteral("integer value conversion failed");
        return false;
    }

    auto assign_signed = [&](auto dummy_tag) -> bool {
        using T = decltype(dummy_tag);
        const std::int64_t min_v = static_cast<std::int64_t>(std::numeric_limits<T>::min());
        const std::int64_t max_v = static_cast<std::int64_t>(std::numeric_limits<T>::max());
        if (src_unsigned) {
            if (uval > static_cast<std::uint64_t>(max_v)) {
                out_detail = QStringLiteral("unsigned integer %1 overflows %2")
                                 .arg(uval)
                                 .arg(QString::fromUtf8(target.name()));
                return false;
            }
            out = QVariant::fromValue(static_cast<T>(uval));
            return true;
        }
        if (sval < min_v || sval > max_v) {
            out_detail = QStringLiteral("integer %1 is out of range for %2")
                             .arg(sval)
                             .arg(QString::fromUtf8(target.name()));
            return false;
        }
        out = QVariant::fromValue(static_cast<T>(sval));
        return true;
    };

    auto assign_unsigned = [&](auto dummy_tag) -> bool {
        using T = decltype(dummy_tag);
        const std::uint64_t max_v = static_cast<std::uint64_t>(std::numeric_limits<T>::max());
        if (src_unsigned) {
            if (uval > max_v) {
                out_detail = QStringLiteral("unsigned integer %1 exceeds max of %2")
                                 .arg(uval)
                                 .arg(QString::fromUtf8(target.name()));
                return false;
            }
            out = QVariant::fromValue(static_cast<T>(uval));
            return true;
        }
        if (sval < 0 || static_cast<std::uint64_t>(sval) > max_v) {
            out_detail = QStringLiteral("integer %1 is out of range for unsigned %2")
                             .arg(sval)
                             .arg(QString::fromUtf8(target.name()));
            return false;
        }
        out = QVariant::fromValue(static_cast<T>(sval));
        return true;
    };

    switch (target_id) {
    case QMetaType::SChar:
        return assign_signed(static_cast<signed char>(0));
    case QMetaType::Short:
        return assign_signed(static_cast<short>(0));
    case QMetaType::Int:
        return assign_signed(static_cast<int>(0));
    case QMetaType::Long:
        return assign_signed(static_cast<long>(0));
    case QMetaType::LongLong:
        return assign_signed(static_cast<qlonglong>(0));
    case QMetaType::UChar:
        return assign_unsigned(static_cast<unsigned char>(0));
    case QMetaType::UShort:
        return assign_unsigned(static_cast<unsigned short>(0));
    case QMetaType::UInt:
        return assign_unsigned(static_cast<unsigned int>(0));
    case QMetaType::ULong:
        return assign_unsigned(static_cast<unsigned long>(0));
    case QMetaType::ULongLong:
        return assign_unsigned(static_cast<qulonglong>(0));
    default:
        return false;
    }
}

bool convert_variant_strict(
    const QVariant& src,
    const QMetaType& target,
    QVariant& out,
    QString& out_detail)
{
    const int target_id = target.id();
    if (target_id == QMetaType::QVariant) {
        out = src;
        return true;
    }

    if (!src.isValid()) {
        out_detail = QStringLiteral("expected %1, got null/invalid QVariant")
                         .arg(QString::fromUtf8(target.name() ? target.name() : "<unknown>"));
        return false;
    }

    const int src_id = src.typeId();
    if (src_id == target_id) {
        out = src;
        return true;
    }

    if (target_id == QMetaType::Bool) {
        if (src_id == QMetaType::Bool) {
            out = src;
            return true;
        }
    } else if (is_integral_type(target_id)) {
        if (convert_integral_checked(src, target, out, out_detail)) {
            return true;
        }
    } else if (is_floating_type(target_id)) {
        if (is_integral_type(src_id) || is_floating_type(src_id)) {
            bool ok = false;
            const double d = src.toDouble(&ok);
            if (ok) {
                out = (target_id == QMetaType::Float)
                    ? QVariant::fromValue(static_cast<float>(d))
                    : QVariant::fromValue(d);
                return true;
            }
        }
    } else if (target_id == QMetaType::QString) {
        if (src_id == QMetaType::QString) {
            out = src;
            return true;
        }
    } else if (target_id == QMetaType::QUrl) {
        if (src_id == QMetaType::QUrl) {
            out = src;
            return true;
        }
        if (src_id == QMetaType::QString) {
            out = QVariant::fromValue(QUrl(src.toString()));
            return true;
        }
    } else if (target_id == QMetaType::QStringList) {
        if (src_id == QMetaType::QVariantList) {
            const QVariantList list = src.toList();
            QStringList strs;
            strs.reserve(list.size());
            for (const QVariant& item : list) {
                if (item.typeId() != QMetaType::QString) {
                    out_detail = QStringLiteral("expected QStringList elements of type QString, got %1")
                                     .arg(QString::fromUtf8(item.typeName() ? item.typeName() : "<null>"));
                    return false;
                }
                strs.append(item.toString());
            }
            out = QVariant::fromValue(strs);
            return true;
        }
    } else if (target_id == QMetaType::QVariantList) {
        if (src_id == QMetaType::QStringList) {
            out = QVariant::fromValue(src.toList());
            return true;
        }
    } else if (target == QMetaType::fromType<QList<int>>()) {
        if (src_id == QMetaType::QVariantList) {
            const QVariantList list = src.toList();
            QList<int> ints;
            ints.reserve(list.size());
            for (const QVariant& item : list) {
                QVariant converted_item;
                if (!convert_integral_checked(
                        item, QMetaType::fromType<int>(), converted_item, out_detail)) {
                    if (out_detail.isEmpty()) {
                        out_detail = QStringLiteral("expected QList<int> elements of integral type, got %1")
                                         .arg(QString::fromUtf8(item.typeName() ? item.typeName() : "<null>"));
                    }
                    return false;
                }
                ints.append(converted_item.toInt());
            }
            out = QVariant::fromValue(ints);
            return true;
        }
    }

    if (out_detail.isEmpty()) {
        out_detail = QStringLiteral("expected %1, got %2")
                         .arg(
                             QString::fromUtf8(target.name() ? target.name() : "<unknown>"),
                             QString::fromUtf8(src.typeName() ? src.typeName() : "<invalid>"));
    }
    return false;
}

bool is_supported_method_metatype(const QMetaType& mt, bool allow_void_and_js)
{
    const int id = mt.id();
    if (allow_void_and_js && (id == QMetaType::Void || id == qMetaTypeId<QJSValue>())) {
        return true;
    }
    if (id == QMetaType::QVariant || id == QMetaType::Bool || is_integral_type(id)
        || is_floating_type(id) || id == QMetaType::QString || id == QMetaType::QUrl
        || id == QMetaType::QStringList || id == QMetaType::QVariantList
        || id == QMetaType::QVariantMap || mt == QMetaType::fromType<QList<int>>()) {
        return true;
    }
    return false;
}

QVariant normalize_qml_return_variant(const QVariant& val)
{
    if (val.userType() == qMetaTypeId<QJSValue>()) {
        return val.value<QJSValue>().toVariant();
    }
    if (val.typeId() == QMetaType::QVariantList) {
        const QVariantList list = val.toList();
        QVariantList normalized;
        normalized.reserve(list.size());
        for (const QVariant& elem : list) {
            normalized.append(normalize_qml_return_variant(elem));
        }
        return QVariant::fromValue(normalized);
    }
    if (val.typeId() == QMetaType::QVariantMap) {
        const QVariantMap map = val.toMap();
        QVariantMap normalized;
        for (auto it = map.cbegin(); it != map.cend(); ++it) {
            normalized.insert(it.key(), normalize_qml_return_variant(it.value()));
        }
        return QVariant::fromValue(normalized);
    }
    return val;
}

struct InvokeDepthRaii {
    InvokeDepthRaii() { ++s_active_invoke_depth; }
    ~InvokeDepthRaii() { --s_active_invoke_depth; }
};

struct MetaTypeBufferGuard {
    QMetaType type;
    void* ptr = nullptr;
    ~MetaTypeBufferGuard()
    {
        if (ptr) {
            type.destroy(ptr);
        }
    }
};

} // namespace

QVariant rinui_root_get_property_checked(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name,
    std::int32_t& out_status,
    QString& out_detail)
{
    out_detail.clear();
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->root.isNull()) {
        out_status = 1; // RootDisposed
        return QVariant();
    }
    if (it->session_id != session_id) {
        out_status = 2; // NotOwned
        return QVariant();
    }

    QObject* obj = it->root.data();
    const QByteArray prop_name = name.toUtf8();
    const QMetaObject* meta = obj->metaObject();
    const int prop_idx = meta ? meta->indexOfProperty(prop_name.constData()) : -1;
    if (prop_idx >= 0) {
        out_status = 0;
        return normalize_qml_return_variant(meta->property(prop_idx).read(obj));
    }

    if (obj->dynamicPropertyNames().contains(prop_name)) {
        out_status = 0;
        return normalize_qml_return_variant(obj->property(prop_name.constData()));
    }

    out_status = 3; // PropertyNotFound
    out_detail = QStringLiteral("property '%1' does not exist on root object").arg(name);
    return QVariant();
}

std::int32_t rinui_root_set_property_checked(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& name,
    const QVariant& value,
    QString& out_detail)
{
    out_detail.clear();
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->root.isNull()) {
        return 1; // RootDisposed
    }
    if (it->session_id != session_id) {
        return 2; // NotOwned
    }

    QObject* obj = it->root.data();
    const QByteArray prop_name = name.toUtf8();
    const QMetaObject* meta = obj->metaObject();
    const int prop_idx = meta ? meta->indexOfProperty(prop_name.constData()) : -1;
    if (prop_idx < 0) {
        out_detail = QStringLiteral("meta-property '%1' not declared on root object").arg(name);
        return 3; // PropertyNotFound (never creates dynamic property!)
    }

    const QMetaProperty prop = meta->property(prop_idx);
    if (!prop.isWritable()) {
        out_detail = QStringLiteral("property '%1' is read-only").arg(name);
        return 4; // PropertyReadOnly
    }

    QVariant converted;
    if (!convert_variant_strict(value, prop.metaType(), converted, out_detail)) {
        return 5; // TypeIncompatible
    }

    if (!prop.write(obj, converted)) {
        out_detail = QStringLiteral("QMetaProperty::write returned false for '%1'").arg(name);
        return 6; // WriteFailed
    }
    return 0;
}

QVariant rinui_root_invoke_method(
    std::uint64_t session_id,
    std::uint64_t root_id,
    const QString& method_name,
    rust::Slice<const QVariant> args,
    std::int32_t& out_status,
    QString& out_detail)
{
    out_detail.clear();
    if (args.size() > 10) {
        out_status = 7; // TooManyArguments
        return QVariant();
    }

    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->root.isNull()) {
        out_status = 1; // RootDisposed
        return QVariant();
    }
    if (it->session_id != session_id) {
        out_status = 2; // NotOwned
        return QVariant();
    }

    QPointer<QObject> target(it->root.data());
    const QMetaObject* meta = target->metaObject();
    if (!meta) {
        out_status = 3;
        return QVariant();
    }

    const QByteArray utf8_name = method_name.toUtf8();
    QMetaMethod matched_method;
    bool found_name = false;
    QStringList available_arities;

    for (int i = meta->methodCount() - 1; i >= 0; --i) {
        const QMetaMethod m = meta->method(i);
        if (m.methodType() != QMetaMethod::Method && m.methodType() != QMetaMethod::Slot) {
            continue;
        }
        if (m.name() != utf8_name) {
            continue;
        }
        found_name = true;
        available_arities.append(QString::number(m.parameterCount()));
        if (m.parameterCount() == static_cast<int>(args.size())) {
            matched_method = m;
            break;
        }
    }

    if (!found_name) {
        out_status = 3; // MethodNotFound
        out_detail = QStringLiteral("no invokable method or slot named '%1'").arg(method_name);
        return QVariant();
    }
    if (!matched_method.isValid()) {
        out_status = 4; // ArgumentCountMismatch
        out_detail = QStringLiteral("expected arity [%1], got %2")
                         .arg(available_arities.join(QStringLiteral(", ")))
                         .arg(args.size());
        return QVariant();
    }

    const QMetaType ret_type = matched_method.returnMetaType();
    if (!is_supported_method_metatype(ret_type, true)) {
        out_status = 5; // UnsupportedSignature
        out_detail = QStringLiteral("unsupported return type '%1'")
                         .arg(QString::fromUtf8(ret_type.name() ? ret_type.name() : "<unknown>"));
        return QVariant();
    }

    std::vector<QMetaType> param_types;
    std::vector<QVariant> converted_args;
    param_types.reserve(args.size());
    converted_args.reserve(args.size());

    for (std::size_t i = 0; i < args.size(); ++i) {
        const QMetaType pt = matched_method.parameterMetaType(static_cast<int>(i));
        if (!is_supported_method_metatype(pt, false)) {
            out_status = 5; // UnsupportedSignature
            out_detail = QStringLiteral("unsupported parameter #%1 type '%2'")
                             .arg(i)
                             .arg(QString::fromUtf8(pt.name() ? pt.name() : "<unknown>"));
            return QVariant();
        }
        QVariant conv;
        if (!convert_variant_strict(args[i], pt, conv, out_detail)) {
            out_status = 5; // UnsupportedSignature / incompatible arg
            out_detail = QStringLiteral("argument #%1 mismatch: %2").arg(i).arg(out_detail);
            return QVariant();
        }
        param_types.push_back(pt);
        converted_args.push_back(std::move(conv));
    }

    QVariant ret_qvariant;
    MetaTypeBufferGuard concrete_ret_guard;

    InvokeDepthRaii depth_guard;

#if QT_VERSION >= QT_VERSION_CHECK(6, 5, 0)
    QTemplatedMetaMethodReturnArgument<void> ret_arg{};
    if (ret_type.id() == QMetaType::QVariant) {
        const QMetaType var_mt = QMetaType::fromType<QVariant>();
        ret_arg = QTemplatedMetaMethodReturnArgument<void>{var_mt.iface(), var_mt.name(), &ret_qvariant};
    } else if (ret_type.id() != QMetaType::Void) {
        concrete_ret_guard.type = ret_type;
        concrete_ret_guard.ptr = ret_type.create();
        ret_arg = QTemplatedMetaMethodReturnArgument<void>{
            ret_type.iface(), ret_type.name(), concrete_ret_guard.ptr};
    }

    auto make_arg = [&](std::size_t idx) -> QMetaMethodArgument {
        if (param_types[idx].id() == QMetaType::QVariant) {
            const QMetaType var_mt = QMetaType::fromType<QVariant>();
            return QMetaMethodArgument{var_mt.iface(), var_mt.name(), &converted_args[idx]};
        }
        return QMetaMethodArgument{
            param_types[idx].iface(), param_types[idx].name(), converted_args[idx].constData()};
    };

    bool ok = false;
    switch (args.size()) {
    case 0:
        ok = matched_method.invoke(target.data(), Qt::DirectConnection, ret_arg);
        break;
    case 1:
        ok = matched_method.invoke(target.data(), Qt::DirectConnection, ret_arg, make_arg(0));
        break;
    case 2:
        ok = matched_method.invoke(
            target.data(), Qt::DirectConnection, ret_arg, make_arg(0), make_arg(1));
        break;
    case 3:
        ok = matched_method.invoke(
            target.data(), Qt::DirectConnection, ret_arg, make_arg(0), make_arg(1), make_arg(2));
        break;
    case 4:
        ok = matched_method.invoke(
            target.data(),
            Qt::DirectConnection,
            ret_arg,
            make_arg(0),
            make_arg(1),
            make_arg(2),
            make_arg(3));
        break;
    case 5:
        ok = matched_method.invoke(
            target.data(),
            Qt::DirectConnection,
            ret_arg,
            make_arg(0),
            make_arg(1),
            make_arg(2),
            make_arg(3),
            make_arg(4));
        break;
    case 6:
        ok = matched_method.invoke(
            target.data(),
            Qt::DirectConnection,
            ret_arg,
            make_arg(0),
            make_arg(1),
            make_arg(2),
            make_arg(3),
            make_arg(4),
            make_arg(5));
        break;
    case 7:
        ok = matched_method.invoke(
            target.data(),
            Qt::DirectConnection,
            ret_arg,
            make_arg(0),
            make_arg(1),
            make_arg(2),
            make_arg(3),
            make_arg(4),
            make_arg(5),
            make_arg(6));
        break;
    case 8:
        ok = matched_method.invoke(
            target.data(),
            Qt::DirectConnection,
            ret_arg,
            make_arg(0),
            make_arg(1),
            make_arg(2),
            make_arg(3),
            make_arg(4),
            make_arg(5),
            make_arg(6),
            make_arg(7));
        break;
    case 9:
        ok = matched_method.invoke(
            target.data(),
            Qt::DirectConnection,
            ret_arg,
            make_arg(0),
            make_arg(1),
            make_arg(2),
            make_arg(3),
            make_arg(4),
            make_arg(5),
            make_arg(6),
            make_arg(7),
            make_arg(8));
        break;
    case 10:
        ok = matched_method.invoke(
            target.data(),
            Qt::DirectConnection,
            ret_arg,
            make_arg(0),
            make_arg(1),
            make_arg(2),
            make_arg(3),
            make_arg(4),
            make_arg(5),
            make_arg(6),
            make_arg(7),
            make_arg(8),
            make_arg(9));
        break;
    default:
        out_status = 7;
        return QVariant();
    }
#else
    QGenericReturnArgument ret_arg;
    if (ret_type.id() == QMetaType::QVariant) {
        ret_arg = QGenericReturnArgument("QVariant", &ret_qvariant);
    } else if (ret_type.id() != QMetaType::Void) {
        concrete_ret_guard.type = ret_type;
        concrete_ret_guard.ptr = ret_type.create();
        ret_arg = QGenericReturnArgument(ret_type.name(), concrete_ret_guard.ptr);
    }

    auto make_arg = [&](std::size_t idx) -> QGenericArgument {
        if (idx >= args.size()) {
            return QGenericArgument();
        }
        if (param_types[idx].id() == QMetaType::QVariant) {
            return QGenericArgument("QVariant", &converted_args[idx]);
        }
        return QGenericArgument(param_types[idx].name(), converted_args[idx].constData());
    };

    const bool ok = matched_method.invoke(
        target.data(),
        Qt::DirectConnection,
        ret_arg,
        make_arg(0),
        make_arg(1),
        make_arg(2),
        make_arg(3),
        make_arg(4),
        make_arg(5),
        make_arg(6),
        make_arg(7),
        make_arg(8),
        make_arg(9));
#endif

    if (!ok) {
        out_status = 6; // InvocationFailed
        out_detail = QStringLiteral("QMetaMethod::invoke returned false for '%1'").arg(method_name);
        return QVariant();
    }

    out_status = 0;
    if (ret_type.id() == QMetaType::Void) {
        return QVariant();
    }
    if (ret_type.id() == QMetaType::QVariant) {
        return normalize_qml_return_variant(ret_qvariant);
    }
    QVariant wrapped(ret_type, concrete_ret_guard.ptr);
    return normalize_qml_return_variant(wrapped);
}

bool rinui_window_is_alive_owned(std::uint64_t session_id, std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    return it != s_windows.cend() && it->session_id == session_id && !it->window.isNull();
}

std::size_t rinui_window_win_id_owned(std::uint64_t session_id, std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    if (it == s_windows.cend() || it->session_id != session_id || it->window.isNull()) {
        return 0;
    }
    return static_cast<std::size_t>(it->window->winId());
}

QVariant rinui_window_get_property_owned(
    std::uint64_t session_id,
    std::uint64_t window_id,
    const QString& name)
{
    const auto it = s_windows.constFind(window_id);
    if (it == s_windows.cend() || it->session_id != session_id || it->window.isNull()) {
        return QVariant();
    }
    const QByteArray prop = name.toUtf8();
    return it->window->property(prop.constData());
}

bool rinui_window_set_property_owned(
    std::uint64_t session_id,
    std::uint64_t window_id,
    const QString& name,
    const QVariant& value)
{
    const auto it = s_windows.constFind(window_id);
    if (it == s_windows.cend() || it->session_id != session_id || it->window.isNull()) {
        return false;
    }
    const QByteArray prop = name.toUtf8();
    return it->window->setProperty(prop.constData(), value);
}

void rinui_window_show_owned(std::uint64_t session_id, std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    if (it != s_windows.cend() && it->session_id == session_id && !it->window.isNull()) {
        it->window->show();
    }
}

void rinui_window_close_owned(std::uint64_t session_id, std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    if (it != s_windows.cend() && it->session_id == session_id && !it->window.isNull()) {
        it->window->close();
    }
}

void rinui_set_app_icon_owned(
    const QString& icon_path,
    std::uint64_t session_id,
    std::uint64_t root_id,
    bool set_app_icon)
{
    QString icon_fs_or_qrc = icon_path;
    QUrl qml_url;
    if (icon_path.startsWith(QStringLiteral("qrc:/"))) {
        icon_fs_or_qrc = icon_path.mid(3); // ":/..."
        qml_url = QUrl(icon_path);
    } else if (icon_path.startsWith(QStringLiteral(":/"))) {
        qml_url = QUrl(QStringLiteral("qrc") + icon_path);
    } else {
        qml_url = QUrl::fromLocalFile(QFileInfo(icon_path).absoluteFilePath());
    }

    if (set_app_icon) {
        QGuiApplication::setWindowIcon(QIcon(icon_fs_or_qrc));
    }
    if (root_id != 0) {
        rinui_root_set_property_owned(
            session_id, root_id, QStringLiteral("icon"), QVariant::fromValue(qml_url));
    }
}

void rinui_engine_retranslate(QQmlApplicationEngine& engine)
{
    engine.retranslate();

    QSet<QObject*> seen;
    QList<QPointer<QObject>> targets;
    auto append_target = [&](QObject* obj) {
        if (obj && !seen.contains(obj)) {
            seen.insert(obj);
            targets.append(QPointer<QObject>(obj));
        }
    };

    const auto roots = engine.rootObjects();
    for (QObject* root : roots) {
        if (!root || !is_root_already_registered(root)) {
            continue;
        }
        append_target(root);
        const auto items = root->findChildren<QQuickItem*>();
        for (QQuickItem* item : items) {
            append_target(item);
        }
    }

    const std::uint64_t eid = s_id_by_engine.value(static_cast<const QQmlEngine*>(&engine), 0);
    if (eid != 0) {
        for (auto it = s_windows.cbegin(); it != s_windows.cend(); ++it) {
            if (it->engine_id == eid && !it->window.isNull()) {
                QQuickWindow* win = it->window.data();
                append_target(win);
                const auto win_items = win->findChildren<QQuickItem*>();
                for (QQuickItem* item : win_items) {
                    append_target(item);
                }
                if (QQuickItem* content = win->contentItem()) {
                    append_target(content);
                    const auto content_items = content->findChildren<QQuickItem*>();
                    for (QQuickItem* item : content_items) {
                        append_target(item);
                    }
                }
            }
        }
    }

    for (const auto& target : targets) {
        if (!target.isNull()) {
            QEvent ev(QEvent::LocaleChange);
            QCoreApplication::sendEvent(target.data(), &ev);
        }
    }
}

void rinui_engine_set_extra_file_selectors(
    QQmlApplicationEngine& engine,
    const QStringList& selectors)
{
    engine.setExtraFileSelectors(selectors);
}

void rinui_engine_clear_component_cache(QQmlApplicationEngine& engine)
{
    engine.clearComponentCache();
}

void rinui_engine_collect_garbage(QQmlApplicationEngine& engine)
{
    engine.collectGarbage();
}

namespace {

// Qt 6.9.x returns false from QCoreApplication::installTranslator() for an
// empty catalog even though the translator is installed (Qt >= 6.10 returns
// true). An empty catalog is a valid no-op: upstream RinUI's en_US.qm is
// header-only because English source strings require no translation.
bool install_translator_or_empty(QTranslator* translator)
{
    const bool installed = QCoreApplication::installTranslator(translator);
    return installed || translator->isEmpty();
}

} // namespace

bool rinui_translator_apply_transaction(
    const QString& active_locale,
    const QString& rinui_qm_path,
    const QStringList& app_catalog_ids,
    const QStringList& app_qm_paths,
    QString& out_failed_path)
{
    out_failed_path.clear();
    QCoreApplication* app = QCoreApplication::instance();
    if (!app) {
        out_failed_path = rinui_qm_path;
        return false;
    }
    if (app_catalog_ids.size() != app_qm_paths.size()) {
        out_failed_path = QStringLiteral("<catalog-argument-mismatch>");
        return false;
    }

    // Phase 1: Load all replacement catalogs into temporary QTranslator objects
    // before mutating any installed translator or QLocale::default().
    auto next_rinui = std::make_unique<QTranslator>();
    if (!next_rinui->load(rinui_qm_path)) {
        out_failed_path = rinui_qm_path;
        return false;
    }

    struct LoadedAppCatalog {
        std::string id;
        QString path;
        std::unique_ptr<QTranslator> translator;
    };
    std::vector<LoadedAppCatalog> loaded_apps;
    loaded_apps.reserve(static_cast<std::size_t>(app_catalog_ids.size()));

    for (qsizetype i = 0; i < app_catalog_ids.size(); ++i) {
        const QString& qm_path = app_qm_paths.at(i);
        if (qm_path.isEmpty()) {
            continue;
        }
        auto next_app = std::make_unique<QTranslator>();
        if (!next_app->load(qm_path)) {
            out_failed_path = qm_path;
            return false;
        }
        loaded_apps.push_back(LoadedAppCatalog{
            app_catalog_ids.at(i).toStdString(),
            qm_path,
            std::move(next_app),
        });
    }

    // Phase 2: Swap translators atomically with full rollback if installation fails.
    std::unique_ptr<QTranslator> prev_rinui = std::move(s_rinui_translator);
    if (prev_rinui) {
        QCoreApplication::removeTranslator(prev_rinui.get());
    }

    std::unordered_map<std::string, std::unique_ptr<QTranslator>> prev_apps =
        std::move(s_app_translators);
    s_app_translators.clear();
    for (auto& [id, prev_t] : prev_apps) {
        Q_UNUSED(id);
        if (prev_t) {
            QCoreApplication::removeTranslator(prev_t.get());
        }
    }

    auto rollback = [&]() {
        if (s_rinui_translator) {
            QCoreApplication::removeTranslator(s_rinui_translator.get());
            s_rinui_translator.reset();
        }
        for (auto& [id, new_t] : s_app_translators) {
            Q_UNUSED(id);
            if (new_t) {
                QCoreApplication::removeTranslator(new_t.get());
            }
        }
        s_app_translators.clear();

        if (prev_rinui) {
            install_translator_or_empty(prev_rinui.get());
            s_rinui_translator = std::move(prev_rinui);
        }
        for (auto& [id, prev_t] : prev_apps) {
            if (prev_t) {
                install_translator_or_empty(prev_t.get());
                s_app_translators.emplace(id, std::move(prev_t));
            }
        }
    };

    if (!install_translator_or_empty(next_rinui.get())) {
        out_failed_path = rinui_qm_path;
        rollback();
        return false;
    }
    s_rinui_translator = std::move(next_rinui);

    for (auto& entry : loaded_apps) {
        if (!install_translator_or_empty(entry.translator.get())) {
            out_failed_path = entry.path;
            rollback();
            return false;
        }
        s_app_translators.emplace(std::move(entry.id), std::move(entry.translator));
    }

    QLocale::setDefault(QLocale(active_locale));
    return true;
}

void rinui_translator_clear_all()
{
    if (s_rinui_translator) {
        if (QCoreApplication::instance()) {
            QCoreApplication::removeTranslator(s_rinui_translator.get());
        }
        s_rinui_translator.reset();
    }

    for (auto& [id, translator] : s_app_translators) {
        Q_UNUSED(id);
        if (translator && QCoreApplication::instance()) {
            QCoreApplication::removeTranslator(translator.get());
        }
    }
    s_app_translators.clear();
}

void rinui_platform_apply_theme(const QString& actual_theme, const QString& configured_theme)
{
    rinui::platform::apply_theme(actual_theme, configured_theme);
}

std::int32_t rinui_platform_apply_backdrop(
    const QString& effect_type,
    bool is_dark,
    std::uint32_t win10_light,
    std::uint32_t win10_dark)
{
    return rinui::platform::apply_backdrop(effect_type, is_dark, win10_light, win10_dark);
}

bool rinui_qt_file_exists(const QString& path)
{
    return QFileInfo::exists(path);
}

void rinui_set_clipboard_text(const QString& text)
{
    if (auto* clipboard = QGuiApplication::clipboard()) {
        clipboard->setText(text);
    }
}

void rinui_schedule_quit_after_ms(std::int32_t delay_ms)
{
    QTimer::singleShot(delay_ms < 0 ? 0 : delay_ms, QCoreApplication::instance(), []() {
        QCoreApplication::quit();
    });
}
