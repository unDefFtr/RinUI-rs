#pragma once

#include <cstdint>
#include <QString>

class QObject;
class QQmlApplicationEngine;
class QQuickWindow;

namespace rinui::platform {

/// Register platform-specific QML context objects (e.g. WinEventManager on Windows).
void register_engine_objects(QQmlApplicationEngine& engine);

/// Store a non-owning pointer to the active ThemeManager so platform callbacks can sync backdrop/effects.
void set_theme_manager(QObject* theme_manager);

/// Set up native window integration for a discovered RinUI window.
void setup_window(QQuickWindow* window, bool native_mac_frame, int offset_x, int offset_down);

/// Tear down native window hooks/filters/observers for a single window before destruction.
void teardown_window(QQuickWindow* window);

/// Tear down per-window hooks, native event filters, and platform observers while engines may still exist.
void teardown_windows_and_filters();

/// Tear down runtime-lifetime platform services (such as WinEventManager) AFTER all QML engines are destroyed.
void teardown_runtime_services();

/// Tear down all platform hooks and runtime services before QGuiApplication exits.
void teardown_all();

/// Apply theme color mode to native window frames (e.g. DWM dark mode or NSWindow appearance).
void apply_theme(const QString& actual_theme, const QString& configured_theme);

/// Apply platform backdrop effect across registered windows.
/// Returns 0 on success, -1 if effect unsupported, -2 if platform unavailable or no windows.
int32_t apply_backdrop(
    const QString& effect_type,
    bool is_dark,
    uint32_t win10_light,
    uint32_t win10_dark);

} // namespace rinui::platform
