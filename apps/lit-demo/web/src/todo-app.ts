// The state owner: rows, draft and selection live here, children render
// from attributes. Keyboard input is a plain keydown listener on the host
// element — the byte-identical path in the browser and the terminal — and
// template event values are method references (the terminal host binds
// them; see crates/uic_js/README.md, Runtime mechanics).
import { css, html, LitElement } from 'lit';
import { classMap } from 'lit/directives/class-map.js';
import { repeat } from 'lit/directives/repeat.js';
import './todo-item.js';

interface TodoRow {
    id: number;
    text: string;
    done: boolean;
}

export class TodoApp extends LitElement {
    static properties = {
        items: { state: true },
        draft: { state: true },
        selected: { state: true },
        editing: { state: true },
    };

    static styles = css`
        :host {
            display: block;
            padding: 1rem;
        }
        .title {
            font-weight: bold;
            color: #e5c07b;
        }
        todo-item {
            display: block;
        }
        todo-item.selected {
            background-color: #26415e;
        }
        .draft {
            color: #e5c07b;
        }
        .count {
            color: #808a93;
        }
    `;

    declare items: TodoRow[];
    declare draft: string;
    declare selected: number;
    declare editing: number;

    constructor() {
        super();
        this.items = [
            { id: 1, text: 'render a web app in the terminal', done: true },
            { id: 2, text: 'type a todo of your own', done: false },
        ];
        this.draft = '';
        this.selected = 0;
        this.editing = -1;
        this.addEventListener('keydown', (event: KeyboardEvent) => this.onKey(event));
    }

    connectedCallback(): void {
        super.connectedCallback();
        if (this.tabIndex < 0) {
            this.tabIndex = 0;
        }
        this.focus();
    }

    onKey(event: KeyboardEvent): void {
        const key = event.key;
        if (key === 'Enter') {
            if (this.editing >= 0) {
                this.finishEdit();
            } else if (this.draft.length > 0) {
                const id = this.items.reduce((max, item) => Math.max(max, item.id), 0) + 1;
                this.items = this.items.concat([{ id, text: this.draft, done: false }]);
                this.draft = '';
                this.selected = this.items.length - 1;
            } else if (this.items.length > 0) {
                this.editing = this.selected;
            }
        } else if (key === 'Backspace') {
            if (this.editing >= 0) {
                this.editText(this.itemText(this.editing).slice(0, -1));
            } else {
                this.draft = this.draft.slice(0, -1);
            }
        } else if (key === 'ArrowDown') {
            if (this.editing < 0) {
                this.selected = Math.min(this.selected + 1, this.items.length - 1);
            }
        } else if (key === 'ArrowUp') {
            if (this.editing < 0) {
                this.selected = Math.max(this.selected - 1, 0);
            }
        } else if (key.length === 1) {
            if (this.editing >= 0) {
                this.editText(this.itemText(this.editing) + key);
            } else if (key === ' ' && this.draft.length === 0) {
                this.toggle(this.selected);
            } else {
                this.draft = this.draft + key;
            }
        } else {
            return;
        }
        event.preventDefault();
    }

    onItemClick(event: Event): void {
        if (this.editing >= 0) {
            this.finishEdit();
        }
        const index = Number((event.currentTarget as Element).getAttribute('data-index'));
        this.selected = index;
        this.toggle(index);
    }

    toggle(index: number): void {
        this.items = this.items.map((item, at) =>
            at === index ? { id: item.id, text: item.text, done: !item.done } : item,
        );
    }

    itemText(index: number): string {
        const item = this.items[index];
        return item ? item.text : '';
    }

    // Edits land in the row as they are typed — remote mirrors watch the
    // text change letter by letter.
    editText(text: string): void {
        const index = this.editing;
        this.items = this.items.map((item, at) =>
            at === index ? { id: item.id, text, done: item.done } : item,
        );
    }

    finishEdit(): void {
        const index = this.editing;
        this.editing = -1;
        if (this.itemText(index).length === 0) {
            this.items = this.items.filter((item, at) => at !== index);
            this.selected = Math.min(this.selected, Math.max(this.items.length - 1, 0));
        }
    }

    render() {
        const remaining = this.items.filter((item) => !item.done).length;
        return html`
            <p class="title">todos</p>
            ${repeat(
                this.items,
                (item) => item.id,
                (item, index) => html`<todo-item
                    class="${classMap({ selected: index === this.selected })}"
                    data-index="${index}"
                    text="${item.text}"
                    ?done=${item.done}
                    ?editing=${index === this.editing}
                    @click=${this.onItemClick}
                ></todo-item>`,
            )}
            <p class="draft">&gt; ${this.draft}</p>
            <p class="count">
                ${remaining} remaining · type + Enter adds · Space toggles · Enter edits ·
                arrows select · click toggles
            </p>
        `;
    }
}
customElements.define('todo-app', TodoApp);
