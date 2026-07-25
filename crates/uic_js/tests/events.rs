//! The event polyfill: bubbling order, the target/currentTarget contract,
//! the stop family, preventDefault as the cancelable-keydown return, and
//! non-bubbling delivery.

use uic_js::JsHost;
use uic_tui::KeyStroke;

const NEST: &str = r#"
import { html, LitElement } from 'lit';

class EventNest extends LitElement {
    // log and stopMode are deliberately PLAIN fields: reactive ones would
    // re-render on every reset and swap the nodes under the test's feet.
    constructor() {
        super();
        this.log = '';
        this.stopMode = 'none';
        this.addEventListener('click', (event) => this.first(event));
        this.addEventListener('click', () => this.second());
        this.addEventListener('poke', () => this.poked());
        this.addEventListener('keydown', (event) => event.preventDefault());
    }

    firstUpdated() {
        this.querySelector('input').focus();
    }

    note(event) {
        const name = event.currentTarget.getAttribute('data-name');
        const target = event.target.getAttribute('data-name');
        this.log = this.log + name + '<' + target + ',';
        if (name === 'mid' && this.stopMode === 'stop') {
            event.stopPropagation();
        }
        if (name === 'mid' && this.stopMode === 'hard') {
            event.stopImmediatePropagation();
        }
    }

    first(event) {
        this.log = this.log + 'host<' + event.target.getAttribute('data-name') + ',';
        if (this.stopMode === 'host-stop') {
            event.stopPropagation();
        }
        if (this.stopMode === 'host-hard') {
            event.stopImmediatePropagation();
        }
    }

    second() {
        this.log = this.log + 'host2,';
    }

    poked() {
        this.log = this.log + 'poked,';
    }

    render() {
        return html`<div data-name="mid" @click=${this.note}>
            <p data-name="inner" @click=${this.note}>x</p>
        </div>
        <input type="text" data-path="f" />`;
    }
}

customElements.define('event-nest', EventNest);
"#;

fn inner(host: &JsHost) -> uic_dom::NodeId {
    let state = host.state.borrow();
    let root = state.doc.root();
    let found = state
        .doc
        .descendants(root)
        .find(|&node| state.doc.attribute(node, "data-name") == Some("inner"));
    found.expect("the inner node")
}

fn log(host: &mut JsHost, node: uic_dom::NodeId) -> String {
    let text = host.prop_json(node, "log").unwrap();
    host.set_prop(node, "log", "\"\"").unwrap();
    text
}

#[test]
fn clicks_bubble_target_to_host_with_a_stable_target() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:events", NEST).unwrap();
    let node = host.mount("event-nest", &[]).unwrap();

    let at = inner(&host);
    host.click(at).unwrap();
    // currentTarget varies along the chain; target names the click's node
    // at every stop, and both host listeners run in registration order.
    assert_eq!(
        log(&mut host, node),
        "\"inner<inner,mid<inner,host<inner,host2,\""
    );
}

#[test]
fn stop_propagation_stops_ancestors() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:events", NEST).unwrap();
    let node = host.mount("event-nest", &[]).unwrap();

    host.set_prop(node, "stopMode", "\"stop\"").unwrap();
    host.set_prop(node, "log", "\"\"").unwrap();
    let at = inner(&host);
    host.click(at).unwrap();
    assert_eq!(log(&mut host, node), "\"inner<inner,mid<inner,\"");
}

#[test]
fn stop_family_on_the_last_node_gates_same_node_listeners() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:events", NEST).unwrap();
    let node = host.mount("event-nest", &[]).unwrap();

    // stopImmediatePropagation silences the second host listener.
    host.set_prop(node, "stopMode", "\"host-hard\"").unwrap();
    host.set_prop(node, "log", "\"\"").unwrap();
    let at = inner(&host);
    host.click(at).unwrap();
    assert_eq!(
        log(&mut host, node),
        "\"inner<inner,mid<inner,host<inner,\""
    );

    // Plain stopPropagation only blocks the ancestors — the second
    // listener on the same node still runs, the platform's distinction.
    host.set_prop(node, "stopMode", "\"host-stop\"").unwrap();
    host.set_prop(node, "log", "\"\"").unwrap();
    host.click(at).unwrap();
    assert_eq!(
        log(&mut host, node),
        "\"inner<inner,mid<inner,host<inner,host2,\""
    );
}

#[test]
fn prevent_default_returns_through_dispatch_and_shields_the_widget() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:events", NEST).unwrap();
    let node = host.mount("event-nest", &[]).unwrap();

    // The host cancels every keydown: dispatch reports it and the focused
    // widget never runs its editing default action.
    assert!(host.dispatch(&KeyStroke::new("h")).unwrap());
    let input = {
        let state = host.state.borrow();
        let root = state.doc.root();
        let found = state
            .doc
            .descendants(root)
            .find(|&n| state.doc.attribute(n, "data-path") == Some("f"));
        found.expect("the input")
    };
    let widget_text = {
        let mut state = host.state.borrow_mut();
        let handle = state.handle(input);
        state.widget_value(handle)
    };
    assert_eq!(widget_text, Some("".into()));
    let _ = node;
}

#[test]
fn non_bubbling_events_stay_at_the_target() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:events", NEST).unwrap();
    let node = host.mount("event-nest", &[]).unwrap();

    let at = inner(&host);
    let handle = host.state.borrow_mut().handle(at);
    // A poke at the inner node with bubbles:false never reaches the host
    // listener; the same poke bubbling does.
    host.eval(&format!(
        "__uicDeliver({handle}, 'poke', {{ bubbles: false }})"
    ))
    .unwrap();
    host.run_jobs().unwrap();
    assert_eq!(log(&mut host, node), "\"\"");
    host.eval(&format!("__uicDeliver({handle}, 'poke', {{}})"))
        .unwrap();
    host.run_jobs().unwrap();
    assert_eq!(log(&mut host, node), "\"poked,\"");
}
