//! The Boa host's thread-local view of the shared [`HostState`]
//! (`uic_tui::dom::HostState`): the flat native functions reach it without
//! captures; the browser host owns its state directly instead.

use std::cell::RefCell;
use std::rc::Rc;

use boa_engine::{JsNativeError, JsResult};

pub use uic_tui::dom::HostState;

thread_local! {
    static STATE: RefCell<Option<Rc<RefCell<HostState>>>> = const { RefCell::new(None) };
}

pub(crate) fn install(state: Rc<RefCell<HostState>>) {
    STATE.with(|slot| *slot.borrow_mut() = Some(state));
}

pub(crate) fn with_state<R>(f: impl FnOnce(&mut HostState) -> R) -> JsResult<R> {
    let state = STATE.with(|slot| slot.borrow().clone());
    let state = state
        .ok_or_else(|| JsNativeError::error().with_message("uic_js host state is not installed"))?;
    let result = f(&mut state.borrow_mut());
    Ok(result)
}
