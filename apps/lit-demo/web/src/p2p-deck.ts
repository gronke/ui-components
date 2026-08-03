// The terminal's p2p arrangement (ADR 0029), pairing-first: the pairing
// panel is the boot screen, and the navbar + todo take over once a wire
// stands. The QR docks to the stack's right on a wide terminal, wrapping
// below on a narrow one: real flexbox (the taffy layout) driven by this
// element's terminal-only styles, no host rect math. Only the terminal
// mounts it; the browser page composes its own document.
//
// The deck deliberately has no reactive properties: it renders exactly once,
// so a re-commit can never swap the todo app's live state away. The screens
// gate through the plain wrapper divs instead: the host toggles their
// `hidden` attribute by node, which no component display rule can outrank
// (a component's `:host { display }` would beat `[hidden]` in the cascade).
// It also imports none of the composed elements on purpose: the terminal
// host loads those modules explicitly before mounting, and an import here
// would drag the pairing UI into the browser's todo-app graph.
import { css, html, LitElement } from 'lit';

export class P2pDeck extends LitElement {
    // The stack's width is the flex basis the wrap decision reads: beside
    // the ~87-column QR of a published invite link and the 2-column gap,
    // 111ch puts the break at about 200 terminal columns. min-width lets
    // the stack shrink under its unbreakable link text on narrow terminals
    // (the text clips at the card edge, as it always has). The bar takes
    // its own full-width wrap row above everything.
    static styles = css`
        :host {
            display: flex;
            flex-wrap: wrap;
            gap: 1ch 2ch;
        }
        .bar {
            width: 100%;
        }
        .stack {
            flex-grow: 1;
            width: 111ch;
            min-width: 0;
        }
    `;

    createRenderRoot(): this {
        return this;
    }

    render() {
        return html`<div class="bar" hidden>
                <status-navbar data-bs-theme="dark"></status-navbar>
            </div>
            <div class="stack">
                <div class="todo-pane" hidden><todo-app data-bs-theme="dark"></todo-app></div>
                <div class="pairing-pane"><pair-panel data-bs-theme="dark"></pair-panel></div>
            </div>
            <qr-code></qr-code>`;
    }
}
customElements.define('p2p-deck', P2pDeck);
