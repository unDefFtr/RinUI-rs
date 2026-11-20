import QtQuick 2.15
import RinUI

FluentWindow {
    id: root
    visible: false
    width: 640
    height: 480

    property string appGreeting: qsTranslate("AppI18n", "Hello")
    property string appFarewell: qsTranslate("AppI18n", "Goodbye")
    property string rinuiToday: qsTranslate("Calendar", "Today")
    property string calendarLocaleName: cal.locale.name
    property string calendarMonthZero: cal.locale.monthName(0)

    function readDefaultQmlLocaleName() {
        return Qt.locale().name
    }

    function readDefaultQmlMonthZero() {
        return Qt.locale().monthName(0)
    }

    Calendar {
        id: cal
        visible: false
    }
}
