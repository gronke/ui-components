//! The shared pairing panel under the scripted (terminal) host: the same
//! `<pair-panel>` the browser renders, driven by properties and signalling
//! intent through a polled `command` property — the terminal's contract,
//! since Boa has no `dispatchEvent` (ADR 0029).

use uic_js::JsHost;

// A deliberately minimal fixture: the property / command seam without the
// app's build tree (the dependency points the other way — uic_js cannot
// import the demo's baked package). The REAL pair-panel.ts is exercised
// under this host in apps/lit-demo/tests/p2p_components.rs; this test pins
// the seam contract (properties in, command out), not the app's markup.
const PANEL: &str = r#"
import { html, LitElement } from 'lit';

class PairPanel extends LitElement {
    static properties = {
        mode: {}, link: {}, status: {}, connected: {}, resetLabel: {}, command: {},
    };

    constructor() {
        super();
        this.mode = 'idle';
        this.link = '';
        this.status = '';
        this.connected = null;
        this.resetLabel = '';
        this.command = null;
    }

    createRenderRoot() {
        return this;
    }

    emit(command) {
        this.command = command;
    }

    render() {
        return html`<section class="card">
            <div class="card-header">
                pair another browser
                ${this.connected === null
                    ? ''
                    : html`<span class="badge">${this.connected ? 'connected' : 'disconnected'}</span>`}
            </div>
            <div class="card-body">
                <p class="status">${this.status}</p>
                ${this.mode === 'idle'
                    ? html`<button class="invite" @click=${() => this.emit('invite')}>create an invite</button>`
                    : this.mode === 'invite'
                      ? html`<a class="copy-link" href=${this.link}>${this.link}</a>
                            <button class="connect" @click=${() => this.emit('connect')}>connect</button>`
                      : ''}
                ${this.resetLabel
                    ? html`<button class="reset" @click=${() => this.emit('reset')}>${this.resetLabel}</button>`
                    : ''}
            </div>
        </section>`;
    }
}

customElements.define('pair-panel', PairPanel);
"#;

fn node_by(host: &JsHost, selector: &str) -> uic_dom::NodeId {
    let state = host.state.borrow();
    let root = state.doc.root();
    state
        .doc
        .find_element(root, |el| {
            el.attr("class").is_some_and(|c| c.contains(selector))
        })
        .expect("a matching node")
}

#[test]
fn properties_drive_the_render() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:panel", PANEL).unwrap();
    let panel = host.mount("pair-panel", &[]).unwrap();

    // The host sets the invite state; the panel renders the link and badge.
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "link", "\"https://example/p2p/#abc123\"")
        .unwrap();
    host.set_prop(panel, "status", "\"share the invite\"")
        .unwrap();
    host.set_prop(panel, "connected", "true").unwrap();
    host.set_prop(panel, "resetLabel", "\"start over\"")
        .unwrap();

    let html = host.state.borrow().doc.inner_html(panel);
    assert!(html.contains("abc123"), "the link renders: {html}");
    assert!(
        html.contains("share the invite"),
        "the status renders: {html}"
    );
    assert!(html.contains("connected"), "the badge renders: {html}");
    assert!(
        html.contains("start over"),
        "the reset button renders: {html}"
    );
}

#[test]
fn buttons_signal_intent_through_the_command_property() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:panel", PANEL).unwrap();
    let panel = host.mount("pair-panel", &[]).unwrap();

    // Idle: clicking "create an invite" writes command="invite" — the only
    // channel back to the terminal host (no events under Boa).
    let invite = node_by(&host, "invite");
    host.click(invite).unwrap();
    assert_eq!(host.prop_json(panel, "command").unwrap(), "\"invite\"");

    // The host reads, acts, clears — and the next click writes afresh.
    host.set_prop(panel, "command", "null").unwrap();
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "resetLabel", "\"start over\"")
        .unwrap();

    let reset = node_by(&host, "reset");
    host.click(reset).unwrap();
    assert_eq!(host.prop_json(panel, "command").unwrap(), "\"reset\"");

    host.set_prop(panel, "command", "null").unwrap();
    let connect = node_by(&host, "connect");
    host.click(connect).unwrap();
    assert_eq!(host.prop_json(panel, "command").unwrap(), "\"connect\"");
}
