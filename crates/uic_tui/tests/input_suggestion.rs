//! TestBackend tests for <input-suggestion>: the live `query-changed`
//! stream, host-delivered rows opening the popup, and the pick/commit
//! keyboard and pointer flows.

mod support;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use uic_core::{SelectOption, Value};
use uic_tui::{App, Control};

use support::{click, key, locate, probe, screen, type_str};

/// A tall terminal, so the suggestion popup has room below the widget.
fn app() -> App<TestBackend> {
    support::app(60, 20)
}

fn rows() -> Vec<SelectOption> {
    vec![
        SelectOption::new("apple"),
        SelectOption::new("apricot"),
        SelectOption::new("avocado"),
    ]
}

fn mount(app: &mut App<TestBackend>) -> usize {
    let el = app.mount("input-suggestion").expect("mount");
    app.set_attr(el, "label", "Word");
    app.set_attr(el, "placeholder", "start typing");
    el
}

#[test]
fn typing_emits_the_live_query_per_keystroke() {
    let mut app = app();
    let el = mount(&mut app);
    let queries = probe(&mut app, el, "query-changed");

    type_str(&mut app, "ap");

    let queries = queries.borrow();
    assert_eq!(queries.len(), 2, "one query per keystroke");
    assert_eq!(queries[0].value, Value::Str("a".into()));
    assert_eq!(queries[1].value, Value::Str("ap".into()));
}

#[test]
fn host_rows_open_the_popup_and_enter_picks() {
    let mut app = app();
    let el = mount(&mut app);
    let commits = probe(&mut app, el, "value-changed");

    // The host answers the query between events: the deliver-return-apply
    // pattern of a listener that must not re-enter the app.
    type_str(&mut app, "a");
    app.set_prop(el, "suggestions", rows());

    let open = screen(&mut app);
    assert!(open.contains("apple"), "rows in the popup:\n{open}");
    assert!(open.contains("apricot"), "all rows:\n{open}");

    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);

    let commits = commits.borrow();
    assert_eq!(commits.len(), 1, "the pick commits once");
    assert_eq!(commits[0].value, Value::Str("apricot".into()));

    drop(commits);
    let closed = screen(&mut app);
    assert!(closed.contains("apricot"), "picked text:\n{closed}");
    assert!(!closed.contains("avocado"), "popup closed:\n{closed}");
}

#[test]
fn a_mouse_pick_commits_like_enter() {
    let mut app = app();
    let el = mount(&mut app);
    let commits = probe(&mut app, el, "value-changed");

    type_str(&mut app, "a");
    app.set_prop(el, "suggestions", rows());
    let (x, y) = locate(&mut app, "avocado");
    click(&mut app, x, y);

    let commits = commits.borrow();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].value, Value::Str("avocado".into()));
}

#[test]
fn esc_closes_keeping_the_text_and_a_second_esc_quits() {
    let mut app = app();
    let el = mount(&mut app);
    let commits = probe(&mut app, el, "value-changed");

    type_str(&mut app, "ap");
    app.set_prop(el, "suggestions", rows());
    assert_eq!(key(&mut app, KeyCode::Esc), Control::Continue);

    assert!(commits.borrow().is_empty(), "closing commits nothing");
    let closed = screen(&mut app);
    assert!(closed.contains("ap"), "typed text stays:\n{closed}");
    assert!(!closed.contains("apple"), "popup closed:\n{closed}");
    assert_eq!(key(&mut app, KeyCode::Esc), Control::Quit);
}

#[test]
fn tab_commits_the_typed_text_over_the_open_popup() {
    let mut app = app();
    let el = mount(&mut app);
    let commits = probe(&mut app, el, "value-changed");

    type_str(&mut app, "ap");
    app.set_prop(el, "suggestions", rows());
    key(&mut app, KeyCode::Tab);

    let commits = commits.borrow();
    assert_eq!(commits.len(), 1, "Tab commits the text, not a row");
    assert_eq!(commits[0].value, Value::Str("ap".into()));
}

#[test]
fn typing_reopens_after_esc_when_new_rows_arrive() {
    let mut app = app();
    let el = mount(&mut app);

    type_str(&mut app, "a");
    app.set_prop(el, "suggestions", rows());
    key(&mut app, KeyCode::Esc);
    type_str(&mut app, "p");
    app.set_prop(
        el,
        "suggestions",
        vec![SelectOption::new("apple"), SelectOption::new("apricot")],
    );

    let open = screen(&mut app);
    assert!(open.contains("apricot"), "reopened on new rows:\n{open}");
}

#[test]
fn empty_rows_never_open_a_popup() {
    let mut app = app();
    let el = mount(&mut app);

    type_str(&mut app, "zz");
    app.set_prop(el, "suggestions", Vec::<SelectOption>::new());
    key(&mut app, KeyCode::F(4));

    // No overlay consumed the key: Esc reaches the global quit directly.
    assert_eq!(key(&mut app, KeyCode::Esc), Control::Quit);
}

#[test]
fn disabled_input_emits_no_queries() {
    let mut app = app();
    let el = app.mount("input-suggestion").expect("mount");
    app.set_attr(el, "disabled", "");
    let queries = probe(&mut app, el, "query-changed");

    type_str(&mut app, "ap");

    assert!(queries.borrow().is_empty(), "a disabled widget never edits");
}
