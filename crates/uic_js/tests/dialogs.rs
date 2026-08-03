//! The dialogs feature end-to-end: the runtime's alert/confirm/prompt
//! queue their questions for the host and park promises that only a host
//! answer settles; there are no timers in the mocked runtime, so the
//! host's `answer_dialog` (eval + job drain) is the one way forward.

use uic_js::{DialogKind, JsHost};

fn text_of(host: &mut JsHost, expr: &str) -> String {
    host.eval(&format!("String({expr})"))
        .unwrap()
        .as_string()
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_default()
}

#[test]
fn confirm_rides_the_queue_and_the_answer_settles_it() {
    let mut host = JsHost::new().unwrap();
    host.eval(
        "globalThis.picked = 'unset'; \
         void confirm('drop the current attempt?').then((v) => { globalThis.picked = v; });",
    )
    .unwrap();

    let request = host.take_dialog_request().expect("a queued question");
    assert_eq!(request.kind, DialogKind::Confirm);
    assert_eq!(request.message, "drop the current attempt?");
    assert_eq!(request.default, None);

    // Nothing settles on its own; no timers exist in the runtime.
    host.run_jobs().unwrap();
    assert_eq!(text_of(&mut host, "picked"), "unset");

    host.answer_dialog(request.id, "true").unwrap();
    assert_eq!(text_of(&mut host, "picked"), "true");
}

#[test]
fn prompt_carries_its_default_and_returns_text_or_null() {
    let mut host = JsHost::new().unwrap();
    host.eval(
        "globalThis.name = 'unset'; \
         void prompt('who?', 'world').then((v) => { globalThis.name = v; });",
    )
    .unwrap();
    let request = host.take_dialog_request().expect("a queued question");
    assert_eq!(request.kind, DialogKind::Prompt);
    assert_eq!(request.default.as_deref(), Some("world"));
    host.answer_dialog(request.id, "\"terminal\"").unwrap();
    assert_eq!(text_of(&mut host, "name"), "terminal");

    // A cancel answers null, the browser's own shape; an omitted default
    // prefills empty, also the browser's shape.
    host.eval("void prompt('again?').then((v) => { globalThis.name = String(v); });")
        .unwrap();
    let request = host.take_dialog_request().expect("a second question");
    assert_eq!(request.default.as_deref(), Some(""));
    host.answer_dialog(request.id, "null").unwrap();
    assert_eq!(text_of(&mut host, "name"), "null");
}

#[test]
fn questions_drain_oldest_first() {
    let mut host = JsHost::new().unwrap();
    host.eval("void alert('first'); void alert('second');")
        .unwrap();
    assert_eq!(host.take_dialog_request().unwrap().message, "first");
    assert_eq!(host.take_dialog_request().unwrap().message, "second");
    assert!(host.take_dialog_request().is_none());
}

#[test]
fn a_fresh_host_starts_with_an_empty_queue() {
    let mut first = JsHost::new().unwrap();
    first.eval("void alert('stale')").unwrap();
    drop(first);

    let mut second = JsHost::new().unwrap();
    assert!(second.take_dialog_request().is_none());
}

const ASKS: &str = r#"
import { html, LitElement } from 'lit';

class AsksFirst extends LitElement {
    static properties = { verdict: {} };

    constructor() {
        super();
        this.verdict = 'undecided';
        void confirm('accept the new pairing?').then((v) => {
            this.verdict = v ? 'accepted' : 'declined';
        });
    }

    render() {
        return html`<span>${this.verdict}</span>`;
    }
}

customElements.define('asks-first', AsksFirst);
"#;

// The component spelling of the cross-host contract: `confirm(…)` awaited
// (or then-ed) reads the same in both hosts (the browser answers its sync
// boolean, the terminal resolves through the host), and the answer
// re-renders like any other state change.
#[test]
fn a_component_confirm_answer_re_renders() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:asks", ASKS).unwrap();
    let node = host.mount("asks-first", &[]).unwrap();
    assert_eq!(host.prop_json(node, "verdict").unwrap(), "\"undecided\"");

    let request = host.take_dialog_request().expect("the constructor asked");
    host.answer_dialog(request.id, "true").unwrap();
    assert_eq!(host.prop_json(node, "verdict").unwrap(), "\"accepted\"");
    let html = host.state.borrow().doc.inner_html(node);
    assert!(html.contains("accepted"), "the answer re-rendered: {html}");
}
