//! TestBackend tests for pointer input: clicks focus, pick and blur with the
//! browser's change-on-blur semantics.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::backend::TestBackend;
use uic_core::{NotifyEvent, SelectOption, Value};
use uic_tui::App;

use support::{app, click, key, locate, mouse, probe, screen};

/// The focus ring per group corner: `true` where the border wears it.
fn ring_corners(app: &mut App<TestBackend>) -> Vec<bool> {
    support::corner_colors(app)
        .into_iter()
        .map(|color| color == ratatui::style::Color::LightBlue)
        .collect()
}

/// The committed values a probe collected, in order.
fn committed_values(events: &Rc<RefCell<Vec<NotifyEvent>>>) -> Vec<Value> {
    events.borrow().iter().map(|ev| ev.value.clone()).collect()
}

#[test]
fn a_click_focuses_the_widget_under_it_and_commits_the_left_one() {
    let mut app = app(50, 14);
    let first = app.mount("input-text").expect("mount");
    app.set_attr(first, "label", "First");
    let second = app.mount("input-text").expect("mount");
    app.set_attr(second, "label", "Second");
    let committed = probe(&mut app, first, "value-changed");

    key(&mut app, KeyCode::Char('1'));
    let (x, y) = locate(&mut app, "Second");
    // The input row sits inside the border below its label.
    click(&mut app, x + 2, y + 2);
    assert_eq!(
        committed_values(&committed),
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
    let el = app.mount("input-text").expect("mount");
    app.set_attr(el, "label", "Note");
    let committed = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::Char('x'));
    click(&mut app, 2, 12);
    assert_eq!(
        committed_values(&committed),
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
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "label", "Date");
    app.set_attr(el, "value", "2026-07-07");
    let committed = probe(&mut app, el, "value-changed");

    key(&mut app, KeyCode::F(4));
    let (x, y) = locate(&mut app, "15");
    click(&mut app, x, y);
    assert_eq!(
        committed_values(&committed),
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
    let el = app.mount("input-select").expect("mount");
    app.set_attr(el, "label", "Zone");
    app.set_prop(
        el,
        "options",
        vec![
            SelectOption::new("Europe/Amsterdam").with_short("Amsterdam"),
            SelectOption::new("Europe/Berlin").with_short("Berlin"),
        ],
    );
    let committed = probe(&mut app, el, "value-changed");

    let (x, y) = locate(&mut app, "Zone");
    click(&mut app, x + 2, y + 2);
    assert!(
        screen(&mut app).contains("Europe/Berlin"),
        "the click opened the option list"
    );
    let (x, y) = locate(&mut app, "Europe/Berlin");
    click(&mut app, x + 1, y);
    assert_eq!(
        committed_values(&committed),
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
    let note = app.mount("input-text").expect("mount");
    app.set_attr(note, "label", "Note");
    let select = app.mount("input-select").expect("mount");
    app.set_attr(select, "label", "Zone");
    app.set_prop(select, "options", vec![SelectOption::new("Europe/Berlin")]);

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
    let el = app.mount("input-text").expect("mount");
    app.set_attr(el, "label", "Note");
    app.set_attr(el, "value", "abcdef");

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
