//! TestBackend tests for nested custom elements: a parent component hosting
//! `<input-date>` and `<input-text>` children through template bindings.

mod support;

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use uic_core::{Ctx, CustomElement, PropertyStore, UiEvent, Value};
use uic_tui::App;

use support::{key, probe, screen, type_str};

/// A composite element: bound date child, free-standing text child, computed
/// summary line.
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "demo-form",
    template = "<div>\
                <input-date label=\"Date\" .value=${date_value} ?disabled=${lock_date} \
                @value-changed=${on_date}></input-date>\
                <input-text label=\"Note\" @value-changed=${on_note}></input-text>\
                <p>${summary}</p>\
                </div>"
)]
struct DemoForm {
    /// Mirrors the nested date input's committed value.
    #[property(notify)]
    date_value: String,
    #[property]
    note: Option<String>,
    #[property(reflect)]
    lock_date: bool,
}

impl DemoFormLogic for DemoForm {
    fn summary(&self, store: &PropertyStore) -> Value {
        let date = store.get("date_value").display_text();
        let note = store.get("note").display_text();
        format!("summary: [{date}] [{note}]").into()
    }

    fn on_date(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("date_value", event.target_value.clone().unwrap_or_default());
    }

    fn on_note(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("note", event.target_value.clone().unwrap_or_default());
    }
}

fn app() -> App<TestBackend> {
    support::app(60, 24)
}

#[test]
fn children_render_with_their_chrome_and_bound_state() {
    let mut app = app();
    let el = app.mount("demo-form").expect("mount");
    app.set_attr(el, "date-value", "2026-07-07");

    let screen = screen(&mut app);
    assert!(screen.contains("Date"), "child date label:\n{screen}");
    assert!(screen.contains("Note"), "child text label:\n{screen}");
    assert!(
        screen.contains("2026-07-07"),
        "bound value synced into the child widget:\n{screen}"
    );
    assert!(
        screen.contains("summary: [2026-07-07] []"),
        "computed summary from the parent state:\n{screen}"
    );
}

#[test]
fn child_commit_routes_to_the_parent_handler() {
    let mut app = app();
    let el = app.mount("demo-form").expect("mount");
    let events = probe(&mut app, el, "date-value-changed");

    // Focus starts on the nested date widget (document order).
    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("summary: [2026-08-01] []"),
        "parent state updated by the routed child event:\n{screen}"
    );
    let events = events.borrow();
    assert_eq!(events.len(), 1, "single parent notify (no write-back echo)");
    assert_eq!(events[0].value, Value::Str("2026-08-01".into()));
}

#[test]
fn tab_traverses_into_the_next_child() {
    let mut app = app();
    app.mount("demo-form").expect("mount");

    key(&mut app, KeyCode::Tab); // date → text
    type_str(&mut app, "  a note  ");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("summary: [] [a note]"),
        "text child committed (trimmed) into the parent:\n{screen}"
    );
}

#[test]
fn disabled_child_is_skipped_by_focus_and_input() {
    let mut app = app();
    let el = app.mount("demo-form").expect("mount");
    app.set_attr(el, "lock-date", "");

    // The date widget is disabled through the bool binding: typing has no
    // effect, and focus rests on the text child instead.
    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);
    let screen_before = screen(&mut app);
    assert!(
        screen_before.contains("summary: [] [2026-08-01]"),
        "the typing landed in the text child, not the locked date:\n{screen_before}"
    );
}

#[test]
fn parent_writes_sync_down_without_echo_loops() {
    let mut app = app();
    let el = app.mount("demo-form").expect("mount");
    let events = probe(&mut app, el, "date-value-changed");
    app.set_attr(el, "date-value", "2026-09-09");

    let screen = screen(&mut app);
    assert!(
        screen.contains("2026-09-09"),
        "child widget follows the parent write:\n{screen}"
    );
    assert_eq!(events.borrow().len(), 1, "one event for the external write");
}

#[test]
fn nested_date_slot_opens_its_calendar() {
    let mut app = app();
    let el = app.mount("demo-form").expect("mount");
    app.set_attr(el, "date-value", "2026-07-07");

    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    let after = screen(&mut app);
    assert!(
        after.contains("July 2026"),
        "calendar anchored at the nested widget:\n{after}"
    );
}

#[test]
fn show_timezone_embeds_the_select_inside_the_date() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "show-timezone", "");
    app.set_attr(el, "default-timezone", "Europe/Berlin");
    let events = probe(&mut app, el, "timezone-changed");

    // The embedded select renders next to the date input inside ONE border
    // block (seamless suppresses the child's own), showing the default row's
    // label while no timezone is picked. The placeholder suffix is browser
    // behavior (the terminal date widget renders its mask instead).
    let closed = screen(&mut app);
    assert!(closed.contains("▼"), "embedded select marker:\n{closed}");
    assert!(
        closed.contains("Europe/Berlin"),
        "the closed line fits the whole default label:\n{closed}"
    );
    assert_eq!(
        closed.matches('┌').count(),
        1,
        "one border block, the child is seamless:\n{closed}"
    );

    // Tab reaches the embedded select; its popup routes through the child
    // path (rows clip to the narrow anchor); Enter commits and the event
    // routes up as timezone-changed.
    key(&mut app, KeyCode::Tab);
    screen(&mut app);
    key(&mut app, KeyCode::F(4));
    let open = screen(&mut app);
    assert!(
        open.contains("UTC") && open.contains("Africa/Abi"),
        "zone list through the nested path:\n{open}"
    );
    key(&mut app, KeyCode::Home);
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);

    let events = events.borrow();
    assert_eq!(events.len(), 1, "routed timezone-changed");
    assert_eq!(events[0].value, Value::Str("UTC".into()));
}

#[test]
fn hidden_timezone_branch_keeps_the_focus_stable() {
    let mut app = app();
    let el = app.mount("input-date").expect("mount");
    app.set_attr(el, "value", "2026-07-07");
    let events = probe(&mut app, el, "timezone-changed");

    // With the branch off, the select's nodes do not exist and focus stays
    // on the date widget: Tab wraps back to it, F4 opens the calendar.
    let closed = screen(&mut app);
    assert!(!closed.contains("▼"), "no select rendered:\n{closed}");
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::F(4));
    let after = screen(&mut app);
    assert!(
        after.contains("July 2026"),
        "calendar still owns the focused widget:\n{after}"
    );
    assert!(events.borrow().is_empty());
}

#[test]
fn unrendered_branches_are_unfocusable_by_construction() {
    let mut app = app();
    let date = app.mount("input-date").expect("mount");
    app.set_attr(date, "value", "2026-07-07");
    let note = app.mount("input-text").expect("mount");
    let events = probe(&mut app, note, "value-changed");

    // Without show-timezone the date owns ONE focusable: a single Tab must
    // land on the text root — the unrendered branch's widget node does not
    // exist, so no guard is needed to skip it.
    screen(&mut app);
    key(&mut app, KeyCode::Tab);
    type_str(&mut app, "note");
    key(&mut app, KeyCode::Enter);
    assert_eq!(
        events.borrow().last().map(|ev| ev.value.display_text()),
        Some("note".to_string()),
        "one Tab reached the text root"
    );
}

#[test]
fn shift_tab_returns_into_the_previous_child() {
    let mut app = app();
    app.mount("demo-form").expect("mount");

    // Tab reaches the text child; Shift+Tab returns to the date child, so
    // the typed date lands in the mask again.
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::BackTab);
    type_str(&mut app, "2026-08-01");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("summary: [2026-08-01] []"),
        "the date child took the input after Shift+Tab:\n{screen}"
    );
}

#[test]
fn shift_tab_skips_disabled_widgets() {
    let mut app = app();
    let el = app.mount("demo-form").expect("mount");
    app.set_attr(el, "lock-date", "");

    // Focus sits on the text child (the only enabled one); the backward
    // wrap skips the locked date and lands on the text child again.
    key(&mut app, KeyCode::BackTab);
    type_str(&mut app, "note");
    key(&mut app, KeyCode::Enter);

    let screen = screen(&mut app);
    assert!(
        screen.contains("summary: [] [note]"),
        "focus stayed on the enabled child:\n{screen}"
    );
}
