//! Template expansion: resolves the static template IR against the current
//! property state into a render tree for one frame.

use uic_core::{Behavior, PropertyStore, Value};
use uic_template::{AttrPart, Attribute, Expr, Node, Template};

/// A resolved node: holes evaluated, false conditionals dropped, whitespace
/// collapsed.
#[derive(Debug)]
pub(crate) enum RNode {
    Element {
        classes: Vec<String>,
        /// Index into the instance's widget slots for interactive leaves.
        slot: Option<usize>,
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

pub(crate) fn expand(
    template: &Template,
    store: &PropertyStore,
    behavior: &dyn Behavior,
) -> Vec<RNode> {
    let mut slot_counter = 0;
    expand_nodes(&template.roots, store, behavior, &mut slot_counter)
}

fn expand_nodes(
    nodes: &[Node],
    store: &PropertyStore,
    behavior: &dyn Behavior,
    slot_counter: &mut usize,
) -> Vec<RNode> {
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
                    out.extend(expand_nodes(then, store, behavior, slot_counter));
                } else {
                    // Slot indices are assigned by template order over ALL
                    // branches; keep counting through the skipped subtree.
                    *slot_counter += count_slots(then);
                }
            }
            Node::Element(el) => {
                let slot = if widget_kind(el).is_some() {
                    let index = *slot_counter;
                    *slot_counter += 1;
                    Some(index)
                } else {
                    None
                };
                out.push(RNode::Element {
                    classes: resolve_classes(el, store, behavior),
                    slot,
                    children: expand_nodes(&el.children, store, behavior, slot_counter),
                });
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

/// Widget slots in template order, across all conditional branches.
pub(crate) fn count_slots(nodes: &[Node]) -> usize {
    let mut count = 0;
    for node in nodes {
        match node {
            Node::Element(el) => {
                if widget_kind(el).is_some() {
                    count += 1;
                }
                count += count_slots(&el.children);
            }
            Node::If { then, .. } => count += count_slots(then),
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
    resolved.split_whitespace().map(str::to_string).collect()
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
