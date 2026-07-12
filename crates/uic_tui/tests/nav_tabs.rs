//! TestBackend tests for <nav-tabs>: arrow and pointer picks through the
//! `@input` route, the bound value driving the highlight, and the
//! fallback-to-first rule for unknown values.

mod support;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use uic_core::{SelectOption, Value};
use uic_tui::App;

use support::{click, key, locate, probe, screen};

fn app() -> App<TestBackend> {
    support::app(40, 4)
}

fn mount(app: &mut App<TestBackend>, value: &str) -> usize {
    let el = app.mount("nav-tabs").expect("mount");
    app.set_attr(el, "value", value);
    app.set_prop(
        el,
        "options",
        vec![
            SelectOption::new("form").with_short("Form"),
            SelectOption::new("about").with_short("About"),
        ],
    );
    el
}

#[test]
fn arrows_switch_the_tab_and_notify() {
    let mut app = app();
    let el = mount(&mut app, "form");
    let picks = probe(&mut app, el, "value-changed");

    let frame = screen(&mut app);
    assert!(frame.contains("Form"), "captions on screen:\n{frame}");
    assert!(frame.contains("About"), "captions on screen:\n{frame}");

    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::Left);

    let picks = picks.borrow();
    assert_eq!(picks.len(), 2);
    assert_eq!(picks[0].value, Value::Str("about".into()));
    assert_eq!(picks[1].value, Value::Str("form".into()));
}

#[test]
fn a_click_picks_the_caption_under_the_pointer() {
    let mut app = app();
    let el = mount(&mut app, "form");
    let picks = probe(&mut app, el, "value-changed");

    let (x, y) = locate(&mut app, "About");
    click(&mut app, x, y);

    let picks = picks.borrow();
    assert_eq!(picks.len(), 1, "the pick dispatches once");
    assert_eq!(picks[0].value, Value::Str("about".into()));
}

#[test]
fn clicking_the_selected_tab_stays_silent() {
    let mut app = app();
    let el = mount(&mut app, "form");
    let picks = probe(&mut app, el, "value-changed");

    let (x, y) = locate(&mut app, "Form");
    click(&mut app, x, y);

    assert!(picks.borrow().is_empty(), "no change, no event");
}

#[test]
fn an_external_value_moves_the_selection() {
    let mut app = app();
    let el = mount(&mut app, "form");
    app.set_attr(el, "value", "about");
    let picks = probe(&mut app, el, "value-changed");

    // Left from the synced "about" lands back on the first tab — proof the
    // highlight followed the external write, not the last local pick.
    key(&mut app, KeyCode::Left);

    let picks = picks.borrow();
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0].value, Value::Str("form".into()));
}

#[test]
fn an_unknown_value_falls_back_to_the_first_tab() {
    let mut app = app();
    let el = mount(&mut app, "no-such-tab");
    let picks = probe(&mut app, el, "value-changed");

    // The fallback highlight sits on the first tab, so Right steps to the
    // second — mirroring the browser rows' Math.max(0, findIndex) rule.
    key(&mut app, KeyCode::Right);

    let picks = picks.borrow();
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0].value, Value::Str("about".into()));
}

#[test]
fn an_empty_bar_ignores_keys_and_stays_blank() {
    let mut app = app();
    let el = app.mount("nav-tabs").expect("mount");
    let picks = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::Right);
    key(&mut app, KeyCode::Left);

    assert!(picks.borrow().is_empty());
    let frame = screen(&mut app);
    assert_eq!(frame.trim(), "", "nothing to paint:\n{frame}");
}
