//! `<uic-tree>`: an expandable tree over nested `{id, label, children?}`
//! nodes. A computed flattens the expanded subtrees into visible rows, so
//! the template stays a flat loop and both logic twins express the walk the
//! same way. The collapse markers are generated content: `::before` rules
//! on `[aria-expanded]` rows draw and rotate them in both targets (the
//! terminal's live in `tui-overrides.css`, the browser's in `tree.scss`);
//! no marker glyphs in the logic.
//!
//! A click on a branch row toggles it; a click on a leaf commits the
//! `selected` id and notifies `selected-changed`. The row tells the handler
//! which node it is through its `data-id`, read as `event.data("id")` here
//! and as `event.currentTarget.dataset.id` in the browser twin.

use uic_core::{Ctx, CustomElement, ObjectMap, PropertyStore, UiEvent, Value};

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "uic-tree",
    template_file = "tree.html",
    scss_file = "tree.scss",
    web_impl_file = "tree.impl.ts"
)]
pub struct Tree {
    /// The nodes: `{id, label, children?}` rows, nested.
    #[property]
    pub nodes: Vec<Value>,
    /// The ids of the expanded branches.
    #[property]
    pub expanded: Vec<Value>,
    /// The last clicked leaf's id.
    #[property(notify, default = "")]
    pub selected: String,
}

/// A node's member as text.
fn text(node: &ObjectMap, key: &str) -> String {
    node.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The node's children, empty for a leaf.
fn children(node: &ObjectMap) -> &[Value] {
    node.get("children")
        .and_then(Value::as_array)
        .unwrap_or_default()
}

fn is_expanded(expanded: &[Value], id: &str) -> bool {
    expanded.iter().any(|entry| entry.as_str() == Some(id))
}

/// Depth-first over the expanded subtrees; level is 1-based like
/// `aria-level`. Mirrored for the browser in `tree.impl.ts`; keep both
/// walks in sync.
fn flatten(nodes: &[Value], expanded: &[Value], level: usize, rows: &mut Vec<Value>) {
    for node in nodes {
        let Some(node) = node.as_object() else {
            continue;
        };
        let id = text(node, "id");
        let branch = !children(node).is_empty();
        let open = branch && is_expanded(expanded, &id);
        let mut row = ObjectMap::new();
        row.insert("label", text(node, "label"));
        row.insert("indent", "\u{a0}".repeat(2 * (level - 1)));
        row.insert("branch", branch);
        row.insert("leaf", !branch);
        row.insert("expanded", if open { "true" } else { "false" });
        row.insert("id", id.clone());
        rows.push(Value::Object(row));
        if open {
            flatten(children(node), expanded, level + 1, rows);
        }
    }
}

/// The branch ids of the whole node tree: what a click toggles.
fn branch_ids(nodes: &[Value], out: &mut Vec<String>) {
    for node in nodes {
        let Some(node) = node.as_object() else {
            continue;
        };
        let below = children(node);
        if !below.is_empty() {
            out.push(text(node, "id"));
            branch_ids(below, out);
        }
    }
}

impl TreeLogic for Tree {
    fn rows(&self, store: &PropertyStore) -> Value {
        let nodes = store.get("nodes").as_array().unwrap_or_default();
        let expanded = store.get("expanded").as_array().unwrap_or_default();
        let mut rows = Vec::new();
        flatten(nodes, expanded, 1, &mut rows);
        Value::Array(rows)
    }

    fn on_row_click(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let Some(id) = event.data("id").map(str::to_string) else {
            return;
        };
        let mut branches = Vec::new();
        branch_ids(
            ctx.get("nodes").as_array().unwrap_or_default(),
            &mut branches,
        );
        if !branches.contains(&id) {
            ctx.set("selected", id);
            return;
        }
        let expanded = ctx.get("expanded").as_array().unwrap_or_default();
        let next: Vec<Value> = if is_expanded(expanded, &id) {
            expanded
                .iter()
                .filter(|entry| entry.as_str() != Some(id.as_str()))
                .cloned()
                .collect()
        } else {
            let mut next = expanded.to_vec();
            next.push(Value::Str(id));
            next
        };
        ctx.set("expanded", Value::Array(next));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::testing::{cycle, setup};

    fn node(id: &str, label: &str, children: Vec<Value>) -> Value {
        let mut node = ObjectMap::new();
        node.insert("id", id);
        node.insert("label", label);
        if !children.is_empty() {
            node.insert("children", Value::Array(children));
        }
        Value::Object(node)
    }

    fn sample() -> Value {
        Value::Array(vec![
            node(
                "docs",
                "Documents",
                vec![
                    node("q3", "Q3 Report", vec![]),
                    node("q4", "Q4 Report", vec![]),
                ],
            ),
            node("readme", "README", vec![]),
        ])
    }

    fn row_member(row: &Value, key: &str) -> Value {
        row.as_object()
            .and_then(|row| row.get(key))
            .cloned()
            .unwrap_or(Value::Undefined)
    }

    #[test]
    fn collapsed_branches_hide_their_subtrees() {
        let (mut store, behavior) = setup(Tree::definition());
        store.set("nodes", sample());
        let Value::Array(rows) = behavior.compute(&store, "rows") else {
            panic!("expected rows");
        };
        let labels: Vec<Value> = rows.iter().map(|row| row_member(row, "label")).collect();
        assert_eq!(labels, vec!["Documents".into(), "README".into()]);
        assert_eq!(row_member(&rows[0], "expanded"), "false".into());
        assert_eq!(row_member(&rows[0], "branch"), true.into());
        assert_eq!(row_member(&rows[1], "leaf"), true.into());
    }

    #[test]
    fn expanding_a_branch_reveals_indented_children() {
        let (mut store, behavior) = setup(Tree::definition());
        store.set("nodes", sample());
        store.set("expanded", Value::Array(vec![Value::Str("docs".into())]));
        let Value::Array(rows) = behavior.compute(&store, "rows") else {
            panic!("expected rows");
        };
        assert_eq!(rows.len(), 4);
        assert_eq!(row_member(&rows[0], "expanded"), "true".into());
        assert_eq!(row_member(&rows[1], "label"), "Q3 Report".into());
        assert_eq!(row_member(&rows[1], "indent"), "\u{a0}\u{a0}".into());
    }

    #[test]
    fn a_branch_click_toggles_and_a_leaf_click_selects() {
        let (mut store, mut behavior) = setup(Tree::definition());
        store.set("nodes", sample());

        let mut dataset = ObjectMap::new();
        dataset.insert("id", "docs");
        cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_row_click", &UiEvent::click(dataset.clone()))
        });
        assert_eq!(
            store.get("expanded"),
            &Value::Array(vec![Value::Str("docs".into())]),
            "the first click expands"
        );
        cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_row_click", &UiEvent::click(dataset.clone()))
        });
        assert_eq!(
            store.get("expanded"),
            &Value::Array(Vec::new()),
            "the second click collapses"
        );

        let mut leaf = ObjectMap::new();
        leaf.insert("id", "readme");
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_row_click", &UiEvent::click(leaf.clone()))
        });
        assert_eq!(store.get("selected"), &Value::Str("readme".into()));
        let names: Vec<_> = events.iter().map(|e| e.event_name.as_str()).collect();
        assert_eq!(names, vec!["selected-changed"]);
    }
}
