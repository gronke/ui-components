//! Event dispatch follows the whatwg subset: three phases, two stop flags,
//! cancelation rules, and listener options.

use std::cell::RefCell;
use std::rc::Rc;

use uic_dom::{html, Document, Event, EventPhase, ListenerOptions, NodeId};

fn tree() -> (Document, NodeId, NodeId, NodeId) {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let form = doc.create_element(html::Form);
    let group = doc.create_element(html::Div);
    let input = doc.create_element(html::Input);
    doc.append_child(root, form);
    doc.append_child(form, group);
    doc.append_child(group, input);
    (doc, form, group, input)
}

type Listener = Box<dyn FnMut(&mut Document, &mut Event)>;

fn recorder() -> (Rc<RefCell<Vec<String>>>, impl Fn(&str) -> Listener) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let make = {
        let log = Rc::clone(&log);
        move |name: &str| -> Listener {
            let log = Rc::clone(&log);
            let name = name.to_string();
            Box::new(move |_doc, event| {
                log.borrow_mut().push(format!("{name}:{:?}", event.phase()));
            })
        }
    };
    (log, make)
}

#[test]
fn dispatch_captures_targets_then_bubbles() {
    let (mut doc, form, group, input) = tree();
    let (log, listener) = recorder();
    let capture = ListenerOptions {
        capture: true,
        ..Default::default()
    };
    doc.add_event_listener(form, "change", capture, listener("form-capture"));
    doc.add_event_listener(
        form,
        "change",
        ListenerOptions::default(),
        listener("form-bubble"),
    );
    doc.add_event_listener(
        group,
        "change",
        ListenerOptions::default(),
        listener("group-bubble"),
    );
    // Target listeners run in registration order regardless of the capture flag.
    doc.add_event_listener(
        input,
        "change",
        ListenerOptions::default(),
        listener("target-plain"),
    );
    doc.add_event_listener(input, "change", capture, listener("target-capture"));

    let not_canceled = doc.dispatch_event(input, &mut Event::change());

    assert!(not_canceled);
    assert_eq!(
        *log.borrow(),
        [
            "form-capture:Capturing",
            "target-plain:AtTarget",
            "target-capture:AtTarget",
            "group-bubble:Bubbling",
            "form-bubble:Bubbling",
        ],
    );
}

#[test]
fn the_event_reports_target_and_current_target() {
    let (mut doc, form, _group, input) = tree();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    doc.add_event_listener(
        form,
        "change",
        ListenerOptions::default(),
        move |_doc, event| {
            sink.borrow_mut()
                .push((event.target(), event.current_target()));
        },
    );

    doc.dispatch_event(input, &mut Event::change());

    assert_eq!(*seen.borrow(), [(Some(input), Some(form))]);
}

#[test]
fn non_bubbling_events_still_capture_but_do_not_bubble() {
    let (mut doc, form, _group, input) = tree();
    let (log, listener) = recorder();
    let capture = ListenerOptions {
        capture: true,
        ..Default::default()
    };
    doc.add_event_listener(form, "blur", capture, listener("form-capture"));
    doc.add_event_listener(
        form,
        "blur",
        ListenerOptions::default(),
        listener("form-bubble"),
    );
    doc.add_event_listener(
        input,
        "blur",
        ListenerOptions::default(),
        listener("target"),
    );

    doc.dispatch_event(input, &mut Event::blur());

    assert_eq!(*log.borrow(), ["form-capture:Capturing", "target:AtTarget"]);
}

#[test]
fn stop_propagation_spares_peers_but_halts_later_nodes() {
    let (mut doc, form, _group, input) = tree();
    let (log, listener) = recorder();
    doc.add_event_listener(
        input,
        "change",
        ListenerOptions::default(),
        |_doc, event| event.stop_propagation(),
    );
    doc.add_event_listener(
        input,
        "change",
        ListenerOptions::default(),
        listener("peer"),
    );
    doc.add_event_listener(
        form,
        "change",
        ListenerOptions::default(),
        listener("ancestor"),
    );

    doc.dispatch_event(input, &mut Event::change());

    assert_eq!(*log.borrow(), ["peer:AtTarget"]);
}

#[test]
fn stop_immediate_propagation_halts_peers_too() {
    let (mut doc, form, _group, input) = tree();
    let (log, listener) = recorder();
    doc.add_event_listener(
        input,
        "change",
        ListenerOptions::default(),
        |_doc, event| event.stop_immediate_propagation(),
    );
    doc.add_event_listener(
        input,
        "change",
        ListenerOptions::default(),
        listener("peer"),
    );
    doc.add_event_listener(
        form,
        "change",
        ListenerOptions::default(),
        listener("ancestor"),
    );

    doc.dispatch_event(input, &mut Event::change());

    assert!(log.borrow().is_empty());
}

#[test]
fn a_capture_listener_can_stop_the_descent() {
    let (mut doc, form, group, input) = tree();
    let (log, listener) = recorder();
    let capture = ListenerOptions {
        capture: true,
        ..Default::default()
    };
    doc.add_event_listener(form, "change", capture, |_doc, event: &mut Event| {
        event.stop_propagation();
    });
    doc.add_event_listener(group, "change", capture, listener("group-capture"));
    doc.add_event_listener(
        input,
        "change",
        ListenerOptions::default(),
        listener("target"),
    );

    doc.dispatch_event(input, &mut Event::change());

    assert!(log.borrow().is_empty());
}

#[test]
fn prevent_default_requires_a_cancelable_event() {
    let (mut doc, _form, _group, input) = tree();
    doc.add_event_listener(
        input,
        "change",
        ListenerOptions::default(),
        |_doc, event| event.prevent_default(),
    );
    doc.add_event_listener(
        input,
        "submit",
        ListenerOptions::default(),
        |_doc, event| event.prevent_default(),
    );

    // change is not cancelable; dispatch reports "not canceled".
    assert!(doc.dispatch_event(input, &mut Event::change()));
    // submit is the cancelable one of the native three.
    let mut submit = Event::submit();
    assert!(!doc.dispatch_event(input, &mut submit));
    assert!(submit.default_prevented());
}

#[test]
fn passive_listeners_cannot_cancel() {
    let (mut doc, _form, _group, input) = tree();
    let passive = ListenerOptions {
        passive: true,
        ..Default::default()
    };
    doc.add_event_listener(input, "submit", passive, |_doc, event| {
        event.prevent_default();
    });

    assert!(doc.dispatch_event(input, &mut Event::submit()));
}

#[test]
fn once_listeners_fire_once() {
    let (mut doc, _form, _group, input) = tree();
    let (log, listener) = recorder();
    let once = ListenerOptions {
        once: true,
        ..Default::default()
    };
    doc.add_event_listener(input, "input", once, listener("once"));

    doc.dispatch_event(input, &mut Event::input());
    doc.dispatch_event(input, &mut Event::input());

    assert_eq!(*log.borrow(), ["once:AtTarget"]);
}

#[test]
fn removed_listeners_do_not_fire() {
    let (mut doc, _form, _group, input) = tree();
    let (log, listener) = recorder();
    let id = doc.add_event_listener(input, "input", ListenerOptions::default(), listener("gone"));
    doc.remove_event_listener(input, id);

    doc.dispatch_event(input, &mut Event::input());

    assert!(log.borrow().is_empty());
}

#[test]
fn listeners_mutate_the_document_through_the_public_api() {
    let (mut doc, form, _group, input) = tree();
    doc.add_event_listener(form, "change", ListenerOptions::default(), |doc, event| {
        let target = event.target().expect("dispatched events carry a target");
        doc.add_class(target, "is-invalid");
        let sibling = doc.create_element(html::Span);
        doc.append_child(target, sibling);
    });

    doc.dispatch_event(input, &mut Event::change());

    assert!(doc.has_class(input, "is-invalid"));
    assert_eq!(doc.children(input).count(), 1);
}

#[test]
fn native_constructors_encode_the_bubbling_table() {
    assert!(Event::input().bubbles() && !Event::input().cancelable());
    assert!(Event::change().bubbles() && !Event::change().cancelable());
    assert!(Event::submit().bubbles() && Event::submit().cancelable());
    assert!(!Event::focus().bubbles());
    assert!(!Event::blur().bubbles());
    // Hand-dispatched events default all-false, like CustomEvent.
    let custom = Event::new("value-changed");
    assert!(!custom.bubbles() && !custom.cancelable());
    assert_eq!(custom.phase(), EventPhase::None);
}
