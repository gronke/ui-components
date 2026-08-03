//! The paste default action on the Boa host: one bulk insert into the
//! focused widget and exactly one bubbling `input` event; a pasted
//! credential must not replay as a keystroke hail.

use uic_js::JsHost;

const PASTE_BOX: &str = r#"
import { html, LitElement } from 'lit';

class PasteBox extends LitElement {
    // Plain fields: reactive ones would re-render per input and swap the
    // textarea under the test's feet.
    constructor() {
        super();
        this.peer = '';
        this.inputs = 0;
    }

    firstUpdated() {
        this.querySelector('textarea').focus();
    }

    onInput(event) {
        this.inputs = this.inputs + 1;
        this.peer = event.target.value;
    }

    render() {
        return html`<textarea data-path="peer" @input=${this.onInput}></textarea>`;
    }
}

customElements.define('paste-box', PasteBox);
"#;

#[test]
fn a_paste_is_one_bulk_insert_and_one_input_event() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:paste", PASTE_BOX).unwrap();
    let node = host.mount("paste-box", &[]).unwrap();

    let token = "https://host/p2p/#dWljMVBhc3RlZFRva2Vu";
    assert!(host.paste(token).unwrap());
    assert_eq!(host.prop_json(node, "peer").unwrap(), format!("{token:?}"));
    assert_eq!(
        host.prop_json(node, "inputs").unwrap(),
        "1",
        "one input event for the whole paste"
    );

    // Typing continues from the paste, each key its own input event.
    host.dispatch_key("!").unwrap();
    assert_eq!(host.prop_json(node, "inputs").unwrap(), "2");
    assert_eq!(
        host.prop_json(node, "peer").unwrap(),
        format!("{:?}", format!("{token}!"))
    );
}

#[test]
fn a_pasted_line_break_folds_for_the_textarea() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:paste", PASTE_BOX).unwrap();
    let node = host.mount("paste-box", &[]).unwrap();

    // The terminal spells breaks as \r inside a bracketed paste.
    assert!(host.paste("line one\r\nline two").unwrap());
    assert_eq!(
        host.prop_json(node, "peer").unwrap(),
        "\"line one\\nline two\""
    );
}

#[test]
fn an_unfocused_paste_is_a_no_op() {
    let mut host = JsHost::new().unwrap();
    assert!(!host.paste("nothing to take it").unwrap());
}
