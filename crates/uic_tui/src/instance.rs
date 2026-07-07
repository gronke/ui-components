//! A mounted custom element: property store, behavior, widget slots, and the
//! notify emitter — the DOM-element analog of the terminal runtime.

use chrono::NaiveDate;
use rat_widget::date_input::DateInputState;
use uic_core::{
    attribute_to_value, notify_events, Behavior, Changed, ComponentDef, Ctx, CustomElementRegistry,
    NotifyEvent, PropertyStore, UiEvent, Value,
};
use uic_template::{Attribute, Expr, Node};

use crate::expand::{resolve_expr, widget_kind};
use crate::Error;

type Listener = Box<dyn FnMut(&NotifyEvent)>;

/// One interactive leaf of the template (`data-tui="…"`), with its persistent
/// widget state and the bindings wired in the template.
pub(crate) struct Slot {
    pub state: DateInputState,
    pub value_prop: Option<String>,
    pub change_handler: Option<String>,
    pub disabled: Option<Expr>,
}

impl Slot {
    pub(crate) fn is_disabled(&self, store: &PropertyStore, behavior: &dyn Behavior) -> bool {
        self.disabled
            .as_ref()
            .is_some_and(|expr| resolve_expr(expr, store, behavior).truthy())
    }

    /// The value a commit hands to the change handler: the normalized date
    /// when the mask parses, otherwise the raw text (the component's own
    /// validation produces the error message, like in the browser).
    fn committed_text(&self) -> String {
        match self.state.value() {
            Ok(date) => date.format("%Y-%m-%d").to_string(),
            Err(_) => {
                let raw = self.state.widget.text();
                if raw.chars().any(|c| c.is_ascii_digit()) {
                    raw.split_whitespace().collect::<Vec<_>>().join("")
                } else {
                    String::new()
                }
            }
        }
    }
}

pub struct ElementInstance {
    pub(crate) def: &'static ComponentDef,
    pub(crate) store: PropertyStore,
    pub(crate) behavior: Box<dyn Behavior>,
    pub(crate) slots: Vec<Slot>,
    pub(crate) focused: usize,
    listeners: Vec<(String, Listener)>,
}

impl ElementInstance {
    /// Registry lookup + `connected` lifecycle — the terminal analog of
    /// attaching the element to a document.
    pub(crate) fn mount(tag: &str) -> Result<Self, Error> {
        let def =
            CustomElementRegistry::get(tag).ok_or_else(|| Error::UnknownTag(tag.to_string()))?;
        let slots = discover_slots(&def.template().roots)?;
        let mut instance = ElementInstance {
            def,
            store: PropertyStore::new(def.properties),
            behavior: (def.new_behavior)(),
            slots,
            focused: 0,
            listeners: Vec::new(),
        };
        instance.update_cycle(|behavior, ctx| behavior.connected(ctx));
        // Seed the widgets with the initial property state.
        instance.sync_slots(None);
        Ok(instance)
    }

    pub fn tag_name(&self) -> &'static str {
        self.def.tag_name
    }

    /// Sets an observed attribute, converting per the declared property type
    /// (the `attribute_changed` lifecycle fires inside the update cycle).
    pub fn set_attr(&mut self, name: &str, value: &str) {
        let Some(meta) = self.def.property_by_attribute(name) else {
            return;
        };
        let new_value = attribute_to_value(meta.js_type, Some(value));
        let rust_name = meta.rust_name;
        self.update_cycle(|behavior, ctx| {
            ctx.set(rust_name, new_value);
            behavior.attribute_changed(ctx, name, None, Some(value));
        });
    }

    /// Subscribes to a notify event (`value-changed` …) of this element.
    pub fn on(&mut self, event: &str, listener: impl FnMut(&NotifyEvent) + 'static) {
        self.listeners.push((event.to_string(), Box::new(listener)));
    }

    /// Commits the focused widget's value through its `@change` handler.
    pub(crate) fn commit_focused(&mut self) {
        let Some(slot) = self.slots.get(self.focused) else {
            return;
        };
        if slot.is_disabled(&self.store, self.behavior.as_ref()) {
            return;
        }
        let Some(handler) = slot.change_handler.clone() else {
            return;
        };
        let event = UiEvent::change(slot.committed_text());
        self.update_cycle(|behavior, ctx| behavior.handle(ctx, &handler, &event));
    }

    /// The full ReactiveElement-style update cycle: apply `f`, run
    /// `will_update` (its sets join the same batch), emit notify events, run
    /// `updated`, then push state into the widgets.
    pub(crate) fn update_cycle(&mut self, f: impl FnOnce(&mut dyn Behavior, &mut Ctx)) {
        let mut changed = Changed::default();
        {
            let mut ctx = Ctx::new(&mut self.store, &mut changed);
            f(self.behavior.as_mut(), &mut ctx);
        }
        let snapshot = changed.clone();
        {
            let mut ctx = Ctx::new(&mut self.store, &mut changed);
            self.behavior.will_update(&mut ctx, &snapshot);
        }
        let events = notify_events(self.def, &changed, &self.store);
        for event in &events {
            for (name, listener) in self.listeners.iter_mut() {
                if name == &event.event_name {
                    listener(event);
                }
            }
        }
        {
            // Sets inside `updated` do not start another cycle in v1.
            let mut ignored = Changed::default();
            let mut ctx = Ctx::new(&mut self.store, &mut ignored);
            self.behavior.updated(&mut ctx, &changed);
        }
        self.sync_slots(Some(&changed));
    }

    /// Pushes `.value=${…}`-bound properties into the widget states.
    /// With a change set, only bound properties that actually changed are
    /// written, so uncommitted text in a widget survives unrelated updates
    /// (like the browser, where lit only re-sets a changed `.value`).
    pub(crate) fn sync_slots(&mut self, changed: Option<&Changed>) {
        for slot in &mut self.slots {
            let Some(prop) = &slot.value_prop else {
                continue;
            };
            if let Some(changed) = changed {
                if !changed.has(prop) {
                    continue;
                }
            }
            match self.store.get(prop) {
                Value::Str(s) if !s.is_empty() => match NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                    Ok(date) => slot.state.set_value(date),
                    Err(_) => slot.state.widget.set_text(s.clone()),
                },
                _ => slot.state.clear(),
            }
        }
    }

    /// Moves focus to the next enabled slot.
    pub(crate) fn focus_next(&mut self) {
        if self.slots.is_empty() {
            return;
        }
        let count = self.slots.len();
        for step in 1..=count {
            let candidate = (self.focused + step) % count;
            if !self.slots[candidate].is_disabled(&self.store, self.behavior.as_ref()) {
                self.focused = candidate;
                return;
            }
        }
    }
}

/// Collects the interactive leaves in template order (all branches), wiring
/// each widget's bindings.
fn discover_slots(nodes: &[Node]) -> Result<Vec<Slot>, Error> {
    let mut slots = Vec::new();
    collect_slots(nodes, &mut slots)?;
    Ok(slots)
}

fn collect_slots(nodes: &[Node], slots: &mut Vec<Slot>) -> Result<(), Error> {
    for node in nodes {
        match node {
            Node::Element(el) => {
                if let Some(kind) = widget_kind(el) {
                    slots.push(new_slot(kind, el)?);
                }
                collect_slots(&el.children, slots)?;
            }
            Node::If { then, .. } => collect_slots(then, slots)?,
            Node::Text(_) | Node::TextHole(_) => {}
        }
    }
    Ok(())
}

fn new_slot(kind: &str, el: &uic_template::Element) -> Result<Slot, Error> {
    if kind != "date-input" {
        return Err(Error::UnknownWidget(kind.to_string()));
    }
    let state = DateInputState::new()
        .with_pattern("%Y-%m-%d")
        .map_err(|err| Error::Pattern(err.to_string()))?;
    let mut slot = Slot {
        state,
        value_prop: None,
        change_handler: None,
        disabled: None,
    };
    for attr in &el.attrs {
        match attr {
            Attribute::Prop { name, expr } if name == "value" => {
                slot.value_prop = Some(expr.ident().to_string());
            }
            Attribute::Event { name, handler } if name == "change" => {
                slot.change_handler = Some(handler.clone());
            }
            Attribute::Bool { name, expr } if name == "disabled" => {
                slot.disabled = Some(expr.clone());
            }
            _ => {}
        }
    }
    Ok(slot)
}
