//! `<input-date-range>`: the composite around two date children reads their
//! `@value-changed` events and synchronizes through the ReactiveElement flow.

use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uic_core::NotifyEvent;
use uic_tui::App;

fn app() -> App<TestBackend> {
    ui_components::link();
    let terminal = Terminal::new(TestBackend::new(64, 10)).expect("test terminal");
    App::from_terminal(terminal)
}

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

fn key(app: &mut App<TestBackend>, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code)));
}

fn type_str(app: &mut App<TestBackend>, text: &str) {
    for ch in text.chars() {
        key(app, KeyCode::Char(ch));
    }
}

fn probe(app: &mut App<TestBackend>, index: usize, event: &str) -> Rc<RefCell<Vec<NotifyEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    app.on(index, event, move |ev| sink.borrow_mut().push(ev.clone()));
    events
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
    assert!(
        screen.contains("2026-07-07") && screen.contains("2026-07-11"),
        "both ends synced into the children:\n{screen}"
    );
    assert!(
        screen.contains('–'),
        "the dash separates the ends:\n{screen}"
    );
    // The composite is seamless; each child draws its own border block.
    assert_eq!(
        screen.matches('┌').count(),
        2,
        "two child borders, none around the group:\n{screen}"
    );
}

#[test]
fn committing_a_start_beyond_the_end_pulls_the_end_along() {
    let mut app = app();
    let el = app.mount("input-date-range").expect("mount");
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
