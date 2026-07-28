// One row of the list, rendered from its attributes — a parent re-commit
// swaps rows in fresh, so composition data arrives as attributes.
//
// Light DOM: the browser styles the row through the page's Bootstrap; the
// terminal adopts `static styles` at define (real lit never applies them
// without a shadow root) plus the mapped Bootstrap subset.
import { css, html, LitElement } from 'lit';
import { classMap } from 'lit/directives/class-map.js';
import { live } from 'lit/directives/live.js';
import { terminalTheme } from './theme.js';

// Editing swaps the label for a real input — the browser's native caret
// and outline, the terminal's rat widget twin by element type. The parent
// hears its bubbling `input` events and mirrors the text into the row.

export class TodoItem extends LitElement {
    static properties = {
        text: {},
        done: { type: Boolean },
        editing: { type: Boolean },
    };

    // The terminal's look: the .check span IS the checkbox there — a real
    // element, so its clicks hit-test apart from the row — plus the label
    // colors. The browser hides .check via page.css and shows the real
    // checkbox instead; checkbox and button stay browser-only, while the
    // EDIT input is real in both hosts (its rat widget draws the caret).
    static styles = [
        terminalTheme,
        css`
            .check {
                color: var(--tui-info);
            }
            .done .check {
                color: var(--tui-ok);
            }
            .todo-row.done .label {
                color: var(--tui-muted);
            }
            .todo-row.editing .label {
                color: var(--tui-accent);
            }
            input.form-check-input,
            button {
                display: none;
            }
        `,
    ];

    declare text: string;
    declare done: boolean;
    declare editing: boolean;

    constructor() {
        super();
        this.text = '';
        this.done = false;
        this.editing = false;
    }

    createRenderRoot(): this {
        return this;
    }

    // Checkbox and delete carry tabindex="-1" deliberately: the app-level
    // key routing owns their actions (Space toggles, Delete removes), and
    // the app element is the one focus stop of the list.
    render() {
        return html`<div
            class="todo-row d-flex align-items-center gap-2 ${classMap({
                done: this.done,
                editing: this.editing,
            })}"
        >
            <input
                class="form-check-input mt-0 flex-shrink-0"
                type="checkbox"
                tabindex="-1"
                .checked=${live(this.done)}
            />
            <span class="check flex-shrink-0">${this.done ? '[x]' : '[ ]'}</span>
            ${this.editing
                ? html`<input
                      class="form-control form-control-sm label flex-grow-1"
                      type="text"
                      data-path="edit"
                      .value=${live(this.text)}
                  />`
                : html`<span
                      class="label flex-grow-1 ${classMap({
                          'text-decoration-line-through': this.done,
                          'text-body-tertiary': this.done,
                      })}"
                      >${this.text}</span
                  >`}
            <button
                class="btn-close flex-shrink-0"
                type="button"
                tabindex="-1"
                aria-label="Delete"
            ></button>
        </div>`;
    }

    // The edit input takes the keyboard when it appears. In the browser it
    // renders once per edit; under the mocked lit every parent commit
    // rebuilds the row, so the `:focus` guard keeps this a no-op while the
    // focus survival (by data-path) already resolved onto the fresh node.
    updated(): void {
        if (this.editing) {
            const input = this.renderRoot.querySelector('input.label') as HTMLElement | null;
            if (input && !input.matches(':focus')) {
                input.focus();
            }
        }
    }
}
customElements.define('todo-item', TodoItem);
