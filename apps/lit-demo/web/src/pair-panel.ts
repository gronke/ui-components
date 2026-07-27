// The pairing UI, shared by both hosts. This element is presentation only —
// the invite link, the copyable token, the peer-paste box, the buttons, the
// status line and the connection badge — driven entirely by properties a
// host sets and signalling intent back through the `command` property (a
// button click writes it; the host reads and clears it). The browser also
// gets a real CustomEvent for the same intents, but the property is the
// channel that works under the mocked terminal lit, which has no events.
//
// The transport (WebRTC, the clipboard, the camera) is NOT here — each host
// owns it: the browser's pair-wizard, the terminal's Rust peer. The QR is
// the shared <qr-code> the invite body renders (ADR 0030): the browser draws
// an SVG, the terminal a native block-char widget — one element, two
// renderers, the same reason todo-item keeps a `[x]` span twin.
import { css, html, LitElement } from 'lit';
import { live } from 'lit/directives/live.js';
import './qr-code.js';

export class PairPanel extends LitElement {
    static properties = {
        mode: {},
        link: {},
        token: {},
        status: {},
        connected: {},
        resetLabel: {},
        canScan: { type: Boolean },
        command: {},
        peer: {},
    };

    // The terminal's look: the mapped Bootstrap subset draws the card and
    // the badge; these rules add what the map leaves unset. The inline QR
    // hides here — static styles are the terminal-only layer, and the
    // terminal's p2p deck shows the QR beside the panes instead (ADR 0030);
    // the browser ignores these rules and keeps the QR inside the card.
    static styles = css`
        :host {
            display: block;
        }
        .card-header {
            font-weight: bold;
            color: #e5c07b;
        }
        .status {
            color: #808a93;
        }
        .copy-link {
            color: #6fb3d2;
        }
        .qr-lead,
        qr-code {
            display: none;
        }
    `;

    declare mode: 'idle' | 'invite' | 'answering' | 'connected' | 'dropped' | 'failed';
    declare link: string;
    declare token: string;
    declare status: string;
    declare connected: boolean | null;
    declare resetLabel: string;
    declare canScan: boolean;
    declare command: string | null;
    declare peer: string;

    constructor() {
        super();
        this.mode = 'idle';
        this.link = '';
        this.token = '';
        this.status = '';
        this.connected = null;
        this.resetLabel = '';
        this.canScan = false;
        this.command = null;
        this.peer = '';
    }

    createRenderRoot(): this {
        return this;
    }

    /** The one seam back to the host: a click sets the command property the
     * host polls, and — where the platform has events — dispatches the same
     * intent so a browser controller can just listen. The terminal reads
     * the property and clears it; the browser uses the event. */
    private emit(command: string): void {
        this.command = command;
        if (typeof CustomEvent === 'function' && typeof this.dispatchEvent === 'function') {
            this.dispatchEvent(new CustomEvent(command, { detail: this.peer, bubbles: true }));
        }
    }

    private onInvite(): void {
        this.emit('invite');
    }

    private onReset(): void {
        this.emit('reset');
    }

    private onPeerInput(event: Event): void {
        this.peer = String((event.target as HTMLTextAreaElement).value ?? '');
    }

    private onConnect(): void {
        this.emit('connect');
    }

    private onCopyLink(event: Event): void {
        // The link is for copying — navigating a waiting tab into a payload
        // would tear the session it belongs to.
        event.preventDefault();
        this.emit('copy-link');
    }

    private onCopyToken(): void {
        this.emit('copy-token');
    }

    private onScan(): void {
        this.emit('scan');
    }

    render() {
        return html`<section class="card">
            <div class="card-header d-flex align-items-center justify-content-between">
                pair another browser
                ${this.connected === null
                    ? ''
                    : html`<span
                          class="badge ${this.connected ? 'text-bg-success' : 'text-bg-danger'}"
                          >${this.connected ? 'connected' : 'disconnected'}</span
                      >`}
            </div>
            <div class="card-body">
                <p class="status text-body-secondary">${this.status}</p>
                ${this.renderBody()}
                ${this.resetLabel
                    ? html`<button class="btn btn-outline-secondary d-block mt-3" @click=${this.onReset}>
                          ${this.resetLabel}
                      </button>`
                    : ''}
            </div>
        </section>`;
    }

    private renderBody() {
        switch (this.mode) {
            case 'idle':
                return html`<button class="btn btn-primary" @click=${this.onInvite}>
                    create an invite
                </button>`;
            case 'invite':
                // Pairing is a mutual exchange (ADR 0031): both halves are
                // first-class — share your invite, and open theirs. The QR
                // trails the controls in the browser card; the terminal hides
                // this inline copy and docks the QR beside the panes instead
                // (the p2p deck, ADR 0030).
                return html`<h3 class="h6">share your invite</h3>
                    <a
                        class="copy-link"
                        href=${this.link}
                        title="click to copy"
                        @click=${this.onCopyLink}
                        >${this.link}</a
                    >
                    <textarea
                        name="token"
                        class="form-control font-monospace mt-2"
                        readonly
                        title="click to copy"
                        .value=${live(this.token)}
                        @click=${this.onCopyToken}
                    ></textarea>
                    <h3 class="h6 mt-3">open their invite</h3>
                    ${this.canScan
                        ? html`<button class="btn btn-outline-secondary mb-2" @click=${this.onScan}>
                              scan their code
                          </button>`
                        : ''}
                    <textarea
                        name="peer"
                        class="peer-input form-control font-monospace"
                        placeholder="their link or uics1.…"
                        @input=${this.onPeerInput}
                    ></textarea>
                    <button class="btn btn-primary mt-2" @click=${this.onConnect}>connect</button>
                    <h3 class="h6 qr-lead mt-3">or show this code</h3>
                    <qr-code data=${this.link}></qr-code>`;
            case 'answering':
                return html`<div
                    class="spinner-border spinner-border-sm text-secondary"
                    role="status"
                ></div>`;
            default:
                return html``;
        }
    }
}
customElements.define('pair-panel', PairPanel);
