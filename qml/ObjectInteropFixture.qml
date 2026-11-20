import QtQuick 2.15
import RinUI

FluentWindow {
    id: root
    visible: false
    width: 640
    height: 480

    property bool flagProp: false
    property int countProp: 7
    property double ratioProp: 1.5
    property string titleProp: "initial"
    property var listProp: ["alpha", "beta"]
    property list<string> strListProp: ["one", "two"]
    property list<int> intListProp: [10, 20]
    readonly property int readOnlyNum: 42

    property int pingCalls: 0

    function zeroArgPing() {
        pingCalls += 1
        return "pong:" + pingCalls
    }

    function addInts(a, b) {
        return a + b
    }

    function makeSummaryList(prefix, count) {
        return [prefix, count, flagProp]
    }

    function typedRepeat(label: string, times: int): string {
        var out = ""
        for (var i = 0; i < times; ++i) {
            out += label
        }
        return out
    }

    function sumFive(a, b, c, d, e) {
        return a + b + c + d + e
    }

    function invokeHostCallback(tag) {
        if (typeof HostCallback !== "undefined" && HostCallback) {
            HostCallback.trigger(tag)
        }
        return "callback-done:" + tag
    }
}
