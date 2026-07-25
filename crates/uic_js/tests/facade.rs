//! The node facade polyfill: one wrapper identity per node, the ancestor
//! walk behind closest() (click targets can be text nodes), the tabIndex
//! contract, and the dataset read-through.

use uic_js::JsHost;

const FACADE: &str = r#"
import { html, LitElement } from 'lit';

class FacadeProbe extends LitElement {
    static properties = { seen: {}, same: { type: Boolean }, tab: { type: Number } };

    constructor() {
        super();
        this.seen = '';
        this.same = false;
        this.tab = 0;
        this.addEventListener('click', (event) => this.onClick(event));
    }

    firstUpdated() {
        this.same = this.querySelector('p') === this.querySelector('p');
        this.tab = this.querySelector('p').tabIndex;
    }

    onClick(event) {
        const row = event.target.closest('.row');
        this.seen = row ? row.dataset.path : 'nothing';
    }

    render() {
        return html`<div class="row" data-path="r1"><p>words in a text node</p></div>`;
    }
}

customElements.define('facade-probe', FacadeProbe);
"#;

#[test]
fn wrappers_keep_identity_and_tab_index_defaults() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:facade", FACADE).unwrap();
    let node = host.mount("facade-probe", &[]).unwrap();

    assert_eq!(host.prop_json(node, "same").unwrap(), "true");
    // No tabindex attribute reads as -1, the platform default.
    assert_eq!(host.prop_json(node, "tab").unwrap(), "-1");
}

#[test]
fn closest_walks_up_from_a_text_node_target() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:facade", FACADE).unwrap();
    let node = host.mount("facade-probe", &[]).unwrap();

    // The hit test lands on text nodes; matches() is false there, so the
    // discrimination must ride the ancestor walk.
    let text = {
        let state = host.state.borrow();
        let root = state.doc.root();
        let found = state.doc.descendants(root).find(|&n| {
            matches!(state.doc.node(n), Some(uic_dom::NodeData::Text(t)) if t.contains("words"))
        });
        found.expect("the text node")
    };
    host.click(text).unwrap();
    assert_eq!(host.prop_json(node, "seen").unwrap(), "\"r1\"");
}
