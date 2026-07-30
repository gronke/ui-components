//! The system clipboard on the mocked DOM: `navigator.clipboard` over a
//! backend the app provides — the storage seam's shape. The backend is
//! synchronous; the runtime wraps it in resolved promises to match the
//! browser's async API. Without an installed backend the reads come back
//! empty, the way a browser without permission does.

use std::cell::RefCell;
use std::rc::Rc;

/// The clipboard the app hands the host — arboard in the lit-demo, a fake
/// in tests. `read` yields the text or `None`; `write` reports success.
pub trait ClipboardBackend {
    fn read(&self) -> Option<String>;
    fn write(&self, text: &str) -> bool;
}

thread_local! {
    static BACKEND: RefCell<Option<Rc<dyn ClipboardBackend>>> = const { RefCell::new(None) };
}

/// A fresh host starts with no clipboard — a backend cannot leak between
/// hosts sharing a thread (the discipline the state and storage slots have).
pub(crate) fn reset() {
    BACKEND.with(|slot| *slot.borrow_mut() = None);
}

pub(crate) fn install(backend: Rc<dyn ClipboardBackend>) {
    BACKEND.with(|slot| *slot.borrow_mut() = Some(backend));
}

/// The installed backend's read, or `None` when none is installed — the
/// natives and the host's own read share this.
pub(crate) fn read() -> Option<String> {
    BACKEND.with(|slot| slot.borrow().as_ref().and_then(|backend| backend.read()))
}

pub(crate) fn write(text: &str) -> bool {
    BACKEND.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|backend| backend.write(text))
    })
}
