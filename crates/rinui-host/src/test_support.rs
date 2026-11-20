use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

type RelayHandler = Rc<dyn Fn(&str)>;

thread_local! {
    static ACTIVE_HANDLER: RefCell<Option<RelayHandler>> = const { RefCell::new(None) };
}

#[cxx_qt::bridge(namespace = "rinui::test_support")]
pub mod qobject {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("rinui-host/rinui_test_support.h");
        pub fn rinui_callback_relay_new() -> UniquePtr<CallbackRelay>;
        /// Dispatch Qt DeferredDeletes from the outer manual test driver.
        ///
        /// # Safety
        /// No Qt/QML callback may remain on the calling thread's stack.
        pub unsafe fn rinui_test_dispatch_deferred_deletes();
    }

    extern "RustQt" {
        #[qobject]
        type CallbackRelay = super::CallbackRelayRust;

        #[qinvokable]
        fn trigger(self: Pin<&mut CallbackRelay>, tag: QString);
    }
}

pub use qobject::{rinui_callback_relay_new, rinui_test_dispatch_deferred_deletes, CallbackRelay};

#[derive(Default)]
pub struct CallbackRelayRust;

impl CallbackRelay {
    pub fn trigger(self: Pin<&mut Self>, tag: cxx_qt_lib::QString) {
        let handler = ACTIVE_HANDLER.with(|slot| slot.borrow().as_ref().map(Rc::clone));
        if let Some(cb) = handler {
            let tag_str = tag.to_string();
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cb(&tag_str);
            }));
        }
    }
}

pub struct CallbackRelayGuard {
    _private: (),
}

impl Drop for CallbackRelayGuard {
    fn drop(&mut self) {
        ACTIVE_HANDLER.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

pub fn install_callback_handler(handler: impl Fn(&str) + 'static) -> CallbackRelayGuard {
    let wrapped: RelayHandler = Rc::new(handler);
    ACTIVE_HANDLER.with(|slot| {
        *slot.borrow_mut() = Some(wrapped);
    });
    CallbackRelayGuard { _private: () }
}
