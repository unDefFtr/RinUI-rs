#include "rinui_test_support.h"
#include <QCoreApplication>
#include <QEvent>
#include "rinui-host/src/test_support.cxxqt.h"

std::unique_ptr<rinui::test_support::CallbackRelay> rinui_callback_relay_new()
{
    return std::make_unique<rinui::test_support::CallbackRelay>();
}

void rinui_test_dispatch_deferred_deletes()
{
    QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
}
