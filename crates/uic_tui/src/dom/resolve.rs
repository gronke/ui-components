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

/// Resolves a repeat body hole under a stack of loop scopes (ADR 0001): a
/// member hole `${var.field}` reads the innermost matching loop variable's
/// row member; anything else resolves against the store or a computed, so
/// component values stay reachable inside loop bodies.
pub(super) fn resolve_hole_in_scopes(
    expr: &str,
    scopes: &[(&str, &Value)],
    store: &PropertyStore,
    behavior: &dyn Behavior,
) -> PartValue {
    if let Some((base, field)) = expr.split_once('.') {
        if let Some((_, row)) = scopes.iter().rev().find(|(name, _)| *name == base) {
            return match row.as_object().and_then(|object| object.get(field)) {
                Some(Value::Undefined) | Some(Value::Null) | None => PartValue::Nothing,
                Some(value) => PartValue::Value(value.clone()),
            };
        }
    }
    resolve_hole(expr, store, behavior)
}

/// Resolves one repeat into its [`PartValue::List`]: each row of the array
/// resolves the body holes with the loop variable pushed onto the scope
/// stack, and nested repeats recurse with their `each` read from that scope,
/// nesting their lists at their body slot (ADR 0001).
pub(super) fn resolve_repeat(
    meta: &uic_dom::parts::RepeatMeta,
    each: &Value,
    scopes: &[(&str, &Value)],
    store: &PropertyStore,
    behavior: &dyn Behavior,
) -> PartValue {
    let rows: &[Value] = each.as_array().unwrap_or(&[]);
    let list = rows
        .iter()
        .map(|row| {
            let mut scope: Vec<(&str, &Value)> = scopes.to_vec();
            scope.push((meta.item.as_str(), row));
            meta.body_holes
                .iter()
                .enumerate()
                .map(|(slot, expr)| {
                    if let Some(nested) = meta.nested.iter().find(|n| n.each_hole == slot) {
                        let inner = match resolve_hole_in_scopes(expr, &scope, store, behavior) {
                            PartValue::Value(value) => value,
                            _ => Value::Undefined,
                        };
                        resolve_repeat(nested, &inner, &scope, store, behavior)
                    } else {
                        resolve_hole_in_scopes(expr, &scope, store, behavior)
                    }
                })
                .collect()
        })
        .collect();
    PartValue::List(list)
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
        // A list never reaches a scalar property write.
        PartValue::Nothing | PartValue::NoChange | PartValue::List(_) => Value::Null,
    }
}

/// Applies a template property write onto the terminal widget living in a
/// widget-bearing element's node payload: `.value` syncs the widget text,
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
