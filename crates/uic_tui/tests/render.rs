//! TestBackend tests: the same <input-date> definition the browser runs,
//! rendered into an in-memory terminal and driven by key events.

mod support;

use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::backend::TestBackend;
use uic_core::Value;
use uic_tui::{App, Control};

use support::{corner_colors, key, probe, screen, type_str};

fn app() -> App<TestBackend> {
    support::app(50, 10)
}

/// Dispatches without painting between events: the whole edit lands in one
/// unpainted batch and the first frame lays the grown box out around all of
/// it. With frames between the keys the growing textarea scrolls its first
/// line out of view instead — that grow/scroll interaction is a widget
/// follow-up, and this flavor pins the batch semantics meanwhile.
fn batch_key(app: &mut App<TestBackend>, code: KeyCode) {
    app.handle_event(&Event::Key(KeyEvent::from(code)));
}

fn batch_str(app: &mut App<TestBackend>, text: &str) {
    for ch in text.chars() {
        batch_key(app, KeyCode::Char(ch));
    }
}

#[test]
fn renders_label_value_and_hint() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "label", "Date of purchase");
    app.set_attr(el, "hint", "Format: YYYY-MM-DD");
    app.set_attr(el, "value", "2026-07-07");

    let screen = screen(&mut app);
    assert!(screen.contains("Date of purchase"), "label row:\n{screen}");
    assert!(screen.contains("2026-07-07"), "value in widget:\n{screen}");
    assert!(screen.contains("Format: YYYY-MM-DD"), "hint row:\n{screen}");
}

#[test]
fn commit_updates_value_and_notifies() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "hide-time", "");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "hide-time", "");
    app.set_attr(el, "hint", "Format: YYYY-MM-DD");
    let events = probe(&mut app, el, "value-changed");

    // Years live in the catalog's 1900-2099 window; outside it nothing
    // parses (in-window overflow clamps instead, see the clamp test).
    type_str(&mut app, "2150-01-01");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("Invalid date: 2150-01-01"),
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
fn overflow_input_clamps_like_the_catalog() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "hide-time", "");
    let events = probe(&mut app, el, "value-changed");

    // Temporal constrain: month 13 and day 99 clamp into range.
    type_str(&mut app, "2026-13-99");
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Str("2026-12-31".into()));
}

#[test]
fn partial_input_autocompletes_through_the_mask() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    let events = probe(&mut app, el, "value-changed");

    // The datetime variant is the default; a bare year commits the period
    // start, the catalog's auto-completion.
    type_str(&mut app, "2024");
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Str("2024-01-01 00:00:00".into()));
}

#[test]
fn min_bound_is_enforced() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "min", "2020-01-01");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "disabled", "");
    let events = probe(&mut app, el, "value-changed");

    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    assert!(events.borrow().is_empty(), "disabled input commits nothing");
}

#[test]
fn text_input_renders_chrome_and_commits_trimmed() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    app.set_attr(el, "label", "Note");
    app.set_attr(el, "hint", "Free text");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "allow-null", "");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "value", "prefilled");
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
    support::app(60, 20)
}

#[test]
fn f4_opens_the_calendar_over_the_content_below() {
    let mut app = tall_app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "value", "2026-07-07");
    app.set_attr(el, "hint", "Format: YYYY-MM-DD");

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
    app.set_attr(el, "hide-time", "");
    app.set_attr(el, "value", "2026-07-07");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "hide-time", "");
    app.set_attr(el, "value", "2026-07-01");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "hide-time", "");
    app.set_attr(el, "value", "2026-07-07");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "value", "2026-07-07");

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
    app.set_attr(el, "value", "2026-07-07");
    app.set_attr(el, "min", "2026-07-05");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "disabled", "");

    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    let after = screen(&mut app);
    assert!(!after.contains("Mo Tu We"), "no calendar:\n{after}");
}

#[test]
fn date_commit_notifies_the_zoned_date_in_the_current_timezone() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "default-timezone", "Europe/Berlin");
    let dates = probe(&mut app, el, "date-changed");

    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let dates = dates.borrow();
    assert_eq!(dates.len(), 1, "one date-changed event");
    let zoned = dates[0].value.as_zoned().expect("zoned detail");
    // The .date detail is the UTC instant of the typed Berlin wall clock.
    assert_eq!(zoned.iso(), "2026-07-31T22:00:00+00:00[UTC]");
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
    app.set_attr(el, "label", "Amount");
    let events = probe(&mut app, el, "value-changed");

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
    let el = app.mount("input-number").expect("mount");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "allow-null", "");
    let events = probe(&mut app, el, "value-changed");

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
    app.set_attr(el, "label", "Comment");
    let events = probe(&mut app, el, "value-changed");

    batch_str(&mut app, "line one");
    batch_key(&mut app, KeyCode::Enter); // newline, not a commit
    batch_str(&mut app, "line two");
    assert!(events.borrow().is_empty(), "Enter did not commit");

    let screen = screen(&mut app);
    assert!(screen.contains("line one"), "first line:\n{screen}");
    assert!(screen.contains("line two"), "second line:\n{screen}");
    let first_row = screen.lines().position(|l| l.contains("line one"));
    let second_row = screen.lines().position(|l| l.contains("line two"));
    assert!(first_row < second_row, "lines stack vertically:\n{screen}");

    batch_key(&mut app, KeyCode::Tab); // focus leave commits
    let events = events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, Value::Str("line one\nline two".into()));
}

#[test]
fn the_focus_ring_and_caret_follow_the_focused_root() {
    let mut app = app();
    let first = app.mount("input-text").expect("mount");
    app.set_attr(first, "label", "First");
    let second = app.mount("input-number").expect("mount");
    app.set_attr(second, "label", "Second");

    let focus_ring = ratatui::style::Color::LightBlue;

    // The browser's focus outline in cells: the focused element's group is
    // ringed, the idle one stays dark gray, and Tab moves the ring.
    assert_eq!(
        corner_colors(&mut app),
        [focus_ring, ratatui::style::Color::DarkGray]
    );
    key(&mut app, KeyCode::Tab);
    assert_eq!(
        corner_colors(&mut app),
        [ratatui::style::Color::DarkGray, focus_ring]
    );
}

#[test]
fn the_error_state_outlines_the_group_in_danger_red() {
    let mut app = app();
    let el = app.mount("input-number").expect("mount");
    app.set_attr(el, "label", "Amount");

    // An invalid commit wears the browser's [error] border; a valid one
    // clears it back to the focus ring.
    type_str(&mut app, "abc");
    key(&mut app, KeyCode::Tab);
    assert_eq!(corner_colors(&mut app)[0], ratatui::style::Color::Red);
    clear_input(&mut app);
    type_str(&mut app, "2,50");
    key(&mut app, KeyCode::Tab);
    assert_eq!(corner_colors(&mut app)[0], ratatui::style::Color::LightBlue);
}

#[test]
fn placeholders_show_under_empty_widgets() {
    let mut app = app();
    let el = app.mount("input-text").expect("mount");
    app.set_attr(el, "label", "Note");
    app.set_attr(el, "placeholder", "free text");

    assert!(
        screen(&mut app).contains("free text"),
        "the empty input shows its placeholder:\n{}",
        screen(&mut app)
    );
    type_str(&mut app, "x");
    assert!(
        !screen(&mut app).contains("free text"),
        "typing hides the placeholder:\n{}",
        screen(&mut app)
    );
}

#[test]
fn the_empty_date_shows_its_placeholder_over_the_mask() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "label", "Date");

    let screen = screen(&mut app);
    assert!(
        screen.contains("YYYY-MM-DD"),
        "the computed placeholder covers the pristine mask:\n{screen}"
    );
    assert!(!screen.contains("0000"), "the mask stays hidden:\n{screen}");
}

#[test]
fn the_number_rests_right_aligned_beside_its_unit() {
    let mut app = app();
    let el = app.mount("input-number").expect("mount");
    app.set_attr(el, "label", "Amount");
    app.set_attr(el, "unit", "€");
    app.set_attr(el, "value", "1234.5");

    // At rest (blurred, like an unfocused browser input) the value sits
    // beside its unit at the right edge.
    app.blur();
    let at_rest = screen(&mut app);
    let row = at_rest
        .lines()
        .find(|l| l.contains("1234,50"))
        .expect("value row");
    // One padding cell keeps the unit off the border, and the affix's own
    // padding (the browser's input-group-text) keeps it off the value.
    assert!(
        row.trim_end().ends_with("1234,50 € │"),
        "the value sits one cell from its unit, one cell from the edge: {row:?}"
    );

    // Editing moves the text to the left, where the caret lives — one
    // padding cell in from the border.
    key(&mut app, KeyCode::Char('9'));
    let editing = screen(&mut app);
    let row = editing
        .lines()
        .find(|l| l.contains("234,50"))
        .expect("value row");
    assert!(
        row.starts_with("│ ") && !row.starts_with("│  "),
        "editing is left-aligned inside the padding: {row:?}"
    );
}

#[test]
fn the_textarea_starts_at_one_line_and_grows() {
    let mut app = app();
    let el = app.mount("input-textarea").expect("mount");
    app.set_attr(el, "label", "Comment");

    let box_rows = |screen: &str| {
        let top = screen.lines().position(|l| l.contains('┌')).unwrap();
        let bottom = screen.lines().position(|l| l.contains('└')).unwrap();
        bottom - top - 1
    };
    assert_eq!(
        box_rows(&screen(&mut app)),
        1,
        "one content line, like the browser's initial height"
    );
    type_str(&mut app, "one");
    key(&mut app, KeyCode::Enter);
    type_str(&mut app, "two");
    key(&mut app, KeyCode::Enter);
    type_str(&mut app, "three");
    assert_eq!(box_rows(&screen(&mut app)), 3, "the box grew with content");
}

#[test]
fn shift_tab_walks_the_focus_backward_across_roots() {
    let mut app = app();
    let first = app.mount("input-text").expect("mount");
    app.set_attr(first, "label", "First");
    let second = app.mount("input-number").expect("mount");
    app.set_attr(second, "label", "Second");

    let focus_ring = ratatui::style::Color::LightBlue;
    let idle = ratatui::style::Color::DarkGray;

    // Shift+Tab from the first root wraps backward to the last one, the
    // reverse of Tab crossing element boundaries in a document.
    assert_eq!(corner_colors(&mut app), [focus_ring, idle]);
    key(&mut app, KeyCode::BackTab);
    assert_eq!(corner_colors(&mut app), [idle, focus_ring]);
    // And it exactly reverses a Tab.
    key(&mut app, KeyCode::BackTab);
    assert_eq!(corner_colors(&mut app), [focus_ring, idle]);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::BackTab);
    assert_eq!(corner_colors(&mut app), [focus_ring, idle]);
}

#[test]
fn the_embedded_zone_select_hugs_its_label_and_the_date_grows() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "value", "2026-07-07");
    app.set_attr(el, "show-timezone", "");
    app.set_attr(el, "default-timezone", "Europe/Berlin");

    // The select sizes to its closed label plus rat's marker cells — the
    // catalog's fit-content — instead of a fixed twelve-cell box that cut
    // the zone short; the date input flex-grows through the rest of the row.
    let screen = screen(&mut app);
    let value_row = screen
        .lines()
        .find(|row| row.contains("2026-07-07"))
        .expect("the value row");
    assert!(
        value_row.contains("Europe/Berlin"),
        "the full zone shows beside the date:\n{screen}"
    );
}

#[test]
fn long_hints_wrap_and_push_the_following_flow() {
    let mut app = support::app(40, 14);
    let first = app.mount("input-text").expect("mount");
    app.set_attr(first, "label", "Note");
    app.set_attr(
        first,
        "hint",
        "Trimmed on commit; the empty input becomes null once allow-null is set",
    );
    let second = app.mount("input-text").expect("mount");
    app.set_attr(second, "label", "Next");

    let screen = screen(&mut app);
    let start = screen
        .lines()
        .position(|line| line.contains("Trimmed on commit;"))
        .expect("hint first row");
    assert!(
        screen
            .lines()
            .nth(start + 1)
            .is_some_and(|line| line.contains("null")),
        "the hint wrapped onto a second row:\n{screen}"
    );
    let next = screen
        .lines()
        .position(|line| line.contains("Next"))
        .expect("second root's label");
    assert!(
        next > start + 1,
        "the wrapped hint pushed the flow below it:\n{screen}"
    );

    // The error replaces the hint in place and flows the same way.
    app.set_attr(
        first,
        "error-message",
        "The committed value does not satisfy the imaginary constraint of this test",
    );
    let swapped = support::screen(&mut app);
    assert!(swapped.contains("imaginary"), "error row:\n{swapped}");
    assert!(
        !swapped.contains("Trimmed on commit"),
        "hint swapped out in place:\n{swapped}"
    );
}
