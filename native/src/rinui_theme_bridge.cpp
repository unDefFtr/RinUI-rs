#include "rinui_theme_bridge.h"
#include "rinui_platform_hooks.h"
#include "rinui-host/src/theme.cxxqt.h"

#include <QGuiApplication>
#include <QStyleHints>

std::unique_ptr<rinui::ThemeManager> rinui_theme_manager_new()
{
    return std::make_unique<rinui::ThemeManager>();
}

void rinui_connect_color_scheme_changed(rinui::ThemeManager& manager)
{
    rinui::platform::set_theme_manager(&manager);

    if (auto* hints = QGuiApplication::styleHints()) {
        QObject::connect(
            hints,
            &QStyleHints::colorSchemeChanged,
            &manager,
            &rinui::ThemeManager::handle_system_theme_changed,
            Qt::UniqueConnection);
    }
}

QObject* rinui_theme_manager_as_qobject(rinui::ThemeManager& manager)
{
    return static_cast<QObject*>(&manager);
}
