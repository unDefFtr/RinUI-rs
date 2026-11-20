#include "rinui_platform_hooks.h"

#import <Cocoa/Cocoa.h>

#include <QGuiApplication>
#include <QHash>
#include <QLoggingCategory>
#include <QMouseEvent>
#include <QPointer>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>

#include <cmath>
#include <utility>
#include <vector>

namespace {

Q_LOGGING_CATEGORY(lcMacTitleBar, "rinui.mac.titlebar", QtWarningMsg)

// ExpandedClientAreaHint puts the QML titlebar inside the content view. RinUI
// disables its QML drag MouseArea in native-frame mode, so forward presses on
// the empty titlebar to the platform move operation ourselves. Inspect the
// complete item tree: NavigationBar's back button is outside titleBarHost.
bool interactive_item_at(QQuickItem* item, const QPointF& scene_position)
{
    // MouseArea shadows QQuickItem's non-virtual enabled accessor. Its QML
    // property must also be checked, otherwise the disabled upstream drag
    // area is incorrectly treated as an interactive control covering the bar.
    if (!item || !item->isVisible() || !item->isEnabled()
        || !item->property("enabled").toBool()) {
        return false;
    }
    const QPointF local = item->mapFromScene(scene_position);
    const bool contains = item->contains(local);
    if (item->clip() && !contains) {
        return false;
    }
    for (auto* child : item->childItems()) {
        if (interactive_item_at(child, scene_position)) {
            return true;
        }
    }
    // Qt Quick Controls' content container accepts unhandled presses to stop
    // propagation. Its children, rather than this full-window shield, define
    // the actual interactive regions.
    if (item->inherits("QQuickContentItem")) {
        return false;
    }
    const bool interactive = contains && (item->acceptedMouseButtons() & Qt::LeftButton);
    if (interactive) {
        qCDebug(lcMacTitleBar) << "Interactive titlebar item" << item;
    }
    return interactive;
}

void apply_macos_window_style(QQuickWindow* window);

class MacTitleBarInputFilter final : public QObject {
public:
    explicit MacTitleBarInputFilter(QQuickWindow* window)
        : QObject(window), m_window(window)
    {
        window->installEventFilter(this);
    }

protected:
    bool eventFilter(QObject* watched, QEvent* event) override
    {
        auto* window = m_window.data();
        if (window && watched == window
            && (event->type() == QEvent::Show || event->type() == QEvent::Expose)
            && window->property("useNativeMacFrame").toBool()
            && window->property("_rinuiMacFrameDiagnostic").toString()
                == QStringLiteral("fallback:no_nswindow")) {
            apply_macos_window_style(window);
        }
        if (event->type() == QEvent::MouseButtonPress || event->type() == QEvent::MouseButtonDblClick
            || event->type() == QEvent::NonClientAreaMouseButtonPress) {
            qCDebug(lcMacTitleBar) << "Titlebar input" << event;
        }
        if (!window || watched != window || !window->property("useNativeMacFrame").toBool()
            || window->visibility() == QWindow::FullScreen) {
            m_fallback_drag = false;
            return false;
        }

        if (event->type() == QEvent::Hide || event->type() == QEvent::WindowDeactivate
            || event->type() == QEvent::UngrabMouse) {
            m_fallback_drag = false;
        }
        if (m_fallback_drag && event->type() == QEvent::MouseMove) {
            const auto* mouse = static_cast<QMouseEvent*>(event);
            if (!(mouse->buttons() & Qt::LeftButton)) {
                m_fallback_drag = false;
                return false;
            }
            window->setPosition(m_drag_origin + (mouse->globalPosition() - m_press_position).toPoint());
            return true;
        }
        if (m_fallback_drag && event->type() == QEvent::MouseButtonRelease
            && static_cast<QMouseEvent*>(event)->button() == Qt::LeftButton) {
            m_fallback_drag = false;
            return true;
        }
        if (event->type() != QEvent::MouseButtonPress
            && event->type() != QEvent::MouseButtonDblClick) {
            return false;
        }

        const auto* mouse = static_cast<QMouseEvent*>(event);
        const QPointF position = mouse->position();
        const qreal height = window->property("titleBarHeight").toReal();
        if (mouse->button() != Qt::LeftButton || height <= 0.0
            || position.x() < 0.0 || position.x() >= window->width()
            || position.y() < 0.0 || position.y() >= height
            || interactive_item_at(window->contentItem(), position)) {
            return false;
        }

        if (event->type() == QEvent::MouseButtonDblClick) {
            m_fallback_drag = false;
            if (window->property("maximizeEnabled").toBool()) {
                if (window->visibility() == QWindow::Maximized) {
                    window->showNormal();
                } else {
                    window->showMaximized();
                }
            }
            return true;
        }
        const bool moving = window->startSystemMove();
        qCDebug(lcMacTitleBar) << "System move requested" << moving;
        // Some QPA/event sources cannot start a native move. Keep dragging
        // functional using global Qt coordinates instead of swallowing it.
        m_fallback_drag = !moving;
        if (m_fallback_drag) {
            m_press_position = mouse->globalPosition();
            m_drag_origin = window->position();
        }
        return true;
    }

private:
    QPointer<QQuickWindow> m_window;
    bool m_fallback_drag = false;
    QPointF m_press_position;
    QPoint m_drag_origin;
};

struct MacWindowState {
    QPointer<QQuickWindow> window;
    QPointer<QObject> input_filter;
    QMetaObject::Connection visible_conn;
    int offset_x = 10;
    int offset_down = 10;
    bool shift_applied = false;
    bool has_last_origin = false;
    CGPoint last_set_origin = CGPointZero;
    void* observed_ns_window = nullptr;
    std::vector<id> observers;
};

constexpr int kMaxTrafficLightRetries = 8;
constexpr int kTrafficLightRetryIntervalMs = 50;

QHash<QQuickWindow*, MacWindowState> s_mac_windows;
QString s_last_actual_theme = QStringLiteral("Light");
QString s_last_configured_theme = QStringLiteral("Auto");

bool is_cocoa_platform()
{
    return QGuiApplication::platformName() == QStringLiteral("cocoa");
}

NSWindow* resolve_ns_window(QQuickWindow* window)
{
    if (!window || !is_cocoa_platform()) {
        return nil;
    }
    const WId wid = window->winId();
    if (!wid) {
        return nil;
    }
    NSView* ns_view = (__bridge NSView*)reinterpret_cast<void*>(wid);
    if (!ns_view || ![ns_view isKindOfClass:[NSView class]]) {
        return nil;
    }
    return [ns_view window];
}

void apply_appearance_to_ns_window(NSWindow* ns_window, const QString& actual_theme, const QString& configured_theme)
{
    if (!ns_window) {
        return;
    }
    if (configured_theme == QStringLiteral("Auto")) {
        [ns_window setAppearance:nil];
        return;
    }
    if (actual_theme == QStringLiteral("Dark")) {
        [ns_window setAppearance:[NSAppearance appearanceNamed:NSAppearanceNameDarkAqua]];
    } else {
        [ns_window setAppearance:[NSAppearance appearanceNamed:NSAppearanceNameAqua]];
    }
}

bool shift_traffic_lights_once(QQuickWindow* window, MacWindowState& state)
{
    NSWindow* ns_window = resolve_ns_window(window);
    if (!ns_window) {
        return false;
    }

    NSButton* close_btn = [ns_window standardWindowButton:NSWindowCloseButton];
    NSButton* min_btn = [ns_window standardWindowButton:NSWindowMiniaturizeButton];
    NSButton* zoom_btn = [ns_window standardWindowButton:NSWindowZoomButton];

    NSButton* first_btn = close_btn ? close_btn : (min_btn ? min_btn : zoom_btn);
    if (!first_btn) {
        return false;
    }

    // Move the shared titlebar button container first to preserve native spacing.
    NSView* button_host = [first_btn superview];
    if (button_host) {
        const NSRect frame = [button_host frame];
        if (state.has_last_origin
            && std::abs(frame.origin.x - state.last_set_origin.x) < 0.5
            && std::abs(frame.origin.y - state.last_set_origin.y) < 0.5) {
            return true;
        }
        const NSPoint target = NSMakePoint(
            frame.origin.x + static_cast<CGFloat>(state.offset_x),
            frame.origin.y - static_cast<CGFloat>(state.offset_down));
        [button_host setFrameOrigin:target];
        state.last_set_origin = target;
        state.has_last_origin = true;
        return true;
    }

    NSButton* buttons[] = {close_btn, min_btn, zoom_btn};
    for (NSButton* btn : buttons) {
        if (!btn) {
            continue;
        }
        const NSRect frame = [btn frame];
        [btn setFrameOrigin:NSMakePoint(
            frame.origin.x + static_cast<CGFloat>(state.offset_x),
            frame.origin.y - static_cast<CGFloat>(state.offset_down))];
    }
    return true;
}

void schedule_traffic_light_shift(QQuickWindow* window, int retry_count)
{
    auto it = s_mac_windows.find(window);
    if (it == s_mac_windows.end() || it->shift_applied) {
        return;
    }

    if (shift_traffic_lights_once(window, *it)) {
        it->shift_applied = true;
        return;
    }

    if (retry_count >= kMaxTrafficLightRetries) {
        return;
    }

    QPointer<QQuickWindow> safe_win(window);
    QTimer::singleShot(kTrafficLightRetryIntervalMs, window, [safe_win, next_retry = retry_count + 1]() {
        if (!safe_win.isNull()) {
            schedule_traffic_light_shift(safe_win.data(), next_retry);
        }
    });
}

void remove_observers(MacWindowState& state)
{
    NSNotificationCenter* center = [NSNotificationCenter defaultCenter];
    for (id token : state.observers) {
        if (token) {
            [center removeObserver:token];
        }
    }
    state.observers.clear();
    state.observed_ns_window = nullptr;
}

void install_ns_window_observers(QQuickWindow* window, NSWindow* ns_window, MacWindowState& state)
{
    void* current_ns_window = (__bridge void*)ns_window;
    if (!ns_window) {
        return;
    }
    if (!state.observers.empty()) {
        if (state.observed_ns_window == current_ns_window) {
            return;
        }
        remove_observers(state);
    }

    state.observed_ns_window = current_ns_window;
    NSNotificationCenter* center = [NSNotificationCenter defaultCenter];
    QPointer<QQuickWindow> safe_win(window);

    id resize_obs = [center
        addObserverForName:NSWindowDidResizeNotification
                    object:ns_window
                     queue:[NSOperationQueue mainQueue]
                usingBlock:^(NSNotification*) {
                    if (safe_win.isNull()) {
                        return;
                    }
                    auto it = s_mac_windows.find(safe_win.data());
                    if (it != s_mac_windows.end()) {
                        it->shift_applied = false;
                        schedule_traffic_light_shift(safe_win.data(), 0);
                    }
                }];
    state.observers.push_back(resize_obs);

    id exit_fs_obs = [center
        addObserverForName:NSWindowDidExitFullScreenNotification
                    object:ns_window
                     queue:[NSOperationQueue mainQueue]
                usingBlock:^(NSNotification*) {
                    if (safe_win.isNull()) {
                        return;
                    }
                    auto it = s_mac_windows.find(safe_win.data());
                    if (it != s_mac_windows.end()) {
                        it->shift_applied = false;
                        it->has_last_origin = false;
                        schedule_traffic_light_shift(safe_win.data(), 0);
                    }
                }];
    state.observers.push_back(exit_fs_obs);
}

void set_mac_frame_diagnostic(QQuickWindow* window, const QString& diagnostic)
{
    if (window) {
        window->setProperty("_rinuiMacFrameDiagnostic", diagnostic);
    }
}

void apply_macos_window_style(QQuickWindow* window)
{
    auto it = s_mac_windows.find(window);
    if (it == s_mac_windows.end()) {
        return;
    }

    if (!window->property("useNativeMacFrame").toBool()) {
        set_mac_frame_diagnostic(window, QStringLiteral("self_drawn:qml_disabled"));
        return;
    }

    NSWindow* ns_window = resolve_ns_window(window);
    if (!ns_window) {
        qCWarning(lcMacTitleBar)
            << "Native macOS frame requested but NSWindow handle is not ready for" << window
            << "(QPA:" << QGuiApplication::platformName() << ")";
        set_mac_frame_diagnostic(window, QStringLiteral("fallback:no_nswindow"));
        return;
    }

    [ns_window setTitleVisibility:NSWindowTitleHidden];
    [ns_window setTitlebarAppearsTransparent:YES];
    [ns_window setMovableByWindowBackground:NO];
    const NSWindowStyleMask mask =
        [ns_window styleMask] | NSWindowStyleMaskFullSizeContentView;
    [ns_window setStyleMask:mask];

    apply_appearance_to_ns_window(ns_window, s_last_actual_theme, s_last_configured_theme);
    install_ns_window_observers(window, ns_window, *it);
    schedule_traffic_light_shift(window, 0);
    set_mac_frame_diagnostic(window, QStringLiteral("native_cocoa"));
    qCInfo(lcMacTitleBar) << "Applied native macOS NSWindow frame for" << window;
}

} // namespace

namespace rinui::platform {

void register_engine_objects(QQmlApplicationEngine&)
{
}

void set_theme_manager(QObject*)
{
}

void setup_window(QQuickWindow* window, bool native_mac_frame, int offset_x, int offset_down)
{
    if (!window) {
        return;
    }

    if (!native_mac_frame) {
        if (window->property("useNativeMacFrame").toBool()) {
            qCWarning(lcMacTitleBar)
                << "Overriding QML useNativeMacFrame=true to self-drawn mode post-load for"
                << window;
            window->setProperty("useNativeMacFrame", false);
            set_mac_frame_diagnostic(window, QStringLiteral("self_drawn:overridden_post_load"));
        } else {
            set_mac_frame_diagnostic(window, QStringLiteral("self_drawn"));
        }
        return;
    }

    if (!is_cocoa_platform()) {
        // Headless/offscreen QPA platforms do not back WId with NSView*. Keep useNativeMacFrame
        // intact so QML bindings (such as FluentWindow.titleBarHeight) remain inspectable.
        qCWarning(lcMacTitleBar)
            << "Native macOS frame requested under non-cocoa QPA platform"
            << QGuiApplication::platformName() << "for" << window;
        set_mac_frame_diagnostic(window, QStringLiteral("fallback:non_cocoa_qpa"));
        return;
    }

    if (!window->property("useNativeMacFrame").toBool()) {
        qCInfo(lcMacTitleBar)
            << "QML window opted out of useNativeMacFrame; keeping self-drawn mode for"
            << window;
        set_mac_frame_diagnostic(window, QStringLiteral("self_drawn:qml_disabled"));
        return;
    }

    if (!s_mac_windows.contains(window)) {
        MacWindowState state;
        state.window = QPointer<QQuickWindow>(window);
        state.input_filter = new MacTitleBarInputFilter(window);
        state.offset_x = offset_x;
        state.offset_down = offset_down;

        QPointer<QQuickWindow> safe_win(window);
        state.visible_conn = QObject::connect(window, &QQuickWindow::visibleChanged, window, [safe_win](bool visible) {
            if (safe_win.isNull() || !visible) {
                return;
            }
            if (!safe_win->property("useNativeMacFrame").toBool()) {
                return;
            }
            auto it = s_mac_windows.find(safe_win.data());
            if (it != s_mac_windows.end()) {
                it->shift_applied = false;
                it->has_last_origin = false;
                apply_macos_window_style(safe_win.data());
            }
        });
        s_mac_windows.insert(window, std::move(state));
    } else {
        auto it = s_mac_windows.find(window);
        it->offset_x = offset_x;
        it->offset_down = offset_down;
    }

    apply_macos_window_style(window);
}

void teardown_window(QQuickWindow* window)
{
    auto it = s_mac_windows.find(window);
    if (it == s_mac_windows.end()) {
        return;
    }
    if (it->visible_conn) {
        QObject::disconnect(it->visible_conn);
        it->visible_conn = QMetaObject::Connection{};
    }
    remove_observers(*it);
    delete it->input_filter.data();
    s_mac_windows.erase(it);
}

void teardown_windows_and_filters()
{
    for (auto it = s_mac_windows.begin(); it != s_mac_windows.end(); ++it) {
        if (it->visible_conn) {
            QObject::disconnect(it->visible_conn);
            it->visible_conn = QMetaObject::Connection{};
        }
        remove_observers(*it);
        delete it->input_filter.data();
    }
    s_mac_windows.clear();
}

void teardown_runtime_services()
{
    s_last_actual_theme = QStringLiteral("Light");
    s_last_configured_theme = QStringLiteral("Auto");
}

void teardown_all()
{
    teardown_windows_and_filters();
    teardown_runtime_services();
}

void apply_theme(const QString& actual_theme, const QString& configured_theme)
{
    s_last_actual_theme = actual_theme;
    s_last_configured_theme = configured_theme;

    if (!is_cocoa_platform()) {
        return;
    }

    for (auto it = s_mac_windows.cbegin(); it != s_mac_windows.cend(); ++it) {
        QQuickWindow* window = it->window.data();
        if (!window) {
            continue;
        }
        if (NSWindow* ns_window = resolve_ns_window(window)) {
            apply_appearance_to_ns_window(ns_window, actual_theme, configured_theme);
        }
    }
}

int32_t apply_backdrop(const QString&, bool, uint32_t, uint32_t)
{
    // Windows DWM/Acrylic backdrop APIs are unavailable on macOS.
    return -2;
}

} // namespace rinui::platform
