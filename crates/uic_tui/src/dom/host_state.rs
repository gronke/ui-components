//! The document state a scripted host operates on — the retained document,
//! the focus, and the JS↔node handle table — plus the flat operations every
//! host exposes to the runtime modules: Boa natives on real terminals
//! (`uic_js`), the browser's own engine against the wasm session
//! (`uic_tui_web::DomSession`). One body per operation, two thin wrappers.

use std::collections::HashMap;

use uic_dom::{NodeData, NodeId};

use super::DomDocument;

/// The document and the JS↔node handle table, shared with the host's
/// native functions.
pub struct HostState {
    pub doc: DomDocument,
    pub focused: Option<NodeId>,
    pub dirty: bool,
    handles: Vec<NodeId>,
    handle_of: HashMap<NodeId, usize>,
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
    /// by its `data-path`, the component's own stable row key.
    pub fn commit(&mut self, handle: usize, html: &str) {
        let Some(target) = self.node(handle) else {
            return;
        };
        let focus_path = self
            .focused
            .filter(|&f| f == target || self.doc.ancestors(f).any(|node| node == target));
        let focus_path =
            focus_path.and_then(|f| self.doc.attribute(f, "data-path").map(str::to_string));
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
        self.dirty = true;
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
}

#[cfg(test)]
mod tests {
    use super::HostState;

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
