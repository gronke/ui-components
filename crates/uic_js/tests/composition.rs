//! A committed subtree upgrades its nested custom elements: the parent's
//! render mounts the children it names, a re-commit upgrades the swapped-in
//! replacements from their attributes, and events reach the composition —
//! printable keydowns on the host instance, template `@click` markers on
//! the child tags.

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;

/// A two-component app shaped like a todo list: the parent owns the rows
/// and the draft, the child renders one row from its attributes.
const APP: &str = r#"
import { html, LitElement } from 'lit';
import { repeat } from 'lit/directives/repeat.js';

class DemoItem extends LitElement {
    static properties = { text: {}, done: { type: Boolean } };
    constructor() {
        super();
        this.text = '';
        this.done = false;
    }
    render() {
        return html`<span>${this.done ? '[x] ' : '[ ] '}${this.text}</span>`;
    }
}
customElements.define('demo-item', DemoItem);

class DemoList extends LitElement {
    static properties = {
        items: { state: true },
        draft: { state: true },
        selected: { state: true },
    };
    constructor() {
        super();
        this.items = [
            { id: 1, text: 'alpha', done: false },
            { id: 2, text: 'beta', done: true },
        ];
        this.draft = '';
        this.selected = 0;
        this.addEventListener('keydown', (event) => this.onKey(event));
    }
    onKey(event) {
        const key = event.key;
        if (key === 'Enter') {
            if (this.draft.length > 0) {
                const id = this.items.length + 1;
                this.items = this.items.concat([{ id, text: this.draft, done: false }]);
                this.draft = '';
            } else {
                this.toggle(this.selected);
            }
        } else if (key === 'Backspace') {
            this.draft = this.draft.slice(0, -1);
        } else if (key === 'ArrowDown') {
            this.selected = Math.min(this.selected + 1, this.items.length - 1);
        } else if (key === 'ArrowUp') {
            this.selected = Math.max(this.selected - 1, 0);
        } else if (key.length === 1) {
            this.draft = this.draft + key;
        } else {
            return;
        }
        event.preventDefault();
    }
    onItemClick(event) {
        this.toggle(Number(event.currentTarget.getAttribute('data-index')));
    }
    toggle(index) {
        this.items = this.items.map((item, at) =>
            at === index ? { id: item.id, text: item.text, done: !item.done } : item,
        );
    }
    render() {
        return html`
            ${repeat(this.items, (item) => item.id, (item, index) => html`
                <demo-item data-index="${index}" text="${item.text}" ?done=${item.done}
                    @click=${this.onItemClick}></demo-item>`)}
            <p>draft: ${this.draft}</p>
        `;
    }
}
customElements.define('demo-list', DemoList);
"#;

fn paint(host: &JsHost, terminal: &mut Terminal<TestBackend>) -> String {
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let mut s = state.borrow_mut();
            s.dirty = false;
            let focused = s.focused;
            uic_tui::dom::paint_document(frame, frame.area(), &mut s.doc, focused);
        })
        .unwrap();
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

/// The `<demo-item>` node carrying the given `text` attribute — the click
/// target a `uic_tui::dom::hit_test` would yield.
fn item_node(host: &JsHost, text: &str) -> Option<uic_dom::NodeId> {
    let state = host.state.borrow();
    let root = state.doc.root();
    let found = state
        .doc
        .descendants(root)
        .find(|&node| state.doc.attribute(node, "text") == Some(text));
    found
}

#[test]
fn nested_elements_upgrade_render_and_route_events() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:composition", APP).unwrap();
    let root = host.mount("demo-list", &[]).unwrap();
    host.focus(root).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(44, 12)).unwrap();

    let mounted = paint(&host, &mut terminal);
    assert!(
        mounted.contains("[ ] alpha"),
        "children render on mount:\n{mounted}"
    );
    assert!(
        mounted.contains("[x] beta"),
        "the boolean attribute syncs into the child:\n{mounted}"
    );

    for key in ["g", "a", "m", "m", "a", "x"] {
        assert!(
            host.dispatch_key(key).unwrap(),
            "printable {key:?} is handled"
        );
    }
    host.dispatch_key("Backspace").unwrap();
    let typed = paint(&host, &mut terminal);
    assert!(
        typed.contains("draft: gamma"),
        "typing accumulates:\n{typed}"
    );

    host.dispatch_key("Enter").unwrap();
    let added = paint(&host, &mut terminal);
    assert!(
        added.contains("[ ] gamma"),
        "enter commits the draft as a new child:\n{added}"
    );
    assert!(
        added.contains("[ ] alpha") && added.contains("[x] beta"),
        "the swapped-in children upgrade again:\n{added}"
    );

    host.dispatch_key("Enter").unwrap();
    let toggled = paint(&host, &mut terminal);
    assert!(
        toggled.contains("[x] alpha"),
        "empty-draft enter toggles the selected row:\n{toggled}"
    );

    host.dispatch_key("ArrowDown").unwrap();
    host.dispatch_key("Enter").unwrap();
    let arrowed = paint(&host, &mut terminal);
    assert!(
        arrowed.contains("[ ] beta"),
        "arrows move the selection:\n{arrowed}"
    );

    let gamma = item_node(&host, "gamma").expect("gamma item in the document");
    host.click(gamma).unwrap();
    let clicked = paint(&host, &mut terminal);
    assert!(
        clicked.contains("[x] gamma"),
        "the template @click routes into the parent behavior:\n{clicked}"
    );
}
