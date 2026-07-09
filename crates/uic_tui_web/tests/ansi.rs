use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use uic_tui::{App, Control};
use uic_tui_web::XtermBackend;

fn app(cols: u16, rows: u16) -> (App<XtermBackend>, uic_tui_web::Output) {
    ui_components::link();
    let (backend, out) = XtermBackend::new(cols, rows);
    (App::from_terminal(Terminal::new(backend).unwrap()), out)
}

fn key(app: &mut App<XtermBackend>, code: KeyCode) -> Control {
    // Draw before dispatching, like the real event loop.
    app.draw().unwrap();
    app.handle_event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

#[test]
fn a_frame_is_positioned_styled_ansi() {
    let (mut app, out) = app(44, 10);
    app.mount("input-text").unwrap();
    let root = app.root_mut().unwrap();
    root.set_attr("label", "Note");
    root.set_attr("value", "hello");
    app.draw().unwrap();

    let ansi = out.take();
    assert!(
        ansi.starts_with("\x1b[1;1H"),
        "first cell run starts at the origin: {ansi:?}"
    );
    assert!(ansi.contains("Note"), "label text in the stream");
    assert!(ansi.contains("hello"), "value text in the stream");
    assert!(ansi.contains("\x1b[0;"), "styled runs reset-then-set");
    assert!(
        ansi.contains(";94m"),
        "the focused group wears the palette focus ring: {ansi:?}"
    );
    assert!(
        ansi.contains("\x1b[?25h"),
        "the focused text input shows the caret"
    );

    let screen = app.terminal().backend().screen_text();
    assert!(
        screen.contains("Note"),
        "shadow grid mirrors the frame:\n{screen}"
    );
    assert!(screen.contains("hello"));
}

#[test]
fn keys_travel_through_the_runtime_and_back_as_ansi() {
    let (mut app, out) = app(44, 10);
    app.mount("input-text").unwrap();
    app.root_mut().unwrap().set_attr("label", "Note");
    app.draw().unwrap();
    out.take();

    key(&mut app, KeyCode::Char('A'));
    app.draw().unwrap();
    let ansi = out.take();
    assert!(
        ansi.contains('A'),
        "the typed glyph reaches the stream: {ansi:?}"
    );
    assert!(app.terminal().backend().screen_text().contains('A'));
}

#[test]
fn commits_notify_like_the_terminal_demo() {
    let (mut app, out) = app(44, 10);
    app.mount("input-text").unwrap();
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    {
        let seen = seen.clone();
        app.root_mut().unwrap().on("value-changed", move |notify| {
            seen.borrow_mut().push(notify.value.display_text());
        });
    }
    key(&mut app, KeyCode::Char('A'));
    key(&mut app, KeyCode::Tab);
    drop(out);
    assert_eq!(seen.borrow().as_slice(), ["A"]);
}

#[test]
fn roots_stack_and_tab_crosses_their_boundary() {
    let (mut app, _out) = app(50, 16);
    app.mount("input-text").unwrap();
    app.root_mut().unwrap().set_attr("label", "First");
    app.mount("input-number").unwrap();
    app.root_at_mut(1).unwrap().set_attr("label", "Second");
    app.draw().unwrap();

    let screen = app.terminal().backend().screen_text();
    assert!(screen.contains("First"), "both roots paint:\n{screen}");
    assert!(screen.contains("Second"));
    let first_row = screen.lines().position(|l| l.contains("First")).unwrap();
    let second_row = screen.lines().position(|l| l.contains("Second")).unwrap();
    assert!(first_row < second_row, "roots stack in mount order");

    // One focusable per root: Tab wraps out of the first root and lands in
    // the second, so typing reaches the second input.
    key(&mut app, KeyCode::Char('1'));
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char('2'));
    key(&mut app, KeyCode::Tab);
    app.draw().unwrap();
    let screen = app.terminal().backend().screen_text();
    let row_of = |needle: char| screen.lines().position(|line| line.contains(needle));
    let one = row_of('1').unwrap();
    let two = row_of('2').unwrap();
    assert!(
        first_row < one && one < second_row,
        "the first value sits inside the first root's band:\n{screen}"
    );
    assert!(
        second_row < two,
        "the second value sits inside the second root's band:\n{screen}"
    );
}
