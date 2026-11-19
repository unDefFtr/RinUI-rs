import QtQuick 2.15
import QtQuick.Layouts 2.15
import RinUI

Window {
    id: dialogWindow
    visible: true
    width: 460
    height: 300
    minimumWidth: 360
    minimumHeight: 240
    title: qsTr("Secondary RinUI Window")

    property string actualThemeName: ThemeManager.get_theme()
    property string configuredThemeName: ThemeManager.get_theme_name()

    Connections {
        target: ThemeManager
        function onThemeChanged(theme) {
            dialogWindow.actualThemeName = theme
            dialogWindow.configuredThemeName = ThemeManager.get_theme_name()
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 12

        Text {
            typography: Typography.Subtitle
            text: qsTr("Secondary RinUI Window")
        }

        Text {
            typography: Typography.Body
            wrapMode: Text.Wrap
            Layout.fillWidth: true
            text: qsTr("This independent window shares the RinUI ThemeManager, configuration, and translation engine with the main window.")
        }

        Frame {
            Layout.fillWidth: true
            ColumnLayout {
                spacing: 6
                Text {
                    text: qsTr("Configured Theme: ") + ThemeManager.get_theme_name()
                }
                Text {
                    text: qsTr("Resolved Theme: ") + (Theme.currentTheme ? Theme.currentTheme.name : dialogWindow.actualThemeName)
                }
                Text {
                    text: qsTr("Accent Color: ") + ThemeManager.get_theme_color()
                }
            }
        }

        RowLayout {
            spacing: 8
            Button {
                text: qsTr("Toggle Theme")
                onClicked: Theme.toggleMode()
            }
            Button {
                highlighted: true
                text: qsTr("Close Window")
                onClicked: dialogWindow.close()
            }
        }
    }
}
