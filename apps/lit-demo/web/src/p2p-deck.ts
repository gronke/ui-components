// The terminal's p2p arrangement (ADR 0029): the todo card and the pairing
// panel stack in one column, and the QR docks to their right on a wide
// terminal, wrapping below the panes on a narrow one — real flexbox (the
// taffy layout) driven by this element's terminal-only styles, no host rect
// math. Only the terminal mounts it; the browser page composes its own
// document and keeps the QR inside the panel.
//
// The deck deliberately has no reactive properties: it renders exactly once,
// so a re-commit can never swap the todo app's live state away. The hosts
// drive the nested elements directly by node. It also imports neither
// `./pair-panel.js` nor `./qr-code.js` on purpose — the terminal host loads
// those modules explicitly before mounting, and an import here would drag
// the pairing UI into the browser's todo-app graph.
import { css, html, LitElement } from 'lit';

export class P2pDeck extends LitElement {
    // The stack's width is the flex basis the wrap decision reads: beside
    // the ~87-column QR of a published invite link and the 2-column gap,
    // 111ch puts the break at about 200 terminal columns. min-width lets
    // the stack shrink under its unbreakable link text on narrow terminals
    // (the text clips at the card edge, as it always has).
    static styles = css`
        :host {
            display: flex;
            flex-wrap: wrap;
            gap: 1ch 2ch;
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
        return html`<div class="stack">
                <todo-app data-bs-theme="dark"></todo-app>
                <pair-panel data-bs-theme="dark"></pair-panel>
            </div>
            <qr-code></qr-code>`;
    }
}
customElements.define('p2p-deck', P2pDeck);
