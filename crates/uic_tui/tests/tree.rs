//! TestBackend tests for <uic-tree>: the flattened rows paint with
//! generated-content markers, a pointer click on a branch row toggles its
//! subtree through the plain-element `@click` dispatch, and a leaf click
//! notifies `selected-changed`.

mod support;

use uic_core::{ObjectMap, Value};

use support::{click, locate, probe, screen};

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

#[test]
fn branches_wear_markers_and_clicks_toggle_their_subtrees() {
    let mut app = support::app(40, 10);
    let el = app.mount("uic-tree").expect("mount");
    app.set_prop(el, "nodes", sample());

    let frame = screen(&mut app);
    assert!(
        frame.contains('\u{25b6}'),
        "the collapsed branch wears its ::before marker:\n{frame}"
    );
    assert!(frame.contains("Documents") && frame.contains("README"));
    assert!(
        !frame.contains("Q3 Report"),
        "collapsed subtrees stay hidden:\n{frame}"
    );
    // The marker box is a fixed two-cell slot, so branch and leaf labels
    // align on the same column.
    let (docs_x, _) = locate(&mut app, "Documents");
    let (readme_x, _) = locate(&mut app, "README");
    assert_eq!(docs_x, readme_x, "labels align behind the marker slot");

    // A click anywhere on the row (the label cell here) expands: the
    // handler learns the row from data-id via the event dataset.
    let (x, y) = locate(&mut app, "Documents");
    click(&mut app, x, y);
    let frame = screen(&mut app);
    assert!(
        frame.contains("Q3 Report") && frame.contains("Q4 Report"),
        "the click expands the branch:\n{frame}"
    );
    assert!(
        frame.contains('\u{25bc}'),
        "the marker rotates on expand:\n{frame}"
    );
    let (q3_x, _) = locate(&mut app, "Q3 Report");
    assert_eq!(q3_x, docs_x + 2, "children indent one level:\n{frame}");

    // Clicking the marker cell itself hits the row too (generated content
    // hit-tests to its owner).
    let (marker_x, marker_y) = locate(&mut app, "\u{25bc}");
    click(&mut app, marker_x, marker_y);
    let frame = screen(&mut app);
    assert!(
        !frame.contains("Q3 Report"),
        "the marker click collapses again:\n{frame}"
    );
}

#[test]
fn a_leaf_click_commits_selected_and_notifies() {
    let mut app = support::app(40, 10);
    let el = app.mount("uic-tree").expect("mount");
    app.set_prop(el, "nodes", sample());
    let selections = probe(&mut app, el, "selected-changed");

    let (x, y) = locate(&mut app, "README");
    click(&mut app, x, y);

    let values: Vec<Value> = selections
        .borrow()
        .iter()
        .map(|event| event.value.clone())
        .collect();
    assert_eq!(
        values,
        vec![Value::Str("readme".into())],
        "the leaf click notifies its id"
    );
}
