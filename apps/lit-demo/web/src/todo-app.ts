// The state owner: rows, draft and selection live here, children render
// from attributes. Text entry is a plain `<input type="text">` (the
// browser gives it the native caret, selection and focus outline, the
// terminal mounts its rat widget twin by element type) and the bubbling
// `input` event carries typing into the state. The host keydown listener
// keeps the list chrome: Enter, arrows, and Space/Delete while no input
// holds text.
import { css, html, LitElement } from 'lit';
import { classMap } from 'lit/directives/class-map.js';
import { live } from 'lit/directives/live.js';
import { repeat } from 'lit/directives/repeat.js';
import { terminalTheme } from './theme.js';
import './todo-item.js';

interface TodoRow {
    id: number;
    text: string;
    done: boolean;
}

/** The reactive properties a sync harness mirrors: the app's one state
 * contract, imported by every page that attaches a wire. Rust twin:
 * `STATE_FIELDS` in the demo's main.rs. */
export const STATE_FIELDS: string[] = ['draft', 'editing', 'items', 'selected'];

// Where the rows live between runs: the browser's own localStorage and
// the terminal's storage feature speak the same API.
const STORE_KEY = 'uic-todos';

export class TodoApp extends LitElement {
    static properties = {
        items: { state: true },
        draft: { state: true },
        selected: { state: true },
        editing: { state: true },
    };

    // The terminal's look: the mapped Bootstrap subset draws the card and
    // the active row; these rules add what the map leaves unset.
    static styles = [
        terminalTheme,
        css`
            :host {
                display: block;
            }
            todo-item {
                display: block;
            }
            .prompt,
            .draft {
                color: var(--tui-accent);
            }
            .card-footer {
                color: var(--tui-muted);
            }
        `,
    ];

    declare items: TodoRow[];
    declare draft: string;
    declare selected: number;
    declare editing: number;

    // Transient drag source: plain field, neither reactive nor synced.
    private dragFrom = -1;

    // The last snapshot written to storage. The mocked lit calls updated
    // with an empty map, so saves dedupe by content, not by changed keys.
    private lastSaved: string | null = null;

    constructor() {
        super();
        this.items = this.loadItems() ?? [
            { id: 1, text: 'render a web app in the terminal', done: true },
            { id: 2, text: 'type a todo of your own', done: false },
        ];
        this.draft = '';
        this.selected = 0;
        this.editing = -1;
        this.addEventListener('keydown', (event: KeyboardEvent) => this.onKey(event));
        this.addEventListener('input', (event: Event) => this.onInput(event));
        this.addEventListener('click', (event: Event) => this.onBackgroundClick(event));
    }

    createRenderRoot(): this {
        return this;
    }

    firstUpdated(): void {
        this.focusDraft();
    }

    focusDraft(): void {
        const draft = this.renderRoot.querySelector('input.draft') as HTMLElement | null;
        draft?.focus();
    }

    // The announcement the live bridge listens to, a plain Event carrying
    // nothing: the snapshot is read off the properties. The terminal's
    // mocked lit calls updated with an empty map, so the guard keeps Boa
    // off the (unmocked) dispatchEvent path; there the host reads state
    // directly.
    updated(changed: Map<string, unknown>): void {
        if (STATE_FIELDS.some((name) => changed.has(name))) {
            this.dispatchEvent(new Event('state-changed'));
        }
        this.persist();
    }

    // Rows persisted by an earlier run. A stored empty list is honored;
    // only absence or garbage falls back to the seed rows. On connect the
    // lexical greet winner still overwrites (ADR 0013) and both ends then
    // persist the winning list: the pairing decides, storage remembers.
    private loadItems(): TodoRow[] | null {
        if (typeof localStorage === 'undefined') {
            return null;
        }
        const raw = localStorage.getItem(STORE_KEY);
        if (raw === null) {
            return null;
        }
        try {
            const rows = JSON.parse(raw);
            if (!Array.isArray(rows)) {
                return null;
            }
            this.lastSaved = raw;
            return rows;
        } catch {
            return null;
        }
    }

    // Items only: resurrecting a half-typed draft, a selection or an open
    // edit would be stranger than losing them.
    private persist(): void {
        if (typeof localStorage === 'undefined') {
            return;
        }
        const snapshot = JSON.stringify(this.items);
        if (snapshot === this.lastSaved) {
            return;
        }
        try {
            localStorage.setItem(STORE_KEY, snapshot);
            this.lastSaved = snapshot;
        } catch {
            // A refused write (quota, disk) keeps the previous snapshot;
            // the next change retries.
        }
    }

    // The list chrome. Editing keys never land here (the focused input
    // consumes them natively) and Space/Delete stay editing keys unless
    // no text is in play (the cancelable-keydown contract: preventDefault
    // suppresses the input's default action in both hosts).
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
        } else if (key === 'Delete') {
            if (this.editing >= 0 || this.draft.length > 0 || this.items.length === 0) {
                return;
            }
            this.removeAt(this.selected);
        } else if (key === ' ') {
            if (this.editing >= 0 || this.draft.length > 0) {
                return;
            }
            this.toggle(this.selected);
        } else if (key === 'ArrowDown' && event.shiftKey) {
            if (this.editing >= 0) {
                return;
            }
            this.moveSelected(1);
        } else if (key === 'ArrowUp' && event.shiftKey) {
            if (this.editing >= 0) {
                return;
            }
            this.moveSelected(-1);
        } else if (key === 'ArrowDown') {
            if (this.editing >= 0) {
                return;
            }
            this.selected = Math.min(this.selected + 1, this.items.length - 1);
        } else if (key === 'ArrowUp') {
            if (this.editing >= 0) {
                return;
            }
            this.selected = Math.max(this.selected - 1, 0);
        } else {
            // A key the chrome does not consume (Tab above all) keeps its
            // default: cancel only what was acted on.
            return;
        }
        event.preventDefault();
    }

    // Typing lands in a real input; its bubbling `input` event carries the
    // live text into the state: the draft box or the row being edited.
    onInput(event: Event): void {
        const target = event.target as (Element & { value?: unknown }) | null;
        if (!target || typeof target.closest !== 'function') {
            return;
        }
        const value = String(target.value ?? '');
        if (target.closest('todo-item')) {
            if (this.editing >= 0 && target.matches('.label')) {
                this.editText(value);
            }
        } else if (target.matches('.draft')) {
            this.draft = value;
        }
    }

    // A click away from the row being edited finishes the edit; a click
    // inside it only places the caret.
    onBackgroundClick(event: Event): void {
        if (this.editing < 0) {
            return;
        }
        const target = event.target as Element | null;
        if (target && typeof target.closest === 'function') {
            const row = target.closest('todo-item');
            if (row && Number(row.getAttribute('data-index')) === this.editing) {
                return;
            }
        }
        this.finishEdit();
    }

    // The pointer model: only checkbox interaction toggles (the input in
    // the browser, the .check span in the terminal; closest, not matches:
    // the terminal's hit test lands on text nodes), a plain click selects,
    // and a double click opens the row for editing.
    onItemClick(event: Event): void {
        const index = Number((event.currentTarget as Element).getAttribute('data-index'));
        this.selected = index;
        const target = event.target as Element;
        if (!target || typeof target.closest !== 'function') {
            return;
        }
        if (target.closest('.btn-close')) {
            this.removeAt(index);
        } else if (target.closest('.form-check-input, .check')) {
            // No preventDefault: the native checkbox may pre-toggle, and
            // the live()-bound render reconciles it to the state either way.
            this.toggle(index);
        }
    }

    onItemDblClick(event: Event): void {
        const target = event.target as Element;
        if (target && typeof target.closest === 'function' && target.closest('.form-check-input, .check, .btn-close')) {
            return;
        }
        const index = Number((event.currentTarget as Element).getAttribute('data-index'));
        this.selected = index;
        this.editing = index;
    }

    // The one reorder primitive; Shift+arrows and drag both land here: the
    // row moves and the selection follows it.
    private moveItem(from: number, to: number): void {
        const items = this.items.slice();
        const moved = items.splice(from, 1)[0];
        items.splice(to, 0, moved);
        this.items = items;
        this.selected = to;
    }

    moveSelected(delta: number): void {
        const to = this.selected + delta;
        if (to < 0 || to >= this.items.length) {
            return;
        }
        this.moveItem(this.selected, to);
    }

    onDragStart(event: DragEvent): void {
        if (this.editing >= 0) {
            this.finishEdit();
        }
        const index = Number((event.currentTarget as Element).getAttribute('data-index'));
        this.dragFrom = index;
        this.selected = index;
        event.dataTransfer?.setData('text/plain', String(index));
        if (event.dataTransfer) {
            event.dataTransfer.effectAllowed = 'move';
        }
    }

    // Rows reorder live while the drag hovers them; remote mirrors watch
    // the move happen.
    onDragOver(event: DragEvent): void {
        event.preventDefault();
        const over = Number((event.currentTarget as Element).getAttribute('data-index'));
        if (this.dragFrom < 0 || over === this.dragFrom) {
            return;
        }
        this.moveItem(this.dragFrom, over);
        this.dragFrom = over;
    }

    onDrop(event: DragEvent): void {
        event.preventDefault();
        this.dragFrom = -1;
    }

    onDragEnd(): void {
        this.dragFrom = -1;
    }

    toggle(index: number): void {
        this.items = this.items.map((item, at) =>
            at === index ? { ...item, done: !item.done } : item,
        );
    }

    itemText(index: number): string {
        const item = this.items[index];
        return item ? item.text : '';
    }

    // Edits land in the row as they are typed; remote mirrors watch the
    // text change letter by letter.
    editText(text: string): void {
        const index = this.editing;
        this.items = this.items.map((item, at) => (at === index ? { ...item, text } : item));
    }

    finishEdit(): void {
        if (this.editing < 0) {
            return;
        }
        const index = this.editing;
        this.editing = -1;
        if (this.itemText(index).length === 0) {
            this.removeAt(index);
        }
        // The keyboard returns to the entry row; in both hosts the edit
        // input just unrendered, taking the focus down with it.
        this.focusDraft();
    }

    removeAt(index: number): void {
        this.items = this.items.filter((item, at) => at !== index);
        this.selected = Math.min(this.selected, Math.max(this.items.length - 1, 0));
        if (this.editing === index) {
            this.editing = -1;
        } else if (this.editing > index) {
            this.editing -= 1;
        }
    }

    render() {
        const remaining = this.items.filter((item) => !item.done).length;
        return html`
            <div class="card shadow-sm">
                <div class="card-header">todos</div>
                <ul class="list-group list-group-flush">
                    ${repeat(
                        this.items,
                        (item) => item.id,
                        (item, index) => html`<todo-item
                            class="list-group-item list-group-item-action ${classMap({
                                active: index === this.selected,
                            })}"
                            data-index="${index}"
                            text="${item.text}"
                            ?done=${item.done}
                            ?editing=${index === this.editing}
                            draggable="true"
                            @click=${this.onItemClick}
                            @dblclick=${this.onItemDblClick}
                            @dragstart=${this.onDragStart}
                            @dragover=${this.onDragOver}
                            @drop=${this.onDrop}
                            @dragend=${this.onDragEnd}
                        ></todo-item>`,
                    )}
                </ul>
                <div class="card-body d-flex align-items-center gap-2 font-monospace">
                    <span class="prompt">&gt;</span>
                    <input
                        class="form-control form-control-sm draft"
                        type="text"
                        data-path="draft"
                        placeholder="type to add…"
                        .value=${live(this.draft)}
                    />
                </div>
                <div class="card-footer small">
                    ${remaining} remaining · type + Enter adds · checkbox/Space toggles ·
                    double-click/Enter edits · click selects · Shift+arrows reorder ·
                    Delete removes
                </div>
            </div>
        `;
    }
}
customElements.define('todo-app', TodoApp);
