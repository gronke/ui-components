//! The shared TestBackend harness of the integration suites.
//!
//! Every suite drives the same `App` the terminals run; the helpers mirror
//! the real event loop (draw before dispatch, so widget state and popup
//! anchors sync during the paint pass before an event lands). Each test
//! binary picks its own terminal geometry through `app`.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;
use uic_core::NotifyEvent;
use uic_tui::{App, Control};

pub fn app(cols: u16, rows: u16) -> App<TestBackend> {
    ui_components::link();
    let terminal = Terminal::new(TestBackend::new(cols, rows)).expect("test terminal");
    App::from_terminal(terminal)
}

/// Draws a frame and returns the visible text, row by row.
pub fn screen(app: &mut App<TestBackend>) -> String {
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

/// The screen cell of the first occurrence of `needle` in the last frame.
pub fn locate(app: &mut App<TestBackend>, needle: &str) -> (u16, u16) {
    let screen = screen(app);
    for (y, row) in screen.lines().enumerate() {
        if let Some(index) = row.find(needle) {
            let x = row[..index].chars().count();
            return (x as u16, y as u16);
        }
    }
    panic!("{needle:?} not on screen:\n{screen}");
}

pub fn key(app: &mut App<TestBackend>, code: KeyCode) -> Control {
    app.draw().expect("draw");
    app.handle_event(&Event::Key(KeyEvent::from(code)))
}

pub fn type_str(app: &mut App<TestBackend>, text: &str) {
    for ch in text.chars() {
        key(app, KeyCode::Char(ch));
    }
}

pub fn mouse(app: &mut App<TestBackend>, kind: MouseEventKind, column: u16, row: u16) {
    app.draw().expect("draw");
    app.handle_event(&Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

pub fn click(app: &mut App<TestBackend>, column: u16, row: u16) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), column, row);
}

/// Collects one root's notify events into a shared sink.
pub fn probe(
    app: &mut App<TestBackend>,
    index: usize,
    event: &str,
) -> Rc<RefCell<Vec<NotifyEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    app.on(index, event, move |ev| sink.borrow_mut().push(ev.clone()));
    events
}

/// The foreground color of every `┌` corner, in document order — the group
/// borders wear the focus ring, idle gray or the error red.
pub fn corner_colors(app: &mut App<TestBackend>) -> Vec<Color> {
    app.draw().expect("draw");
    let buffer = app.terminal().backend().buffer();
    let area = buffer.area;
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buffer[(x, y)].symbol() == "┌")
        .map(|(x, y)| buffer[(x, y)].fg)
        .collect()
}
