//! The retained tree: DOM nodes in an indextree arena behind web-shaped
//! element operations.
//!
//! Node identity is a `Copy` [`NodeId`]. Structural operations move subtrees
//! implicitly the way the web DOM does: appending a node that already has a
//! parent detaches it first. [`Document::detach`] mirrors `removeChild`
//! (the subtree stays alive for re-insertion); [`Document::remove`] destroys
//! a subtree and reclaims its arena slots, after which its ids are stale and
//! every accessor returns `None` for them. Mutating through a stale id
//! panics.

use std::collections::HashMap;

use html5ever::{local_name, ns, LocalName, QualName};
use indextree::Arena;
pub use indextree::NodeId;

use crate::event::ListenerEntry;
use crate::html::ElementKind;

/// One document: the arena, its root node and the event-listener registry.
///
/// `T` is the consumer payload carried by every element: the hook the TUI
/// runtime uses to attach widget state to its tree. Parsing and plain trees
/// use `Document<()>`.
pub struct Document<T = ()> {
    pub(crate) arena: Arena<NodeData<T>>,
    root: NodeId,
    pub(crate) listeners: HashMap<NodeId, Vec<ListenerEntry<T>>>,
    pub(crate) next_listener_id: u64,
    /// Diagnostics collected while parsing; recoverable per spec, never fatal.
    pub parse_errors: Vec<String>,
    /// The doctype name of a parsed document, when one was present.
    pub doctype: Option<String>,
}

/// One node's content.
pub enum NodeData<T> {
    /// The document root; exactly one per [`Document`].
    Document,
    /// A markup-less grouping node; `<template>` contents live in one.
    Fragment,
    Element(ElementData<T>),
    Text(String),
    Comment(String),
}

/// An element: qualified name, attributes in insertion order, and the
/// consumer payload.
pub struct ElementData<T> {
    pub name: QualName,
    attrs: Vec<(QualName, String)>,
    /// `<template>` children per the whatwg template-contents model: they
    /// parse into this separate fragment, not the element's child list.
    pub template_contents: Option<NodeId>,
    /// The consumer payload (widget state, in the TUI runtime).
    pub data: T,
}

impl<T> ElementData<T> {
    pub fn tag(&self) -> &LocalName {
        &self.name.local
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(qual, _)| &*qual.local == name)
            .map(|(_, value)| value.as_str())
    }

    /// Attributes as `(name, value)` pairs, in insertion order.
    pub fn attrs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.attrs
            .iter()
            .map(|(qual, value)| (&*qual.local, value.as_str()))
    }

    /// Upserts by local name; new attributes keep insertion order.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        match self.attrs.iter_mut().find(|(qual, _)| &*qual.local == name) {
            Some((_, slot)) => value.clone_into(slot),
            None => self.attrs.push((
                QualName::new(None, ns!(), LocalName::from(name)),
                value.to_string(),
            )),
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(qual, _)| &*qual.local != name);
    }

    pub(crate) fn push_parsed_attr(&mut self, name: QualName, value: String) {
        self.attrs.push((name, value));
    }

    pub(crate) fn qual_attrs(&self) -> impl Iterator<Item = (&QualName, &str)> {
        self.attrs
            .iter()
            .map(|(qual, value)| (qual, value.as_str()))
    }
}

impl<T> Document<T> {
    pub fn new() -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(NodeData::Document);
        Document {
            arena,
            root,
            listeners: HashMap::new(),
            next_listener_id: 0,
            parse_errors: Vec::new(),
            doctype: None,
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Bounds- and generation-checked access: ids of freed or recycled
    /// slots read as absent instead of aliasing (`Arena::get` alone checks
    /// neither the stamp nor the freed state).
    fn live(&self, node: NodeId) -> Option<&indextree::Node<NodeData<T>>> {
        let inner = self.arena.get(node)?;
        if node.is_removed(&self.arena) {
            return None;
        }
        Some(inner)
    }

    pub fn node(&self, node: NodeId) -> Option<&NodeData<T>> {
        self.live(node).map(indextree::Node::get)
    }

    pub fn node_mut(&mut self, node: NodeId) -> Option<&mut NodeData<T>> {
        self.live(node)?;
        self.arena.get_mut(node).map(indextree::Node::get_mut)
    }

    pub fn element(&self, node: NodeId) -> Option<&ElementData<T>> {
        match self.node(node)? {
            NodeData::Element(el) => Some(el),
            _ => None,
        }
    }

    pub fn element_mut(&mut self, node: NodeId) -> Option<&mut ElementData<T>> {
        match self.node_mut(node)? {
            NodeData::Element(el) => Some(el),
            _ => None,
        }
    }

    pub fn tag_name(&self, node: NodeId) -> Option<&LocalName> {
        self.element(node).map(ElementData::tag)
    }

    // -- creation ---------------------------------------------------------

    /// Creates a detached element of a typed kind: `create_element(html::Div)`.
    pub fn create_element(&mut self, kind: impl ElementKind) -> NodeId
    where
        T: Default,
    {
        self.new_element_node(
            QualName::new(None, ns!(html), kind.local_name()),
            Vec::new(),
        )
    }

    /// Creates a detached element by tag name, for dynamic callers.
    pub fn create_element_named(&mut self, tag: &str) -> NodeId
    where
        T: Default,
    {
        self.new_element_node(
            QualName::new(None, ns!(html), LocalName::from(tag)),
            Vec::new(),
        )
    }

    pub fn create_text_node(&mut self, text: &str) -> NodeId {
        self.arena.new_node(NodeData::Text(text.to_string()))
    }

    pub fn create_comment(&mut self, text: &str) -> NodeId {
        self.arena.new_node(NodeData::Comment(text.to_string()))
    }

    /// A detached grouping node, the `DocumentFragment` analog.
    pub(crate) fn create_fragment(&mut self) -> NodeId {
        self.arena.new_node(NodeData::Fragment)
    }

    /// The shared constructor behind typed, named and parsed elements; a
    /// `<template>` gets its contents fragment here, so every creation path
    /// honors the whatwg template model.
    pub(crate) fn new_element_node(
        &mut self,
        name: QualName,
        attrs: Vec<(QualName, String)>,
    ) -> NodeId
    where
        T: Default,
    {
        let template = name.ns == ns!(html) && name.local == local_name!("template");
        let node = self.arena.new_node(NodeData::Element(ElementData {
            name,
            attrs,
            template_contents: None,
            data: T::default(),
        }));
        if template {
            let contents = self.arena.new_node(NodeData::Fragment);
            if let Some(NodeData::Element(el)) = self.node_mut(node) {
                el.template_contents = Some(contents);
            }
        }
        node
    }

    // -- structure --------------------------------------------------------

    /// Appends as the last child; a parented node moves (web semantics).
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        child.detach(&mut self.arena);
        parent.append(child, &mut self.arena);
    }

    /// Inserts `new` as the sibling immediately before `reference`.
    pub fn insert_before(&mut self, new: NodeId, reference: NodeId) {
        new.detach(&mut self.arena);
        reference.insert_before(new, &mut self.arena);
    }

    /// Inserts `new` as the sibling immediately after `reference`.
    pub fn insert_after(&mut self, new: NodeId, reference: NodeId) {
        new.detach(&mut self.arena);
        reference.insert_after(new, &mut self.arena);
    }

    /// Puts `new` where `old` sits; `old` detaches but stays alive.
    pub fn replace_child(&mut self, new: NodeId, old: NodeId) {
        self.insert_before(new, old);
        self.detach(old);
    }

    /// `removeChild` semantics: unlinks the subtree, which stays alive for
    /// re-insertion under any parent.
    pub fn detach(&mut self, node: NodeId) {
        node.detach(&mut self.arena);
    }

    /// Destroys the subtree: arena slots are reclaimed, listeners dropped,
    /// and the template-contents fragments of removed elements go with it.
    pub fn remove(&mut self, node: NodeId) {
        let mut doomed = vec![node];
        while let Some(next) = doomed.pop() {
            let ids: Vec<NodeId> = next.descendants(&self.arena).collect();
            for id in ids {
                self.listeners.remove(&id);
                if let Some(NodeData::Element(el)) = self.node(id) {
                    if let Some(contents) = el.template_contents {
                        doomed.push(contents);
                    }
                }
            }
            next.remove_subtree(&mut self.arena);
        }
    }

    pub(crate) fn reparent_children(&mut self, node: NodeId, new_parent: NodeId) {
        while let Some(child) = self.first_child(node) {
            self.append_child(new_parent, child);
        }
    }

    /// `importNode(…, deep)`: copies a subtree from another document into
    /// this one, detached. Elements keep their name, attributes and template
    /// contents; the consumer payload starts fresh. Every copied pair lands
    /// in `map` (source id → new id), so callers holding references into the
    /// source (a compiled template's part plan) can resolve them against
    /// the copy.
    pub fn import_node<U>(
        &mut self,
        source: &Document<U>,
        node: NodeId,
        map: &mut HashMap<NodeId, NodeId>,
    ) -> Option<NodeId>
    where
        T: Default,
    {
        let copy = match source.node(node)? {
            NodeData::Document | NodeData::Fragment => self.arena.new_node(NodeData::Fragment),
            NodeData::Element(el) => {
                let attrs = el
                    .qual_attrs()
                    .map(|(qual, value)| (qual.clone(), value.to_string()))
                    .collect();
                let copy = self.new_element_node(el.name.clone(), attrs);
                if let Some(contents) = el.template_contents {
                    // The constructor gave the copy a fresh contents
                    // fragment; fill it from the source's.
                    let target = self
                        .element(copy)
                        .and_then(|el| el.template_contents)
                        .expect("template copies carry a contents fragment");
                    let children: Vec<NodeId> = source.children(contents).collect();
                    for child in children {
                        if let Some(imported) = self.import_node(source, child, map) {
                            self.append_child(target, imported);
                        }
                    }
                    map.insert(contents, target);
                }
                copy
            }
            NodeData::Text(text) => self.create_text_node(text),
            NodeData::Comment(text) => self.create_comment(text),
        };
        map.insert(node, copy);
        let children: Vec<NodeId> = source.children(node).collect();
        for child in children {
            if let Some(imported) = self.import_node(source, child, map) {
                self.append_child(copy, imported);
            }
        }
        Some(copy)
    }

    // -- traversal --------------------------------------------------------

    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.live(node)?.parent()
    }

    pub fn first_child(&self, node: NodeId) -> Option<NodeId> {
        self.live(node)?.first_child()
    }

    pub fn last_child(&self, node: NodeId) -> Option<NodeId> {
        self.live(node)?.last_child()
    }

    pub fn next_sibling(&self, node: NodeId) -> Option<NodeId> {
        self.live(node)?.next_sibling()
    }

    pub fn previous_sibling(&self, node: NodeId) -> Option<NodeId> {
        self.live(node)?.previous_sibling()
    }

    pub fn children(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let start = self.live(node).map(|_| node);
        start
            .map(|id| id.children(&self.arena))
            .into_iter()
            .flatten()
    }

    /// The node itself, then its subtree in document order.
    pub fn descendants(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let start = self.live(node).map(|_| node);
        start
            .map(|id| id.descendants(&self.arena))
            .into_iter()
            .flatten()
    }

    /// The node itself, then its chain of parents up to the root.
    pub fn ancestors(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let start = self.live(node).map(|_| node);
        start
            .map(|id| id.ancestors(&self.arena))
            .into_iter()
            .flatten()
    }

    /// The first element under `from` (document order, `from` included)
    /// whose data matches the predicate.
    pub fn find_element(
        &self,
        from: NodeId,
        matches: impl Fn(&ElementData<T>) -> bool,
    ) -> Option<NodeId> {
        self.descendants(from)
            .find(|&node| self.element(node).is_some_and(&matches))
    }

    /// The first element of a tag under `from`: how a host finds the
    /// components a one-shot composition mounted.
    pub fn descendant_by_tag(&self, from: NodeId, tag: &str) -> Option<NodeId> {
        self.find_element(from, |el| el.tag().as_ref() == tag)
    }

    // -- attributes -------------------------------------------------------

    pub fn attribute(&self, node: NodeId, name: &str) -> Option<&str> {
        self.element(node)?.attr(name)
    }

    /// Upserts on elements; a no-op on other node kinds.
    pub fn set_attribute(&mut self, node: NodeId, name: &str, value: &str) {
        if let Some(el) = self.element_mut(node) {
            el.set_attr(name, value);
        }
    }

    pub fn remove_attribute(&mut self, node: NodeId, name: &str) {
        if let Some(el) = self.element_mut(node) {
            el.remove_attr(name);
        }
    }

    // -- class list -------------------------------------------------------

    pub fn classes(&self, node: NodeId) -> impl Iterator<Item = &str> {
        self.attribute(node, "class")
            .unwrap_or_default()
            .split_whitespace()
    }

    pub fn has_class(&self, node: NodeId, class: &str) -> bool {
        self.classes(node).any(|c| c == class)
    }

    pub fn add_class(&mut self, node: NodeId, class: &str) {
        if self.has_class(node, class) {
            return;
        }
        let mut classes = self
            .attribute(node, "class")
            .unwrap_or_default()
            .to_string();
        if !classes.is_empty() {
            classes.push(' ');
        }
        classes.push_str(class);
        self.set_attribute(node, "class", &classes);
    }

    /// Removing the last class keeps an empty `class` attribute, like
    /// `classList.remove` in the browser.
    pub fn remove_class(&mut self, node: NodeId, class: &str) {
        let remaining = self
            .classes(node)
            .filter(|c| *c != class)
            .collect::<Vec<_>>()
            .join(" ");
        if self.attribute(node, "class").is_some() {
            self.set_attribute(node, "class", &remaining);
        }
    }

    /// Returns whether the class is present afterwards.
    pub fn toggle_class(&mut self, node: NodeId, class: &str) -> bool {
        if self.has_class(node, class) {
            self.remove_class(node, class);
            false
        } else {
            self.add_class(node, class);
            true
        }
    }

    // -- text ---------------------------------------------------------------

    pub fn text(&self, node: NodeId) -> Option<&str> {
        match self.node(node)? {
            NodeData::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Rewrites a text node's content; a no-op on other node kinds.
    pub fn set_text(&mut self, node: NodeId, text: &str) {
        if let Some(NodeData::Text(slot)) = self.node_mut(node) {
            text.clone_into(slot);
        }
    }

    /// Concatenated descendant text, `textContent`-style.
    pub fn text_content(&self, node: NodeId) -> String {
        let mut out = String::new();
        for id in self.descendants(node) {
            if let Some(NodeData::Text(text)) = self.node(id) {
                out.push_str(text);
            }
        }
        out
    }

    // -- parser helpers -----------------------------------------------------

    /// Appends text, merging into an adjacent text sibling like the parser
    /// contract asks.
    pub(crate) fn append_text(&mut self, parent: NodeId, text: &str) {
        if let Some(last) = self.last_child(parent) {
            if let Some(NodeData::Text(existing)) = self.node_mut(last) {
                existing.push_str(text);
                return;
            }
        }
        let node = self.create_text_node(text);
        self.append_child(parent, node);
    }

    /// Inserts text before a sibling, merging into an adjacent text node.
    pub(crate) fn insert_text_before(&mut self, text: &str, sibling: NodeId) {
        if let Some(prev) = self.previous_sibling(sibling) {
            if let Some(NodeData::Text(existing)) = self.node_mut(prev) {
                existing.push_str(text);
                return;
            }
        }
        let node = self.create_text_node(text);
        self.insert_before(node, sibling);
    }
}

impl<T> Default for Document<T> {
    fn default() -> Self {
        Self::new()
    }
}
