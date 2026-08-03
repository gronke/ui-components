//! The supported directive set, each proven by an inline component (the
//! skeleton pattern): the generic surface a third-party element relies on,
//! no npm package required.

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;

fn screen_of(source: &str, tag: &str) -> String {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:directive", source).unwrap();
    let _node = host.mount(tag, &[]).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(44, 6)).unwrap();
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let doc = &mut state.borrow_mut().doc;
            uic_tui::dom::paint_document(frame, frame.area(), doc, None);
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

#[test]
fn repeat_renders_items_unkeyed() {
    let screen = screen_of(
        r#"
import { html, LitElement } from 'lit';
import { repeat } from 'lit/directives/repeat.js';

class RepeatProof extends LitElement {
    render() {
        const rows = ['one', 'two'];
        return html`<span>${repeat(rows, (r) => r, (r, i) => html`${i}:${r} `)}</span>`;
    }
}
customElements.define('repeat-proof', RepeatProof);
"#,
        "repeat-proof",
    );
    assert!(screen.contains("0:one 1:two"), "{screen}");
}

#[test]
fn repeat_without_a_key_function_renders_too() {
    let screen = screen_of(
        r#"
import { html, LitElement } from 'lit';
import { repeat } from 'lit/directives/repeat.js';

class RepeatShort extends LitElement {
    render() {
        return html`<span>${repeat(['a', 'b'], (r) => html`${r}`)}</span>`;
    }
}
customElements.define('repeat-short', RepeatShort);
"#,
        "repeat-short",
    );
    assert!(screen.contains("ab"), "{screen}");
}

#[test]
fn if_defined_renders_values_and_empties_undefined() {
    let screen = screen_of(
        r#"
import { html, LitElement } from 'lit';
import { ifDefined } from 'lit/directives/if-defined.js';

class IfDefinedProof extends LitElement {
    render() {
        const there = 'yes';
        const missing = undefined;
        return html`<span title="${ifDefined(there)}">t:${ifDefined(there)} m:${ifDefined(missing)}.</span>`;
    }
}
customElements.define('ifdefined-proof', IfDefinedProof);
"#,
        "ifdefined-proof",
    );
    assert!(screen.contains("t:yes m:."), "{screen}");
}

#[test]
fn choose_picks_the_matching_case() {
    let screen = screen_of(
        r#"
import { html, LitElement } from 'lit';
import { choose } from 'lit/directives/choose.js';

class ChooseProof extends LitElement {
    render() {
        const pick = (value) => choose(value, [
            ['a', () => html`alpha`],
            ['b', () => html`beta`],
        ], () => html`other`);
        return html`<span>${pick('b')} ${pick('z')}</span>`;
    }
}
customElements.define('choose-proof', ChooseProof);
"#,
        "choose-proof",
    );
    assert!(screen.contains("beta other"), "{screen}");
}

#[test]
fn join_interleaves_and_range_counts() {
    let screen = screen_of(
        r#"
import { html, LitElement } from 'lit';
import { join } from 'lit/directives/join.js';
import { range } from 'lit/directives/range.js';
import { map } from 'lit/directives/map.js';

class JoinRangeProof extends LitElement {
    render() {
        return html`<span>${join(['x', 'y', 'z'], '|')} ${map(range(1, 4), (n) => html`${n}`)}</span>`;
    }
}
customElements.define('joinrange-proof', JoinRangeProof);
"#,
        "joinrange-proof",
    );
    assert!(screen.contains("x|y|z 123"), "{screen}");
}

#[test]
fn keyed_guard_cache_and_live_pass_their_values_through() {
    let screen = screen_of(
        r#"
import { html, LitElement } from 'lit';
import { keyed } from 'lit/directives/keyed.js';
import { guard } from 'lit/directives/guard.js';
import { cache } from 'lit/directives/cache.js';
import { live } from 'lit/directives/live.js';

class PassProof extends LitElement {
    render() {
        return html`<span>${keyed('k', 'kv')} ${guard(['d'], () => 'gv')} ${cache('cv')} ${live('lv')}</span>`;
    }
}
customElements.define('pass-proof', PassProof);
"#,
        "pass-proof",
    );
    assert!(screen.contains("kv gv cv lv"), "{screen}");
}

#[test]
fn style_map_serializes_declarations() {
    // The terminal cascade ignores inline style, but the attribute must
    // serialize faithfully for the web target and the DOM.
    let mut host = JsHost::new().unwrap();
    host.load_module(
        "test:directive",
        r#"
import { html, LitElement } from 'lit';
import { styleMap } from 'lit/directives/style-map.js';

class StyleProof extends LitElement {
    render() {
        return html`<div style="${styleMap({ backgroundColor: 'red', '--gap': '2px', skipped: undefined })}">s</div>`;
    }
}
customElements.define('style-proof', StyleProof);
"#,
    )
    .unwrap();
    let node = host.mount("style-proof", &[]).unwrap();
    let state = host.state.borrow();
    let styled = state
        .doc
        .descendants(node)
        .find(|&n| state.doc.attribute(n, "style").is_some())
        .expect("styled node");
    assert_eq!(
        state.doc.attribute(styled, "style"),
        Some("background-color: red; --gap: 2px")
    );
}

#[test]
fn a_missing_module_names_itself_and_the_provided_set() {
    let mut host = JsHost::new().unwrap();
    let err = host
        .load_module(
            "test:gap",
            "import { until } from 'lit/directives/until.js';\nexport const x = until;",
        )
        .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("lit/directives/until.js"),
        "names the gap: {message}"
    );
    assert!(
        message.contains("lit-html/directives/repeat.js") && message.contains("js/src"),
        "lists the provided modules and where extensions live: {message}"
    );
}
