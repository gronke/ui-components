//! The walking skeleton (#65): a component written against the mocked `lit`
//! renders through the Boa engine into the retained document, and the
//! existing layout/paint pipeline draws the frame.

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;

const HELLO: &str = r#"
import { html, LitElement } from 'lit';

class HelloWorld extends LitElement {
    static properties = { count: { type: Number } };

    constructor() {
        super();
        this.count = 0;
    }

    render() {
        return html`<div class="card"><span class="fw-bold">Count: ${this.count}</span></div>`;
    }
}

customElements.define('hello-world', HelloWorld);
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

#[test]
fn a_mock_lit_component_paints_a_frame() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:hello", HELLO).unwrap();
    let node = host.mount("hello-world", &[]).unwrap();

    host.set_prop(node, "count", "3").unwrap();

    let mut terminal = Terminal::new(TestBackend::new(30, 5)).unwrap();
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let doc = &mut state.borrow_mut().doc;
            uic_tui::dom::paint_document(frame, frame.area(), doc, None);
        })
        .unwrap();

    let screen = screen_text(&terminal);
    assert!(
        screen.contains("Count: 3"),
        "expected the committed property in the frame:\n{screen}"
    );
}
