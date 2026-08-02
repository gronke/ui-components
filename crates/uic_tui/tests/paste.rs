//! Bracketed paste on the App: one `Event::Paste` is one bulk insert —
//! not a key hail — with the widget's line discipline applied and the
//! live-text routing intact.

mod support;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use uic_core::Value;
use uic_tui::App;

use support::{key, paste, probe, screen, type_str};

fn app() -> App<TestBackend> {
    support::app(60, 12)
}

#[test]
fn one_paste_event_is_one_insert_and_one_change() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    app.set_attr(el, "label", "Token");
    let commits = probe(&mut app, el, "value-changed");

    paste(&mut app, "hello pasted world");
    let frame = screen(&mut app);
    assert!(
        frame.contains("hello pasted world"),
        "the whole phrase lands in one event:\n{frame}"
    );

    key(&mut app, KeyCode::Tab);
    let commits = commits.borrow();
    assert_eq!(commits.len(), 1, "one commit, not one per character");
    assert_eq!(commits[0].value, Value::Str("hello pasted world".into()));
}

#[test]
fn a_paste_lands_at_the_caret_between_typed_text() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    let commits = probe(&mut app, el, "value-changed");

    type_str(&mut app, "ab");
    key(&mut app, KeyCode::Left);
    paste(&mut app, "XY");
    key(&mut app, KeyCode::Tab);

    assert_eq!(
        commits.borrow().last().map(|ev| ev.value.clone()),
        Some(Value::Str("aXYb".into())),
        "the paste inserted at the caret, not replacing the value"
    );
}

#[test]
fn a_paste_unblurs_like_a_key() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    app.set_attr(el, "label", "Note");
    app.blur();

    paste(&mut app, "back in play");
    let frame = screen(&mut app);
    assert!(
        frame.contains("back in play"),
        "the paste reached the focused widget and un-blurred:\n{frame}"
    );
}

#[test]
fn a_paste_routes_the_suggestion_inputs_live_text_once() {
    let mut app = app();
    let el = app.mount("input-suggestion").expect("mount");
    app.set_attr(el, "label", "Word");
    let queries = probe(&mut app, el, "query-changed");

    paste(&mut app, "apricot");

    let queries = queries.borrow();
    assert_eq!(queries.len(), 1, "one live query for the whole paste");
    assert_eq!(queries[0].value, Value::Str("apricot".into()));
}

#[test]
fn a_multiline_paste_into_a_textarea_keeps_its_lines() {
    let mut app = app();
    let el = app.mount("input-textarea").expect("mount");
    let commits = probe(&mut app, el, "value-changed");

    // The terminal spells line breaks as \r inside a bracketed paste.
    paste(&mut app, "line one\rline two");
    key(&mut app, KeyCode::Tab);

    assert_eq!(
        commits.borrow().last().map(|ev| ev.value.clone()),
        Some(Value::Str("line one\nline two".into())),
        "breaks fold to \\n and survive in the textarea"
    );
}

#[test]
fn a_single_line_input_strips_pasted_line_breaks() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    let commits = probe(&mut app, el, "value-changed");

    paste(&mut app, "one\r\ntwo");
    key(&mut app, KeyCode::Tab);

    assert_eq!(
        commits.borrow().last().map(|ev| ev.value.clone()),
        Some(Value::Str("onetwo".into())),
        "the single-line input drops the breaks, like the browser"
    );
}
