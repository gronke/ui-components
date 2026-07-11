//! TestBackend tests for <input-select>: the dropdown widget, its option
//! popup, and the commit/revert keyboard flows.

mod support;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use uic_core::{SelectOption, Value};
use uic_tui::{App, Control};

use support::{key, probe, screen};

/// A tall terminal, so the option popup has room below the widget.
fn app() -> App<TestBackend> {
    support::app(60, 20)
}

fn zone_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new("Europe/Amsterdam").with_short("Amsterdam"),
        SelectOption::new("Europe/Berlin").with_short("Berlin"),
        SelectOption::new("Pacific/Auckland").with_short("Auckland"),
    ]
}

fn mount_zones(app: &mut App<TestBackend>) -> usize {
    let el = app.mount("input-select").expect("mount");
    app.set_attr(el, "label", "Time zone");
    app.set_attr(el, "value", "Europe/Amsterdam");
    app.set_prop(el, "options", zone_options());
    el
}

#[test]
fn closed_select_shows_the_short_label_and_marker() {
    let mut app = app();
    mount_zones(&mut app);

    let screen = screen(&mut app);
    assert!(screen.contains("Time zone"), "label row:\n{screen}");
    assert!(screen.contains("Amsterdam"), "short label:\n{screen}");
    assert!(
        !screen.contains("Europe/Amsterdam"),
        "closed line is compact:\n{screen}"
    );
    assert!(screen.contains("▼"), "closed marker:\n{screen}");
}

#[test]
fn f4_opens_the_popup_with_full_labels() {
    let mut app = app();
    mount_zones(&mut app);

    key(&mut app, KeyCode::F(4));
    let screen = screen(&mut app);
    assert!(
        screen.contains("Europe/Amsterdam"),
        "full labels in the open list:\n{screen}"
    );
    assert!(screen.contains("Europe/Berlin"), "all rows:\n{screen}");
}

#[test]
fn down_opens_and_enter_commits_through_the_change_path() {
    let mut app = app();
    let el = mount_zones(&mut app);
    let events = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "one value-changed event");
    assert_eq!(events[0].value, Value::Str("Europe/Berlin".into()));
    assert_eq!(events[0].old_value, Value::Str("Europe/Amsterdam".into()));

    drop(events);
    let screen = screen(&mut app);
    assert!(
        screen.contains("Berlin"),
        "committed short label:\n{screen}"
    );
}

#[test]
fn esc_reverts_browsing_and_a_second_esc_quits() {
    let mut app = app();
    let el = mount_zones(&mut app);
    let events = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::F(4));
    key(&mut app, KeyCode::Down);
    assert_eq!(key(&mut app, KeyCode::Esc), Control::Continue);

    assert!(events.borrow().is_empty(), "no event from browsing");
    let screen = screen(&mut app);
    assert!(
        screen.contains("Amsterdam") && !screen.contains("Europe/Berlin"),
        "reverted and closed:\n{screen}"
    );
    assert_eq!(key(&mut app, KeyCode::Esc), Control::Quit);
}

#[test]
fn tab_commits_the_browsed_value_and_falls_through() {
    let mut app = app();
    let el = mount_zones(&mut app);
    let events = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::F(4));
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Tab);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "Tab commits the highlighted option");
    assert_eq!(events[0].value, Value::Str("Europe/Berlin".into()));
}

#[test]
fn type_ahead_commits_while_closed() {
    let mut app = app();
    let el = mount_zones(&mut app);
    let events = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::Char('p'));

    let events = events.borrow();
    assert_eq!(events.len(), 1, "closed type-ahead commits");
    assert_eq!(events[0].value, Value::Str("Pacific/Auckland".into()));
}

#[test]
fn the_default_row_commits_null() {
    let mut app = app();
    let el = app.mount("input-select").expect("mount");
    app.set_attr(el, "default", "Pick a zone");
    app.set_attr(el, "value", "Europe/Amsterdam");
    app.set_prop(el, "options", zone_options());
    let events = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::F(4));
    let open = screen(&mut app);
    assert!(open.contains("Pick a zone"), "default row label:\n{open}");
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Null);
}

#[test]
fn disabled_select_ignores_the_open_keys() {
    let mut app = app();
    let el = app.mount("input-select").expect("mount");
    app.set_attr(el, "disabled", "");
    app.set_prop(el, "options", zone_options());

    key(&mut app, KeyCode::F(4));
    key(&mut app, KeyCode::Down);
    let screen = screen(&mut app);
    assert!(
        !screen.contains("Europe/Amsterdam"),
        "no popup on a disabled select:\n{screen}"
    );
}

#[test]
fn long_lists_scroll_to_the_end() {
    let mut app = app();
    let el = app.mount("input-select").expect("mount");
    let options: Vec<SelectOption> = (0..15)
        .map(|i| SelectOption::new(format!("Zone/A{i:02}")))
        .collect();
    app.set_attr(el, "value", "Zone/A00");
    app.set_prop(el, "options", options);

    key(&mut app, KeyCode::F(4));
    let top = screen(&mut app);
    assert!(top.contains("Zone/A00"), "list starts at the top:\n{top}");
    assert!(!top.contains("Zone/A14"), "tail beyond the page:\n{top}");

    key(&mut app, KeyCode::End);
    let bottom = screen(&mut app);
    assert!(
        bottom.contains("Zone/A14"),
        "scrolled to the end:\n{bottom}"
    );
}

#[test]
fn input_timezone_serves_the_platform_zone_list() {
    let mut app = app();
    let el = app.mount("input-timezone").expect("mount");
    app.set_attr(el, "label", "Time zone");
    app.set_attr(el, "value", "Europe/Berlin");
    let events = probe(&mut app, el, "value-changed");

    let closed = screen(&mut app);
    assert!(closed.contains("Berlin"), "short label:\n{closed}");
    assert!(
        !closed.contains("Europe/Berlin"),
        "compact closed line:\n{closed}"
    );

    key(&mut app, KeyCode::F(4));
    let open = screen(&mut app);
    assert!(open.contains("Europe/"), "full ids listed:\n{open}");

    // UTC is pinned to the first row; Home reaches it, Enter commits.
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Enter);
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Str("UTC".into()));
}

#[test]
fn empty_options_render_and_commit_without_events() {
    let mut app = app();
    let el = app.mount("input-select").expect("mount");
    let events = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::F(4));
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Enter);

    assert!(events.borrow().is_empty(), "nothing to commit");
    let screen = screen(&mut app);
    assert!(screen.contains("▼"), "widget still renders:\n{screen}");
}
