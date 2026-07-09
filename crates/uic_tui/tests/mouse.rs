//! TestBackend tests for pointer input: clicks focus, pick and blur with the
//! browser's change-on-blur semantics.

use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uic_core::{SelectOption, Value};
use uic_tui::App;

fn app(width: u16, height: u16) -> App<TestBackend> {
    ui_components::link();
    let terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
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

/// The screen cell of the first occurrence of `needle` in the last frame.
fn locate(app: &mut App<TestBackend>, needle: &str) -> (u16, u16) {
    let screen = screen(app);
    for (y, row) in screen.lines().enumerate() {
        if let Some(index) = row.find(needle) {
            let x = row[..index].chars().count();
            return (x as u16, y as u16);
        }
    }
    panic!("{needle:?} not on screen:\n{screen}");
}

fn key(app: &mut App<TestBackend>, code: KeyCode) {
    app.draw().expect("draw");
    app.handle_event(&Event::Key(KeyEvent::from(code)));
}

fn mouse(app: &mut App<TestBackend>, kind: MouseEventKind, column: u16, row: u16) {
    // Draw before dispatching: hit-testing reads the painted widget areas.
    app.draw().expect("draw");
    app.handle_event(&Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn click(app: &mut App<TestBackend>, column: u16, row: u16) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), column, row);
}

fn ring_corners(app: &mut App<TestBackend>) -> Vec<bool> {
    app.draw().expect("draw");
    let buffer = app.terminal().backend().buffer();
    let area = buffer.area;
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buffer[(x, y)].symbol() == "┌")
        .map(|(x, y)| buffer[(x, y)].fg == ratatui::style::Color::LightBlue)
        .collect()
}

#[test]
fn a_click_focuses_the_widget_under_it_and_commits_the_left_one() {
    let mut app = app(50, 14);
    app.mount("input-text").expect("mount");
    app.root_mut().expect("root").set_attr("label", "First");
    app.mount("input-text").expect("mount");
    app.root_at_mut(1)
        .expect("root")
        .set_attr("label", "Second");
    let committed: Rc<RefCell<Vec<Value>>> = Rc::default();
    {
        let committed = committed.clone();
        app.root_mut()
            .expect("root")
            .on("value-changed", move |ev| {
                committed.borrow_mut().push(ev.value.clone());
            });
    }

    key(&mut app, KeyCode::Char('1'));
    let (x, y) = locate(&mut app, "Second");
    // The input row sits inside the border below its label.
    click(&mut app, x + 2, y + 2);
    assert_eq!(
        committed.borrow().as_slice(),
        [Value::Str("1".into())],
        "leaving the first input committed it"
    );
    assert_eq!(
        ring_corners(&mut app),
        [false, true],
        "the ring follows the click"
    );

    key(&mut app, KeyCode::Char('2'));
    let screen = screen(&mut app);
    let second_row = screen.lines().position(|l| l.contains("Second")).unwrap();
    assert!(
        screen
            .lines()
            .nth(second_row + 2)
            .unwrap_or_default()
            .contains('2'),
        "typing lands in the clicked input:\n{screen}"
    );
}

#[test]
fn a_click_outside_blurs_and_a_key_refocuses() {
    let mut app = app(50, 14);
    app.mount("input-text").expect("mount");
    app.root_mut().expect("root").set_attr("label", "Note");
    let committed: Rc<RefCell<Vec<Value>>> = Rc::default();
    {
        let committed = committed.clone();
        app.root_mut()
            .expect("root")
            .on("value-changed", move |ev| {
                committed.borrow_mut().push(ev.value.clone());
            });
    }

    key(&mut app, KeyCode::Char('x'));
    click(&mut app, 2, 12);
    assert_eq!(
        committed.borrow().as_slice(),
        [Value::Str("x".into())],
        "blur commits, like @change on focus leave"
    );
    assert_eq!(ring_corners(&mut app), [false], "no ring while blurred");

    key(&mut app, KeyCode::Char('y'));
    assert_eq!(ring_corners(&mut app), [true], "input refocuses");
}

#[test]
fn a_click_picks_a_calendar_day() {
    let mut app = app(50, 16);
    app.mount("input-date").expect("mount");
    let root = app.root_mut().expect("root");
    root.set_attr("label", "Date");
    root.set_attr("value", "2026-07-07");
    let committed: Rc<RefCell<Vec<Value>>> = Rc::default();
    {
        let committed = committed.clone();
        app.root_mut()
            .expect("root")
            .on("value-changed", move |ev| {
                committed.borrow_mut().push(ev.value.clone());
            });
    }

    key(&mut app, KeyCode::F(4));
    let (x, y) = locate(&mut app, "15");
    click(&mut app, x, y);
    assert_eq!(
        committed.borrow().as_slice(),
        [Value::Str("2026-07-15".into())],
        "the clicked day commits"
    );
    assert!(
        !screen(&mut app).contains("Mo Tu"),
        "the calendar closed with the pick"
    );
}

#[test]
fn a_click_opens_a_select_and_picks_an_option() {
    let mut app = app(50, 16);
    app.mount("input-select").expect("mount");
    let root = app.root_mut().expect("root");
    root.set_attr("label", "Zone");
    root.set_prop(
        "options",
        vec![
            SelectOption::new("Europe/Amsterdam").with_short("Amsterdam"),
            SelectOption::new("Europe/Berlin").with_short("Berlin"),
        ],
    );
    let committed: Rc<RefCell<Vec<Value>>> = Rc::default();
    {
        let committed = committed.clone();
        app.root_mut()
            .expect("root")
            .on("value-changed", move |ev| {
                committed.borrow_mut().push(ev.value.clone());
            });
    }

    let (x, y) = locate(&mut app, "Zone");
    click(&mut app, x + 2, y + 2);
    assert!(
        screen(&mut app).contains("Europe/Berlin"),
        "the click opened the option list"
    );
    let (x, y) = locate(&mut app, "Europe/Berlin");
    click(&mut app, x + 1, y);
    assert_eq!(
        committed.borrow().as_slice(),
        [Value::Str("Europe/Berlin".into())],
        "the clicked option commits"
    );
    assert!(
        screen(&mut app).contains("Berlin"),
        "the closed line shows the short label"
    );
}

#[test]
fn a_press_outside_an_open_list_dismisses_it_and_focuses_the_hit() {
    let mut app = app(50, 20);
    app.mount("input-text").expect("mount");
    app.root_mut().expect("root").set_attr("label", "Note");
    app.mount("input-select").expect("mount");
    let select = app.root_at_mut(1).expect("root");
    select.set_attr("label", "Zone");
    select.set_prop("options", vec![SelectOption::new("Europe/Berlin")]);

    // The list opens below the select, leaving the input above it visible.
    let (x, y) = locate(&mut app, "Note");
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::F(4));
    assert!(screen(&mut app).contains("Europe/Berlin"), "list open");
    click(&mut app, x + 2, y + 2);
    let after = screen(&mut app);
    assert!(
        !after.contains("Europe/Berlin"),
        "the outside press dismissed the list:\n{after}"
    );
    assert_eq!(
        ring_corners(&mut app),
        [true, false],
        "the same press focused the input it landed on"
    );
}

#[test]
fn a_click_places_the_caret_and_a_drag_selects() {
    let mut app = app(50, 10);
    app.mount("input-text").expect("mount");
    let root = app.root_mut().expect("root");
    root.set_attr("label", "Note");
    root.set_attr("value", "abcdef");

    // Click between c and d: typing inserts there.
    let (x, y) = locate(&mut app, "abcdef");
    click(&mut app, x + 3, y);
    key(&mut app, KeyCode::Char('X'));
    assert!(
        screen(&mut app).contains("abcXdef"),
        "the caret landed under the click:\n{}",
        screen(&mut app)
    );

    // Drag from the start over two more cells and overtype the selection.
    click(&mut app, x, y);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), x + 2, y);
    key(&mut app, KeyCode::Char('Y'));
    assert!(
        screen(&mut app).contains("YcXdef"),
        "the drag selected the overtyped range:\n{}",
        screen(&mut app)
    );
}
