//! Template expansion: resolves the static template IR against the current
//! property state into a render tree for one frame.
//!
//! Nested registered custom elements expand inline: the child's template is
//! resolved against the child instance's own store and behavior, and its
//! widget leaves carry the child-index path back to their owning instance.

use uic_core::{Behavior, CustomElementRegistry, PropertyStore, Value};
use uic_template::{AttrPart, Attribute, Expr, Node, Template};

use crate::instance::ElementInstance;

/// A widget leaf's owner: the child-binding path from the rendering instance
/// down to the owning instance, and the slot index there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotRef {
    pub path: Vec<usize>,
    pub slot: usize,
}

/// A resolved node: holes evaluated, false conditionals dropped, whitespace
/// collapsed.
#[derive(Debug)]
pub(crate) enum RNode {
    Element {
        classes: Vec<String>,
        /// The owning widget slot for interactive leaves.
        slot: Option<SlotRef>,
        /// Layout height (cells) of a widget leaf; multi-line widgets grow
        /// with their content.
        widget_height: u16,
        children: Vec<RNode>,
    },
    Text(String),
}

/// Evaluates a hole expression: declared properties read the store, other
/// names dispatch to the component's computed getters.
pub(crate) fn resolve_expr(expr: &Expr, store: &PropertyStore, behavior: &dyn Behavior) -> Value {
    let ident = expr.ident();
    let base = if store.has(ident) {
        store.get(ident).clone()
    } else {
        behavior.compute(store, ident)
    };
    match expr {
        Expr::Ident(_) => base,
        Expr::Not(_) => Value::Bool(!base.truthy()),
    }
}

pub(crate) fn expand(template: &Template, instance: &ElementInstance) -> Vec<RNode> {
    let mut counters = Counters::default();
    expand_nodes(&template.roots, instance, &mut counters, &[])
}

#[derive(Default)]
struct Counters {
    slot: usize,
    child: usize,
}

fn expand_nodes(
    nodes: &[Node],
    instance: &ElementInstance,
    counters: &mut Counters,
    path: &[usize],
) -> Vec<RNode> {
    let store = &instance.store;
    let behavior = instance.behavior.as_ref();
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Text(text) => {
                let text = collapse_whitespace(text);
                if !text.is_empty() {
                    out.push(RNode::Text(text));
                }
            }
            Node::TextHole(expr) => {
                let text = resolve_expr(expr, store, behavior).display_text();
                if !text.is_empty() {
                    out.push(RNode::Text(text));
                }
            }
            Node::If { cond, then } => {
                if resolve_expr(cond, store, behavior).truthy() {
                    out.extend(expand_nodes(then, instance, counters, path));
                } else {
                    // Slot and child indices are assigned by template order
                    // over ALL branches; keep counting through the skipped
                    // subtree.
                    counters.slot += count_slots(then);
                    counters.child += count_children(then);
                }
            }
            Node::Element(el) => {
                if widget_kind(el).is_some() {
                    let slot = SlotRef {
                        path: path.to_vec(),
                        slot: counters.slot,
                    };
                    let widget_height = widget_height(instance, counters.slot);
                    counters.slot += 1;
                    out.push(RNode::Element {
                        classes: resolve_classes(el, store, behavior),
                        slot: Some(slot),
                        widget_height,
                        children: expand_nodes(&el.children, instance, counters, path),
                    });
                } else if is_registered_child(el) {
                    let index = counters.child;
                    counters.child += 1;
                    let child = &instance.children[index].instance;
                    let mut child_path = path.to_vec();
                    child_path.push(index);
                    let mut child_counters = Counters::default();
                    let children = expand_nodes(
                        &child.def.template().roots,
                        child,
                        &mut child_counters,
                        &child_path,
                    );
                    out.push(RNode::Element {
                        // The custom tag's own classes resolve against the
                        // PARENT state; the subtree against the child's.
                        classes: resolve_classes(el, store, behavior),
                        slot: None,
                        widget_height: 1,
                        children,
                    });
                } else {
                    out.push(RNode::Element {
                        classes: resolve_classes(el, store, behavior),
                        slot: None,
                        widget_height: 1,
                        children: expand_nodes(&el.children, instance, counters, path),
                    });
                }
            }
        }
    }
    out
}

/// The `data-tui` marker naming the terminal widget for an element.
pub(crate) fn widget_kind(el: &uic_template::Element) -> Option<&str> {
    el.attrs.iter().find_map(|attr| match attr {
        Attribute::Static { name, value } if name == "data-tui" => Some(value.as_str()),
        _ => None,
    })
}

/// Layout height of a widget leaf: single-line widgets are one cell; a
/// textarea starts at one line like the browser's initial height and grows
/// with its content up to the component's `max_lines` property (10 when
/// absent).
fn widget_height(instance: &ElementInstance, slot: usize) -> u16 {
    use crate::instance::WidgetState;
    match instance.slots.get(slot).map(|s| &s.state) {
        Some(WidgetState::TextArea(state)) => {
            let max_lines = match instance.store.has("max_lines") {
                true => match instance.store.get("max_lines") {
                    Value::Num(n) if *n >= 1.0 => *n as u16,
                    _ => 10,
                },
                false => 10,
            };
            // rat's text is newline-terminated: the count includes an empty
            // tail line that never shows in the browser.
            let lines = (state.len_lines() as u16).saturating_sub(1).max(1);
            lines.clamp(1, max_lines.max(1))
        }
        _ => 1,
    }
}

/// A custom tag that mounted as a child instance (unregistered custom tags
/// stay plain blocks). Must mirror the discovery predicate in `instance.rs`.
fn is_registered_child(el: &uic_template::Element) -> bool {
    el.is_custom() && CustomElementRegistry::get(&el.tag).is_some()
}

/// Widget slots in template order, across all conditional branches; does not
/// descend into registered children (their slots belong to the child).
pub(crate) fn count_slots(nodes: &[Node]) -> usize {
    let mut count = 0;
    for node in nodes {
        match node {
            Node::Element(el) => {
                if widget_kind(el).is_some() {
                    count += 1;
                    count += count_slots(&el.children);
                } else if !is_registered_child(el) {
                    count += count_slots(&el.children);
                }
            }
            Node::If { then, .. } => count += count_slots(then),
            Node::Text(_) | Node::TextHole(_) => {}
        }
    }
    count
}

/// Registered custom children in template order, across all branches.
pub(crate) fn count_children(nodes: &[Node]) -> usize {
    let mut count = 0;
    for node in nodes {
        match node {
            Node::Element(el) => {
                if is_registered_child(el) {
                    count += 1;
                } else {
                    count += count_children(&el.children);
                }
            }
            Node::If { then, .. } => count += count_children(then),
            Node::Text(_) | Node::TextHole(_) => {}
        }
    }
    count
}

fn resolve_classes(
    el: &uic_template::Element,
    store: &PropertyStore,
    behavior: &dyn Behavior,
) -> Vec<String> {
    let mut resolved = String::new();
    for attr in &el.attrs {
        match attr {
            Attribute::Static { name, value } if name == "class" => {
                resolved.push_str(value);
            }
            Attribute::Attr { name, parts } if name == "class" => {
                for part in parts {
                    match part {
                        AttrPart::Static(text) => resolved.push_str(text),
                        AttrPart::Expr(expr) => {
                            resolved.push_str(&resolve_expr(expr, store, behavior).display_text())
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut classes: Vec<String> = resolved.split_whitespace().map(str::to_string).collect();
    // A seamless input renders borderless: the chrome's input-group keeps
    // its flex layout but drops the border block (the browser reaches the
    // same through the [seamless] stylesheet rules).
    if store.has("seamless") && store.get("seamless").truthy() {
        for class in &mut classes {
            if class == "input-group" {
                *class = "d-flex".to_string();
            }
        }
    }
    // The error state colors the surviving border red: the browser reaches
    // the same through the [error] stylesheet rules on the reflected
    // attribute (seamless drops the border entirely, there as here).
    if classes.iter().any(|class| class == "input-group")
        && store.has("error")
        && store.get("error").truthy()
    {
        classes.push("is-invalid".to_string());
    }
    classes
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
