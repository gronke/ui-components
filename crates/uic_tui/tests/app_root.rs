//! TestBackend tests for `<app-root>`: the state object trickling down into
//! the form children, child commits folding back into `state`, and the
//! echo-free `state-changed` contract (ADR 0013).

mod support;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use uic_core::{ObjectMap, Value};
use uic_tui::App;

use support::{click, key, locate, probe, screen, type_str};

/// Tall enough for the whole carded form plus the select popup.
fn app() -> App<TestBackend> {
    support::app(72, 60)
}

fn state(entries: &[(&str, Value)]) -> ObjectMap {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

/// The text inside the card border, trimmed; row probes stay readable
/// although every form row now starts and ends with the card's `│`.
fn inner(line: &str) -> &str {
    line.trim()
        .trim_start_matches('│')
        .trim_end_matches('│')
        .trim()
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
    let events = probe(&mut app, el, "state-changed");

    // Focus starts on the tab bar (document order); one Tab reaches the
    // date widget.
    key(&mut app, KeyCode::Tab);
    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "one state-changed per commit, no echo");
    let state = events[0].value.as_object().expect("object state");
    assert_eq!(
        state.get("date"),
        Some(&Value::Str("2026-08-01 00:00:00".into())),
        "the committed datetime joined the state"
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
    let events = probe(&mut app, el, "state-changed");

    // Document order: the tab bar, date, its embedded timezone select,
    // range start and end, note, amount; the seventh Tab reaches the pick
    // select.
    for _ in 0..7 {
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
    let events = probe(&mut app, el, "state-changed");

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
    let events = probe(&mut app, el, "state-changed");

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
    let events = probe(&mut app, el, "state-changed");

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

#[test]
fn the_word_pool_answers_typing_through_the_suggestion_child() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    let events = probe(&mut app, el, "state-changed");

    // Click into the word input (its placeholder locates it) and type a
    // prefix: the in-component pool answers within the same cycle, so the
    // popup fills without any host round-trip (ADR 0014).
    let (x, y) = locate(&mut app, "start typing");
    click(&mut app, x, y);
    type_str(&mut app, "ap");
    let open = screen(&mut app);
    assert!(open.contains("apple"), "pool rows in the popup:\n{open}");
    assert!(open.contains("apricot"), "both matches:\n{open}");

    // Down to apple, Down to apricot, Enter picks and commits into state.
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    let state = events
        .last()
        .expect("the pick commits")
        .value
        .as_object()
        .expect("object state");
    assert_eq!(state.get("word"), Some(&Value::Str("apricot".into())));
}

#[test]
fn bootstrap_margins_separate_the_stacked_controls() {
    let mut app = app();
    app.mount("app-root").expect("mount");

    // The mb-4 on each control pushes the next one down, like the browser:
    // a blank margin row stands between the date's hint and the range's
    // label.
    let screen = screen(&mut app);
    let hint = screen
        .lines()
        .position(|line| line.contains("Partials complete"))
        .expect("date hint row");
    let next = screen
        .lines()
        .position(|line| inner(line).starts_with("Stay"))
        .expect("range label row");
    assert!(
        next >= hint + 2,
        "a margin row separates the controls:\n{screen}"
    );
    assert_eq!(
        screen.lines().nth(hint + 1).map(inner),
        Some(""),
        "the margin row is blank:\n{screen}"
    );

    // The textarea carries the same class as its siblings: the timezone
    // label keeps its distance from the textarea's hint.
    let hint = screen
        .lines()
        .position(|line| line.contains("Grows with its content"))
        .expect("textarea hint row");
    let next = screen
        .lines()
        .position(|line| inner(line).starts_with("Default time zone"))
        .expect("timezone label row");
    assert!(
        next >= hint + 2,
        "a margin row separates the textarea from the timezone:\n{screen}"
    );
    assert_eq!(
        screen.lines().nth(hint + 1).map(inner),
        Some(""),
        "the margin row is blank:\n{screen}"
    );
}

#[test]
fn tabs_switch_the_pane_and_land_in_state() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");
    app.set_prop(el, "state", state(&[("note", "hello".into())]));
    let events = probe(&mut app, el, "state-changed");

    // The About pane replaces the form; the pick joins the state beside
    // the untouched members.
    let (x, y) = locate(&mut app, "About");
    click(&mut app, x, y);
    let about = screen(&mut app);
    assert!(
        !about.contains("Date of purchase"),
        "the form pane tore down:\n{about}"
    );
    assert!(
        about.contains("defined once in Rust"),
        "the about prose renders:\n{about}"
    );
    assert!(
        about.contains("note: hello · tab: about"),
        "the pick joined the state line:\n{about}"
    );
    {
        let events = events.borrow();
        let state = events.last().expect("the pick").value.as_object().unwrap();
        assert_eq!(state.get("tab"), Some(&Value::Str("about".into())));
        assert_eq!(state.get("note"), Some(&Value::Str("hello".into())));
    }

    // Back to the form: the branch re-mounts and the members re-sync.
    let (x, y) = locate(&mut app, "Form");
    click(&mut app, x, y);
    let form = screen(&mut app);
    assert!(
        form.contains("Date of purchase"),
        "the form pane returned:\n{form}"
    );
    assert!(
        form.contains("hello"),
        "the note member survived the round trip:\n{form}"
    );
}

#[test]
fn arrow_keys_switch_the_tab_from_the_bar() {
    let mut app = app();
    app.mount("app-root").expect("mount");

    // Focus starts on the tab bar (document order).
    key(&mut app, KeyCode::Right);
    let about = screen(&mut app);
    assert!(
        about.contains("defined once in Rust"),
        "Right flips to the about pane:\n{about}"
    );

    key(&mut app, KeyCode::Left);
    let form = screen(&mut app);
    assert!(
        form.contains("Date of purchase"),
        "Left returns to the form:\n{form}"
    );
}

#[test]
fn external_tab_state_flips_the_pane() {
    let mut app = app();
    let el = app.mount("app-root").expect("mount");

    app.set_prop(el, "state", state(&[("tab", "about".into())]));
    let about = screen(&mut app);
    assert!(
        about.contains("defined once in Rust"),
        "the external tab member selects the pane:\n{about}"
    );
    assert!(
        !about.contains("Date of purchase"),
        "the form is gone:\n{about}"
    );

    app.set_prop(el, "state", state(&[("tab", "form".into())]));
    let form = screen(&mut app);
    assert!(
        form.contains("Date of purchase"),
        "the external write brings the form back:\n{form}"
    );
}

#[test]
fn the_card_frames_the_panes_with_a_static_border() {
    let mut app = app();
    app.mount("app-root").expect("mount");

    let frame = screen(&mut app);
    let lines: Vec<&str> = frame.lines().collect();
    assert!(
        lines[0].starts_with('┌') && lines[0].ends_with('┐'),
        "the card's top border spans the first row:\n{frame}"
    );
    assert!(
        inner(lines[1]).starts_with("Form") && lines[1].contains("About"),
        "the tab bar sits in the card header:\n{frame}"
    );
    let bottom = lines
        .iter()
        .position(|line| line.starts_with('└'))
        .expect("card bottom border");
    let state_line = lines
        .iter()
        .position(|line| line.contains("state ·"))
        .expect("state line");
    assert!(
        state_line > bottom,
        "the state line stays outside the card:\n{frame}"
    );

    // The boot state stays empty: no member sneaks in through the bar's
    // mount-time sync (the value-changed echo brake).
    assert!(
        !frame.contains("tab:"),
        "no boot write into the state:\n{frame}"
    );
}
