#pragma once

#include <memory>

namespace rinui {
class ThemeManager;
}

class QObject;

/// Construct a new ThemeManager QObject on the heap for Rust ownership.
std::unique_ptr<rinui::ThemeManager> rinui_theme_manager_new();

/// Connect QGuiApplication::styleHints()->colorSchemeChanged to ThemeManager and register it with platform hooks.
void rinui_connect_color_scheme_changed(rinui::ThemeManager& manager);

/// Disconnect QGuiApplication::styleHints()->colorSchemeChanged and clear the platform ThemeManager pointer.
void rinui_disconnect_color_scheme_changed();

/// Upcast ThemeManager& to raw QObject* for context property registration.
QObject* rinui_theme_manager_as_qobject(rinui::ThemeManager& manager);
