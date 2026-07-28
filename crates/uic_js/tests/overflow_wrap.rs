//! `overflow-wrap: anywhere` under the scripted host: an unbreakable token
//! (a long URL) wraps across lines instead of pinning its box one clipped
//! line wide — min-content drops to one cell, and the existing height count
//! and paint break the word (they always could; the measurement was the
//! blocker).

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;

// Flex containers are where min-content decides (a definite block width
// always wrapped): a flex item cannot shrink below its min-content, so an
// unbreakable token pins its item wider than the screen — unless
// overflow-wrap drops the minimum to one cell.
const CARDS: &str = r#"
import { css, html, LitElement } from 'lit';

class WrapCard extends LitElement {
    static styles = css`
        .row {
            display: flex;
        }
        .wrapped {
            overflow-wrap: anywhere;
        }
    `;

    createRenderRoot() {
        return this;
    }

    render() {
        return html`<div class="row">
                <p class="wrapped">AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHH</p>
            </div>
            <div class="row">
                <p class="plain">MMMMNNNNOOOOPPPPQQQQRRRRSSSSTTTT</p>
            </div>`;
    }
}

customElements.define('wrap-card', WrapCard);
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
fn an_unbreakable_token_wraps_under_overflow_wrap_anywhere() {
    let mut host = JsHost::new().unwrap();
    host.load_module("test:wrap", CARDS).unwrap();
    host.mount("wrap-card", &[]).unwrap();

    // 20 columns: the 32-char tokens cannot fit one line.
    let mut terminal = Terminal::new(TestBackend::new(20, 8)).unwrap();
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let doc = &mut state.borrow_mut().doc;
            uic_tui::dom::paint_document(frame, frame.area(), doc, None);
        })
        .unwrap();

    let screen = screen_text(&terminal);
    // The wrapped token continues past the first row: its tail chunk is on
    // screen even though the head filled a full line.
    assert!(
        screen.contains("AAAABBBB"),
        "the token's head paints:\n{screen}"
    );
    assert!(
        screen.contains("GGGGHHHH"),
        "the token's tail wraps onto a later row:\n{screen}"
    );
    // The plain flex item keeps the old behavior: min-content pins it wider
    // than the screen, one line, clipped.
    assert!(
        screen.contains("MMMMNNNN"),
        "the plain token's head paints:\n{screen}"
    );
    assert!(
        !screen.contains("SSSSTTTT"),
        "the plain token must stay a single clipped line:\n{screen}"
    );
}
