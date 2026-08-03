//! The composite side of a [`Mount`](super::host::Mount): child mounts on
//! custom-tag nodes, attribute sync down the tree, and event routing back
//! up through the template's `@event` bindings.

use std::collections::HashMap;

use uic_core::{CustomElementRegistry, NotifyEvent, ObjectMap, UiEvent};
use uic_dom::NodeId;

use super::host::Mount;
use super::DomDocument;

impl Mount {
    /// Mounts registered custom tags that appeared in the instance subtree
    /// (fresh instantiation or a conditional branch) and drops mounts whose
    /// nodes a branch teardown destroyed.
    pub(super) fn adopt_new_children(&mut self, doc: &mut DomDocument) {
        self.children.retain(|node, _| doc.node(*node).is_some());
        self.synced_attrs
            .retain(|node, _| doc.node(*node).is_some());
        let descendants: Vec<NodeId> = doc.descendants(self.host).skip(1).collect();
        for node in descendants {
            if self.children.contains_key(&node) {
                continue;
            }
            let Some(tag) = doc.tag_name(node).map(|t| t.to_string()) else {
                continue;
            };
            if !tag.contains('-') || CustomElementRegistry::get(&tag).is_none() {
                continue;
            }
            // Children of mounted children belong to those mounts.
            if self.owned_by_child(doc, node) {
                continue;
            }
            if let Ok(mut child) = Mount::create_at(doc, node) {
                let events = child.update_cycle(doc, |behavior, ctx| behavior.connected(ctx));
                self.children.insert(node, child);
                let synced = self.sync_child_attrs(doc, node);
                let mut all = events;
                all.extend(synced);
                // Mount-time events have no listeners yet on the way up;
                // route them so template bindings still observe them.
                let routed = self.route_child_events(doc, node, all);
                drop(routed);
            }
        }
    }

    fn owned_by_child(&self, doc: &DomDocument, node: NodeId) -> bool {
        let mut current = doc.parent(node);
        while let Some(ancestor) = current {
            if ancestor == self.host {
                return false;
            }
            if self.children.contains_key(&ancestor) {
                return true;
            }
            current = doc.parent(ancestor);
        }
        false
    }

    /// Diffs the child's host-node attributes against the last synced set
    /// and applies the changes as observed-attribute updates: additions,
    /// changes AND removals (a boolean part clearing `?disabled` arrives as
    /// an absent attribute).
    pub(super) fn sync_child_attrs(
        &mut self,
        doc: &mut DomDocument,
        node: NodeId,
    ) -> Vec<NotifyEvent> {
        let Some(child) = self.children.get_mut(&node) else {
            return Vec::new();
        };
        let current: HashMap<String, String> = doc
            .element(node)
            .map(|el| {
                el.attrs()
                    .map(|(name, value)| (name.to_string(), value.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let last = self.synced_attrs.entry(node).or_default();
        let mut events = Vec::new();
        for (name, value) in &current {
            if last.get(name).map(String::as_str) != Some(value.as_str()) {
                events.extend(child.set_attr(doc, name, Some(value)));
            }
        }
        let removed: Vec<String> = last
            .keys()
            .filter(|name| !current.contains_key(*name))
            .cloned()
            .collect();
        for name in removed {
            events.extend(child.set_attr(doc, &name, None));
        }
        *last = current;
        events
    }

    /// A child's notify events reach this mount through the `@event`
    /// bindings its custom tag declares, becoming handler calls, the
    /// browser's addEventListener-per-binding, resolved statically.
    pub(super) fn route_child_events(
        &mut self,
        doc: &mut DomDocument,
        node: NodeId,
        events: Vec<NotifyEvent>,
    ) -> Vec<NotifyEvent> {
        let mut own = Vec::new();
        for event in events {
            let handlers: Vec<String> = self
                .bindings
                .iter()
                .filter(|binding| binding.node == node && binding.event == event.event_name)
                .map(|binding| binding.handler.clone())
                .collect();
            for handler in handlers {
                let ui_event = UiEvent::notify(&event);
                own.extend(self.update_cycle(doc, |behavior, ctx| {
                    behavior.handle(ctx, &handler, &ui_event);
                }));
            }
        }
        own
    }

    /// Routes a widget's commit into the `@change` binding its template
    /// declares, descending into the child mount that owns the node.
    pub(crate) fn dispatch_widget_change(
        &mut self,
        doc: &mut DomDocument,
        node: NodeId,
        text: &str,
    ) -> Vec<NotifyEvent> {
        self.dispatch_widget_event(doc, node, "change", text)
    }

    /// Routes a widget's live text into the `@input` binding, the
    /// per-keystroke half beside the commit's `@change`.
    pub(crate) fn dispatch_widget_input(
        &mut self,
        doc: &mut DomDocument,
        node: NodeId,
        text: &str,
    ) -> Vec<NotifyEvent> {
        self.dispatch_widget_event(doc, node, "input", text)
    }

    fn dispatch_widget_event(
        &mut self,
        doc: &mut DomDocument,
        node: NodeId,
        event_name: &str,
        text: &str,
    ) -> Vec<NotifyEvent> {
        let handlers: Vec<String> = self
            .bindings
            .iter()
            .filter(|binding| binding.node == node && binding.event == event_name)
            .map(|binding| binding.handler.clone())
            .collect();
        if !handlers.is_empty() {
            let mut events = Vec::new();
            for handler in handlers {
                let ui_event = match event_name {
                    "input" => UiEvent::input(text.to_string()),
                    _ => UiEvent::change(text.to_string()),
                };
                events.extend(self.update_cycle(doc, |behavior, ctx| {
                    behavior.handle(ctx, &handler, &ui_event);
                }));
            }
            return events;
        }
        let child_hosts: Vec<NodeId> = self.children.keys().copied().collect();
        for host in child_hosts {
            if host == node || doc.ancestors(node).any(|ancestor| ancestor == host) {
                let events = {
                    let child = self.children.get_mut(&host).expect("listed");
                    child.dispatch_widget_event(doc, node, event_name, text)
                };
                return self.route_child_events(doc, host, events);
            }
        }
        Vec::new()
    }

    /// Routes a pointer click on a plain node into the nearest enclosing
    /// `@click` binding (the clicked element or an ancestor within this
    /// instance), descending into the child mount that owns the node first,
    /// so an unclaimed click bubbles out of the child like the browser's.
    /// `None` when no binding anywhere claims the click.
    pub(crate) fn dispatch_click(
        &mut self,
        doc: &mut DomDocument,
        node: NodeId,
    ) -> Option<Vec<NotifyEvent>> {
        let child_hosts: Vec<NodeId> = self.children.keys().copied().collect();
        for host in child_hosts {
            if host == node || doc.ancestors(node).any(|ancestor| ancestor == host) {
                let claimed = {
                    let child = self.children.get_mut(&host).expect("listed");
                    child.dispatch_click(doc, node)
                };
                if let Some(events) = claimed {
                    return Some(self.route_child_events(doc, host, events));
                }
                break;
            }
        }
        let mut current = Some(node);
        while let Some(candidate) = current {
            let handlers: Vec<String> = self
                .bindings
                .iter()
                .filter(|binding| binding.node == candidate && binding.event == "click")
                .map(|binding| binding.handler.clone())
                .collect();
            if !handlers.is_empty() {
                let ui_event = UiEvent::click(dataset_of(doc, candidate));
                let mut events = Vec::new();
                for handler in handlers {
                    events.extend(self.update_cycle(doc, |behavior, ctx| {
                        behavior.handle(ctx, &handler, &ui_event);
                    }));
                }
                return Some(events);
            }
            if candidate == self.host {
                break;
            }
            current = doc.parent(candidate);
        }
        None
    }

    /// Runs `f` on the mount whose host node is `node`, anywhere below.
    pub(super) fn with_mount_at(
        &mut self,
        node: NodeId,
        doc: &mut DomDocument,
        f: &mut dyn FnMut(&mut Mount, &mut DomDocument) -> Vec<NotifyEvent>,
    ) -> Vec<NotifyEvent> {
        if self.host == node {
            return f(self, doc);
        }
        let child_nodes: Vec<NodeId> = self.children.keys().copied().collect();
        for child_node in child_nodes {
            if child_node == node {
                let events = {
                    let child = self.children.get_mut(&child_node).expect("listed");
                    f(child, doc)
                };
                return self.route_child_events(doc, child_node, events);
            }
            let routed = {
                let child = self.children.get_mut(&child_node).expect("listed");
                child.with_mount_at(node, doc, f)
            };
            if !routed.is_empty() {
                return self.route_child_events(doc, child_node, routed);
            }
        }
        Vec::new()
    }
}

/// The element's `data-*` attributes with camelCased keys, the browser's
/// DOMStringMap (`data-row-id` reads as `dataset.rowId`).
fn dataset_of(doc: &DomDocument, node: NodeId) -> ObjectMap {
    let mut dataset = ObjectMap::default();
    if let Some(el) = doc.element(node) {
        for (name, value) in el.attrs() {
            if let Some(rest) = name.strip_prefix("data-") {
                dataset.insert(camel_case(rest), value.to_string());
            }
        }
    }
    dataset
}

fn camel_case(kebab: &str) -> String {
    let mut out = String::with_capacity(kebab.len());
    let mut upper = false;
    for c in kebab.chars() {
        if c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}
