//! `<input-date-range>`: the composite around two date children reads their
//! `@value-changed` events and synchronizes through the ReactiveElement flow.

mod support;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use uic_tui::App;

use support::{key, probe, screen, type_str};

fn app() -> App<TestBackend> {
    support::app(64, 10)
}

#[test]
fn the_range_renders_two_dates_under_one_chrome() {
    let mut app = app();
    let el = app.mount("input-date-range").expect("mount");
    app.set_attr(el, "label", "Stay");
    app.set_attr(el, "hint", "The end never precedes the start");
    app.set_attr(el, "start", "2026-07-07");
    app.set_attr(el, "end", "2026-07-11");

    let screen = screen(&mut app);
    assert!(screen.contains("Stay"), "shared label:\n{screen}");
    let row = screen
        .lines()
        .find(|line| line.contains("2026-07-07"))
        .expect("the value row");
    assert!(
        row.contains("2026-07-11"),
        "both ends share the row:\n{screen}"
    );
    assert!(
        row.contains(" - "),
        "the separator cell sits between the ends:\n{screen}"
    );
    // The group draws THE single border; the seamless children none.
    assert_eq!(
        screen.matches('┌').count(),
        1,
        "one border around the whole range:\n{screen}"
    );
}

#[test]
fn a_narrow_range_wraps_its_end_segment() {
    let mut app = support::app(24, 12);
    let el = app.mount("input-date-range").expect("mount");
    app.set_attr(el, "start", "2026-07-07");
    app.set_attr(el, "end", "2026-07-11");

    // Too narrow for both segments: the `- end` segment drops to its own
    // line inside the same border, the catalog's small-screen break.
    let screen = screen(&mut app);
    let start_row = screen
        .lines()
        .position(|line| line.contains("2026-07-07"))
        .expect("start row");
    let end_row = screen
        .lines()
        .position(|line| line.contains("2026-07-11"))
        .expect("end row");
    assert!(
        end_row > start_row,
        "the end segment wrapped below the start:\n{screen}"
    );
    assert_eq!(
        screen.matches('┌').count(),
        1,
        "still one border around the wrapped group:\n{screen}"
    );
}

#[test]
fn show_timezone_appends_the_group_select_and_feeds_the_children() {
    let mut app = support::app(72, 16);
    let el = app.mount("input-date-range").expect("mount");
    app.set_attr(el, "show-timezone", "");
    app.set_attr(el, "default-timezone", "Europe/Berlin");
    let zones = probe(&mut app, el, "timezone-changed");

    // The trailing select closes on the short label of the default zone.
    let closed = screen(&mut app);
    assert!(closed.contains('▼'), "the group select marker:\n{closed}");
    assert!(
        closed.contains("Berlin"),
        "the default zone's short label:\n{closed}"
    );
    assert_eq!(
        closed.matches('┌').count(),
        1,
        "one border around dates and select:\n{closed}"
    );

    // Focus order: start date, end date, then the group select; UTC is
    // pinned to the first row after the default, and Enter commits it as
    // the range's timezone.
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::F(4));
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);

    let zones = zones.borrow();
    assert_eq!(zones.len(), 1, "one timezone-changed commit");
    assert_eq!(zones[0].value.display_text(), "UTC");
}

#[test]
fn committing_a_start_beyond_the_end_pulls_the_end_along() {
    let mut app = app();
    let el = app.mount("input-date-range").expect("mount");
    app.set_attr(el, "hide-time", "");
    app.set_attr(el, "end", "2026-07-15");
    let values = probe(&mut app, el, "value-changed");
    let ends = probe(&mut app, el, "end-changed");

    // Focus starts on the start child; commit a date past the end.
    screen(&mut app);
    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert_eq!(
        screen.matches("2026-08-01").count(),
        2,
        "the end followed the start:\n{screen}"
    );
    assert_eq!(
        ends.borrow().last().map(|ev| ev.value.display_text()),
        Some("2026-08-01".to_string()),
        "end-changed notified the clamp"
    );
    assert_eq!(
        values.borrow().last().map(|ev| ev.value.display_text()),
        Some("2026-08-01/2026-08-01".to_string()),
        "the combined value committed the interval"
    );
}

#[test]
fn committing_an_end_before_the_start_pulls_the_start_back() {
    let mut app = app();
    let el = app.mount("input-date-range").expect("mount");
    app.set_attr(el, "hide-time", "");
    app.set_attr(el, "start", "2026-07-10");
    let starts = probe(&mut app, el, "start-changed");

    screen(&mut app);
    key(&mut app, KeyCode::Tab);
    type_str(&mut app, "2026-07-05");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert_eq!(
        screen.matches("2026-07-05").count(),
        2,
        "the start followed the end:\n{screen}"
    );
    assert_eq!(
        starts.borrow().last().map(|ev| ev.value.display_text()),
        Some("2026-07-05".to_string()),
        "start-changed notified the clamp"
    );
}

#[test]
fn an_external_value_decomposes_into_both_ends() {
    let mut app = app();
    let el = app.mount("input-date-range").expect("mount");
    let starts = probe(&mut app, el, "start-changed");
    let ends = probe(&mut app, el, "end-changed");

    app.set_attr(el, "value", "2026-01-01/2026-02-01");

    let screen = screen(&mut app);
    assert!(
        screen.contains("2026-01-01") && screen.contains("2026-02-01"),
        "the interval decomposed into the children:\n{screen}"
    );
    assert_eq!(starts.borrow().len(), 1);
    assert_eq!(ends.borrow().len(), 1);
}

#[test]
fn an_inverted_external_value_normalizes() {
    let mut app = app();
    let el = app.mount("input-date-range").expect("mount");
    let values = probe(&mut app, el, "value-changed");

    app.set_attr(el, "value", "2026-09-01/2026-08-01");

    let screen = screen(&mut app);
    assert_eq!(
        screen.matches("2026-09-01").count(),
        2,
        "the inverted end clamped to the start:\n{screen}"
    );
    assert_eq!(
        values.borrow().last().map(|ev| ev.value.display_text()),
        Some("2026-09-01/2026-09-01".to_string()),
        "the normalized interval is what committed"
    );
}
