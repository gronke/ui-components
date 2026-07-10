//! TestBackend tests for `<app-root>`: the state object trickling down into
//! the form children, child commits folding back into `state`, and the
//! echo-free `state-changed` contract (ADR 0013).

use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uic_core::{NotifyEvent, ObjectMap, Value};
use uic_tui::{App, Control};

/// Tall enough for the whole form plus the select popup.
fn app() -> App<TestBackend> {
    ui_components::link();
    let terminal = Terminal::new(TestBackend::new(72, 50)).expect("test terminal");
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

/// Draw before dispatching, like the real event loop: widget state and
/// popup anchors sync during the paint pass.
fn key(app: &mut App<TestBackend>, code: KeyCode) -> Control {
    app.draw().expect("draw");
    app.handle_event(&Event::Key(KeyEvent::from(code)))
}

fn type_str(app: &mut App<TestBackend>, text: &str) {
    for ch in text.chars() {
        key(app, KeyCode::Char(ch));
    }
}

fn probe(app: &mut App<TestBackend>, index: usize) -> Rc<RefCell<Vec<NotifyEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    app.on(index, "state-changed", move |ev| {
        sink.borrow_mut().push(ev.clone())
    });
    events
}

fn state(entries: &[(&str, Value)]) -> ObjectMap {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

#[test]
fn state_pushdown_reaches_the_children_and_the_state_line() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    app.set_prop(
        el,
        "state",
        state(&[
            ("date", "2026-07-07".into()),
            ("note", "hello".into()),
            ("pick", "Europe/Berlin".into()),
        ]),
    );

    let screen = screen(&mut app);
    assert!(
        screen.contains("2026-07-07"),
        "date member in the child widget:\n{screen}"
    );
    assert!(
        screen.contains("hello"),
        "note member in the child widget:\n{screen}"
    );
    // The closed pick select shows the short label (the embedded timezone
    // select and the state line carry the full id).
    assert!(
        screen.lines().any(|line| line.contains("Berlin ")
            && line.contains('▼')
            && !line.contains("Europe/")),
        "pick member as the select's short label:\n{screen}"
    );
    assert!(
        screen.contains("date: 2026-07-07 · note: hello · pick: Europe/Berlin"),
        "the state line renders the sorted members:\n{screen}"
    );
}

#[test]
fn child_commit_updates_state_once_and_keeps_siblings() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    app.set_prop(el, "state", state(&[("note", "hello".into())]));
    let events = probe(&mut app, el);

    // Focus starts on the date widget (document order).
    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "one state-changed per commit, no echo");
    let state = events[0].value.as_object().expect("object state");
    assert_eq!(
        state.get("date"),
        Some(&Value::Str("2026-08-01".into())),
        "the committed date joined the state"
    );
    assert_eq!(
        state.get("note"),
        Some(&Value::Str("hello".into())),
        "sibling members stay intact"
    );
}

#[test]
fn select_pick_lands_in_state() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    let events = probe(&mut app, el);

    // Document order: date, its embedded timezone select, range start and
    // end, note, amount — the sixth Tab reaches the pick select.
    for _ in 0..6 {
        key(&mut app, KeyCode::Tab);
    }
    key(&mut app, KeyCode::F(4));
    let open = screen(&mut app);
    assert!(
        open.contains("Europe/Amsterdam"),
        "the option popup is open with full labels:\n{open}"
    );
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Enter);

    // Two state changes: tabbing through the untouched note input commits
    // empty → null (its allow-null contract), then the pick itself.
    let events = events.borrow();
    assert_eq!(events.len(), 2, "the null pass-through commit and the pick");
    let first = events[0].value.as_object().expect("object state");
    assert_eq!(first.get("note"), Some(&Value::Null));
    let state = events[1].value.as_object().expect("object state");
    assert_eq!(
        state.get("pick"),
        Some(&Value::Str("Pacific/Auckland".into()))
    );
}

#[test]
fn external_equal_state_is_suppressed() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    let events = probe(&mut app, el);

    let snapshot = state(&[("date", "2026-07-07".into()), ("note", "x".into())]);
    app.set_prop(el, "state", snapshot.clone());
    app.set_prop(el, "state", snapshot);

    assert_eq!(
        events.borrow().len(),
        1,
        "the deeply equal re-write is no change — the transport echo dies here"
    );
}

#[test]
fn sparse_state_leaves_child_defaults_and_stays_silent() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    let events = probe(&mut app, el);

    let screen = screen(&mut app);
    assert!(
        screen.contains("Pick a zone"),
        "the select rests on its default row:\n{screen}"
    );
    assert!(
        screen.contains("state ·"),
        "the empty state line renders:\n{screen}"
    );
    assert!(
        events.borrow().is_empty(),
        "missing members resolve to the children's own defaults — no boot writes"
    );
}

#[test]
fn range_inversion_heals_through_the_child_back_into_state() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    let events = probe(&mut app, el);

    // The template pushes start before end, so the inverted end pulls the
    // start along (the range's will_update rule for an end-only change).
    app.set_prop(
        el,
        "state",
        state(&[("start", "2026-07-20".into()), ("end", "2026-07-10".into())]),
    );

    let events = events.borrow();
    let healed = events.last().expect("healed state").value.clone();
    let healed = healed.as_object().expect("object state");
    assert_eq!(healed.get("start"), healed.get("end"), "coherent interval");
    assert_eq!(healed.get("end"), Some(&Value::Str("2026-07-10".into())));
}
