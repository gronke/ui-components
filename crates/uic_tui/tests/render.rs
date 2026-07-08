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
