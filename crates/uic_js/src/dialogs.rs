//! Browser dialogs behind the terminal runtime: the mocked `alert`,
//! `confirm` and `prompt` queue their questions here — a thread-local,
//! because the flat natives capture nothing — and the host answers later
//! through [`JsHost::answer_dialog`](crate::JsHost::answer_dialog). The
//! promises and their resolvers stay JS-side (runtime/dialogs.ts); this
//! module is only the one-way question queue.

use std::cell::RefCell;
use std::collections::VecDeque;

/// What a dialog asks and how it answers: alert acknowledges, confirm
/// decides, prompt collects a line of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Alert,
    Confirm,
    Prompt,
}

/// One question a component asked; `id` routes the answer back to the
/// promise waiting on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogRequest {
    pub id: u32,
    pub kind: DialogKind,
    pub message: String,
    /// The prompt's prefilled text; `None` for alert and confirm.
    pub default: Option<String>,
}

thread_local! {
    static PENDING: RefCell<VecDeque<DialogRequest>> = const { RefCell::new(VecDeque::new()) };
}

/// A fresh host starts with an empty queue — questions cannot leak between
/// hosts sharing a thread (the same discipline the state slot has).
pub(crate) fn reset() {
    PENDING.with(|queue| queue.borrow_mut().clear());
}

pub(crate) fn push(request: DialogRequest) {
    PENDING.with(|queue| queue.borrow_mut().push_back(request));
}

pub(crate) fn take() -> Option<DialogRequest> {
    PENDING.with(|queue| queue.borrow_mut().pop_front())
}
