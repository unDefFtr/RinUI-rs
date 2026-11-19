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
#include <QPointer>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QStandardPaths>
#include <QStyleHints>
#include <QTextStream>
#include <QThread>
#include <QTimer>
#include <QTranslator>
#include <QUrl>

#include <memory>
#include <string>
#include <unordered_map>
#include <utility>

namespace {

struct RootRecord {
    QPointer<QObject> root;
    QList<std::uint64_t> window_ids;
};

std::uint64_t s_next_id = 1;
QHash<std::uint64_t, RootRecord> s_roots;
QHash<std::uint64_t, QPointer<QQuickWindow>> s_windows;
QHash<const QQmlApplicationEngine*, QStringList> s_engine_warnings;
std::unique_ptr<QTranslator> s_rinui_translator;
std::unordered_map<std::string, std::unique_ptr<QTranslator>> s_app_translators;

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
        if (it.value().data() == target) {
            return it.key();
        }
    }
    return 0;
}

std::uint64_t register_window_internal(
    QQuickWindow* win,
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
    s_windows.insert(wid, QPointer<QQuickWindow>(win));

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

bool rinui_is_on_app_thread()
{
    const QCoreApplication* app = QCoreApplication::instance();
    return !app || QThread::currentThread() == app->thread();
}

void rinui_flush_deferred_deletes()
{
    if (QCoreApplication::instance()) {
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }
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

void rinui_platform_register_engine_objects(QQmlApplicationEngine& engine)
{
    rinui::platform::register_engine_objects(engine);
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

std::uint64_t rinui_engine_register_latest_root(
    QQmlApplicationEngine& engine,
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down)
{
    const auto roots = engine.rootObjects();
    if (roots.isEmpty()) {
        return 0;
    }

    QObject* root = roots.last();
    if (!root) {
        return 0;
    }

    const std::uint64_t root_id = s_next_id++;
    RootRecord rec;
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
                win, native_mac_frame, mac_offset_x, mac_offset_down);
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

rust::Vec<std::uint64_t> rinui_root_windows(std::uint64_t root_id)
{
    rust::Vec<std::uint64_t> out;
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend()) {
        return out;
    }
    for (std::uint64_t wid : it->window_ids) {
        if (rinui_window_is_alive(wid)) {
            out.push_back(wid);
        }
    }
    return out;
}

rust::Vec<std::uint64_t> rinui_sync_top_level_windows(
    bool native_mac_frame,
    std::int32_t mac_offset_x,
    std::int32_t mac_offset_down)
{
    rust::Vec<std::uint64_t> discovered;
    if (!QCoreApplication::instance()) {
        return discovered;
    }
    const auto top_windows = QGuiApplication::topLevelWindows();
    for (QWindow* qw : top_windows) {
        auto* quick_win = qobject_cast<QQuickWindow*>(qw);
        if (!quick_win) {
            continue;
        }
        if (!quick_win->property("isRinUIWindow").toBool()) {
            continue;
        }
        const bool already = find_window_id(quick_win) != 0;
        const auto wid = register_window_internal(
            quick_win, native_mac_frame, mac_offset_x, mac_offset_down);
        if (wid != 0 && !already) {
            discovered.push_back(wid);
        }
    }
    return discovered;
}

rust::Vec<std::uint64_t> rinui_all_alive_windows()
{
    rust::Vec<std::uint64_t> out;
    for (auto it = s_windows.cbegin(); it != s_windows.cend(); ++it) {
        if (!it.value().isNull()) {
            out.push_back(it.key());
        }
    }
    return out;
}

void rinui_root_release(std::uint64_t root_id)
{
    const auto it = s_roots.find(root_id);
    if (it == s_roots.end()) {
        return;
    }
    RootRecord rec = std::move(it.value());
    s_roots.erase(it);

    bool root_already_torn_down = false;
    QObject* root_ptr = rec.root.data();

    for (std::uint64_t wid : rec.window_ids) {
        if (auto win = s_windows.take(wid); !win.isNull()) {
            if (static_cast<QObject*>(win.data()) == root_ptr) {
                root_already_torn_down = true;
            }
            rinui::platform::teardown_window(win.data());
            win->hide();
            win->releaseResources();
        }
    }

    if (!rec.root.isNull()) {
        QObject* root = rec.root.data();
        if (!root_already_torn_down) {
            if (auto* root_win = qobject_cast<QQuickWindow*>(root)) {
                rinui::platform::teardown_window(root_win);
                root_win->hide();
                root_win->releaseResources();
            }
        }
        root->deleteLater();
    }
}

void rinui_release_all_roots_and_platform()
{
    const auto keys = s_roots.keys();
    for (std::uint64_t root_id : keys) {
        rinui_root_release(root_id);
    }

    const auto win_keys = s_windows.keys();
    for (std::uint64_t wid : win_keys) {
        if (auto win = s_windows.take(wid); !win.isNull()) {
            rinui::platform::teardown_window(win.data());
        }
    }

    rinui::platform::teardown_all();
    rinui::platform::set_theme_manager(nullptr);
}

bool rinui_root_is_alive(std::uint64_t root_id)
{
    const auto it = s_roots.constFind(root_id);
    return it != s_roots.cend() && !it->root.isNull();
}

QVariant rinui_root_get_property(std::uint64_t root_id, const QString& name)
{
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->root.isNull()) {
        return QVariant();
    }
    const QByteArray prop = name.toUtf8();
    return it->root->property(prop.constData());
}

bool rinui_root_set_property(std::uint64_t root_id, const QString& name, const QVariant& value)
{
    const auto it = s_roots.constFind(root_id);
    if (it == s_roots.cend() || it->root.isNull()) {
        return false;
    }
    const QByteArray prop = name.toUtf8();
    return it->root->setProperty(prop.constData(), value);
}

bool rinui_window_is_alive(std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    return it != s_windows.cend() && !it.value().isNull();
}

std::size_t rinui_window_win_id(std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    if (it == s_windows.cend() || it.value().isNull()) {
        return 0;
    }
    return static_cast<std::size_t>(it.value()->winId());
}

QVariant rinui_window_get_property(std::uint64_t window_id, const QString& name)
{
    const auto it = s_windows.constFind(window_id);
    if (it == s_windows.cend() || it.value().isNull()) {
        return QVariant();
    }
    const QByteArray prop = name.toUtf8();
    return it.value()->property(prop.constData());
}

bool rinui_window_set_property(std::uint64_t window_id, const QString& name, const QVariant& value)
{
    const auto it = s_windows.constFind(window_id);
    if (it == s_windows.cend() || it.value().isNull()) {
        return false;
    }
    const QByteArray prop = name.toUtf8();
    return it.value()->setProperty(prop.constData(), value);
}

void rinui_window_show(std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    if (it != s_windows.cend() && !it.value().isNull()) {
        it.value()->show();
    }
}

void rinui_window_close(std::uint64_t window_id)
{
    const auto it = s_windows.constFind(window_id);
    if (it != s_windows.cend() && !it.value().isNull()) {
        it.value()->close();
    }
}

void rinui_set_app_icon(const QString& icon_path, std::uint64_t root_id)
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

    QGuiApplication::setWindowIcon(QIcon(icon_fs_or_qrc));
    if (root_id != 0) {
        rinui_root_set_property(root_id, QStringLiteral("icon"), QVariant::fromValue(qml_url));
    }
}

void rinui_engine_retranslate(QQmlApplicationEngine& engine)
{
    engine.retranslate();
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

bool rinui_translator_load_rinui(const QString& qm_path, const QString& locale_name)
{
    QCoreApplication* app = QCoreApplication::instance();
    if (!app) {
        return false;
    }

    auto next = std::make_unique<QTranslator>();
    if (!next->load(qm_path)) {
        return false;
    }

    std::unique_ptr<QTranslator> previous = std::move(s_rinui_translator);
    if (previous) {
        QCoreApplication::removeTranslator(previous.get());
    }

    if (!install_translator_or_empty(next.get())) {
        if (previous) {
            QCoreApplication::installTranslator(previous.get());
            s_rinui_translator = std::move(previous);
        }
        return false;
    }

    QLocale::setDefault(QLocale(locale_name));
    s_rinui_translator = std::move(next);
    return true;
}

bool rinui_translator_load_app(const QString& catalog_id, const QString& qm_path)
{
    QCoreApplication* app = QCoreApplication::instance();
    if (!app) {
        return false;
    }

    auto next = std::make_unique<QTranslator>();
    if (!next->load(qm_path)) {
        return false;
    }

    const std::string key = catalog_id.toStdString();
    std::unique_ptr<QTranslator> previous;
    if (auto it = s_app_translators.find(key); it != s_app_translators.end()) {
        previous = std::move(it->second);
        if (previous) {
            QCoreApplication::removeTranslator(previous.get());
        }
        s_app_translators.erase(it);
    }

    if (!install_translator_or_empty(next.get())) {
        if (previous) {
            QCoreApplication::installTranslator(previous.get());
            s_app_translators.emplace(key, std::move(previous));
        }
        return false;
    }

    s_app_translators.emplace(key, std::move(next));
    return true;
}

void rinui_translator_unload_app(const QString& catalog_id)
{
    const std::string key = catalog_id.toStdString();
    if (auto it = s_app_translators.find(key); it != s_app_translators.end()) {
        if (it->second && QCoreApplication::instance()) {
            QCoreApplication::removeTranslator(it->second.get());
        }
        s_app_translators.erase(it);
    }
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
