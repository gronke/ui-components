// One row of the list, rendered from its attributes — a parent re-commit
// swaps rows in fresh, so composition data arrives as attributes.
import { css, html, LitElement } from 'lit';
import { classMap } from 'lit/directives/class-map.js';

export class TodoItem extends LitElement {
    static properties = {
        text: {},
        done: { type: Boolean },
        editing: { type: Boolean },
    };

    static styles = css`
        .row::before {
            content: '[ ] ';
            color: #6fb3d2;
        }
        .row.done::before {
            content: '[x] ';
            color: #a3be8c;
        }
        .row.done .label {
            text-decoration: line-through;
            color: #808a93;
        }
        .row.editing .label {
            color: #e5c07b;
        }
        .cursor {
            color: #e5c07b;
            font-weight: bold;
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

    render() {
        return html`<div class="row ${classMap({ done: this.done, editing: this.editing })}">
            <span class="label">${this.text}</span>${this.editing ? html`<span class="cursor">▏</span>` : ''}
        </div>`;
    }
}
customElements.define('todo-item', TodoItem);
