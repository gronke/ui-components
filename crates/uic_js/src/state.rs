//! The document state the natives operate on: one per host, installed
//! thread-locally so the flat native functions reach it without captures.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use boa_engine::{JsNativeError, JsResult};
use uic_dom::NodeId;
use uic_tui::dom::DomDocument;

/// The document and the JS↔node handle table, shared with the natives.
pub struct HostState {
    pub doc: DomDocument,
    pub focused: Option<NodeId>,
    pub dirty: bool,
    handles: Vec<NodeId>,
    handle_of: HashMap<NodeId, usize>,
}

impl HostState {
    pub(crate) fn new() -> Self {
        HostState {
            doc: DomDocument::new(),
            focused: None,
            dirty: false,
            handles: Vec::new(),
            handle_of: HashMap::new(),
        }
    }

    /// The stable JS-side handle for a node.
    pub fn handle(&mut self, node: NodeId) -> usize {
        if let Some(&handle) = self.handle_of.get(&node) {
            return handle;
        }
        let handle = self.handles.len();
        self.handles.push(node);
        self.handle_of.insert(node, handle);
        handle
    }

    pub fn node(&self, handle: usize) -> Option<NodeId> {
        self.handles.get(handle).copied()
    }
}

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
