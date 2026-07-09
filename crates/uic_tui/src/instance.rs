//! A mounted custom element: property store, behavior, widget slots, nested
//! child elements, and the notify emitter — the DOM-element analog of the
//! terminal runtime.
//!
//! Registered custom tags inside a template mount as child instances: their
//! `.prop=${…}` and `?attr=${…}` bindings sync down on parent updates, their
//! `@<event>=${handler}` bindings route child notify events into the parent
//! behavior, and focus traverses parent and child widgets in template order.
//! Unregistered custom tags stay plain blocks (browser parity), and a child's
//! light-DOM template children are not projected.

use chrono::{Datelike, Days, Months, NaiveDate};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEvent, MouseEventKind};
use rat_widget::calendar::{selection::SingleSelection, MonthState};
use rat_widget::choice::ChoiceState;
use rat_widget::date_input::DateInputState;
use rat_widget::event::{CalOutcome, ChoiceOutcome, HandleEvent, Regular};
use rat_widget::popup::PopupCoreState;
use rat_widget::text_input::TextInputState;
use rat_widget::textarea::TextAreaState;
use ratatui::layout::{Position, Rect};
use uic_core::{
    attribute_to_value, notify_events, Behavior, Changed, ComponentDef, Ctx, CustomElementRegistry,
    NotifyEvent, PropertyMeta, PropertyStore, SelectOption, UiEvent, Value,
};
use uic_template::{AttrPart, Attribute, Expr, Node};

use crate::expand::{resolve_expr, widget_kind};
use crate::Error;

type Listener = Box<dyn FnMut(&NotifyEvent)>;

/// The calendar overlay attached to a date slot. `core.is_active()` is the
/// open flag; the anchor is the widget rect recorded during the paint pass.
pub(crate) struct DatePopup {
    pub core: PopupCoreState,
    pub month: MonthState<SingleSelection>,
    pub anchor: Rect,
}

impl DatePopup {
    fn new() -> Box<Self> {
        Box::new(DatePopup {
            core: PopupCoreState::new(),
            month: MonthState::new(),
            anchor: Rect::default(),
        })
    }
}

/// The persistent terminal widget behind a `data-tui` leaf.
/// The calendar state is boxed to keep the variants close in size.
pub(crate) enum WidgetState {
    Date {
        input: DateInputState,
        popup: Box<DatePopup>,
    },
    Text(TextInputState),
    /// A plain text widget; parsing and comma-decimal formatting are the
    /// component's job, like the browser's `type="text"` numeric input.
    Number(TextInputState),
    TextArea(Box<TextAreaState>),
    /// A dropdown select; the option list is data resolved at paint time
    /// (ADR 0006) and `ChoiceState` owns its popup state.
    Select(Box<ChoiceState<String>>),
}

impl WidgetState {
    pub(crate) fn set_focus(&mut self, focused: bool) {
        match self {
            WidgetState::Date { input, .. } => input.widget.focus.set(focused),
            WidgetState::Text(state) | WidgetState::Number(state) => state.focus.set(focused),
            WidgetState::TextArea(state) => state.focus.set(focused),
            WidgetState::Select(state) => state.focus.set(focused),
        }
    }

    /// True when the widget consumes Enter itself (newline instead of
    /// commit); such widgets commit on focus leave, like `@change` on blur.
    pub(crate) fn is_multiline(&self) -> bool {
        matches!(self, WidgetState::TextArea(_))
    }

    /// The screen cells the widget covered in the last paint, for pointer
    /// hit-testing.
    pub(crate) fn area(&self) -> Rect {
        match self {
            WidgetState::Date { input, .. } => input.widget.area,
            WidgetState::Text(state) | WidgetState::Number(state) => state.area,
            WidgetState::TextArea(state) => state.area,
            WidgetState::Select(state) => state.area,
        }
    }

    /// The value a commit hands to the change handler. The masked date input
    /// passes the normalized date (or the digit-bearing raw text); the plain
    /// text widgets pass their raw text — trimming, parsing and validation
    /// are the component's job, like in the browser.
    pub(crate) fn committed_text(&self) -> String {
        match self {
            WidgetState::Date { input, .. } => match input.value() {
                Ok(date) => date.format("%Y-%m-%d").to_string(),
                Err(_) => {
                    // The pristine mask is all zeros: commit it as empty,
                    // like an untouched browser input fires no change.
                    let raw = input.widget.text();
                    if raw.chars().any(|c| c.is_ascii_digit() && c != '0') {
                        raw.split_whitespace().collect::<Vec<_>>().join("")
                    } else {
                        String::new()
                    }
                }
            },
            WidgetState::Text(state) | WidgetState::Number(state) => state.text().to_string(),
            WidgetState::TextArea(state) => state.text(),
            WidgetState::Select(state) => state.value(),
        }
    }

    /// Pushes a property value into the widget.
    fn sync(&mut self, value: &Value) {
        match self {
            WidgetState::Date { input, popup } => match value {
                Value::Str(text) if !text.is_empty() => {
                    match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                        Ok(date) => {
                            input.set_value(date);
                            // An open calendar follows external value writes.
                            if popup.core.is_active() {
                                popup.month.set_start_date(date);
                                popup.month.select_date(date);
                            }
                        }
                        Err(_) => input.widget.set_text(text.clone()),
                    }
                }
                _ => input.clear(),
            },
            WidgetState::Text(state) | WidgetState::Number(state) => match value {
                Value::Str(text) if !text.is_empty() => state.set_text(text.clone()),
                _ => {
                    state.clear();
                }
            },
            WidgetState::TextArea(state) => match value {
                Value::Str(text) if !text.is_empty() => state.set_text(text),
                _ => {
                    state.clear();
                }
            },
            WidgetState::Select(state) => match value {
                // Empty is a legitimate select value (the null/default row).
                Value::Str(text) => {
                    state.set_value(text.clone());
                }
                _ => {
                    state.set_value(String::new());
                }
            },
        }
    }

    /// Forwards a terminal event to the widget's own handling. Returns true
    /// when the widget changed its committed value and wants a commit (a
    /// closed select's type-ahead, like the browser's dropdown navigation).
    pub(crate) fn handle(&mut self, focused: bool, event: &Event) -> bool {
        match self {
            WidgetState::Date { input, .. } => {
                let _ = rat_widget::date_input::handle_events(input, focused, event);
                false
            }
            WidgetState::Text(state) | WidgetState::Number(state) => {
                let _ = rat_widget::text_input::handle_events(state, focused, event);
                false
            }
            WidgetState::TextArea(state) => {
                let _ = rat_widget::textarea::handle_events(state, focused, event);
                false
            }
            WidgetState::Select(state) => {
                // Navigation keys are filtered while closed: opening goes
                // through the global F4/Down gate, and a closed select must
                // not spin its value. Printables (first-char type-ahead),
                // Space (opens) and Backspace/Delete still reach the widget.
                if let Event::Key(key) = event {
                    if key.kind == KeyEventKind::Press
                        && matches!(
                            key.code,
                            KeyCode::Up
                                | KeyCode::Down
                                | KeyCode::Home
                                | KeyCode::End
                                | KeyCode::PageUp
                                | KeyCode::PageDown
                        )
                    {
                        return false;
                    }
                }
                rat_widget::choice::handle_events(state, focused, event) == ChoiceOutcome::Value
            }
        }
    }
}

/// One interactive leaf of the template (`data-tui="…"`), with its persistent
/// widget state and the bindings wired in the template.
pub(crate) struct Slot {
    pub state: WidgetState,
    pub value_prop: Option<String>,
    /// `.options=${…}`: the option list a select resolves at paint time.
    pub options_prop: Option<String>,
    /// The `placeholder` attribute's parts, resolved at paint time and
    /// painted under an empty widget — rat has no placeholder notion.
    pub placeholder: Option<Vec<AttrPart>>,
    pub change_handler: Option<String>,
    pub disabled: Option<Expr>,
    /// The value last pushed into the widget — lit-style dirty check, so
    /// uncommitted typing survives unrelated updates and computed bindings
    /// re-sync only when their result actually changes.
    last_synced: Option<Value>,
}

impl Slot {
    pub(crate) fn is_disabled(&self, store: &PropertyStore, behavior: &dyn Behavior) -> bool {
        self.disabled
            .as_ref()
            .is_some_and(|expr| resolve_expr(expr, store, behavior).truthy())
    }
}

/// A registered custom element mounted inside a parent template, with the
/// bindings its custom tag carries.
pub(crate) struct ChildBinding {
    pub instance: ElementInstance,
    /// `.prop=${expr}`: parent expression → child property.
    prop_bindings: Vec<(Expr, &'static PropertyMeta)>,
    /// `?attr=${expr}`: parent expression → child property, as a boolean.
    bool_bindings: Vec<(Expr, &'static PropertyMeta)>,
    /// `@<event>=${handler}`: child notify event name → parent handler.
    event_bindings: Vec<(String, String)>,
}

/// Flat focus-order entry: an own widget slot or a nested child subtree
/// (whose focusables count recursively), in template order.
#[derive(Clone, Copy)]
enum Focusable {
    OwnSlot(usize),
    Child(usize),
}

pub struct ElementInstance {
    pub(crate) def: &'static ComponentDef,
    pub(crate) store: PropertyStore,
    pub(crate) behavior: Box<dyn Behavior>,
    pub(crate) slots: Vec<Slot>,
    pub(crate) children: Vec<ChildBinding>,
    focusables: Vec<Focusable>,
    /// Flat focus index over own slots and child subtrees.
    pub(crate) focused: usize,
    listeners: Vec<(String, Listener)>,
    /// Guard for parent/child update cascades; each level terminates via the
    /// store's no-change suppression, like Lit re-entrancy in the browser.
    cascade_depth: u8,
}

impl ElementInstance {
    /// Registry lookup + `connected` lifecycle — the terminal analog of
    /// attaching the element to a document. Nested registered custom tags
    /// mount recursively.
    pub(crate) fn mount(tag: &str) -> Result<Self, Error> {
        let def =
            CustomElementRegistry::get(tag).ok_or_else(|| Error::UnknownTag(tag.to_string()))?;
        let discovered = discover(&def.template().roots)?;
        let mut instance = ElementInstance {
            def,
            store: PropertyStore::new(def.properties),
            behavior: (def.new_behavior)(),
            slots: discovered.slots,
            children: discovered.children,
            focusables: discovered.focusables,
            focused: 0,
            listeners: Vec::new(),
            cascade_depth: 0,
        };
        instance.update_cycle(|behavior, ctx| behavior.connected(ctx));
        // Seed the widgets and bound child properties with the initial state;
        // nobody listens yet, so mount-time notify events are dropped.
        instance.sync_slots();
        instance.sync_children(None);
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

    /// Sets a property by its Rust name — the JS `el.prop = …` analog, and
    /// the only way in for property-only types like option lists.
    pub fn set_prop(&mut self, name: &str, value: impl Into<Value>) {
        let Some(meta) = self.def.property(name) else {
            return;
        };
        let rust_name = meta.rust_name;
        let value = value.into();
        self.update_cycle(|_, ctx| {
            ctx.set(rust_name, value);
        });
    }

    /// Subscribes to a notify event (`value-changed` …) of this element.
    pub fn on(&mut self, event: &str, listener: impl FnMut(&NotifyEvent) + 'static) {
        self.listeners.push((event.to_string(), Box::new(listener)));
    }

    /// Commits the focused widget's value through its `@change` handler,
    /// wherever it lives in the tree; child notify events route upward
    /// through the `@event` bindings along the path.
    pub(crate) fn commit_focused(&mut self) {
        let flat = self.focused;
        let _ = self.commit_flat(flat);
    }

    fn commit_flat(&mut self, mut flat: usize) -> Vec<NotifyEvent> {
        for index in 0..self.focusables.len() {
            match self.focusables[index] {
                Focusable::OwnSlot(slot) => {
                    if flat == 0 {
                        return self.commit_slot(slot);
                    }
                    flat -= 1;
                }
                Focusable::Child(child) => {
                    let len = self.children[child].instance.focus_len();
                    if flat < len {
                        let child_events = self.children[child].instance.commit_flat(flat);
                        return self.route_child_events(child, child_events);
                    }
                    flat -= len;
                }
            }
        }
        Vec::new()
    }

    fn commit_slot(&mut self, index: usize) -> Vec<NotifyEvent> {
        let Some(slot) = self.slots.get(index) else {
            return Vec::new();
        };
        if slot.is_disabled(&self.store, self.behavior.as_ref()) {
            return Vec::new();
        }
        let Some(handler) = slot.change_handler.clone() else {
            return Vec::new();
        };
        let event = UiEvent::change(slot.state.committed_text());
        self.update_cycle(|behavior, ctx| behavior.handle(ctx, &handler, &event))
    }

    /// The full ReactiveElement-style update cycle: apply `f`, run
    /// `will_update` (its sets join the same batch), emit notify events,
    /// push state into the widgets and bound children, then run `updated`
    /// on the committed state (its sets start a follow-up cycle).
    /// Returns the notify events for the caller (a parent instance routes
    /// them through its `@event` bindings).
    pub(crate) fn update_cycle(
        &mut self,
        f: impl FnOnce(&mut dyn Behavior, &mut Ctx),
    ) -> Vec<NotifyEvent> {
        self.cascade_depth += 1;
        debug_assert!(
            self.cascade_depth < 16,
            "runaway parent/child update cascade in <{}>",
            self.def.tag_name
        );
        let mut changed = Changed::default();
        {
            let mut ctx = Ctx::new(&mut self.store, &mut changed);
            f(self.behavior.as_mut(), &mut ctx);
        }
        // ReactiveElement's order per cycle: `will_update` before the commit
        // (the widget and child sync stand in for Lit's render), `updated`
        // after it. Writes inside `updated` request a follow-up cycle, like
        // the browser; the store's no-change suppression makes it converge.
        let mut events = Vec::new();
        let mut cycles: u8 = 0;
        loop {
            cycles += 1;
            debug_assert!(
                cycles < 16,
                "updated() keeps requesting follow-up cycles in <{}>",
                self.def.tag_name
            );
            let snapshot = changed.clone();
            {
                let mut ctx = Ctx::new(&mut self.store, &mut changed);
                self.behavior.will_update(&mut ctx, &snapshot);
            }
            let cycle_events = notify_events(self.def, &changed, &self.store);
            for event in &cycle_events {
                for (name, listener) in self.listeners.iter_mut() {
                    if name == &event.event_name {
                        listener(event);
                    }
                }
            }
            events.extend(cycle_events);
            self.sync_slots();
            events.extend(self.sync_children(Some(&changed)));
            let mut follow_up = Changed::default();
            {
                let mut ctx = Ctx::new(&mut self.store, &mut follow_up);
                self.behavior.updated(&mut ctx, &changed);
            }
            if follow_up.is_empty() {
                break;
            }
            changed = follow_up;
        }
        self.cascade_depth -= 1;
        events
    }

    /// Pushes `.value=${…}`-bound properties into the widget states.
    /// A lit-style dirty check (against the last pushed value) means only
    /// bindings whose resolved value actually changed are written, so
    /// uncommitted text in a widget survives unrelated updates — and
    /// computed bindings (`.value=${display_value}`) work without change
    /// tracking.
    pub(crate) fn sync_slots(&mut self) {
        for index in 0..self.slots.len() {
            let Some(prop) = self.slots[index].value_prop.clone() else {
                continue;
            };
            let value = if self.store.has(&prop) {
                self.store.get(&prop).clone()
            } else {
                self.behavior.compute(&self.store, &prop)
            };
            let slot = &mut self.slots[index];
            if slot.last_synced.as_ref() == Some(&value) {
                continue;
            }
            slot.state.sync(&value);
            slot.last_synced = Some(value);
        }
    }

    /// Pushes changed parent expressions into bound child properties, driving
    /// each affected child's update cycle, and routes the children's notify
    /// events back through this instance's `@event` bindings. Returns the
    /// notify events this instance emitted while routing.
    fn sync_children(&mut self, changed: Option<&Changed>) -> Vec<NotifyEvent> {
        let mut own_events = Vec::new();
        for index in 0..self.children.len() {
            let mut writes: Vec<(&'static str, Value)> = Vec::new();
            {
                let child = &self.children[index];
                for (expr, meta) in &child.prop_bindings {
                    if self.binding_triggered(expr, changed) {
                        writes.push((
                            meta.rust_name,
                            resolve_expr(expr, &self.store, self.behavior.as_ref()),
                        ));
                    }
                }
                for (expr, meta) in &child.bool_bindings {
                    if self.binding_triggered(expr, changed) {
                        let truthy =
                            resolve_expr(expr, &self.store, self.behavior.as_ref()).truthy();
                        writes.push((meta.rust_name, Value::Bool(truthy)));
                    }
                }
            }
            if writes.is_empty() {
                continue;
            }
            let child_events = self.children[index].instance.update_cycle(|_, ctx| {
                for (prop, value) in writes {
                    ctx.set(prop, value);
                }
            });
            own_events.extend(self.route_child_events(index, child_events));
        }
        own_events
    }

    /// A binding re-resolves when its ident is in the batch — or always, when
    /// it reads a computed getter (whose inputs are not tracked).
    fn binding_triggered(&self, expr: &Expr, changed: Option<&Changed>) -> bool {
        match changed {
            None => true,
            Some(changed) => changed.has(expr.ident()) || !self.store.has(expr.ident()),
        }
    }

    /// Dispatches a child's notify events into this instance's matching
    /// `@event=${handler}` bindings; each match runs a fresh update cycle
    /// (which may cascade further). The store's no-change suppression bounds
    /// the recursion, like Lit's in the browser.
    fn route_child_events(
        &mut self,
        child: usize,
        child_events: Vec<NotifyEvent>,
    ) -> Vec<NotifyEvent> {
        let mut own_events = Vec::new();
        for event in child_events {
            let handlers: Vec<String> = self.children[child]
                .event_bindings
                .iter()
                .filter(|(name, _)| *name == event.event_name)
                .map(|(_, handler)| handler.clone())
                .collect();
            if handlers.is_empty() {
                continue;
            }
            let ui_event = UiEvent::notify(&event);
            for handler in handlers {
                own_events.extend(
                    self.update_cycle(|behavior, ctx| behavior.handle(ctx, &handler, &ui_event)),
                );
            }
        }
        own_events
    }

    /// Widgets reachable by focus: own slots plus child subtrees, recursive.
    pub(crate) fn focus_len(&self) -> usize {
        self.focusables
            .iter()
            .map(|focusable| match focusable {
                Focusable::OwnSlot(_) => 1,
                Focusable::Child(child) => self.children[*child].instance.focus_len(),
            })
            .sum()
    }

    /// Resolves a flat focus index to the owning instance and its local slot.
    fn locate(&self, mut flat: usize) -> Option<(&ElementInstance, usize)> {
        for focusable in &self.focusables {
            match *focusable {
                Focusable::OwnSlot(slot) => {
                    if flat == 0 {
                        return Some((self, slot));
                    }
                    flat -= 1;
                }
                Focusable::Child(child) => {
                    let len = self.children[child].instance.focus_len();
                    if flat < len {
                        return self.children[child].instance.locate(flat);
                    }
                    flat -= len;
                }
            }
        }
        None
    }

    fn locate_mut(&mut self, mut flat: usize) -> Option<(&mut ElementInstance, usize)> {
        enum Hit {
            Own(usize),
            Child(usize, usize),
        }
        let mut hit = None;
        for focusable in &self.focusables {
            match *focusable {
                Focusable::OwnSlot(slot) => {
                    if flat == 0 {
                        hit = Some(Hit::Own(slot));
                        break;
                    }
                    flat -= 1;
                }
                Focusable::Child(child) => {
                    let len = self.children[child].instance.focus_len();
                    if flat < len {
                        hit = Some(Hit::Child(child, flat));
                        break;
                    }
                    flat -= len;
                }
            }
        }
        match hit? {
            Hit::Own(slot) => Some((self, slot)),
            Hit::Child(child, flat) => self.children[child].instance.locate_mut(flat),
        }
    }

    /// The child-index path and local slot behind a flat focus index, for the
    /// render pass (which resolves owners along `SlotRef` paths).
    pub(crate) fn locate_path(&self, flat: usize) -> Option<(Vec<usize>, usize)> {
        fn walk(
            instance: &ElementInstance,
            mut flat: usize,
            path: &mut Vec<usize>,
        ) -> Option<usize> {
            for focusable in &instance.focusables {
                match *focusable {
                    Focusable::OwnSlot(slot) => {
                        if flat == 0 {
                            return Some(slot);
                        }
                        flat -= 1;
                    }
                    Focusable::Child(child) => {
                        let len = instance.children[child].instance.focus_len();
                        if flat < len {
                            path.push(child);
                            return walk(&instance.children[child].instance, flat, path);
                        }
                        flat -= len;
                    }
                }
            }
            None
        }
        let mut path = Vec::new();
        let slot = walk(self, flat, &mut path)?;
        Some((path, slot))
    }

    /// Walks child bindings down to the instance a `SlotRef` path names.
    pub(crate) fn descend_mut(&mut self, path: &[usize]) -> &mut ElementInstance {
        path.iter().fold(self, |instance, &child| {
            &mut instance.children[child].instance
        })
    }

    fn flat_disabled(&self, flat: usize) -> bool {
        match self.locate(flat) {
            Some((owner, slot)) => {
                owner.slots[slot].is_disabled(&owner.store, owner.behavior.as_ref())
            }
            None => true,
        }
    }

    /// Moves focus to the next enabled widget, traversing into children in
    /// template order. Returns true when the cycle wrapped past the end, so
    /// a host with several roots can hand focus to the following one.
    pub(crate) fn focus_next(&mut self) -> bool {
        let count = self.focus_len();
        if count == 0 {
            return true;
        }
        for step in 1..=count {
            let candidate = (self.focused + step) % count;
            if !self.flat_disabled(candidate) {
                let wrapped = candidate <= self.focused;
                self.focused = candidate;
                return wrapped;
            }
        }
        true
    }

    /// Focuses the first enabled widget, for a root that just received focus.
    pub(crate) fn focus_first(&mut self) {
        self.focused = 0;
        for candidate in 0..self.focus_len() {
            if !self.flat_disabled(candidate) {
                self.focused = candidate;
                return;
            }
        }
    }

    /// Moves focus to the previous enabled widget — Shift+Tab's direction;
    /// the flat index walks backward through child subtrees on its own.
    /// Returns true when the cycle wrapped past the start, so a host with
    /// several roots can hand focus to the preceding one.
    pub(crate) fn focus_prev(&mut self) -> bool {
        let count = self.focus_len();
        if count == 0 {
            return true;
        }
        for step in 1..=count {
            let candidate = (self.focused + count - step) % count;
            if !self.flat_disabled(candidate) {
                let wrapped = candidate >= self.focused;
                self.focused = candidate;
                return wrapped;
            }
        }
        true
    }

    /// Focuses the last enabled widget, for a root receiving focus backward.
    pub(crate) fn focus_last(&mut self) {
        self.focused = 0;
        for candidate in (0..self.focus_len()).rev() {
            if !self.flat_disabled(candidate) {
                self.focused = candidate;
                return;
            }
        }
    }

    /// Places the caret under the pointer for the focused text-bearing
    /// widget (a drag extends the selection), or opens a select's list —
    /// the click semantics of the browser. rat's own mouse path stays
    /// unused everywhere: its click arming reads the system clock, which
    /// wasm32 does not have.
    pub(crate) fn place_cursor(&mut self, column: u16, row: u16, extend: bool) {
        let flat = self.focused;
        let Some((owner, slot)) = self.locate_mut(flat) else {
            return;
        };
        match &mut owner.slots[slot].state {
            WidgetState::Date { input, .. } => {
                let x = column as i16 - input.widget.area.x as i16;
                input.widget.set_screen_cursor(x, extend);
            }
            WidgetState::Text(state) | WidgetState::Number(state) => {
                let x = column as i16 - state.area.x as i16;
                state.set_screen_cursor(x, extend);
            }
            WidgetState::TextArea(state) => {
                let x = column as i16 - state.area.x as i16;
                let y = row as i16 - state.area.y as i16;
                state.set_screen_cursor((x, y), extend);
            }
            WidgetState::Select(state) => {
                if !extend && !state.is_popup_active() {
                    state.set_popup_active(true);
                    state.scroll_to_selected();
                }
            }
        }
    }

    /// The flat focus index of the enabled widget under the given screen
    /// cell, resolved against the areas recorded during the last paint.
    pub(crate) fn hit_test(&self, column: u16, row: u16) -> Option<usize> {
        let position = Position::new(column, row);
        (0..self.focus_len()).find(|&flat| {
            !self.flat_disabled(flat)
                && matches!(
                    self.locate(flat),
                    Some((owner, slot)) if owner.slots[slot].state.area().contains(position)
                )
        })
    }

    /// Forwards a terminal event to the focused widget, wherever it lives.
    /// A widget reporting a committed-value change (closed-select type-ahead)
    /// commits through the normal change-handler path.
    pub(crate) fn handle_focused(&mut self, event: &Event) {
        let flat = self.focused;
        if self.flat_disabled(flat) {
            return;
        }
        let committed = match self.locate_mut(flat) {
            Some((owner, slot)) => owner.slots[slot].state.handle(true, event),
            None => false,
        };
        if committed {
            self.commit_focused();
        }
    }

    /// True when the focused widget consumes Enter itself (a textarea).
    pub(crate) fn focused_multiline(&self) -> bool {
        match self.locate(self.focused) {
            Some((owner, slot)) => owner.slots[slot].state.is_multiline(),
            None => false,
        }
    }

    /// True when the focused widget is an enabled date input.
    pub(crate) fn focused_date_enabled(&self) -> bool {
        match self.locate(self.focused) {
            Some((owner, slot)) => {
                matches!(owner.slots[slot].state, WidgetState::Date { .. })
                    && !owner.slots[slot].is_disabled(&owner.store, owner.behavior.as_ref())
            }
            None => false,
        }
    }

    /// True when the focused widget is an enabled select.
    pub(crate) fn focused_select_enabled(&self) -> bool {
        match self.locate(self.focused) {
            Some((owner, slot)) => {
                matches!(owner.slots[slot].state, WidgetState::Select(_))
                    && !owner.slots[slot].is_disabled(&owner.store, owner.behavior.as_ref())
            }
            None => false,
        }
    }

    /// True when the focused widget's overlay (calendar or option list) is
    /// open.
    pub(crate) fn popup_open(&self) -> bool {
        match self.locate(self.focused) {
            Some((owner, slot)) => match &owner.slots[slot].state {
                WidgetState::Date { popup, .. } => popup.core.is_active(),
                WidgetState::Select(state) => state.is_popup_active(),
                _ => false,
            },
            None => false,
        }
    }

    /// Opens the focused widget's overlay: the calendar seeded from the
    /// widget's current date (falling back to today), or the option list
    /// scrolled to the current selection.
    pub(crate) fn open_popup(&mut self) {
        let flat = self.focused;
        if self.flat_disabled(flat) {
            return;
        }
        let Some((owner, slot)) = self.locate_mut(flat) else {
            return;
        };
        match &mut owner.slots[slot].state {
            WidgetState::Date { input, popup } => {
                let seed = input
                    .value()
                    .ok()
                    .unwrap_or_else(|| chrono::Local::now().date_naive());
                popup.month.set_start_date(seed);
                popup.month.select_date(seed);
                popup.month.focus.set(true);
                popup.core.set_active(true);
            }
            WidgetState::Select(state) => {
                state.set_popup_active(true);
                state.scroll_to_selected();
            }
            _ => {}
        }
    }

    pub(crate) fn close_popup(&mut self) {
        let flat = self.focused;
        if let Some((owner, slot)) = self.locate_mut(flat) {
            match &mut owner.slots[slot].state {
                WidgetState::Date { popup, .. } => {
                    popup.core.set_active(false);
                    popup.month.focus.set(false);
                    popup.core.clear_areas();
                }
                WidgetState::Select(state) => {
                    state.set_popup_active(false);
                    state.popup.clear_areas();
                }
                _ => {}
            }
        }
    }

    /// Routes a key press while an overlay is open (overlays are modal).
    /// Returns whether the event was consumed; Tab closes and reports
    /// unconsumed so the global commit-and-focus handling still runs.
    pub(crate) fn handle_popup_event(&mut self, event: &Event) -> bool {
        if let Event::Mouse(mouse) = event {
            return self.handle_popup_mouse(*mouse);
        }
        let Event::Key(key) = event else {
            return false;
        };
        if key.kind != KeyEventKind::Press {
            return true;
        }
        let select_open = matches!(
            self.locate(self.focused),
            Some((owner, slot)) if matches!(owner.slots[slot].state, WidgetState::Select(_))
        );
        if select_open {
            return self.handle_select_popup_event(event, key.code);
        }
        match key.code {
            KeyCode::Esc => {
                self.close_popup();
                true
            }
            KeyCode::Tab => {
                self.close_popup();
                false
            }
            KeyCode::Enter => {
                let flat = self.focused;
                let mut picked = false;
                if let Some((owner, slot)) = self.locate_mut(flat) {
                    if let WidgetState::Date { input, popup } = &mut owner.slots[slot].state {
                        if let Some(date) = popup.month.selected_date() {
                            input.set_value(date);
                            picked = true;
                        }
                    }
                }
                self.close_popup();
                if picked {
                    self.commit_focused();
                }
                true
            }
            KeyCode::PageUp => {
                self.shift_popup_month(-1);
                true
            }
            KeyCode::PageDown => {
                self.shift_popup_month(1);
                true
            }
            code @ (KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down) => {
                let flat = self.focused;
                let Some((owner, slot)) = self.locate_mut(flat) else {
                    return true;
                };
                let WidgetState::Date { popup, .. } = &mut owner.slots[slot].state else {
                    return true;
                };
                if popup.month.handle(event, Regular) == CalOutcome::Continue {
                    // The month widget stops at its edges; roll over into the
                    // neighboring month like the browser's date picker.
                    if let Some(selected) = popup.month.selected_date() {
                        let target = match code {
                            KeyCode::Left => selected.checked_sub_days(Days::new(1)),
                            KeyCode::Right => selected.checked_add_days(Days::new(1)),
                            KeyCode::Up => selected.checked_sub_days(Days::new(7)),
                            KeyCode::Down => selected.checked_add_days(Days::new(7)),
                            _ => None,
                        };
                        if let Some(target) = target {
                            popup.month.set_start_date(target);
                            popup.month.select_date(target);
                        }
                    }
                }
                true
            }
            _ => {
                let flat = self.focused;
                if let Some((owner, slot)) = self.locate_mut(flat) {
                    if let WidgetState::Date { popup, .. } = &mut owner.slots[slot].state {
                        let _ = popup.month.handle(event, Regular);
                    }
                }
                true
            }
        }
    }

    /// Routes the pointer while an overlay is open: a click picks the day or
    /// option under it (committing like Enter), the wheel and drags browse,
    /// and a press outside dismisses the overlay and reports unconsumed so
    /// the click still focuses whatever it landed on.
    fn handle_popup_mouse(&mut self, mouse: MouseEvent) -> bool {
        enum Overlay {
            Date,
            Select,
        }
        let position = Position::new(mouse.column, mouse.row);
        let flat = self.focused;
        let (overlay, inside) = match self.locate(flat) {
            Some((owner, slot)) => match &owner.slots[slot].state {
                WidgetState::Date { popup, .. } => {
                    (Overlay::Date, popup.core.area.contains(position))
                }
                WidgetState::Select(state) => {
                    (Overlay::Select, state.popup.area.contains(position))
                }
                _ => return false,
            },
            None => return false,
        };
        if !inside {
            if matches!(mouse.kind, MouseEventKind::Down(_)) {
                if matches!(overlay, Overlay::Select) {
                    self.revert_select();
                }
                self.close_popup();
                return false;
            }
            return true;
        }
        // Picks resolve against the overlay's published geometry (day rects,
        // option rows) instead of rat's mouse handling — see [`Self::place_cursor`].
        match overlay {
            Overlay::Date => match mouse.kind {
                MouseEventKind::Down(_) => {
                    let mut picked = None;
                    if let Some((owner, slot)) = self.locate_mut(flat) {
                        if let WidgetState::Date { popup, .. } = &mut owner.slots[slot].state {
                            let start = popup.month.start_date();
                            picked = popup
                                .month
                                .area_days
                                .iter()
                                .position(|day| day.contains(position))
                                .and_then(|index| start.with_day(index as u32 + 1));
                        }
                    }
                    if let Some(date) = picked {
                        if let Some((owner, slot)) = self.locate_mut(flat) {
                            if let WidgetState::Date { input, .. } = &mut owner.slots[slot].state {
                                input.set_value(date);
                            }
                        }
                        self.close_popup();
                        self.commit_focused();
                    }
                    true
                }
                MouseEventKind::ScrollUp => {
                    self.shift_popup_month(-1);
                    true
                }
                MouseEventKind::ScrollDown => {
                    self.shift_popup_month(1);
                    true
                }
                _ => true,
            },
            Overlay::Select => match mouse.kind {
                MouseEventKind::Down(_) => {
                    let mut picked = false;
                    if let Some((owner, slot)) = self.locate_mut(flat) {
                        if let WidgetState::Select(state) = &mut owner.slots[slot].state {
                            if let Some(row) = state
                                .item_areas
                                .iter()
                                .position(|item| item.contains(position))
                            {
                                let _ = state.select(state.offset() + row);
                                picked = true;
                            }
                        }
                    }
                    if picked {
                        self.close_popup();
                        self.commit_focused();
                    }
                    true
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    // The wheel scrolls the list window without moving the
                    // selection, like the browser's open dropdown.
                    if let Some((owner, slot)) = self.locate_mut(flat) {
                        if let WidgetState::Select(state) = &mut owner.slots[slot].state {
                            let offset = state.offset();
                            let target = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                                offset.saturating_sub(1)
                            } else {
                                offset.saturating_add(1)
                            };
                            let _ = state.set_offset(target);
                        }
                    }
                    true
                }
                _ => true,
            },
        }
    }

    /// Routes a key press while the option list is open. Browsing (arrows,
    /// paging, type-ahead) mutates the widget value silently; Enter commits,
    /// Esc reverts to the bound value, Tab closes and falls through so the
    /// global handling commits the browsed value and advances focus.
    fn handle_select_popup_event(&mut self, event: &Event, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc => {
                self.revert_select();
                self.close_popup();
                true
            }
            KeyCode::Tab => {
                self.close_popup();
                false
            }
            KeyCode::Enter => {
                self.close_popup();
                self.commit_focused();
                true
            }
            _ => {
                let flat = self.focused;
                if let Some((owner, slot)) = self.locate_mut(flat) {
                    if let WidgetState::Select(state) = &mut owner.slots[slot].state {
                        let _ = rat_widget::choice::handle_events(state, true, event);
                    }
                }
                true
            }
        }
    }

    /// Restores the focused select's widget value from its bound property —
    /// rat's browsing mutates the value continuously, so Esc reverts like
    /// the browser's dropdown.
    fn revert_select(&mut self) {
        let flat = self.focused;
        let Some((owner, slot)) = self.locate_mut(flat) else {
            return;
        };
        let Some(prop) = owner.slots[slot].value_prop.clone() else {
            return;
        };
        let value = if owner.store.has(&prop) {
            owner.store.get(&prop).clone()
        } else {
            owner.behavior.compute(&owner.store, &prop)
        };
        if let WidgetState::Select(state) = &mut owner.slots[slot].state {
            let text = match &value {
                Value::Str(text) => text.clone(),
                _ => String::new(),
            };
            state.set_value(text);
        }
    }

    /// Pages the open calendar by whole months, keeping the selected
    /// day-of-month (clamped to the target month's length).
    fn shift_popup_month(&mut self, months: i32) {
        let flat = self.focused;
        let Some((owner, slot)) = self.locate_mut(flat) else {
            return;
        };
        let WidgetState::Date { popup, .. } = &mut owner.slots[slot].state else {
            return;
        };
        let base = popup
            .month
            .selected_date()
            .unwrap_or_else(|| popup.month.start_date());
        let target = if months < 0 {
            base.checked_sub_months(Months::new(months.unsigned_abs()))
        } else {
            base.checked_add_months(Months::new(months as u32))
        };
        if let Some(target) = target {
            popup.month.set_start_date(target);
            popup.month.select_date(target);
        }
    }
}

struct Discovered {
    slots: Vec<Slot>,
    children: Vec<ChildBinding>,
    focusables: Vec<Focusable>,
}

/// Collects the interactive leaves and nested registered custom elements in
/// template order (all branches), wiring each one's bindings.
fn discover(nodes: &[Node]) -> Result<Discovered, Error> {
    let mut discovered = Discovered {
        slots: Vec::new(),
        children: Vec::new(),
        focusables: Vec::new(),
    };
    collect(nodes, &mut discovered)?;
    Ok(discovered)
}

fn collect(nodes: &[Node], discovered: &mut Discovered) -> Result<(), Error> {
    for node in nodes {
        match node {
            Node::Element(el) => {
                if let Some(kind) = widget_kind(el) {
                    discovered
                        .focusables
                        .push(Focusable::OwnSlot(discovered.slots.len()));
                    discovered.slots.push(new_slot(kind, el)?);
                    collect(&el.children, discovered)?;
                } else if let Some(binding) = child_binding(el)? {
                    discovered
                        .focusables
                        .push(Focusable::Child(discovered.children.len()));
                    discovered.children.push(binding);
                    // A registered child renders its own template; its
                    // light-DOM children are not projected.
                } else {
                    collect(&el.children, discovered)?;
                }
            }
            Node::If { then, .. } => collect(then, discovered)?,
            Node::Text(_) | Node::TextHole(_) => {}
        }
    }
    Ok(())
}

/// Mounts a nested registered custom element and wires the bindings on its
/// tag; unregistered custom tags return `None` and stay plain blocks.
fn child_binding(el: &uic_template::Element) -> Result<Option<ChildBinding>, Error> {
    if !el.is_custom() || CustomElementRegistry::get(&el.tag).is_none() {
        return Ok(None);
    }
    let mut instance = ElementInstance::mount(&el.tag)?;
    let mut prop_bindings = Vec::new();
    let mut bool_bindings = Vec::new();
    let mut event_bindings = Vec::new();
    for attr in &el.attrs {
        match attr {
            Attribute::Static { name, value } if name != "class" => {
                instance.set_attr(name, value);
            }
            Attribute::Prop { name, expr } => {
                if let Some(meta) = instance.def.property_by_js_name(name) {
                    prop_bindings.push((expr.clone(), meta));
                }
            }
            Attribute::Bool { name, expr } => {
                if let Some(meta) = instance.def.property_by_attribute(name) {
                    bool_bindings.push((expr.clone(), meta));
                }
            }
            Attribute::Event { name, handler } => {
                event_bindings.push((name.clone(), handler.clone()));
            }
            Attribute::Static { .. } | Attribute::Attr { .. } => {}
        }
    }
    Ok(Some(ChildBinding {
        instance,
        prop_bindings,
        bool_bindings,
        event_bindings,
    }))
}

fn new_slot(kind: &str, el: &uic_template::Element) -> Result<Slot, Error> {
    let state = match kind {
        "date-input" => WidgetState::Date {
            input: DateInputState::new()
                .with_pattern("%Y-%m-%d")
                .map_err(|err| Error::Pattern(err.to_string()))?,
            popup: DatePopup::new(),
        },
        "text-input" => WidgetState::Text(TextInputState::new()),
        "number-input" => WidgetState::Number(TextInputState::new()),
        "text-area" => WidgetState::TextArea(Box::new(TextAreaState::new())),
        "select" => WidgetState::Select(Box::new(ChoiceState::new())),
        _ => return Err(Error::UnknownWidget(kind.to_string())),
    };
    let mut slot = Slot {
        state,
        value_prop: None,
        options_prop: None,
        placeholder: None,
        change_handler: None,
        disabled: None,
        last_synced: None,
    };
    for attr in &el.attrs {
        match attr {
            Attribute::Prop { name, expr } if name == "value" => {
                slot.value_prop = Some(expr.ident().to_string());
            }
            Attribute::Prop { name, expr } if name == "options" => {
                slot.options_prop = Some(expr.ident().to_string());
            }
            Attribute::Static { name, value } if name == "placeholder" => {
                slot.placeholder = Some(vec![AttrPart::Static(value.clone())]);
            }
            Attribute::Attr { name, parts } if name == "placeholder" => {
                slot.placeholder = Some(parts.clone());
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

/// Resolves an `.options` binding to its list (store or computed), for the
/// paint pass and the select widget's item supply.
pub(crate) fn resolve_options(
    store: &PropertyStore,
    behavior: &dyn Behavior,
    prop: &str,
) -> Vec<SelectOption> {
    let value = if store.has(prop) {
        store.get(prop).clone()
    } else {
        behavior.compute(store, prop)
    };
    match value {
        Value::Options(options) => options,
        _ => Vec::new(),
    }
}
