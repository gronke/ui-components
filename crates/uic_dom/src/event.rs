//! DOM events over the retained tree: the whatwg dispatch subset one
//! light-DOM tree needs — capture, target and bubble phases, both stop
//! flags, cancelation — with listeners registered directly per node the way
//! lit's EventPart attaches them (no delegation; bubbling reaches ancestor
//! handlers).

use uic_core::Value;

use crate::tree::{Document, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    None,
    Capturing,
    AtTarget,
    Bubbling,
}

/// `addEventListener` options; `passive` listeners cannot cancel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListenerOptions {
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListenerId(u64);

/// One event in flight. Hand-dispatched events default to non-bubbling and
/// non-cancelable like `CustomEvent`; the named constructors encode the
/// native table.
pub struct Event {
    event_type: String,
    /// The payload, `CustomEvent.detail`'s analog.
    pub detail: Value,
    bubbles: bool,
    cancelable: bool,
    target: Option<NodeId>,
    current_target: Option<NodeId>,
    phase: EventPhase,
    default_prevented: bool,
    propagation_stopped: bool,
    immediate_stopped: bool,
    /// Set around passive listeners, where `prevent_default` is ignored.
    in_passive: bool,
}

impl Event {
    pub fn new(event_type: &str) -> Self {
        Event {
            event_type: event_type.to_string(),
            detail: Value::Null,
            bubbles: false,
            cancelable: false,
            target: None,
            current_target: None,
            phase: EventPhase::None,
            default_prevented: false,
            propagation_stopped: false,
            immediate_stopped: false,
            in_passive: false,
        }
    }

    /// `input`: fires on the control per value change; bubbles, not
    /// cancelable.
    pub fn input() -> Self {
        Event::new("input").with_bubbles(true)
    }

    /// `change`: fires on the control at commit; bubbles, not cancelable.
    pub fn change() -> Self {
        Event::new("change").with_bubbles(true)
    }

    /// `click`: fires on the element under the pointer; bubbles and
    /// cancels, like the browser's.
    pub fn click() -> Self {
        Event::new("click").with_bubbles(true).with_cancelable(true)
    }

    /// `submit`: fires on the form; bubbles AND cancels (the one place
    /// `prevent_default` matters among the three).
    pub fn submit() -> Self {
        Event::new("submit")
            .with_bubbles(true)
            .with_cancelable(true)
    }

    /// `focus` does not bubble (capture still runs); delegation wants
    /// `focusin`.
    pub fn focus() -> Self {
        Event::new("focus")
    }

    /// `blur` does not bubble; delegation wants `focusout`.
    pub fn blur() -> Self {
        Event::new("blur")
    }

    pub fn with_bubbles(mut self, bubbles: bool) -> Self {
        self.bubbles = bubbles;
        self
    }

    pub fn with_cancelable(mut self, cancelable: bool) -> Self {
        self.cancelable = cancelable;
        self
    }

    pub fn with_detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// The dispatch origin, stable through all phases.
    pub fn target(&self) -> Option<NodeId> {
        self.target
    }

    /// The node whose listener is currently running.
    pub fn current_target(&self) -> Option<NodeId> {
        self.current_target
    }

    pub fn phase(&self) -> EventPhase {
        self.phase
    }

    pub fn bubbles(&self) -> bool {
        self.bubbles
    }

    pub fn cancelable(&self) -> bool {
        self.cancelable
    }

    pub fn default_prevented(&self) -> bool {
        self.default_prevented
    }

    /// Later nodes on the path see nothing more; peers on the current node
    /// still run.
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }

    /// Also halts the remaining listeners on the current node.
    pub fn stop_immediate_propagation(&mut self) {
        self.propagation_stopped = true;
        self.immediate_stopped = true;
    }

    /// Honored only when the event is cancelable and the listener is not
    /// passive.
    pub fn prevent_default(&mut self) {
        if self.cancelable && !self.in_passive {
            self.default_prevented = true;
        }
    }
}

pub(crate) type Callback<T> = Box<dyn FnMut(&mut Document<T>, &mut Event)>;

pub(crate) struct ListenerEntry<T> {
    id: ListenerId,
    event_type: String,
    options: ListenerOptions,
    /// Taken out for the duration of its own invocation, so the callback may
    /// borrow the document (and even mutate this very registry).
    callback: Option<Callback<T>>,
}

impl<T> Document<T> {
    /// Registers directly on the node, lit-EventPart-style. The callback
    /// receives the document mutably — the public DOM API works inside
    /// handlers.
    pub fn add_event_listener(
        &mut self,
        node: NodeId,
        event_type: &str,
        options: ListenerOptions,
        callback: impl FnMut(&mut Document<T>, &mut Event) + 'static,
    ) -> ListenerId {
        self.next_listener_id += 1;
        let id = ListenerId(self.next_listener_id);
        self.listeners.entry(node).or_default().push(ListenerEntry {
            id,
            event_type: event_type.to_string(),
            options,
            callback: Some(Box::new(callback)),
        });
        id
    }

    pub fn remove_event_listener(&mut self, node: NodeId, id: ListenerId) {
        if let Some(entries) = self.listeners.get_mut(&node) {
            entries.retain(|entry| entry.id != id);
        }
    }

    /// The whatwg dispatch: capture from the root down, the target (both
    /// listener kinds, registration order), then — only for bubbling events
    /// — back up. Returns `!default_prevented`, like `dispatchEvent`.
    pub fn dispatch_event(&mut self, target: NodeId, event: &mut Event) -> bool {
        event.target = Some(target);
        let path: Vec<NodeId> = self.ancestors(target).collect();

        event.phase = EventPhase::Capturing;
        for &node in path.iter().skip(1).rev() {
            self.invoke_listeners(node, event, Some(true));
            if event.propagation_stopped {
                break;
            }
        }

        if !event.propagation_stopped {
            event.phase = EventPhase::AtTarget;
            self.invoke_listeners(target, event, None);
        }

        if event.bubbles && !event.propagation_stopped {
            event.phase = EventPhase::Bubbling;
            for &node in path.iter().skip(1) {
                self.invoke_listeners(node, event, Some(false));
                if event.propagation_stopped {
                    break;
                }
            }
        }

        event.phase = EventPhase::None;
        event.current_target = None;
        !event.default_prevented
    }

    /// Runs the node's matching listeners; `capture: None` at the target
    /// takes both kinds. The id snapshot means listeners added to this node
    /// during dispatch wait for the next event, while removed ones are
    /// skipped — the spec's behavior.
    fn invoke_listeners(&mut self, node: NodeId, event: &mut Event, capture: Option<bool>) {
        let Some(entries) = self.listeners.get(&node) else {
            return;
        };
        let ids: Vec<ListenerId> = entries
            .iter()
            .filter(|entry| {
                entry.event_type == event.event_type
                    && capture.is_none_or(|wanted| entry.options.capture == wanted)
            })
            .map(|entry| entry.id)
            .collect();
        event.current_target = Some(node);
        for id in ids {
            if event.immediate_stopped {
                break;
            }
            let Some(entry) = self
                .listeners
                .get_mut(&node)
                .and_then(|entries| entries.iter_mut().find(|entry| entry.id == id))
            else {
                continue;
            };
            let options = entry.options;
            let Some(mut callback) = entry.callback.take() else {
                continue;
            };
            event.in_passive = options.passive;
            callback(self, event);
            event.in_passive = false;
            if let Some(entries) = self.listeners.get_mut(&node) {
                if options.once {
                    entries.retain(|entry| entry.id != id);
                } else if let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) {
                    entry.callback = Some(callback);
                }
            }
        }
    }
}
