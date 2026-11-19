import QtQuick 2.15
import RinUI

UpstreamGallery {
    id: root

    Component.onCompleted: {
        var items = root.navigationItems.slice()
        var demoItem = {
            title: qsTr("Rust Host Demo"),
            page: Qt.resolvedUrl("RustHostDemoPage.qml"),
            icon: "ic_fluent_cube_20_regular",
            position: Position.Bottom
        }

        var settingsIndex = -1
        for (var i = 0; i < items.length; ++i) {
            if (items[i] && items[i].page && String(items[i].page).indexOf("Settings.qml") !== -1) {
                settingsIndex = i
                break
            }
        }
        if (settingsIndex >= 0) {
            items.splice(settingsIndex, 0, demoItem)
        } else {
            items.push(demoItem)
        }
        root.navigationItems = items
    }
}
