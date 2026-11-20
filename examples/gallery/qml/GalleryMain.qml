import QtQuick 2.15
import RinUI

UpstreamGallery {
    id: root
    useNativeMacFrame: isMacOS
        && (typeof RinUINativeMacFrame !== "undefined"
            ? RinUINativeMacFrame
            : (typeof NativeMacFrameEnabled !== "undefined" && NativeMacFrameEnabled))

    property string smokeCurrentPageObjectName: ""
    property string smokeCurrentPageTitle: ""

    function _findActiveStackView(obj) {
        if (!obj) return null
        if (typeof obj.depth === "number" && obj.currentItem !== undefined) {
            return obj
        }
        var kids = obj.children || []
        for (var i = 0; i < kids.length; ++i) {
            var found = _findActiveStackView(kids[i])
            if (found) return found
        }
        return null
    }

    function _syncSmokePageState() {
        var sv = _findActiveStackView(root.navigationView)
        if (sv && sv.currentItem) {
            root.smokeCurrentPageObjectName = String(sv.currentItem.objectName || "")
            root.smokeCurrentPageTitle = sv.currentItem.title !== undefined
                ? String(sv.currentItem.title)
                : ""
        }
    }

    function _walkSmokeTree(obj, predicate, seen) {
        if (!obj) return false
        if (seen.indexOf(obj) !== -1) return false
        seen.push(obj)
        if (predicate(obj)) return true
        if (obj.contentItem && _walkSmokeTree(obj.contentItem, predicate, seen)) {
            return true
        }
        var kids = obj.children || []
        for (var i = 0; i < kids.length; ++i) {
            if (_walkSmokeTree(kids[i], predicate, seen)) {
                return true
            }
        }
        var dataKids = obj.data || []
        for (var j = 0; j < dataKids.length; ++j) {
            if (_walkSmokeTree(dataKids[j], predicate, seen)) {
                return true
            }
        }
        return false
    }

    function smokeIsNavigationBusy() {
        var sv = _findActiveStackView(root.navigationView)
        return Boolean((root.navigationView && root.navigationView.pushInProgress) || (sv && sv.busy))
    }

    function smokeNavigationCurrentPage() {
        return root.navigationView ? String(root.navigationView.currentPage || "") : ""
    }

    function smokeCurrentPageErrorMessage() {
        var sv = _findActiveStackView(root.navigationView)
        if (sv && sv.currentItem && sv.currentItem.errorMessage !== undefined) {
            return String(sv.currentItem.errorMessage || "")
        }
        return ""
    }

    function smokePushValidSample() {
        if (!root.navigationView) return false
        root.navigationView.push(Qt.resolvedUrl("pages/controls/TextArea.qml"))
        return true
    }

    function smokePushErrorPage() {
        if (!root.navigationView) return false
        root.navigationView.push("unexist/page")
        return true
    }

    function smokePopNavigation() {
        if (!root.navigationView) return false
        root.navigationView.safePop()
        return true
    }

    function smokeHasScrollView() {
        var sv = _findActiveStackView(root.navigationView)
        if (!sv || !sv.currentItem) return false
        return _walkSmokeTree(sv.currentItem, function(node) {
            var desc = String(node)
            return desc.indexOf("ScrollView") !== -1
                || desc.indexOf("ScrollableTextArea") !== -1
                || desc.indexOf("QQuickScrollView") !== -1
        }, [])
    }

    function smokeHasFlickable() {
        var sv = _findActiveStackView(root.navigationView)
        if (!sv || !sv.currentItem) return false
        return _walkSmokeTree(sv.currentItem, function(node) {
            return typeof node.contentHeight === "number"
                && typeof node.contentY === "number"
        }, [])
    }

    Connections {
        target: root.navigationView
        function onPageChanged() {
            root._syncSmokePageState()
        }
    }

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
        Qt.callLater(root._syncSmokePageState)
    }
}
