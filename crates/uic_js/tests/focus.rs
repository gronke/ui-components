//! The focus polyfill: focusout before focusin with relatedTarget carrying
//! the counterpart (WHATWG order), refocus as a no-op, and a null
//! relatedTarget on the very first focus.

use uic_js::JsHost;

const PAIR: &str = r#"
import { html, LitElement } from 'lit';

class FocusPair extends LitElement {
    // log stays a PLAIN field: a reactive one would re-render per reset
    // and swap the inputs the test holds NodeIds for.
    constructor() {
        super();
        this.log = '';
        this.addEventListener('focusin', (event) => this.note(event));
        this.addEventListener('focusout', (event) => this.note(event));
    }

    note(event) {
        const related = event.relatedTarget
            ? event.relatedTarget.getAttribute('data-path')
            : 'null';
        this.log =
            this.log + event.type + ':' + event.target.getAttribute('data-path') + ':' + related + ',';
    }

    render() {
        return html`<input type="text" data-path="a" />
            <input type="text" data-path="b" />`;
    }
}

customElements.define('focus-pair', FocusPair);
"#;

fn by_path(host: &JsHost, path: &str) -> uic_dom::NodeId {
    let state = host.state.borrow();
    let root = state.doc.root();
    let found = state
        .doc
        .descendants(root)
        .find(|&node| state.doc.attribute(node, "data-path") == Some(path));
    found.expect("the input")
}

fn log(host: &mut JsHost, node: uic_dom::NodeId) -> String {
    let text = host.prop_json(node, "log").unwrap();
    host.set_prop(node, "log", "\"\"").unwrap();
    text
}

#[test]
fn focus_moves_in_whatwg_order_with_related_targets() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:focus", PAIR).unwrap();
    let node = host.mount("focus-pair", &[]).unwrap();

    let a = by_path(&host, "a");
    let b = by_path(&host, "b");

    // The very first focus has nothing to blur and no relatedTarget.
    host.focus(a).unwrap();
    assert_eq!(log(&mut host, node), "\"focusin:a:null,\"");

    // Moving: the old node blurs first, each event naming the other side.
    host.focus(b).unwrap();
    assert_eq!(log(&mut host, node), "\"focusout:a:b,focusin:b:a,\"");

    // Refocusing the focused node is a no-op.
    host.focus(b).unwrap();
    assert_eq!(log(&mut host, node), "\"\"");
}
