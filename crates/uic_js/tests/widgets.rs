//! The scripted host drives native widgets: a plain mocked-lit component
//! renders an ordinary `<input>`, the element type mounts its rat widget
//! (ADR 0026), an uncancelled keydown runs the widget as the browser's
//! editing default action, the synthesized `input` event reads the live
//! text back through `target.value`, and the widget (text AND caret)
//! survives the component echoing the value through its subtree-swap
//! re-render.

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;
use uic_tui::KeyStroke;

const FIELD: &str = r#"
import { html, LitElement } from 'lit';
import { live } from 'lit/directives/live.js';

class FieldProof extends LitElement {
    static properties = { text: {}, keydowns: { type: Number }, inputs: { type: Number } };

    constructor() {
        super();
        this.text = '';
        this.keydowns = 0;
        this.inputs = 0;
        this.addEventListener('keydown', (event) => this.onKey(event));
        this.addEventListener('input', (event) => this.onInput(event));
    }

    firstUpdated() {
        this.querySelector('input').focus();
    }

    onKey(event) {
        this.keydowns = this.keydowns + 1;
    }

    onInput(event) {
        this.inputs = this.inputs + 1;
        this.text = event.target.value;
    }

    render() {
        return html`<input type="text" data-path="field" placeholder="type here" .value=${live(this.text)} />`;
    }
}

customElements.define('field-proof', FieldProof);
"#;

fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn paint(host: &JsHost) -> String {
    let mut terminal = Terminal::new(TestBackend::new(30, 4)).unwrap();
    let state = host.state.clone();
    let focused = state.borrow().focused;
    terminal
        .draw(|frame| {
            let doc = &mut state.borrow_mut().doc;
            uic_tui::dom::paint_document(frame, frame.area(), doc, focused);
        })
        .unwrap();
    screen_text(&terminal)
}

#[test]
fn keystrokes_drive_the_widget_and_input_events_read_back() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:field", FIELD).unwrap();
    let node = host.mount("field-proof", &[]).unwrap();

    // The empty input paints its placeholder.
    let screen = paint(&host);
    assert!(
        screen.contains("type here"),
        "expected the placeholder in the frame:\n{screen}"
    );

    // Typing lands in the widget; each text change synthesizes `input`,
    // whose listener echoes `target.value` into the component property.
    assert!(!host.dispatch_key("h").unwrap());
    assert!(!host.dispatch_key("i").unwrap());
    assert_eq!(host.prop_json(node, "text").unwrap(), "\"hi\"");
    assert_eq!(host.prop_json(node, "inputs").unwrap(), "2");
    assert_eq!(host.prop_json(node, "keydowns").unwrap(), "2");

    // A caret move is keydown-only; typing then lands mid-string; the
    // widget (and its caret) survived the echo re-renders in between.
    host.dispatch(&KeyStroke::new("ArrowLeft")).unwrap();
    assert_eq!(host.prop_json(node, "inputs").unwrap(), "2");
    host.dispatch_key("X").unwrap();
    assert_eq!(host.prop_json(node, "text").unwrap(), "\"hXi\"");

    // Enter changes no text: keydown flows, `input` does not.
    host.dispatch_key("Enter").unwrap();
    assert_eq!(host.prop_json(node, "inputs").unwrap(), "3");
    assert_eq!(host.prop_json(node, "keydowns").unwrap(), "5");

    let screen = paint(&host);
    assert!(
        screen.contains("hXi"),
        "expected the typed text in the frame:\n{screen}"
    );
}

const OVERRIDE: &str = r#"
import { html, LitElement } from 'lit';

class OverrideProof extends LitElement {
    static properties = { text: {} };

    constructor() {
        super();
        this.text = '';
        this.addEventListener('input', (event) => this.onInput(event));
    }

    firstUpdated() {
        this.querySelector('span').focus();
    }

    onInput(event) {
        this.text = event.target.value;
    }

    render() {
        return html`<span data-tui="text-input" data-path="o"></span>`;
    }
}

customElements.define('override-proof', OverrideProof);
"#;

const DATEFIELD: &str = r#"
import { html, LitElement } from 'lit';
import { live } from 'lit/directives/live.js';

class DateProof extends LitElement {
    static properties = { date: {} };

    constructor() {
        super();
        this.date = '';
        this.addEventListener('input', (event) => this.onInput(event));
    }

    firstUpdated() {
        this.querySelector('input').focus();
    }

    onInput(event) {
        this.date = event.target.value;
    }

    render() {
        return html`<input type="date" data-path="d" .value=${live(this.date)} />`;
    }
}

customElements.define('date-proof', DateProof);
"#;

#[test]
fn data_tui_still_overrides_detection() {
    // The explicit marker mounts a widget on any tag, the extension
    // point detection deliberately leaves alone.
    let mut host = JsHost::new().unwrap();
    host.load_module("test:override", OVERRIDE).unwrap();
    let node = host.mount("override-proof", &[]).unwrap();

    host.dispatch_key("o").unwrap();
    host.dispatch_key("k").unwrap();
    assert_eq!(host.prop_json(node, "text").unwrap(), "\"ok\"");
}

#[test]
fn a_plain_date_input_mounts_the_date_adapter() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:datefield", DATEFIELD).unwrap();
    let node = host.mount("date-proof", &[]).unwrap();

    // Typing digits fills the year section of the mask; a text adapter
    // would have produced plain "2026", so this pins the date detection.
    for key in ["2", "0", "2", "6"] {
        host.dispatch_key(key).unwrap();
    }
    assert_eq!(host.prop_json(node, "date").unwrap(), "\"2026-00-00\"");

    // The ISO value channel: a scripted write lands in the date mask and
    // paints in the variant's format.
    host.set_prop(node, "date", "\"2026-07-01\"").unwrap();
    let screen = paint(&host);
    assert!(
        screen.contains("2026-07-01"),
        "expected the ISO date in the frame:\n{screen}"
    );
}

#[test]
fn an_external_value_write_resets_the_widget_text() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:field", FIELD).unwrap();
    let node = host.mount("field-proof", &[]).unwrap();

    host.dispatch_key("a").unwrap();
    host.dispatch_key("b").unwrap();
    assert_eq!(host.prop_json(node, "text").unwrap(), "\"ab\"");

    // A remote snapshot (the live-sync path) writes a different text: the
    // echo-skip does not apply, the widget re-syncs, the caret parks at
    // the end; further typing appends.
    host.set_prop(node, "text", "\"xyz\"").unwrap();
    host.dispatch_key("!").unwrap();
    assert_eq!(host.prop_json(node, "text").unwrap(), "\"xyz!\"");

    let screen = paint(&host);
    assert!(
        screen.contains("xyz!"),
        "expected the replaced text in the frame:\n{screen}"
    );
}
