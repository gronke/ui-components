// One row of the list, rendered from its attributes — a parent re-commit
// swaps rows in fresh, so composition data arrives as attributes.
//
// Light DOM: the browser styles the row through the page's Bootstrap; the
// terminal adopts `static styles` at define (real lit never applies them
// without a shadow root) plus the mapped Bootstrap subset.
import { css, html, LitElement } from 'lit';
import { classMap } from 'lit/directives/class-map.js';

export class TodoItem extends LitElement {
    static properties = {
        text: {},
        done: { type: Boolean },
        editing: { type: Boolean },
    };

    // The terminal's look: checkbox markers as generated content, the
    // editing cursor, and no badge — the marker already says done.
    static styles = css`
        .todo-row::before {
            content: '[ ] ';
            color: #6fb3d2;
        }
        .todo-row.done::before {
            content: '[x] ';
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
        .badge {
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
            class="todo-row d-flex justify-content-between align-items-center ${classMap({
                done: this.done,
                editing: this.editing,
            })}"
        >
            <span
                class="label ${classMap({
                    'text-decoration-line-through': this.done,
                    'text-body-tertiary': this.done,
                })}"
                >${this.text}</span
            >${this.editing ? html`<span class="cursor">▏</span>` : ''}
            ${this.done ? html`<span class="badge text-bg-success">done</span>` : ''}
        </div>`;
    }
}
customElements.define('todo-item', TodoItem);
