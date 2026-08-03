//! LitElement semantics on the retained DOM: the same component definitions
//! render their state into a Document, data flows down through child
//! mounts, and notify events flow up through bindings and bubbling.

use std::cell::RefCell;
use std::rc::Rc;

use uic_core::{Ctx, CustomElement, UiEvent, Value};
use uic_dom::ListenerOptions;
use uic_tui::dom::DomHost;

fn host(tag: &str) -> DomHost {
    ui_components_tui::link();
    DomHost::mount(tag).expect("mounts")
}

#[test]
fn a_component_renders_its_state_into_the_dom() {
    let mut host = host("input-text");
    host.set_attr("label", "Note");
    host.set_attr("hint", "Free text");
    host.set_attr("placeholder", "free text");

    let html = host.outer_html();
    assert!(html.contains(">Note</label>"), "label committed:\n{html}");
    assert!(html.contains("Free text"), "hint branch rendered:\n{html}");
    assert!(
        html.contains("placeholder=\"free text\""),
        "attribute part committed onto the input:\n{html}"
    );
    assert!(
        html.starts_with("<input-text"),
        "the component is an element node:\n{html}"
    );
}

#[test]
fn conditional_chrome_follows_state() {
    let mut host = host("input-text");
    host.set_attr("hint", "Free text");
    assert!(host.outer_html().contains("Free text"));

    // An error message swaps the hint row for the error row, exactly the
    // chrome's two conditionals.
    host.set_attr("error-message", "Nope");
    let html = host.outer_html();
    assert!(html.contains(">Nope</div>"), "error row:\n{html}");
    assert!(!html.contains("Free text"), "hint row gone:\n{html}");

    host.set_attr("error-message", "");
    let html = host.outer_html();
    assert!(html.contains("Free text"), "hint row back:\n{html}");
}

#[test]
fn data_flows_down_into_child_components() {
    let mut host = host("input-date-range");
    // The interval decomposes in will_update, the ends flow into the two
    // input-date children as `.value` writes, and each child's own
    // will_update rejects the garbage, visible in the CHILD's DOM.
    host.set_attr("value", "bad/worse");

    let html = host.outer_html();
    assert!(
        html.contains("Invalid date: bad") && html.contains("Invalid date: worse"),
        "child lifecycles ran on the pushed values:\n{html}"
    );

    // Valid ends clear the children's error rows again.
    host.set_attr("value", "2026-07-07/2026-07-11");
    let html = host.outer_html();
    assert!(
        !html.contains("Invalid date"),
        "children recovered:\n{html}"
    );
}

#[test]
fn child_events_route_up_into_composite_handlers() {
    let mut host = host("input-date-range");
    host.set_attr("start", "2026-07-07");
    host.set_attr("end", "2026-07-15");
    let values = Rc::new(RefCell::new(Vec::new()));
    let sink = values.clone();
    host.on("value-changed", move |event| {
        sink.borrow_mut().push(event.value.display_text());
    });

    // A commit inside the START child (the widget stand-in): its notify
    // event routes through @value-changed into on_start_changed, the clamp
    // runs, and the END child's store follows.
    let start = host.find("input-date").expect("the start child");
    host.set_prop_at(start, "value", "2026-08-01");

    assert_eq!(
        values.borrow().last().map(String::as_str),
        Some("2026-08-01/2026-08-01"),
        "the composite clamped and committed the interval"
    );
}

#[test]
fn notify_events_bubble_through_the_document() {
    let mut host = host("input-date-range");
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = heard.clone();
    let root = host.doc().root();
    host.doc_mut().add_event_listener(
        root,
        "start-changed",
        ListenerOptions::default(),
        move |_doc, event| {
            sink.borrow_mut()
                .push((event.event_type().to_string(), event.detail.display_text()));
        },
    );

    host.set_attr("start", "2026-07-07");

    assert_eq!(
        heard.borrow().last(),
        Some(&("start-changed".to_string(), "2026-07-07".to_string())),
        "the notify event bubbled from the component node to the document"
    );
}

#[test]
fn boolean_parts_reach_children_as_absent_attributes() {
    let mut host = host("input-date-range");
    host.set_attr("start", "2026-07-07");

    // disabled=true flows into both children through the ?disabled binding.
    host.set_attr("disabled", "");
    let html = host.outer_html();
    assert!(
        html.matches("<input-date disabled=\"\"").count() == 2
            || html.matches("disabled=\"\"").count() >= 2,
        "the boolean part committed on both children:\n{html}"
    );

    // Clearing it removes the attribute; the child sees the REMOVAL as an
    // observed-attribute change back to false.
    host.set_prop("disabled", false);
    let html = host.outer_html();
    assert!(
        !html.contains("<input-date disabled"),
        "the boolean cleared on the children:\n{html}"
    );
}

/// A parent binding an optional member onto a child property: the fixture
/// of the property-part contract at the child boundary (ADR 0008).
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "null-flow",
    template = "<input-text .value=${text} @value-changed=${on_text}></input-text><p>${seen}</p>"
)]
struct NullFlow {
    /// The bound member: `None` resolves the hole to nothing.
    #[property]
    text: Option<String>,
    /// What the child reported back, with null spelled out.
    #[property(notify)]
    seen: String,
}

impl NullFlowLogic for NullFlow {
    fn on_text(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let seen = match event.detail.as_ref() {
            Some(Value::Null) => "null".to_string(),
            Some(value) => value.display_text(),
            None => "-".to_string(),
        };
        ctx.set("seen", seen);
    }
}

#[test]
fn a_nothing_hole_still_writes_null_into_the_child() {
    let mut host = host("null-flow");
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    host.on("seen-changed", move |event| {
        sink.borrow_mut().push(event.value.display_text())
    });

    // A real value flows down; the child commits and reports it back.
    host.set_prop("text", "draft");
    assert_eq!(seen.borrow().as_slice(), ["draft"]);

    // The member goes null: the hole resolves to Nothing, and Nothing still
    // WRITES: the child receives `value = null` (the browser's
    // `el.prop = null`) and notifies. Skipping the write here would strand
    // the child on the stale value.
    host.set_prop("text", Value::Null);
    assert_eq!(seen.borrow().as_slice(), ["draft", "null"]);

    // An unchanged hole produces no write at all (NoChange).
    host.set_prop("text", Value::Null);
    assert_eq!(seen.borrow().len(), 2, "no write for an unchanged hole");
}

#[test]
fn the_range_timezone_feeds_the_children_as_their_default() {
    let mut host = host("input-date-range");
    // Without a zone the children carry no default-timezone attribute.
    assert!(
        !host.outer_html().contains("default-timezone"),
        "no zone, no attribute:\n{}",
        host.outer_html()
    );

    // The range's default reaches both children as their default zone.
    host.set_attr("default-timezone", "Europe/Berlin");
    let html = host.outer_html();
    assert_eq!(
        html.matches("default-timezone=\"Europe/Berlin\"").count(),
        2,
        "both children follow the range default:\n{html}"
    );

    // A picked timezone wins over the default and flips the attribute.
    host.set_prop("timezone", "America/New_York");
    let html = host.outer_html();
    assert_eq!(
        html.matches("default-timezone=\"America/New_York\"")
            .count(),
        2,
        "the picked zone replaced the default on both children:\n{html}"
    );
}
