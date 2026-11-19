import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 2.15
import RinUI

FluentPage {
    id: page
    title: qsTr("Rust Host Demo")

    property int counterValue: Backend.getRustCounter()
    property int windowCount: Backend.getOpenWindowCount()
    property string lastEvent: qsTr("Ready (hosted by rinui-rs)")

    Connections {
        target: Backend
        function onRustCounterChanged() {
            page.counterValue = Backend.getRustCounter()
        }
        function onOpenWindowCountChanged() {
            page.windowCount = Backend.getOpenWindowCount()
        }
        function onHostEventLogged(message) {
            page.lastEvent = message
        }
    }

    Column {
        Layout.fillWidth: true
        spacing: 6

        Text {
            typography: Typography.BodyStrong
            text: qsTr("Rust-Owned QObject State (CXX-Qt)")
        }

        SettingCard {
            width: parent.width
            title: qsTr("Rust Backend Counter")
            description: qsTr("Current counter: %1 — Last event: %2").arg(page.counterValue).arg(page.lastEvent)
            icon.name: "ic_fluent_developer_board_20_regular"

            Button {
                text: qsTr("Reset")
                onClicked: Backend.resetCounter()
            }
            Button {
                highlighted: true
                text: qsTr("Increment (+1)")
                onClicked: Backend.incrementCounter()
            }
        }
    }

    Column {
        Layout.fillWidth: true
        spacing: 6

        Text {
            typography: Typography.BodyStrong
            text: qsTr("Multi-Window & Native Lifecycle")
        }

        SettingCard {
            width: parent.width
            title: qsTr("Secondary Window Management")
            description: qsTr("Active RinUI windows tracked by RinUIHost: %1").arg(page.windowCount)
            icon.name: "ic_fluent_grid_20_regular"

            Button {
                text: qsTr("Close Secondary")
                enabled: page.windowCount > 1
                onClicked: Backend.closeSecondaryWindows()
            }
            Button {
                highlighted: true
                text: qsTr("Open Secondary Window")
                onClicked: Backend.openSecondaryWindow()
            }
        }
    }

    Column {
        Layout.fillWidth: true
        spacing: 6

        Text {
            typography: Typography.BodyStrong
            text: qsTr("Host & Platform Diagnostics")
        }

        SettingCard {
            width: parent.width
            title: qsTr("Quick Theme & Language Controls")
            description: qsTr("Theme and locale changes synchronize across every open RinUI window.")
            icon.name: "ic_fluent_color_20_regular"

            Button {
                text: qsTr("Toggle Light / Dark")
                onClicked: Theme.toggleMode()
            }
            Button {
                text: "English (US)"
                onClicked: Backend.setLanguage("en_US")
            }
            Button {
                text: "简体中文"
                onClicked: Backend.setLanguage("zh_CN")
            }
        }

        Frame {
            width: parent.width
            ColumnLayout {
                width: parent.width
                spacing: 6

                Text {
                    typography: Typography.BodyStrong
                    text: qsTr("Runtime Environment")
                }
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Qt Platform: %1 | Resource Mode: %2")
                        .arg(Backend.getPlatformName())
                        .arg(Backend.getResourceMode())
                }
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Windows DWM Backdrop Supported: %1")
                        .arg(Backend.isBackdropSupported() ? qsTr("Yes") : qsTr("No (Windows only)"))
                }
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("Config Path: %1").arg(Backend.getConfigPath())
                }
                Text {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    color: Theme.currentTheme.colors.textSecondaryColor
                    text: Backend.getHostSummary()
                }
            }
        }
    }
}
