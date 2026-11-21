#include "rinui_platform_hooks.h"

namespace rinui::platform {

void register_engine_objects(QQmlApplicationEngine&)
{
}

void set_theme_manager(QObject*)
{
}

void setup_window(QQuickWindow*, bool, int, int)
{
    // On Linux (X11/Wayland), RinUI relies on Qt's FramelessWindowHint and startSystemResize()
    // compositor integration without custom native event filters.
}

void teardown_window(QQuickWindow*)
{
}

void teardown_windows_and_filters()
{
}

void teardown_runtime_services()
{
}

void teardown_all()
{
    teardown_windows_and_filters();
    teardown_runtime_services();
}

void apply_theme(const QString&, const QString&)
{
}

int32_t apply_backdrop(const QString&, bool, uint32_t, uint32_t)
{
    // Windows DWM/Acrylic backdrop APIs are unavailable on X11/Wayland compositors.
    return -2;
}

} // namespace rinui::platform
