// The connected screen's chrome, shared by both hosts: brand, the
// connection badge, the address the wire stands on, and the disconnect
// control that returns to the pairing screen. Presentation only, the
// pair-panel mold — properties in, intent out through the polled `command`
// property (plus the guarded CustomEvent where the platform has events).
// justify-content never reaches the terminal's layout, so the right edge
// rides a flex-grow spacer instead.
import { css, html, LitElement } from 'lit';
import { terminalTheme } from './theme.js';

export class StatusNavbar extends LitElement {
    static properties = {
        connected: {},
        status: {},
        address: {},
        hint: {},
        command: {},
    };

    // The terminal's look: the badge chip comes from the mapped Bootstrap
    // subset; these rules add the brand accent and mute the periphery.
    static styles = [
        terminalTheme,
        css`
            :host {
                display: block;
            }
            .brand {
                color: var(--tui-accent);
                font-weight: bold;
            }
            .address,
            .hint {
                color: var(--tui-muted);
            }
        `,
    ];

    declare connected: boolean | null;
    declare status: string;
    declare address: string;
    declare hint: string;
    declare command: string | null;

    constructor() {
        super();
        this.connected = null;
        this.status = '';
        this.address = '';
        this.hint = '';
        this.command = null;
    }

    createRenderRoot(): this {
        return this;
    }

    // The pair-panel seam: a click writes the polled property, and — where
    // the platform has events — dispatches the same intent so a browser
    // controller can just listen.
    private emit(command: string): void {
        this.command = command;
        if (typeof CustomEvent === 'function' && typeof this.dispatchEvent === 'function') {
            this.dispatchEvent(new CustomEvent(command, { bubbles: true }));
        }
    }

    private onDisconnect(): void {
        this.emit('disconnect');
    }

    render() {
        return html`<nav class="d-flex align-items-center gap-2">
            <span class="brand">lit-todo p2p</span>
            ${this.connected === null
                ? ''
                : html`<span class="badge ${this.connected ? 'text-bg-success' : 'text-bg-danger'}"
                      >${this.connected ? 'connected' : 'disconnected'}</span
                  >`}
            <span class="address">${this.address}</span>
            <span class="status text-body-secondary">${this.status}</span>
            <span class="flex-grow-1"></span>
            ${this.hint ? html`<span class="hint">${this.hint}</span>` : ''}
            <button class="disconnect btn btn-outline-secondary btn-sm" @click=${this.onDisconnect}>
                disconnect
            </button>
        </nav>`;
    }
}
customElements.define('status-navbar', StatusNavbar);
