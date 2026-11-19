import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 2.15
import RinUI

FluentWindow {
    id: root
    visible: false
    width: 720
    height: 480
    title: "rinui-rs Smoke Test"

    property string currentThemeName: Theme.getTheme()
    property string actualThemeName: ThemeManager.get_theme()
    property string translatedCalendarToday: qsTranslate("Calendar", "Today")
    property string datePickerPlaceholder: datePicker.placeholderText
    property int themeChangedSignalCount: 0
    property int backdropChangedSignalCount: 0
    property string lastBackdropSignal: ""
    readonly property bool singletonsValid: Theme.currentTheme !== null
        && Utils.colors !== null
        && Theme.currentTheme.colors === Utils.colors
        && Colors !== null
        && Typography !== null
        && Severity !== null
        && Position !== null
    readonly property string customGreeting: typeof CustomGreeting !== "undefined" ? String(CustomGreeting) : ""

    Connections {
        target: ThemeManager
        function onThemeChanged(theme) {
            root.themeChangedSignalCount += 1
            root.actualThemeName = theme
        }
        function onBackdropChanged(effect) {
            root.backdropChangedSignalCount += 1
            root.lastBackdropSignal = effect
        }
    }

    function setThemeMode(mode) {
        Theme.setTheme(mode)
        currentThemeName = Theme.getTheme()
        actualThemeName = ThemeManager.get_theme()
    }

    function applyBackdrop(effect) {
        return ThemeManager.apply_backdrop_effect(effect)
    }

    function setAccentColor(hex) {
        Theme.setThemeColor(hex)
    }

    function readAccentColor() {
        return ThemeManager.get_theme_color()
    }

    function checkSingletonsMatch() {
        return Theme.currentTheme !== null
            && Utils.colors !== null
            && Colors !== null
            && Typography !== null
            && Severity !== null
            && Position !== null
            && Theme.currentTheme.colors === Utils.colors
    }

    function checkCalendarTranslation() {
        return calendarPreview.todayTranslation
    }

    Calendar {
        id: calendarPreview
        visible: false
        readonly property string todayTranslation: qsTranslate("Calendar", "Today")
    }

    CalendarDatePicker {
        id: datePicker
        visible: false
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Text {
            id: statusText
            typography: Typography.BodyStrong
            text: "Theme: " + Theme.getTheme() + " (" + ThemeManager.get_theme() + ")"
        }

        Button {
            id: sampleButton
            text: qsTranslate("Calendar", "Today")
        }

        AcrylicBrush {
            id: acrylicSample
            width: 120
            height: 60
        }
    }
}
