//! The success bar of #65: json-viewer's OWN keyboard navigation and
//! click-to-toggle drive the terminal rendering through synthesized events.

use std::path::Path;

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;

const DATA: &str = r#"{"name":"Schuhkarton","tags":{"first":"a","second":"b"},"active":true}"#;

fn paint(host: &JsHost, terminal: &mut Terminal<TestBackend>) -> String {
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let mut s = state.borrow_mut();
            s.dirty = false;
            let focused = s.focused;
            uic_tui::dom::paint_document(frame, frame.area(), &mut s.doc, focused);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn node_by_path(host: &JsHost, path: &str) -> Option<uic_dom::NodeId> {
    let state = host.state.borrow();
    let root = state.doc.root();
    let found = state
        .doc
        .descendants(root)
        .find(|&node| state.doc.attribute(node, "data-path") == Some(path));
    found
}

/// A `tags.first` treeitem exists only while `tags` is expanded — the
/// document is the unambiguous expansion signal, the frame the visual one.
fn expanded(host: &JsHost) -> bool {
    node_by_path(host, "tags.first").is_some()
}

fn focused_path(host: &JsHost) -> Option<String> {
    let state = host.state.borrow();
    let focused = state.focused?;
    state
        .doc
        .attribute(focused, "data-path")
        .map(str::to_string)
}

#[test]
fn keyboard_navigation_and_click_toggle() {
    let mut host = JsHost::new().unwrap();
    host.load_dist_dir(Path::new(env!("UIC_JS_VENDOR_DIST")), "json-viewer.js")
        .unwrap();
    let root = host.mount("json-viewer", &[("data", DATA)]).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 16)).unwrap();

    let collapsed = paint(&host, &mut terminal);
    assert!(collapsed.contains("tags:"), "keys render:\n{collapsed}");
    assert!(!expanded(&host), "starts collapsed");

    // Focusing the component redirects to the first treeitem (the
    // component's own focusin handler with roving tabindex).
    host.focus(root).unwrap();
    assert_eq!(focused_path(&host).as_deref(), Some("name"));

    // ArrowDown walks to `tags`; ArrowRight expands it — both handled by
    // the component's own `@keydown` handler.
    assert!(host.dispatch_key("ArrowDown").unwrap());
    assert_eq!(focused_path(&host).as_deref(), Some("tags"));
    assert!(host.dispatch_key("ArrowRight").unwrap());
    assert!(expanded(&host), "ArrowRight expands the focused node");
    let frame = paint(&host, &mut terminal);
    assert!(
        frame.contains("first:") && frame.contains("second:"),
        "expanded members paint:\n{frame}"
    );

    // Focus survived the subtree swap by its data-path.
    assert_eq!(focused_path(&host).as_deref(), Some("tags"));

    // ArrowLeft collapses again.
    assert!(host.dispatch_key("ArrowLeft").unwrap());
    assert!(!expanded(&host), "ArrowLeft collapses");

    // A click on the key span toggles too (the `@click` template binding).
    let tags_item = node_by_path(&host, "tags").expect("tags treeitem");
    let key_span = {
        let state = host.state.borrow();
        let found = state
            .doc
            .descendants(tags_item)
            .find(|&node| state.doc.attribute(node, "data-uic-l-click").is_some());
        found.expect("clickable key span")
    };
    host.click(key_span).unwrap();
    assert!(expanded(&host), "click expands again");
    let frame = paint(&host, &mut terminal);
    assert!(
        frame.contains("first:"),
        "clicked-open members paint:\n{frame}"
    );
}
