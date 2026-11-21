#pragma once

#include <memory>

namespace rinui::test_support {
class CallbackRelay;
}

std::unique_ptr<rinui::test_support::CallbackRelay> rinui_callback_relay_new();

// Outer manual event driver only; mirrors Qt's documented DeferredDelete dispatch.
void rinui_test_dispatch_deferred_deletes();
