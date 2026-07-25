//! The document state a scripted host operates on — the retained document,
//! the focus, and the JS↔node handle table — plus the flat operations every
//! host exposes to the runtime modules: Boa natives on real terminals
//! (`uic_js`), the browser's own engine against the wasm session
//! (`uic_tui_web::DomSession`). One body per operation, two thin wrappers.

use std::collections::HashMap;

use crossterm::event::Event;
use uic_core::Value;
use uic_dom::{NodeData, NodeId};

use super::widget::{self, WidgetBox};
use super::DomDocument;
use crate::KeyStroke;

/// A widget in flight across a subtree swap: the `data-path` key and the
/// boxed state itself — kind and variant ride inside the box.
type StashedWidget = (String, WidgetBox);

/// The document and the JS↔node handle table, shared with the host's
/// native functions.
pub struct HostState {
    pub doc: DomDocument,
    pub focused: Option<NodeId>,
    pub dirty: bool,
    handles: Vec<NodeId>,
    handle_of: HashMap<NodeId, usize>,
    /// The focused widget a subtree swap orphaned, waiting for the node
    /// that re-renders it: a keystroke in a nested input is two commits —
    /// the parent's swap destroys the input, the child's own commit
    /// re-creates it a microtask later. One slot, keyed like focus
    /// survival by `data-path` (plus the box's own kind and variant).
    stash: Option<StashedWidget>,
}

impl Default for HostState {
    fn default() -> Self {
        Self::new()
    }
}

impl HostState {
    pub fn new() -> Self {
        HostState {
            doc: DomDocument::new(),
            focused: None,
            dirty: false,
            handles: Vec::new(),
            handle_of: HashMap::new(),
            stash: None,
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

    /// Creates and appends the host element — the node half of a mount;
    /// the runtime's `__uicMount` upgrades it afterwards.
    pub fn create_root(&mut self, tag: &str, attrs: &[(&str, &str)]) -> usize {
        let root = self.doc.root();
        let node = self.doc.create_element_named(tag);
        for (name, value) in attrs {
            self.doc.set_attribute(node, name, value);
        }
        self.doc.append_child(root, node);
        self.dirty = true;
        self.handle(node)
    }

    /// Replaces the element's children with the parsed fragment — the
    /// subtree-swap render path. Focus inside the swapped subtree survives
    /// by its `data-path`, the component's own stable row key; mounted
    /// widgets survive the same way, so typing and caret outlive the
    /// re-render that echoes them.
    pub fn commit(&mut self, handle: usize, html: &str) {
        let Some(target) = self.node(handle) else {
            return;
        };
        let focus_path = self
            .focused
            .filter(|&f| f == target || self.doc.ancestors(f).any(|node| node == target));
        let focus_path =
            focus_path.and_then(|f| self.doc.attribute(f, "data-path").map(str::to_string));
        // The outgoing widgets leave keyed by data-path before the swap
        // destroys their nodes; the focused one may have to wait in the
        // stash for a later commit (nested inputs re-render in two).
        let mut harvest: Vec<(StashedWidget, bool)> = Vec::new();
        let outgoing: Vec<NodeId> = self.doc.descendants(target).skip(1).collect();
        for node in outgoing {
            let Some(path) = self.doc.attribute(node, "data-path").map(str::to_string) else {
                continue;
            };
            let was_focused = self.focused == Some(node);
            if let Some(widget) = self
                .doc
                .element_mut(node)
                .and_then(|el| el.data.widget.take())
            {
                harvest.push(((path, widget), was_focused));
            }
        }
        let scratch: DomDocument = uic_dom::Document::parse_fragment(html, "body");
        let children: Vec<NodeId> = self.doc.children(target).collect();
        for child in children {
            self.doc.remove(child);
        }
        let sources: Vec<NodeId> = scratch.children(scratch.root()).collect();
        let mut map = HashMap::new();
        for source in sources {
            if let Some(copy) = self.doc.import_node(&scratch, source, &mut map) {
                self.doc.append_child(target, copy);
            }
        }
        if let Some(focused) = self.focused {
            if self.doc.node(focused).is_none() {
                let resolved = focus_path.and_then(|path| {
                    self.doc
                        .descendants(target)
                        .find(|&node| self.doc.attribute(node, "data-path") == Some(path.as_str()))
                });
                self.focused = Some(resolved.unwrap_or(target));
            }
        }
        widget::mount_widgets(&mut self.doc, target);
        for ((path, widget), was_focused) in harvest {
            match self.widget_slot(target, &path, &widget) {
                Some(node) => self.install_widget(node, widget),
                None if was_focused => self.stash = Some((path, widget)),
                None => {}
            }
        }
        if let Some((path, widget)) = self.stash.take() {
            match self.widget_slot(target, &path, &widget) {
                Some(node) => self.install_widget(node, widget),
                // Not this commit — keep waiting for the input's own render.
                None => self.stash = Some((path, widget)),
            }
        }
        // The value channel: the serializer mirrors `.value=` on
        // value-carrying tags as the `value` attribute; syncing it here is
        // the scripted hosts' property write, echo-skipped so a component
        // echoing back typed text never moves the caret. No attribute
        // means an uncontrolled input — the transplanted text stays.
        let controlled: Vec<(NodeId, String)> = self
            .doc
            .descendants(target)
            .skip(1)
            .filter_map(|node| {
                let el = self.doc.element(node)?;
                el.data.widget.as_ref()?;
                Some((node, el.attr("value")?.to_string()))
            })
            .collect();
        for (node, value) in controlled {
            if let Some(widget) = self
                .doc
                .element_mut(node)
                .and_then(|el| el.data.widget.as_mut())
            {
                widget.sync_committed(&Value::Str(value));
            }
        }
        self.dirty = true;
    }

    /// The fresh node a harvested widget belongs on — the same `data-path`
    /// key focus survival uses, plus a freshly mounted widget of the same
    /// kind and variant (the mount already ran, so detection has spoken).
    fn widget_slot(&self, target: NodeId, path: &str, widget: &WidgetBox) -> Option<NodeId> {
        self.doc.descendants(target).skip(1).find(|&node| {
            self.doc.element(node).is_some_and(|el| {
                el.attr("data-path") == Some(path)
                    && el.data.widget.as_ref().is_some_and(|fresh| {
                        fresh.kind == widget.kind && fresh.variant == widget.variant
                    })
            })
        })
    }

    fn install_widget(&mut self, node: NodeId, widget: WidgetBox) {
        if let Some(el) = self.doc.element_mut(node) {
            el.data.widget = Some(widget);
        }
    }

    pub fn attribute(&self, handle: usize, name: &str) -> Option<String> {
        self.node(handle)
            .and_then(|node| self.doc.attribute(node, name).map(str::to_string))
    }

    pub fn set_attribute(&mut self, handle: usize, name: &str, value: &str) {
        if let Some(node) = self.node(handle) {
            self.doc.set_attribute(node, name, value);
            self.dirty = true;
        }
    }

    pub fn has_attribute(&self, handle: usize, name: &str) -> bool {
        self.node(handle)
            .is_some_and(|node| self.doc.attribute(node, name).is_some())
    }

    pub fn remove_attribute(&mut self, handle: usize, name: &str) {
        if let Some(node) = self.node(handle) {
            self.doc.remove_attribute(node, name);
            self.dirty = true;
        }
    }

    pub fn text(&self, handle: usize) -> String {
        self.node(handle)
            .map(|node| self.doc.text_content(node))
            .unwrap_or_default()
    }

    /// `__uic_query`: descendants matching the selector, as handles. The
    /// selector engine is the cascade's own — servo's parser and matcher
    /// through uic_css (the ADR 0021 follow-up that retired the attribute
    /// micro-matcher).
    pub fn query(&mut self, handle: usize, selector: &str) -> Result<Vec<usize>, String> {
        let list = uic_css::parse_selector_list(selector)?;
        let Some(root) = self.node(handle) else {
            return Ok(Vec::new());
        };
        let focused = self.focused;
        let nodes: Vec<NodeId> = self
            .doc
            .descendants(root)
            .filter(|&node| uic_css::matches(&self.doc, node, &list, None, focused))
            .collect();
        Ok(nodes.into_iter().map(|node| self.handle(node)).collect())
    }

    /// Whether the element matches the selector — the same engine the
    /// cascade uses, `:focus` fed from the host's focus.
    pub fn matches(&self, handle: usize, selector: &str) -> Result<bool, String> {
        let Some(node) = self.node(handle) else {
            return Ok(false);
        };
        let list = uic_css::parse_selector_list(selector)?;
        Ok(uic_css::matches(&self.doc, node, &list, None, self.focused))
    }

    pub fn contains(&self, outer: usize, inner: usize) -> bool {
        let (Some(outer), Some(inner)) = (self.node(outer), self.node(inner)) else {
            return false;
        };
        outer == inner || self.doc.ancestors(inner).any(|node| node == outer)
    }

    /// The nearest element parent's handle.
    pub fn parent(&mut self, handle: usize) -> Option<usize> {
        let parent = self.node(handle).and_then(|node| self.doc.parent(node));
        parent
            .filter(|&p| matches!(self.doc.node(p), Some(NodeData::Element(_))))
            .map(|p| self.handle(p))
    }

    pub fn focused_handle(&mut self) -> Option<usize> {
        self.focused.map(|node| self.handle(node))
    }

    pub fn set_focused_handle(&mut self, handle: Option<usize>) {
        self.focused = handle.and_then(|h| self.node(h));
        self.dirty = true;
    }

    /// Routes a key into the focused node's widget — the editing default
    /// action the browser runs after an uncancelled keydown. Returns the
    /// new live text when the key changed it; the host synthesizes the
    /// bubbling `input` event from that.
    pub fn widget_default_action(&mut self, stroke: &KeyStroke) -> Option<String> {
        let node = self.focused?;
        if self.doc.attribute(node, "disabled").is_some() {
            return None;
        }
        let event = Event::Key(stroke.to_crossterm()?);
        let widget = self
            .doc
            .element_mut(node)
            .and_then(|el| el.data.widget.as_mut())?;
        let before = widget.adapter.committed_text();
        widget.adapter.handle(true, &event);
        let after = widget.adapter.committed_text();
        self.dirty = true;
        (after != before).then_some(after)
    }

    /// The focused-input facade: the widget's live text behind `el.value`.
    pub fn widget_value(&self, handle: usize) -> Option<String> {
        let node = self.node(handle)?;
        let widget = self.doc.element(node)?.data.widget.as_ref()?;
        Some(widget.adapter.committed_text())
    }

    /// `el.value = …` from scripts — echo-skipped like the commit sync.
    pub fn set_widget_value(&mut self, handle: usize, text: &str) {
        let Some(node) = self.node(handle) else {
            return;
        };
        if let Some(widget) = self
            .doc
            .element_mut(node)
            .and_then(|el| el.data.widget.as_mut())
        {
            widget.sync_committed(&Value::Str(text.to_string()));
            self.dirty = true;
        }
    }

    /// Whether the node carries a mounted widget — the click-focus guard.
    pub fn has_widget(&self, handle: usize) -> bool {
        self.node(handle)
            .and_then(|node| self.doc.element(node))
            .is_some_and(|el| el.data.widget.is_some())
    }

    /// Places the caret under the pointer — the browser's click semantics.
    pub fn place_caret(&mut self, handle: usize, column: u16, row: u16) {
        let Some(node) = self.node(handle) else {
            return;
        };
        if let Some(widget) = self
            .doc
            .element_mut(node)
            .and_then(|el| el.data.widget.as_mut())
        {
            widget.adapter.place_cursor(column, row, false);
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HostState;
    use crate::KeyStroke;

    fn typed(state: &mut HostState, keys: &str) {
        for c in keys.chars() {
            state.widget_default_action(&KeyStroke::new(c.to_string()));
        }
    }

    #[test]
    fn commit_mounts_widgets_and_keys_route_into_the_focused_one() {
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(root, r#"<input data-path="f" value="">"#);
        let input = state.query(root, "input").unwrap()[0];
        assert!(state.has_widget(input));
        // Unfocused: the default action has no target.
        assert_eq!(state.widget_default_action(&KeyStroke::new("h")), None);
        state.set_focused_handle(Some(input));
        assert_eq!(
            state.widget_default_action(&KeyStroke::new("h")),
            Some("h".into())
        );
        assert_eq!(
            state.widget_default_action(&KeyStroke::new("i")),
            Some("hi".into())
        );
        assert_eq!(state.widget_value(input), Some("hi".into()));
        // Enter and ArrowUp change no text — keydown-only keys.
        assert_eq!(state.widget_default_action(&KeyStroke::new("Enter")), None);
        assert_eq!(
            state.widget_default_action(&KeyStroke::new("ArrowUp")),
            None
        );
        // A caret move changes no text either, but repositions typing.
        assert_eq!(
            state.widget_default_action(&KeyStroke::new("ArrowLeft")),
            None
        );
        assert_eq!(
            state.widget_default_action(&KeyStroke::new("X")),
            Some("hXi".into())
        );
    }

    #[test]
    fn the_swap_transplants_the_widget_and_echoes_skip_the_caret() {
        // This one stays on data-tui: it pins the explicit override path
        // beside the element-type detection the other tests exercise.
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(
            root,
            r#"<input data-tui="text-input" data-path="f" value="">"#,
        );
        let input = state.query(root, "input").unwrap()[0];
        state.set_focused_handle(Some(input));
        typed(&mut state, "abc");
        state.widget_default_action(&KeyStroke::new("ArrowLeft"));
        // The component echoes the typed text back — same value attribute.
        state.commit(
            root,
            r#"<input data-tui="text-input" data-path="f" value="abc">"#,
        );
        let fresh = state.query(root, "input").unwrap()[0];
        assert_eq!(state.widget_value(fresh), Some("abc".into()));
        // The caret survived at position 2: typing lands mid-string. A
        // clobbering set_text would have parked it at the end.
        state.widget_default_action(&KeyStroke::new("X"));
        assert_eq!(state.widget_value(fresh), Some("abXc".into()));
        // A genuinely different value replaces the text (remote edits win).
        state.commit(
            root,
            r#"<input data-tui="text-input" data-path="f" value="xyz">"#,
        );
        let fresh = state.query(root, "input").unwrap()[0];
        assert_eq!(state.widget_value(fresh), Some("xyz".into()));
    }

    #[test]
    fn an_uncontrolled_input_keeps_its_text_across_the_swap() {
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(root, r#"<p>before</p><input data-path="f">"#);
        let input = state.query(root, "input").unwrap()[0];
        state.set_focused_handle(Some(input));
        typed(&mut state, "kept");
        state.commit(root, r#"<p>after</p><input data-path="f">"#);
        let fresh = state.query(root, "input").unwrap()[0];
        assert_eq!(state.widget_value(fresh), Some("kept".into()));
    }

    #[test]
    fn the_stash_carries_the_focused_widget_across_an_absent_commit() {
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(root, r#"<input data-path="edit" value="">"#);
        let input = state.query(root, "input").unwrap()[0];
        state.set_focused_handle(Some(input));
        typed(&mut state, "mid");
        state.widget_default_action(&KeyStroke::new("ArrowLeft"));
        // The parent commit drops the input entirely (the nested child
        // renders it a beat later) — the focused widget waits in the stash.
        state.commit(root, r#"<todo-row data-path="row"></todo-row>"#);
        assert!(state.query(root, "input").unwrap().is_empty());
        // The child's own commit re-renders the input: text AND caret back.
        state.commit(root, r#"<input data-path="edit" value="mid">"#);
        let fresh = state.query(root, "input").unwrap()[0];
        state.set_focused_handle(Some(fresh));
        assert_eq!(state.widget_value(fresh), Some("mid".into()));
        state.widget_default_action(&KeyStroke::new("X"));
        assert_eq!(state.widget_value(fresh), Some("miXd".into()));
    }

    #[test]
    fn a_disabled_input_swallows_no_keys() {
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(root, r#"<input data-path="f" disabled value="">"#);
        let input = state.query(root, "input").unwrap()[0];
        state.set_focused_handle(Some(input));
        assert_eq!(state.widget_default_action(&KeyStroke::new("h")), None);
        assert_eq!(state.widget_value(input), Some("".into()));
    }

    #[test]
    fn detection_mounts_by_element_type() {
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(
            root,
            r#"<input data-path="t"><textarea data-path="a"></textarea><select data-path="s"></select><input type="checkbox" data-path="c"><select tabindex="-1" data-path="front"></select>"#,
        );
        let widget_at = |state: &mut HostState, selector: &str| {
            let handle = state.query(root, selector).unwrap()[0];
            state.has_widget(handle)
        };
        assert!(widget_at(&mut state, r#"[data-path="t"]"#));
        assert!(widget_at(&mut state, "textarea"));
        assert!(widget_at(&mut state, r#"[data-path="s"]"#));
        // Controls and presentation twins stay plain elements.
        assert!(!widget_at(&mut state, r#"[data-path="c"]"#));
        assert!(!widget_at(&mut state, r#"[data-path="front"]"#));
    }

    #[test]
    fn a_plain_date_input_carries_iso_dates() {
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(
            root,
            r#"<input type="date" data-path="d" value="2026-07-24">"#,
        );
        let input = state.query(root, "input").unwrap()[0];
        assert!(state.has_widget(input));
        assert_eq!(state.widget_value(input), Some("2026-07-24".into()));
    }

    #[test]
    fn a_type_flip_recreates_the_widget() {
        let mut state = HostState::new();
        let root = state.create_root("x-app", &[]);
        state.commit(root, r#"<input data-path="f" value="">"#);
        let input = state.query(root, "input").unwrap()[0];
        state.set_focused_handle(Some(input));
        typed(&mut state, "abc");
        // The same data-path re-renders as a date input: a different kind,
        // so the typed text resets with the fresh adapter — a kind flip is
        // configuration, like a variant flip.
        state.commit(root, r#"<input type="date" data-path="f" value="">"#);
        let fresh = state.query(root, "input").unwrap()[0];
        assert!(state.has_widget(fresh));
        assert_ne!(state.widget_value(fresh), Some("abc".into()));
    }

    #[test]
    fn the_selector_surface_is_the_cascade_engine() {
        let mut state = HostState::new();
        let root = state.create_root("x-demo", &[("class", "host")]);
        // Full selectors now serve the facades: classes, descendants, dir.
        assert!(state.matches(root, ".host").unwrap());
        assert!(state.matches(root, ":dir(ltr)").unwrap());
        assert!(!state.matches(root, ":dir(rtl)").unwrap());
        assert!(state.matches(root, "x-demo.host").unwrap());
        // Garbage still errors loudly instead of mismatching. (An
        // unterminated bracket is NOT garbage — CSS error recovery closes
        // it at end of input.)
        assert!(state.query(root, "~").is_err());
        assert!(state.matches(root, ":nonsense-pseudo").is_err());
    }
}
