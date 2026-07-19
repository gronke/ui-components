//! The walking skeleton: a component written against the mocked `lit`
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

#[test]
fn a_nested_dist_tree_loads_across_directories() {
    // A dist spanning subdirectories: the entry imports downward, the inner
    // module upward — both resolve because specifiers keep their paths.
    let root = std::env::temp_dir().join(format!("uic-js-dist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("lib/deep")).unwrap();
    std::fs::write(root.join("shared.js"), "export const WORD = 'across';").unwrap();
    std::fs::write(
        root.join("lib/deep/label.js"),
        "import { WORD } from '../../shared.js';\nexport const LABEL = `nested ${WORD}`;",
    )
    .unwrap();
    std::fs::write(
        root.join("entry.js"),
        r#"
import { html, LitElement } from 'lit';
import { LABEL } from './lib/deep/label.js';

class NestedDist extends LitElement {
    render() {
        return html`<span>${LABEL} directories</span>`;
    }
}

customElements.define('nested-dist', NestedDist);
"#,
    )
    .unwrap();

    let mut host = JsHost::new().unwrap();
    host.load_dist_dir(&root, "entry.js").unwrap();
    let _node = host.mount("nested-dist", &[]).unwrap();

    let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let doc = &mut state.borrow_mut().doc;
            uic_tui::dom::paint_document(frame, frame.area(), doc, None);
        })
        .unwrap();
    let screen = screen_text(&terminal);
    assert!(
        screen.contains("nested across directories"),
        "the nested dist renders:\n{screen}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn both_specifier_families_resolve() {
    // Components import through `lit/...` or straight from the producing
    // channel `lit-html/...` — the module table serves both spellings.
    const CHANNELS: &str = r#"
import { html, LitElement } from 'lit';
import { when } from 'lit/directives/when.js';
import { map } from 'lit-html/directives/map.js';
import { classMap } from 'lit-html/directives/class-map.js';

class ChannelProof extends LitElement {
    render() {
        const parts = ['a', 'b'];
        return html`<span class="${classMap({ ok: true })}">${when(
            true,
            () => html`${map(parts, (p) => html`${p}`)}`,
        )} channels</span>`;
    }
}

customElements.define('channel-proof', ChannelProof);
"#;
    let mut host = JsHost::new().unwrap();
    host.load_module("test:channels", CHANNELS).unwrap();
    let _node = host.mount("channel-proof", &[]).unwrap();

    let mut terminal = Terminal::new(TestBackend::new(30, 4)).unwrap();
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let doc = &mut state.borrow_mut().doc;
            uic_tui::dom::paint_document(frame, frame.area(), doc, None);
        })
        .unwrap();
    let screen = screen_text(&terminal);
    assert!(
        screen.contains("ab channels"),
        "both families render:\n{screen}"
    );
}
