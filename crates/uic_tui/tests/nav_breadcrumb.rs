//! TestBackend tests for <nav-breadcrumb>: the static trail renders inline
//! with data-decorated dividers, links degrade to plain text, and an empty
//! trail paints nothing.

mod support;

use uic_core::{ObjectMap, Value};

use support::screen;

fn item(label: &str, href: Option<&str>) -> Value {
    let mut row = ObjectMap::new();
    row.insert("label", label);
    if let Some(href) = href {
        row.insert("href", href);
    }
    Value::Object(row)
}

#[test]
fn the_trail_renders_inline_with_dividers() {
    let mut app = support::app(40, 3);
    let el = app.mount("nav-breadcrumb").expect("mount");
    app.set_prop(
        el,
        "items",
        Value::Array(vec![
            item("Documents", Some("/documents")),
            item("Reports", Some("/documents/reports")),
            item("Q3", None),
        ]),
    );
    let frame = screen(&mut app);
    assert!(
        frame.contains("Documents › Reports › Q3"),
        "trail on screen:\n{frame}"
    );
}

#[test]
fn a_custom_divider_replaces_the_default() {
    let mut app = support::app(40, 3);
    let el = app.mount("nav-breadcrumb").expect("mount");
    app.set_attr(el, "divider", "/");
    app.set_prop(
        el,
        "items",
        Value::Array(vec![item("Home", Some("/")), item("Files", None)]),
    );
    let frame = screen(&mut app);
    assert!(frame.contains("Home / Files"), "trail on screen:\n{frame}");
}

#[test]
fn an_empty_trail_paints_nothing() {
    let mut app = support::app(40, 3);
    app.mount("nav-breadcrumb").expect("mount");
    let frame = screen(&mut app);
    assert_eq!(frame.trim(), "", "nothing to paint:\n{frame}");
}
