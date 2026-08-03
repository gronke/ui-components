// The pairing UI, shared by both hosts. This element is presentation only
// (the invite link, the peer-paste box, the buttons, the status line and the
// connection badge), driven entirely by properties a host sets and
// signalling intent back through the `command` property (a button click
// writes it; the host reads and clears it). The browser also gets a real
// CustomEvent for the same intents, but the property is the channel that
// works under the mocked terminal lit, which has no events.
//
// The transport (WebRTC, the clipboard, the camera) is NOT here; each host
// owns it: the browser's pair-wizard, the terminal's Rust peer. The QR is
// the shared <qr-code> the invite body renders (ADR 0029): the browser draws
// an SVG, the terminal a native block-char widget: one element, two
// renderers, the same reason todo-item keeps a `[x]` span twin. The invite
// itself shows once per host: the browser a compact 🔗 copy-link (it has a
// clipboard), the terminal the full link as wrapped, selectable text.
import { css, html, LitElement } from 'lit';
import { terminalTheme } from './theme.js';
import './qr-code.js';

/** The mode vocabulary hosts drive the panel with. The native session
 * produces the first five (Rust twin: `uic_sync::session::PanelMode`); the
 * browser wizard adds the takeover roles and the no-WebRTC dead end. Only
 * `idle` and `invite` render a body; every other mode speaks through the
 * status line, the badge and the action/reset buttons. */
export type PanelMode =
    | 'idle'
    | 'invite'
    | 'connected'
    | 'dropped'
    | 'failed'
    | 'handed'
    | 'moved'
    | 'nortc';

export class PairPanel extends LitElement {
    static properties = {
        mode: {},
        link: {},
        status: {},
        connected: {},
        resetLabel: {},
        actionLabel: {},
        canScan: { type: Boolean },
        canPaste: { type: Boolean },
        command: {},
        peer: {},
        step: { type: Number },
    };

    // The terminal's look: the mapped Bootstrap subset draws the card and
    // the badge; these rules add what the map leaves unset. Host-specific
    // slots hide here; static styles are the terminal-only layer: the
    // inline QR (the p2p deck docks it beside the panes, ADR 0029) and the
    // copy-link anchor (no clipboard) vanish, while the link text wraps
    // mid-token so the long URL reads across lines. The browser ignores
    // these rules; its page css hides the link text instead.
    static styles = [
        terminalTheme,
        css`
            :host {
                display: block;
            }
            .status {
                color: var(--tui-muted);
            }
            .link-text {
                overflow-wrap: anywhere;
                color: var(--tui-info);
            }
            .copy-link,
            .qr-lead,
            qr-code {
                display: none;
            }
        `,
    ];

    declare mode: PanelMode;
    declare link: string;
    declare status: string;
    declare connected: boolean | null;
    declare resetLabel: string;
    declare actionLabel: string;
    declare canScan: boolean;
    declare canPaste: boolean;
    declare command: string | null;
    declare peer: string;
    /** The pairing wizard's active step (1..3); future steps mute. */
    declare step: number;

    constructor() {
        super();
        this.mode = 'idle';
        this.link = '';
        this.status = '';
        this.connected = null;
        this.resetLabel = '';
        this.actionLabel = '';
        this.canScan = false;
        this.canPaste = false;
        this.command = null;
        this.peer = '';
        this.step = 1;
    }

    createRenderRoot(): this {
        return this;
    }

    /** The focused control's stable identity, captured before a re-render:
     * a structural update (the mode body swapping, a label button appearing
     * or vanishing) tears the focused node down, the browser drops focus to
     * `body`, and a keyboard user gets knocked back to the top of the page.
     * Captured by the control's `name` or its leading class and restored
     * onto the successor. Guarded: the terminal host has no `document`. */
    private focusedKey: string | null = null;

    willUpdate(): void {
        this.focusedKey = null;
        if (typeof document === 'undefined') {
            return;
        }
        const active = document.activeElement;
        if (active instanceof HTMLElement && this.contains(active)) {
            this.focusedKey = active.getAttribute('name') ?? active.classList[0] ?? null;
        }
    }

    updated(): void {
        if (!this.focusedKey || typeof document === 'undefined') {
            return;
        }
        const active = document.activeElement;
        if (active && active !== document.body && this.contains(active)) {
            return;
        }
        // The same control re-rendered, or (when a mode swap removed it,
        // e.g. "create an invite" giving way to the invite card) the new
        // body's first control, so the keyboard walk continues in place.
        const successor =
            this.querySelector(`[name="${this.focusedKey}"]`) ??
            this.querySelector('.' + this.focusedKey) ??
            this.querySelector('a[href], button, textarea, input');
        (successor as HTMLElement | null)?.focus?.();
    }

    /** The one seam back to the host: a click sets the command property the
     * host polls, and (where the platform has events) dispatches the same
     * intent so a browser controller can just listen. The terminal reads
     * the property and clears it; the browser uses the event. Only
     * `connect` carries a detail (the pasted peer text); `action` stays a
     * deliberately generic name: the panel is presentation-only and the
     * host supplies the label (the tab takeover today). */
    private emit(command: string): void {
        this.command = command;
        if (typeof CustomEvent === 'function' && typeof this.dispatchEvent === 'function') {
            const detail = command === 'connect' ? this.peer : undefined;
            this.dispatchEvent(new CustomEvent(command, { detail, bubbles: true }));
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
        // The link is for copying; navigating a waiting tab into a payload
        // would tear the session it belongs to.
        event.preventDefault();
        this.emit('copy-link');
    }

    private onAction(): void {
        this.emit('action');
    }

    private onScan(): void {
        this.emit('scan');
    }

    private onPaste(): void {
        this.emit('paste');
    }

    render() {
        // The invite is a three-step wizard; every other mode is the plain
        // card the status line, badge and buttons carry.
        if (this.mode === 'invite') {
            return this.renderWizard();
        }
        return html`<section class="card">
            <div class="card-header d-flex align-items-center justify-content-between">
                pair another device
                ${this.connected === null
                    ? ''
                    : html`<span
                          class="badge ${this.connected ? 'text-bg-success' : 'text-bg-danger'}"
                          >${this.connected ? 'connected' : 'disconnected'}</span
                      >`}
            </div>
            <div class="card-body">
                ${this.mode === 'failed'
                    ? html`<div class="status alert alert-danger text-danger" role="alert">
                          ${this.status}
                      </div>`
                    : html`<p class="status text-body-secondary">${this.status}</p>`}
                ${this.mode === 'idle'
                    ? html`<button class="invite btn btn-primary" @click=${this.onInvite}>
                          create an invite
                      </button>`
                    : ''}
                ${this.actionLabel
                    ? html`<button class="action btn btn-primary d-block mt-3" @click=${this.onAction}>
                          ${this.actionLabel}
                      </button>`
                    : ''}
                ${this.resetLabel
                    ? html`<button class="reset btn btn-outline-secondary d-block mt-3" @click=${this.onReset}>
                          ${this.resetLabel}
                      </button>`
                    : ''}
            </div>
        </section>`;
    }

    // The wizard: three step cards, only the reachable one lit. The live
    // status rides inside the active card (renderStep prepends it), so the
    // narration always sits with the step it describes, step 3 included,
    // which would otherwise read as a dead box. Future and done steps render
    // a muted summary with no controls: the terminal has no pointer-events,
    // so a card with no buttons is a card that cannot be clicked into.
    private renderWizard() {
        return html`${this.renderStep(1, 'start a pairing', this.renderStart(), 'share your invite, open theirs')}
            ${this.renderStep(2, 'acknowledge', this.renderAcknowledge(), 'send your reply so they can connect')}
            ${this.renderStep(3, 'connect', this.renderConnect(), 'connects automatically, and says why if it cannot')}
            ${this.resetLabel
                ? html`<button class="reset btn btn-outline-secondary d-block mt-2" @click=${this.onReset}>
                      ${this.resetLabel}
                  </button>`
                : ''}`;
    }

    private renderStep(n: number, title: string, body: unknown, summary: string) {
        const active = this.step === n;
        const done = this.step > n;
        return html`<section class="step card mb-2">
            <div class="card-header ${active ? '' : 'text-muted'}">${done ? '✓' : n} · ${title}</div>
            <div class="card-body ${active ? '' : 'small text-muted'}">
                ${active
                    ? html`<p class="status text-body-secondary">${this.status}</p>
                          ${body}`
                    : summary}
            </div>
        </section>`;
    }

    // Step 1: share your invite and open theirs, the mutual exchange
    // (ADR 0028). The copy-link and QR are browser-only (the terminal hides
    // them and docks its own QR beside the panes); the wrapped link text is
    // the terminal's share surface.
    private renderStart() {
        return html`<h3 class="h6">share your invite</h3>
            <a class="copy-link" href=${this.link} title="copy the invite link" @click=${this.onCopyLink}
                >🔗 copy link</a
            >
            <p class="link-text">${this.link}</p>
            <h3 class="h6 mt-3">open their invite</h3>
            ${this.canScan
                ? html`<button class="scan btn btn-outline-secondary mb-2" @click=${this.onScan}>
                      scan their code
                  </button>`
                : ''}
            ${this.canPaste
                ? html`<button class="paste btn btn-outline-secondary mb-2" @click=${this.onPaste}>
                      paste from clipboard
                  </button>`
                : ''}
            <textarea
                name="peer"
                data-path="peer"
                class="peer-input form-control font-monospace"
                placeholder="their link or code…"
                @input=${this.onPeerInput}
            ></textarea>
            <button class="connect btn btn-primary mt-2" @click=${this.onConnect}>connect</button>
            <h3 class="h6 qr-lead mt-3">or show this code</h3>
            <qr-code data=${this.link}></qr-code>`;
    }

    // Step 2: the peer opened a fresh invite, so the link is now our reply
    // for them to open back; the connect already rides behind it.
    private renderAcknowledge() {
        return html`<a class="copy-link" href=${this.link} title="copy the reply link" @click=${this.onCopyLink}
                >🔗 copy reply</a
            >
            <p class="link-text">${this.link}</p>
            <qr-code data=${this.link}></qr-code>`;
    }

    // Step 3: connecting. The step's own body is the live status the active
    // card already carries ("Connecting…", the bounded retry attempt, or the
    // honest reason it stopped), so it adds nothing of its own; the reset
    // control below is the way to bail. It is only ever shown while a connect
    // is in flight: success leaves invite mode for the todo screen, and a
    // failure that cannot resume renews an invite back at step 1.
    private renderConnect() {
        return html``;
    }
}
customElements.define('pair-panel', PairPanel);
