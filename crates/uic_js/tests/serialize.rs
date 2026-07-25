//! The serializer polyfill: lit's binding prefixes recover from the static
//! strings, text and attribute values escape, and property bindings map to
//! the browser contract — `.value` becomes the attribute on value-carrying
//! elements only, `.hidden` becomes the attribute, everything else drops.

use uic_js::JsHost;

const PROBE: &str = r#"
import { html, LitElement } from 'lit';

class SerializeProbe extends LitElement {
    static properties = { mode: {} };

    constructor() {
        super();
        this.mode = 'empty';
    }

    onNoop() {}

    render() {
        if (this.mode === 'escape') {
            return html`<p title=${'<b>&"</b>'}>${'<i>&</i>'}</p>`;
        }
        if (this.mode === 'marker') {
            return html`<p @click=${this.onNoop}>x</p>`;
        }
        if (this.mode === 'disabled-on') {
            return html`<button ?disabled=${true}>x</button>`;
        }
        if (this.mode === 'disabled-off') {
            return html`<button ?disabled=${false}>x</button>`;
        }
        if (this.mode === 'value-input') {
            return html`<input .value=${'typed'} />`;
        }
        if (this.mode === 'value-textarea') {
            return html`<textarea .value=${'long'}></textarea>`;
        }
        if (this.mode === 'value-custom') {
            return html`<x-thing .value=${'typed'}></x-thing>`;
        }
        if (this.mode === 'hidden-on') {
            return html`<p .hidden=${true}>x</p>`;
        }
        if (this.mode === 'flatten') {
            return html`<p>${['a', ['b', 'c']]}${null}${undefined}${false}${7}</p>`;
        }
        return html``;
    }
}

customElements.define('serialize-probe', SerializeProbe);
"#;

fn committed(host: &mut JsHost, node: uic_dom::NodeId, mode: &str) -> String {
    host.set_prop(node, "mode", &format!("\"{mode}\"")).unwrap();
    host.state.borrow().doc.inner_html(node)
}

#[test]
fn text_and_attribute_values_escape() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:serialize", PROBE).unwrap();
    let node = host.mount("serialize-probe", &[]).unwrap();

    let html = committed(&mut host, node, "escape");
    assert!(
        !html.contains("<b>"),
        "attribute value leaked markup: {html}"
    );
    assert!(!html.contains("<i>"), "text hole leaked markup: {html}");
    assert!(html.contains("&lt;i&gt;"), "text did not escape: {html}");
}

#[test]
fn event_bindings_become_listener_markers() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:serialize", PROBE).unwrap();
    let node = host.mount("serialize-probe", &[]).unwrap();

    let html = committed(&mut host, node, "marker");
    assert!(
        html.contains("data-uic-l-click="),
        "expected the render-scoped listener marker: {html}"
    );
}

#[test]
fn boolean_bindings_toggle_the_attribute() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:serialize", PROBE).unwrap();
    let node = host.mount("serialize-probe", &[]).unwrap();

    let on = committed(&mut host, node, "disabled-on");
    assert!(on.contains("disabled"), "{on}");
    let off = committed(&mut host, node, "disabled-off");
    assert!(!off.contains("disabled"), "{off}");
}

#[test]
fn value_bindings_reach_value_carrying_elements_only() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:serialize", PROBE).unwrap();
    let node = host.mount("serialize-probe", &[]).unwrap();

    let input = committed(&mut host, node, "value-input");
    assert!(input.contains(r#"value="typed""#), "{input}");
    let textarea = committed(&mut host, node, "value-textarea");
    assert!(textarea.contains(r#"value="long""#), "{textarea}");
    // lit-SSR's rule: a custom element's `value` PROPERTY is not the
    // browser attribute contract — it must not serialize.
    let custom = committed(&mut host, node, "value-custom");
    assert!(!custom.contains("value="), "{custom}");
}

#[test]
fn hidden_maps_to_its_attribute() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:serialize", PROBE).unwrap();
    let node = host.mount("serialize-probe", &[]).unwrap();

    let html = committed(&mut host, node, "hidden-on");
    assert!(html.contains("hidden"), "{html}");
}

#[test]
fn holes_flatten_arrays_and_skip_nothing_values() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:serialize", PROBE).unwrap();
    let node = host.mount("serialize-probe", &[]).unwrap();

    let html = committed(&mut host, node, "flatten");
    assert!(html.contains("abc"), "arrays flatten in order: {html}");
    assert!(html.contains('7'), "numbers stringify: {html}");
    assert!(!html.contains("null"), "{html}");
    assert!(!html.contains("undefined"), "{html}");
    assert!(!html.contains("false"), "{html}");
}
