// One row of the list, rendered from its attributes — a parent re-commit
// swaps rows in fresh, so composition data arrives as attributes.
//
// Light DOM: the browser styles the row through the page's Bootstrap; the
// terminal adopts `static styles` at define (real lit never applies them
// without a shadow root) plus the mapped Bootstrap subset.
import { css, html, LitElement } from 'lit';
import { classMap } from 'lit/directives/class-map.js';
import { live } from 'lit/directives/live.js';

export class TodoItem extends LitElement {
    static properties = {
        text: {},
        done: { type: Boolean },
        editing: { type: Boolean },
    };

    // The terminal's look: the .check span IS the checkbox there — a real
    // element, so its clicks hit-test apart from the row — plus the label
    // and cursor colors. The browser hides .check via page.css and shows
    // the real input instead; input and button stay browser-only.
    static styles = css`
        .check {
            color: #6fb3d2;
        }
        .done .check {
            color: #a3be8c;
        }
        .todo-row.done .label {
            color: #808a93;
        }
        .todo-row.editing .label {
            color: #e5c07b;
        }
        .cursor {
            color: #e5c07b;
            font-weight: bold;
        }
        input,
        button {
            display: none;
        }
    `;

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
            <span
                class="label flex-grow-1 ${classMap({
                    'text-decoration-line-through': this.done,
                    'text-body-tertiary': this.done,
                })}"
                >${this.text}${this.editing ? html`<span class="cursor">▏</span>` : ''}</span
            >
            <button
                class="btn-close flex-shrink-0"
                type="button"
                tabindex="-1"
                aria-label="Delete"
            ></button>
        </div>`;
    }
}
customElements.define('todo-item', TodoItem);
