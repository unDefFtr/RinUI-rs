#include "rinui_platform_hooks.h"

#import <Cocoa/Cocoa.h>

#include <QGuiApplication>
#include <QHash>
#include <QPointer>
#include <QQuickWindow>
#include <QTimer>

#include <cmath>
#include <utility>
#include <vector>

namespace {

struct MacWindowState {
    QPointer<QQuickWindow> window;
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

void apply_macos_window_style(QQuickWindow* window)
{
    auto it = s_mac_windows.find(window);
    if (it == s_mac_windows.end()) {
        return;
    }

    if (!window->property("useNativeMacFrame").toBool()) {
        return;
    }

    NSWindow* ns_window = resolve_ns_window(window);
    if (!ns_window) {
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
            window->setProperty("useNativeMacFrame", false);
        }
        return;
    }

    if (!is_cocoa_platform()) {
        // Headless/offscreen QPA platforms do not back WId with NSView*.
        return;
    }

    if (!window->property("useNativeMacFrame").toBool()) {
        return;
    }

    if (!s_mac_windows.contains(window)) {
        MacWindowState state;
        state.window = QPointer<QQuickWindow>(window);
        state.offset_x = offset_x;
        state.offset_down = offset_down;
        s_mac_windows.insert(window, std::move(state));

        QPointer<QQuickWindow> safe_win(window);
        QObject::connect(window, &QQuickWindow::visibleChanged, window, [safe_win](bool visible) {
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
    remove_observers(*it);
    s_mac_windows.erase(it);
}

void teardown_all()
{
    for (auto it = s_mac_windows.begin(); it != s_mac_windows.end(); ++it) {
        remove_observers(*it);
    }
    s_mac_windows.clear();
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
