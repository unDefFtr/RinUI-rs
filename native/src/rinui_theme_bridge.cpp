#include "rinui_theme_bridge.h"
#include "rinui_platform_hooks.h"
#include "rinui-host/src/theme.cxxqt.h"

#include <QGuiApplication>
#include <QMetaObject>
#include <QStyleHints>

namespace {
QMetaObject::Connection s_color_scheme_conn;
}

std::unique_ptr<rinui::ThemeManager> rinui_theme_manager_new()
{
    return std::make_unique<rinui::ThemeManager>();
}

void rinui_connect_color_scheme_changed(rinui::ThemeManager& manager)
{
    rinui::platform::set_theme_manager(&manager);

    if (s_color_scheme_conn) {
        QObject::disconnect(s_color_scheme_conn);
        s_color_scheme_conn = QMetaObject::Connection{};
    }

    if (auto* hints = QGuiApplication::styleHints()) {
        s_color_scheme_conn = QObject::connect(
            hints,
            &QStyleHints::colorSchemeChanged,
            &manager,
            &rinui::ThemeManager::handle_system_theme_changed,
            Qt::UniqueConnection);
    }
}

void rinui_disconnect_color_scheme_changed()
{
    if (s_color_scheme_conn) {
        QObject::disconnect(s_color_scheme_conn);
        s_color_scheme_conn = QMetaObject::Connection{};
    }
    rinui::platform::set_theme_manager(nullptr);
}

QObject* rinui_theme_manager_as_qobject(rinui::ThemeManager& manager)
{
    return static_cast<QObject*>(&manager);
}
