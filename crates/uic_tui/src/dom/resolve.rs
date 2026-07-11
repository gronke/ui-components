//! The template expression language meets the parts engine: holes resolve
//! against the store (or a computed), and committed part values convert
//! back to property values on their way into children and widgets.

use uic_core::{Behavior, PropertyStore, Value};
use uic_dom::parts::PartValue;
use uic_dom::NodeId;

use super::DomDocument;

/// `ident` reads the store or dispatches to a computed getter; `!ident`
/// negates its truthiness — the template expression language, resolved to
/// part values. Null and undefined clear their part, like lit's `nothing`.
pub(super) fn resolve_hole(
    expr: &str,
    store: &PropertyStore,
    behavior: &dyn Behavior,
) -> PartValue {
    let (negated, ident) = match expr.strip_prefix('!') {
        Some(ident) => (true, ident),
        None => (false, expr),
    };
    let base = if store.has(ident) {
        store.get(ident).clone()
    } else {
        behavior.compute(store, ident)
    };
    if negated {
        return PartValue::Bool(!base.truthy());
    }
    match base {
        Value::Undefined | Value::Null => PartValue::Nothing,
        value => PartValue::Value(value),
    }
}

/// A committed part value as a property value, for `.prop` writes into
/// children and widgets. The asymmetry is load-bearing: `Nothing` (a hole
/// that resolved null/undefined) still WRITES — it arrives as `Value::Null`
/// in the child, the browser's `el.prop = null`. Only `NoChange` skips the
/// write entirely, and the parts engine emits no write for it.
pub(super) fn part_value_to_value(value: &PartValue) -> Value {
    match value {
        PartValue::Text(text) => Value::Str(text.clone()),
        PartValue::Bool(b) => Value::Bool(*b),
        PartValue::Value(value) => value.clone(),
        PartValue::Nothing | PartValue::NoChange => Value::Null,
    }
}

/// Applies a template property write onto the terminal widget living in a
/// plain `data-tui` element's node payload: `.value` syncs the widget text,
/// `.options` replaces its option rows.
pub(super) fn apply_widget_write(doc: &mut DomDocument, node: NodeId, name: &str, value: Value) {
    if let Some(widget) = doc.element_mut(node).and_then(|el| el.data.widget.as_mut()) {
        match name {
            "value" => widget.sync_value(&value),
            "options" => {
                if let Value::Options(options) = value {
                    widget.adapter.set_options(options);
                }
            }
            _ => {}
        }
    }
}
