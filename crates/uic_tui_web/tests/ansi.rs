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
    let el = app.mount("input-text").unwrap();
    app.set_attr(el, "label", "Note");
    app.set_attr(el, "value", "hello");
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
    let el = app.mount("input-text").unwrap();
    app.set_attr(el, "label", "Note");
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
    let el = app.mount("input-text").unwrap();
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    {
        let seen = seen.clone();
        app.on(el, "value-changed", move |notify| {
            seen.borrow_mut().push(notify.value.display_text());
        });
    }
    key(&mut app, KeyCode::Char('A'));
    key(&mut app, KeyCode::Tab);
    drop(out);
    assert_eq!(seen.borrow().as_slice(), ["A"]);
}

/// The state wire format, composed exactly like `TuiSession::set_prop_json`
/// and `on_notify`: JSON in through `value_from_json`, the notify value back
/// out through `value_to_json` as the canonical sorted-key snapshot.
#[test]
fn json_state_drives_the_screen_like_the_session() {
    let (mut app, _out) = app(72, 60);
    let el = app.mount("app-root").unwrap();
    // The probe records the canonical string; the session's wire format
    // (`value_to_json(..).to_string()`) agrees for object state because the
    // ObjectMap iterates sorted under either serde_json map flavor.
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    {
        let seen = seen.clone();
        app.on(el, "state-changed", move |notify| {
            seen.borrow_mut()
                .push(uic_core::json::canonical_json(&notify.value));
        });
    }

    let parsed: serde_json::Value =
        serde_json::from_str(r#"{"note":"hi","date":"2026-07-07"}"#).unwrap();
    app.set_prop(el, "state", uic_core::json::value_from_json(&parsed));
    app.draw().unwrap();

    let screen = app.terminal().backend().screen_text();
    assert!(screen.contains("2026-07-07"), "date member:\n{screen}");
    assert!(
        screen.contains("date: 2026-07-07 · note: hi"),
        "state line:\n{screen}"
    );
    assert_eq!(
        seen.borrow().as_slice(),
        [r#"{"date":"2026-07-07","note":"hi"}"#],
        "one notify carrying the canonical sorted-key snapshot"
    );
}

#[test]
fn roots_stack_and_tab_crosses_their_boundary() {
    let (mut app, _out) = app(50, 16);
    let first = app.mount("input-text").unwrap();
    app.set_attr(first, "label", "First");
    let second = app.mount("input-number").unwrap();
    app.set_attr(second, "label", "Second");
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

/// The option wire format of `TuiSession::set_options_json` — the deliberate
/// array escape hatch beside the state-shaped `set_prop_json` (ADR 0005):
/// JSON rows in through `options_from_json`, applied as the options property
/// of a bare select.
#[test]
fn json_option_rows_serve_a_bare_select() {
    let (mut app, _out) = app(60, 16);
    let el = app.mount("input-select").unwrap();
    app.set_attr(el, "label", "Zone");
    app.set_attr(el, "value", "Europe/Berlin");
    let options = uic_tui_web::options_from_json(
        r#"[{"value":"Europe/Berlin","short":"Berlin"},{"value":"Pacific/Auckland"}]"#,
    )
    .unwrap();
    app.set_prop(el, "options", options);
    app.draw().unwrap();

    let screen = app.terminal().backend().screen_text();
    assert!(screen.contains("Berlin"), "closed short label:\n{screen}");

    key(&mut app, KeyCode::F(4));
    app.draw().unwrap();
    let open = app.terminal().backend().screen_text();
    assert!(open.contains("Europe/Berlin"), "full label open:\n{open}");
    assert!(
        open.contains("Pacific/Auckland"),
        "the value serves as the missing label:\n{open}"
    );

    let bad = uic_tui_web::options_from_json(r#"[{"short":"nope"}]"#);
    assert!(bad.is_err(), "rows require a value");
}

#[test]
fn the_theme_attribute_flips_the_resolved_variables() {
    let (mut app, out) = app(60, 20);
    let el = app.mount("app-root").unwrap();
    app.draw().unwrap();
    let light = out.take();
    assert!(
        light.contains("48;2;255;255;255"),
        "the light card fill paints white: {light:?}"
    );
    assert!(
        !light.contains("48;2;33;37;41"),
        "no dark body background in light mode"
    );

    // The host attribute selects the dark variable block; source order
    // lets [data-bs-theme=dark] win over the light :root at equal
    // specificity — this pins the generated sheet's block order.
    app.set_dom_attr(0, "data-bs-theme", Some("dark"));
    app.draw().unwrap();
    let dark = out.take();
    assert!(
        dark.contains("48;2;33;37;41"),
        "the dark card fill paints the dark body background: {dark:?}"
    );
    assert!(
        !dark.contains("48;2;255;255;255"),
        "no white card fill in dark mode"
    );

    app.set_dom_attr(0, "data-bs-theme", None);
    app.draw().unwrap();
    let back = out.take();
    assert!(
        back.contains("48;2;255;255;255"),
        "removing the attribute returns to light"
    );
    let _ = el;
}

#[test]
fn a_resize_reflows_and_fully_repaints() {
    let (mut app, out) = app(44, 10);
    let el = app.mount("input-text").unwrap();
    app.set_attr(el, "label", "A reasonably long label line");
    app.draw().unwrap();
    let _ = out.take();

    app.terminal_mut().backend_mut().resize(70, 10);
    app.draw().unwrap();
    let wider = out.take();
    assert!(
        wider.contains("\x1b[2J"),
        "the resize clears before the repaint: {wider:?}"
    );
    assert!(
        wider.contains("A reasonably long label line"),
        "content repaints at the new width"
    );
    let screen = app.terminal().backend().screen_text();
    assert!(
        screen.lines().count() <= 10,
        "rows stay at the resized height:\n{screen}"
    );

    app.terminal_mut().backend_mut().resize(30, 10);
    app.draw().unwrap();
    let narrower = out.take();
    assert!(narrower.contains("\x1b[2J"));
    let screen = app.terminal().backend().screen_text();
    assert!(
        screen.lines().all(|line| line.chars().count() <= 30),
        "content reflows inside the narrow width:\n{screen}"
    );
}

#[test]
fn option_rows_seed_by_property_name() {
    let (mut app, out) = app(50, 12);
    let el = app.mount("input-suggestion").unwrap();
    app.set_attr(el, "label", "Word");
    // The rows land as Value::Options (ADR 0005), the type a plain JSON
    // array cannot express — the widget offers them once the popup opens.
    app.set_prop(
        el,
        "suggestions",
        vec![
            uic_core::SelectOption::new("apple"),
            uic_core::SelectOption::new("apricot"),
        ],
    );
    app.draw().unwrap();
    let _ = out.take();
    key(&mut app, KeyCode::F(4));
    app.draw().unwrap();
    let ansi = out.take();
    assert!(
        ansi.contains("apple") && ansi.contains("apricot"),
        "the popup offers the seeded rows: {ansi:?}"
    );
}
