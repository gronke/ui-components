//! TestBackend tests: the same <input-date> definition the browser runs,
//! rendered into an in-memory terminal and driven by key events.

use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uic_core::{NotifyEvent, Value};
use uic_tui::{App, Control};

fn app() -> App<TestBackend> {
    ui_components::link();
    let terminal = Terminal::new(TestBackend::new(50, 10)).expect("test terminal");
    App::from_terminal(terminal)
}

/// Draws a frame and returns the visible text, row by row.
fn screen(app: &mut App<TestBackend>) -> String {
    app.draw().expect("draw");
    let buffer = app.terminal().backend().buffer();
    let area = buffer.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(app: &mut App<TestBackend>, code: KeyCode) -> Control {
    app.handle_event(&Event::Key(KeyEvent::from(code)))
}

fn type_str(app: &mut App<TestBackend>, text: &str) {
    for ch in text.chars() {
        key(app, KeyCode::Char(ch));
    }
}

fn events_probe(app: &mut App<TestBackend>) -> Rc<RefCell<Vec<NotifyEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    app.root_mut()
        .expect("mounted")
        .on("value-changed", move |ev| {
            sink.borrow_mut().push(ev.clone())
        });
    events
}

#[test]
fn renders_label_value_and_hint() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("label", "Date of purchase");
    el.set_attr("hint", "Format: YYYY-MM-DD");
    el.set_attr("value", "2026-07-07");

    let screen = screen(&mut app);
    assert!(screen.contains("Date of purchase"), "label row:\n{screen}");
    assert!(screen.contains("2026-07-07"), "value in widget:\n{screen}");
    assert!(screen.contains("Format: YYYY-MM-DD"), "hint row:\n{screen}");
}

#[test]
fn commit_updates_value_and_notifies() {
    let mut app = app();
    app.mount("input-date").expect("mount");
    let events = events_probe(&mut app);

    // Type in mask order (separators jump sections), commit with Enter.
    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "one value-changed event");
    assert_eq!(events[0].event_name, "value-changed");
    assert_eq!(events[0].value, Value::Str("2026-08-01".into()));
    assert_eq!(events[0].old_value, Value::Str(String::new()));
}

#[test]
fn invalid_input_renders_the_error_line_and_hides_the_hint() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("hint", "Format: YYYY-MM-DD");
    let events = events_probe(&mut app);

    type_str(&mut app, "2026-13-99");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("Invalid date: 2026-13-99"),
        "error line:\n{screen}"
    );
    assert!(
        !screen.contains("Format: YYYY-MM-DD"),
        "hint hidden while error shows:\n{screen}"
    );
    assert!(
        events.borrow().is_empty(),
        "no value event on invalid input"
    );
}

#[test]
fn min_bound_is_enforced() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("min", "2020-01-01");
    let events = events_probe(&mut app);

    type_str(&mut app, "2019-01-01");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("Date before minimum 2020-01-01"),
        "min error:\n{screen}"
    );
    assert!(events.borrow().is_empty());
}

#[test]
fn disabled_widget_ignores_input() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("disabled", "");
    let events = events_probe(&mut app);

    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    assert!(events.borrow().is_empty(), "disabled input commits nothing");
}

#[test]
fn text_input_renders_chrome_and_commits_trimmed() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    el.set_attr("label", "Note");
    el.set_attr("hint", "Free text");
    let events = events_probe(&mut app);

    let before = screen(&mut app);
    assert!(before.contains("Note"), "chrome label:\n{before}");
    assert!(before.contains("Free text"), "chrome hint:\n{before}");

    type_str(&mut app, "  hello  ");
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "one value-changed event");
    assert_eq!(events[0].value, Value::Str("hello".into()));
}

#[test]
fn text_input_allow_null_commits_null_for_empty() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    el.set_attr("allow-null", "");
    let events = events_probe(&mut app);

    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Null);
    assert_eq!(events[0].old_value, Value::Str(String::new()));
}

#[test]
fn text_input_value_attribute_syncs_into_the_widget() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    el.set_attr("value", "prefilled");
    let screen = screen(&mut app);
    assert!(screen.contains("prefilled"), "synced value:\n{screen}");
}

#[test]
fn esc_quits() {
    let mut app = app();
    app.mount("input-date").expect("mount");
    assert_eq!(key(&mut app, KeyCode::Esc), Control::Quit);
}

/// A taller terminal so the calendar overlay fits below the input chrome.
fn tall_app() -> App<TestBackend> {
    ui_components::link();
    let terminal = Terminal::new(TestBackend::new(60, 20)).expect("test terminal");
    App::from_terminal(terminal)
}

#[test]
fn f4_opens_the_calendar_over_the_content_below() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("value", "2026-07-07");
    el.set_attr("hint", "Format: YYYY-MM-DD");

    let before = screen(&mut app);
    assert!(before.contains("Format: YYYY-MM-DD"), "hint row:\n{before}");
    assert!(!before.contains("July 2026"), "calendar closed:\n{before}");

    key(&mut app, KeyCode::F(4));
    let after = screen(&mut app);
    assert!(after.contains("July 2026"), "month title:\n{after}");
    assert!(after.contains("Mo Tu We"), "weekday header:\n{after}");
    assert!(
        !after.contains("Format: YYYY-MM-DD"),
        "overlay paints over the hint row:\n{after}"
    );
}

#[test]
fn calendar_enter_commits_through_the_change_path() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("value", "2026-07-07");
    let events = events_probe(&mut app);

    screen(&mut app);
    key(&mut app, KeyCode::Down); // Down opens like F4
    key(&mut app, KeyCode::Right); // 2026-07-08
    key(&mut app, KeyCode::Enter);

    let after = screen(&mut app);
    assert!(!after.contains("July 2026"), "popup closed:\n{after}");
    assert!(
        after.contains("2026-07-08"),
        "picked date in widget:\n{after}"
    );
    let events = events.borrow();
    assert_eq!(events.len(), 1, "one value-changed event");
    assert_eq!(events[0].value, Value::Str("2026-07-08".into()));
    assert_eq!(events[0].old_value, Value::Str("2026-07-07".into()));
}

#[test]
fn calendar_arrows_roll_over_month_edges() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("value", "2026-07-01");
    let events = events_probe(&mut app);

    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    key(&mut app, KeyCode::Left); // rolls into June
    let june = screen(&mut app);
    assert!(june.contains("June 2026"), "rolled into June:\n{june}");
    key(&mut app, KeyCode::Enter);

    assert_eq!(events.borrow()[0].value, Value::Str("2026-06-30".into()));
}

#[test]
fn calendar_pages_by_month() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("value", "2026-07-07");
    let events = events_probe(&mut app);

    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    key(&mut app, KeyCode::PageDown);
    let paged = screen(&mut app);
    assert!(paged.contains("August 2026"), "paged forward:\n{paged}");
    key(&mut app, KeyCode::Enter);

    assert_eq!(events.borrow()[0].value, Value::Str("2026-08-07".into()));
}

#[test]
fn calendar_esc_closes_before_quit() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("value", "2026-07-07");

    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    assert!(screen(&mut app).contains("July 2026"));

    assert_eq!(key(&mut app, KeyCode::Esc), Control::Continue);
    assert!(!screen(&mut app).contains("July 2026"), "popup closed");
    assert_eq!(key(&mut app, KeyCode::Esc), Control::Quit);
}

#[test]
fn calendar_commit_respects_the_min_bound() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("value", "2026-07-07");
    el.set_attr("min", "2026-07-05");
    let events = events_probe(&mut app);

    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    for _ in 0..3 {
        key(&mut app, KeyCode::Left); // 2026-07-04, below min
    }
    key(&mut app, KeyCode::Enter);

    let after = screen(&mut app);
    assert!(
        after.contains("Date before minimum 2026-07-05"),
        "validation runs on picked dates too:\n{after}"
    );
    assert!(events.borrow().is_empty(), "no event for a rejected pick");
}

#[test]
fn disabled_date_ignores_the_calendar_keys() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("disabled", "");

    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    let after = screen(&mut app);
    assert!(!after.contains("Mo Tu We"), "no calendar:\n{after}");
}

#[test]
fn date_commit_notifies_the_zoned_date_in_the_current_timezone() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    el.set_attr("default-timezone", "Europe/Berlin");
    let dates = Rc::new(RefCell::new(Vec::new()));
    let sink = dates.clone();
    el.on("date-changed", move |ev| sink.borrow_mut().push(ev.clone()));

    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let dates = dates.borrow();
    assert_eq!(dates.len(), 1, "one date-changed event");
    let zoned = dates[0].value.as_zoned().expect("zoned detail");
    assert_eq!(zoned.iso(), "2026-08-01T00:00:00+02:00[Europe/Berlin]");
}

/// Clears the focused single-line widget (the number input starts at the
/// formatted default, e.g. `0,00`).
fn clear_input(app: &mut App<TestBackend>) {
    key(app, KeyCode::End);
    for _ in 0..12 {
        key(app, KeyCode::Backspace);
    }
}

#[test]
fn number_commit_parses_separators_and_echoes_the_format() {
    let mut app = app();
    let el = app.mount("input-number").expect("mount");
    el.set_attr("label", "Amount");
    let events = events_probe(&mut app);

    clear_input(&mut app);
    type_str(&mut app, "1.234,5");
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "one value-changed event");
    assert_eq!(events[0].value, Value::Num(1234.5));

    // The computed display binding re-syncs the widget after the commit.
    let screen = screen(&mut app);
    assert!(screen.contains("1234,50"), "formatted echo:\n{screen}");
}

#[test]
fn number_invalid_commit_shows_the_error_line() {
    let mut app = app();
    app.mount("input-number").expect("mount");
    let events = events_probe(&mut app);

    type_str(&mut app, "12x");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("Invalid number: 12x"),
        "error line:\n{screen}"
    );
    assert!(events.borrow().is_empty());
}

#[test]
fn number_allow_null_commits_null_for_empty() {
    let mut app = app();
    let el = app.mount("input-number").expect("mount");
    el.set_attr("allow-null", "");
    let events = events_probe(&mut app);

    clear_input(&mut app);
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Null);
    assert_eq!(events[0].old_value, Value::Num(0.0));
}

#[test]
fn textarea_enter_adds_lines_and_tab_commits() {
    let mut app = tall_app();
    let el = app.mount("input-textarea").expect("mount");
    el.set_attr("label", "Comment");
    let events = events_probe(&mut app);

    type_str(&mut app, "line one");
    key(&mut app, KeyCode::Enter); // newline, not a commit
    type_str(&mut app, "line two");
    assert!(events.borrow().is_empty(), "Enter did not commit");

    let screen = screen(&mut app);
    assert!(screen.contains("line one"), "first line:\n{screen}");
    assert!(screen.contains("line two"), "second line:\n{screen}");
    let first_row = screen.lines().position(|l| l.contains("line one"));
    let second_row = screen.lines().position(|l| l.contains("line two"));
    assert!(first_row < second_row, "lines stack vertically:\n{screen}");

    key(&mut app, KeyCode::Tab); // focus leave commits
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Str("line one\nline two".into()));
}
