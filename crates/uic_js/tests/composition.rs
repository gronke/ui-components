//! A committed subtree upgrades its nested custom elements: the parent's
//! render mounts the children it names, a re-commit upgrades the swapped-in
//! replacements from their attributes, and events reach the composition:
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
        return html`<span class="mark">${this.done ? '[x]' : '[ ]'}</span><span class="label">${this.text}</span>`;
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
        } else if (key === 'ArrowDown' && event.shiftKey) {
            this.move(1);
        } else if (key === 'ArrowUp' && event.shiftKey) {
            this.move(-1);
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
        const index = Number(event.currentTarget.getAttribute('data-index'));
        if (event.target.closest('.label')) {
            this.draft = this.items[index].text;
        } else {
            this.toggle(index);
        }
    }
    onItemDblClick(event) {
        this.draft = 'dbl:' + event.currentTarget.getAttribute('data-index');
    }
    toggle(index) {
        this.items = this.items.map((item, at) =>
            at === index ? { id: item.id, text: item.text, done: !item.done } : item,
        );
    }
    move(delta) {
        const to = this.selected + delta;
        if (to < 0 || to >= this.items.length) {
            return;
        }
        const items = this.items.slice();
        const moved = items.splice(this.selected, 1)[0];
        items.splice(to, 0, moved);
        this.items = items;
        this.selected = to;
    }
    render() {
        return html`
            ${repeat(this.items, (item) => item.id, (item, index) => html`
                <demo-item data-index="${index}" text="${item.text}" ?done=${item.done}
                    @click=${this.onItemClick} @dblclick=${this.onItemDblClick}></demo-item>`)}
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

/// The `<demo-item>` node carrying the given `text` attribute: the click
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

/// The label span inside an item, the deeper target a click on the text
/// resolves to.
fn label_node(host: &JsHost, text: &str) -> Option<uic_dom::NodeId> {
    let item = item_node(host, text)?;
    let state = host.state.borrow();
    let found = state
        .doc
        .descendants(item)
        .find(|&node| state.doc.attribute(node, "class") == Some("label"));
    found
}

/// The text inside the label, the deepest node a terminal hit test lands
/// on; discrimination must walk up from here (closest, not matches).
fn label_text_node(host: &JsHost, text: &str) -> Option<uic_dom::NodeId> {
    let label = label_node(host, text)?;
    let state = host.state.borrow();
    let found = state
        .doc
        .descendants(label)
        .find(|&node| node != label && state.doc.element(node).is_none());
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
        mounted.contains("[ ]alpha"),
        "children render on mount:\n{mounted}"
    );
    assert!(
        mounted.contains("[x]beta"),
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
        added.contains("[ ]gamma"),
        "enter commits the draft as a new child:\n{added}"
    );
    assert!(
        added.contains("[ ]alpha") && added.contains("[x]beta"),
        "the swapped-in children upgrade again:\n{added}"
    );

    host.dispatch_key("Enter").unwrap();
    let toggled = paint(&host, &mut terminal);
    assert!(
        toggled.contains("[x]alpha"),
        "empty-draft enter toggles the selected row:\n{toggled}"
    );

    host.dispatch_key("ArrowDown").unwrap();
    host.dispatch_key("Enter").unwrap();
    let arrowed = paint(&host, &mut terminal);
    assert!(
        arrowed.contains("[ ]beta"),
        "arrows move the selection:\n{arrowed}"
    );

    let gamma = item_node(&host, "gamma").expect("gamma item in the document");
    host.click(gamma).unwrap();
    let clicked = paint(&host, &mut terminal);
    assert!(
        clicked.contains("[x]gamma"),
        "the template @click routes into the parent behavior:\n{clicked}"
    );
}

#[test]
fn modifiers_and_click_targets_reach_the_composition() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:composition", APP).unwrap();
    let root = host.mount("demo-list", &[]).unwrap();
    host.focus(root).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(44, 12)).unwrap();

    // The plain arrow only selects; the shifted one moves the row.
    host.dispatch_key("ArrowDown").unwrap();
    host.dispatch_key("ArrowUp").unwrap();
    let before = paint(&host, &mut terminal);
    assert!(
        before.find("alpha") < before.find("beta"),
        "plain arrows leave the order alone:\n{before}"
    );
    assert!(
        host.dispatch_key_shift("ArrowDown", true).unwrap(),
        "the shifted arrow is handled"
    );
    let moved = paint(&host, &mut terminal);
    assert!(
        moved.find("beta") < moved.find("alpha"),
        "shift+arrow moves the selected row down:\n{moved}"
    );

    // A click on the label's TEXT (the deepest hit-test target) reports
    // editing intent; one on the row element toggles: the target's
    // closest() walk discriminates.
    let label = label_text_node(&host, "beta").expect("beta label text");
    host.click(label).unwrap();
    let after_label = paint(&host, &mut terminal);
    assert!(
        after_label.contains("draft: beta"),
        "label clicks carry the text:\n{after_label}"
    );
    assert!(
        after_label.contains("[x]beta"),
        "label clicks do not toggle:\n{after_label}"
    );

    let row = item_node(&host, "beta").expect("beta item");
    host.click(row).unwrap();
    let after_row = paint(&host, &mut terminal);
    assert!(
        after_row.contains("[ ]beta"),
        "row clicks toggle:\n{after_row}"
    );

    // A double click travels the same marker machinery under its own name.
    // The toggle's re-commit swapped the children, so resolve the node
    // fresh, exactly what a pointer's per-click hit test does.
    let row = item_node(&host, "beta").expect("beta item after the swap");
    host.dblclick(row).unwrap();
    let after_dbl = paint(&host, &mut terminal);
    assert!(
        after_dbl.contains("draft: dbl:"),
        "dblclick routes into the parent behavior:\n{after_dbl}"
    );
}
