//! TestBackend tests for plain text nodes: ASCII whitespace collapses like
//! the browser flows prose, while non-breaking spaces survive as content,
//! including at the start of a line, where a trimming wrap would eat them.

mod support;

use ratatui::backend::TestBackend;
use uic_core::{CustomElement, Value};
use uic_tui::App;

use support::screen;

/// A block of prose fed through a property hole.
#[derive(CustomElement, Default)]
#[custom_element(tag = "demo-prose", template = "<p>${line}</p>")]
struct DemoProse {
    #[property]
    line: String,
}

impl DemoProseLogic for DemoProse {}

fn app() -> App<TestBackend> {
    support::app(24, 4)
}

#[test]
fn leading_non_breaking_spaces_indent_the_line() {
    let mut app = app();
    let el = app.mount("demo-prose").expect("mount");
    app.set_prop(el, "line", Value::from("\u{a0}\u{a0}indented"));
    let frame = screen(&mut app);
    assert!(
        frame.contains("\u{a0}\u{a0}indented"),
        "the indent survives:\n{frame}"
    );
}

#[test]
fn wrapped_prose_stays_flush_without_the_trim() {
    let mut app = app();
    let el = app.mount("demo-prose").expect("mount");
    app.set_prop(el, "line", Value::from("alpha beta gamma delta epsilon"));
    let frame = screen(&mut app);
    assert!(frame.contains("epsilon"), "prose wraps:\n{frame}");
    // Continuation lines start at the block's left edge; the untrimmed
    // wrap must not leak separator spaces onto them.
    for line in frame.lines() {
        assert!(!line.starts_with(' '), "flush lines:\n{frame}");
    }
}
