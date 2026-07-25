//! The property-machinery polyfill: attribute converters at mount,
//! attribute naming, `attribute: false`, custom converters, the decorator
//! merge with `static properties`, and setAttribute replaying through the
//! converters into the properties.

use uic_js::JsHost;

const TYPED: &str = r#"
import { html, LitElement } from 'lit';

class TypedProbe extends LitElement {
    static properties = {
        count: { type: Number },
        on: { type: Boolean },
        data: { type: Object },
        longName: {},
        named: { attribute: 'data-x' },
        internal: { attribute: false },
        shouted: { converter: { fromAttribute: (value) => value + '!' } },
    };

    constructor() {
        super();
        this.count = 0;
        this.on = false;
        this.data = {};
        this.longName = '';
        this.named = '';
        this.internal = 'kept';
        this.shouted = '';
        this.addEventListener('click', () => this.poke());
    }

    poke() {
        this.setAttribute('count', '9');
    }

    render() {
        return html`<p>${this.count}</p>`;
    }
}

customElements.define('typed-probe', TypedProbe);
"#;

const MERGED: &str = r#"
import { html, LitElement } from 'lit';
import { property } from 'lit/decorators.js';

class MergedBase extends LitElement {
    static properties = { fromStatic: { type: Number } };
}

class MergedProbe extends MergedBase {
    constructor() {
        super();
        this.fromStatic = 0;
        this.fromDecorator = 0;
    }

    render() {
        return html`<p>${this.fromStatic}:${this.fromDecorator}</p>`;
    }
}
// The legacy decorator call convention, the shape compiled dists emit.
Object.defineProperty(
    MergedProbe.prototype,
    'fromDecorator',
    property({ type: Number })(MergedProbe.prototype, 'fromDecorator'),
);

customElements.define('merged-probe', MergedProbe);
"#;

#[test]
fn mount_attributes_convert_into_typed_properties() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:converters", TYPED).unwrap();
    let node = host
        .mount(
            "typed-probe",
            &[
                ("count", "5"),
                ("on", ""),
                ("data", r#"{"k":1}"#),
                ("longname", "lowercased"),
                ("data-x", "renamed"),
                ("internal", "ignored"),
                ("shouted", "hey"),
            ],
        )
        .unwrap();

    assert_eq!(host.prop_json(node, "count").unwrap(), "5");
    assert_eq!(host.prop_json(node, "on").unwrap(), "true");
    assert_eq!(host.prop_json(node, "data").unwrap(), r#"{"k":1}"#);
    // A camelCase property syncs from its lowercased attribute name.
    assert_eq!(host.prop_json(node, "longName").unwrap(), "\"lowercased\"");
    // `attribute:` renames; `attribute: false` never syncs.
    assert_eq!(host.prop_json(node, "named").unwrap(), "\"renamed\"");
    assert_eq!(host.prop_json(node, "internal").unwrap(), "\"kept\"");
    // A custom converter object runs its fromAttribute.
    assert_eq!(host.prop_json(node, "shouted").unwrap(), "\"hey!\"");
}

#[test]
fn an_absent_boolean_attribute_reads_false() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:converters", TYPED).unwrap();
    let node = host.mount("typed-probe", &[]).unwrap();
    assert_eq!(host.prop_json(node, "on").unwrap(), "false");
}

#[test]
fn set_attribute_replays_through_the_converters() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:converters", TYPED).unwrap();
    let node = host.mount("typed-probe", &[]).unwrap();

    // The component pokes its own attribute; the converter lands the
    // typed value back in the property.
    host.click(node).unwrap();
    assert_eq!(host.prop_json(node, "count").unwrap(), "9");
}

#[test]
fn decorators_merge_with_static_properties_across_the_chain() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:converters", MERGED).unwrap();
    let node = host
        .mount(
            "merged-probe",
            &[("fromstatic", "3"), ("fromdecorator", "4")],
        )
        .unwrap();

    // Both declaration styles are reactive and attribute-synced.
    assert_eq!(host.prop_json(node, "fromStatic").unwrap(), "3");
    assert_eq!(host.prop_json(node, "fromDecorator").unwrap(), "4");
    host.set_prop(node, "fromDecorator", "8").unwrap();
    let html = host.state.borrow().doc.inner_html(node);
    assert!(
        html.contains("3:8"),
        "the decorator property re-renders: {html}"
    );
}
