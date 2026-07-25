// The pairing flow as one Lit element: every step renders only what the
// user needs right now and says what happens next. The wizard owns the
// whole exchange — the symmetric swap, the QR/link/token surfaces, the
// same-browser handover and the rendezvous relay — and hands the finished
// wire to the page through a single 'wire' event. Browser-only by nature
// (WebRTC), so unlike the todo components it never runs under the mocked
// terminal lit.

import { html, LitElement } from 'lit';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import { payloadRole, swap } from '../@schuhkarton/uic-sync/pair.js';
import type { PairOptions, PairSwap } from '../@schuhkarton/uic-sync/pair.js';
import type { Wire } from '../@schuhkarton/uic-sync/wire.js';
import qrcode from 'qrcode-generator';

type Step = 'idle' | 'invite' | 'answering' | 'connected' | 'dropped' | 'failed' | 'handed' | 'nortc';

const CLAIM_WINDOW_MS = 400;
const SLOW_HINT_MS = 15000;

// The library defaults to no iceServers (one network, mDNS candidates);
// this page opts into a public STUN server so peers on different networks
// still find a route, and 'uic-ice' in localStorage appends any further
// RTCIceServer list — a TURN relay with credentials makes hostile NATs
// reachable without putting a server in the repo.
function iceConfig(): PairOptions {
    const iceServers: RTCIceServer[] = [{ urls: 'stun:stun.l.google.com:19302' }];
    const extra = localStorage.getItem('uic-ice');
    if (extra) {
        try {
            const parsed = JSON.parse(extra) as RTCIceServer[];
            if (Array.isArray(parsed)) {
                iceServers.push(...parsed);
            } else {
                console.warn('[p2p] uic-ice ignored — expected a JSON array of RTCIceServer');
            }
        } catch (error) {
            console.warn('[p2p] uic-ice ignored — not valid JSON:', error);
        }
    }
    return { iceServers };
}

// The rendezvous relay carries exactly one message — the reply payload —
// so opening one link connects both sides; the todo state never touches
// it. 'off' keeps the exchange fully manual, any other value points at a
// self-hosted ntfy server.
const BROKER = (() => {
    const knob = localStorage.getItem('uic-broker');
    if (knob === 'off') {
        return null;
    }
    return knob || 'https://ntfy.sh';
})();

// Module-scoped on purpose: Chrome garbage-collects unreferenced
// BroadcastChannels, listeners and all — a function-local channel goes
// silently deaf once its scope ends. Optional because older WebKit lacks
// the API; without it links simply always adopt in their own tab.
const relay = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel('uic-pair');

/** Important pairing events go to the console — the hint line shows one
 * state, the log keeps the history (and the payloads, for debugging). */
function note(...parts: unknown[]): void {
    console.info('[p2p]', ...parts);
}

/** The payload hidden in a link, pasted text or scanned code — behind
 * '#s=' or '?s=' (a camera flow may strip the fragment), cut before any
 * further parameter; bare payloads pass through untouched. */
function payloadOf(text: string): string {
    let trimmed = text.trim();
    for (const marker of ['#s=', '?s=']) {
        const at = trimmed.indexOf(marker);
        if (at >= 0) {
            trimmed = trimmed.slice(at + marker.length);
            break;
        }
    }
    const tail = trimmed.indexOf('&');
    return decodeURIComponent(tail >= 0 ? trimmed.slice(0, tail) : trimmed);
}

/** The relay topic riding a link, if any — full URLs keep working as
 * plain payload carriers when there is none. */
function viaOf(text: string): string | null {
    const match = /[#?&]via=([\w-]+)/.exec(text);
    return match ? match[1]! : null;
}

function linkFor(payload: string, topic: string | null): string {
    const link = new URL(location.pathname, location.href);
    link.hash = '#s=' + encodeURIComponent(payload) + (topic ? '&via=' + topic : '');
    return link.href;
}

function newTopic(): string {
    const bytes = crypto.getRandomValues(new Uint8Array(12));
    let binary = '';
    for (const byte of bytes) {
        binary += String.fromCharCode(byte);
    }
    return 'uic-' + btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function qrSvg(text: string): string {
    const code = qrcode(0, 'L');
    code.addData(text);
    code.make();
    return code.createSvgTag({ cellSize: 4, margin: 4 });
}

/** Clipboard with the graceful path: plain http on a LAN address has no
 * clipboard API — the text stays selectable either way. Callers invoke
 * this first thing in the tap handler; Safari only honors writes that
 * start inside the gesture. */
async function copied(text: string, what: string): Promise<string> {
    try {
        await navigator.clipboard.writeText(text);
        note(`${what} copied to the clipboard`);
        return `${what} copied — send it over.`;
    } catch {
        return `${what} not copied (needs a secure context) — select and copy it by hand.`;
    }
}

async function scanFor(video: HTMLVideoElement, own: string): Promise<string> {
    const detector = new (window as any).BarcodeDetector({ formats: ['qr_code'] });
    const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'environment' },
    });
    video.srcObject = stream;
    await video.play();
    try {
        for (;;) {
            const codes = await detector.detect(video);
            // Only the PEER's swap payload ends the scan — this side's own
            // code (or stray QR noise) keeps the camera looking.
            const hit = codes.find((code: any) => {
                const payload = payloadOf(String(code.rawValue));
                return payloadRole(payload) === 'offer' && payload !== own;
            });
            if (hit) {
                return String(hit.rawValue);
            }
            await new Promise((resolve) => setTimeout(resolve, 250));
        }
    } finally {
        stream.getTracks().forEach((track) => track.stop());
        video.srcObject = null;
    }
}

export class PairWizard extends LitElement {
    static properties = {
        step: { state: true },
        hint: { state: true },
        linked: { state: true },
        scanning: { state: true },
        starting: { state: true },
        resetLabel: { state: true },
    };

    declare step: Step;
    declare hint: string;
    declare linked: boolean | null;
    declare scanning: boolean;
    declare starting: boolean;
    declare resetLabel: string;

    private side: PairSwap | null = null;
    private paired = false;
    private topic: string | null = null;
    private source: EventSource | null = null;
    private link = '';
    private token = '';
    private qr = '';
    private canScan = 'BarcodeDetector' in window && !!navigator.mediaDevices;
    private booted = false;

    constructor() {
        super();
        this.step = 'idle';
        this.hint = BROKER
            ? 'Share this list over WebRTC — no server carries the state. One of you creates an invite; opening it connects both.'
            : 'Share this list over WebRTC — no server carries the state. Each side sends one link; open theirs, they open yours, done.';
        this.linked = null;
        this.scanning = false;
        this.starting = false;
        this.resetLabel = '';
    }

    createRenderRoot(): this {
        return this;
    }

    connectedCallback(): void {
        super.connectedCallback();
        if (!this.booted) {
            this.booted = true;
            this.boot();
        }
    }

    private boot(): void {
        if (typeof RTCPeerConnection === 'undefined') {
            // WebKit hides WebRTC from in-app browsers and non-HTTPS pages.
            this.step = 'nortc';
            this.hint =
                'This browser view hides WebRTC — open the page in a regular browser over HTTPS (in-app browsers often offer "Open in Browser").';
            return;
        }
        const carried = location.hash.startsWith('#s=')
            ? location.hash
            : new URLSearchParams(location.search).has('s')
              ? location.search
              : null;
        if (carried) {
            note('opened through a peer link');
            // The link is consumed exactly once — a reload or bookmark lands
            // on the clean idle page instead of re-feeding a stale payload.
            history.replaceState(null, '', location.pathname);
            this.relayOrAdopt(payloadOf(carried), viaOf(carried));
            return;
        }
        if (['localhost', '127.0.0.1'].includes(location.hostname)) {
            this.hint = 'Open this page through your LAN address so the links reach other devices.';
        }
    }

    /** A link's payload first offers itself to a waiting tab in this
     * browser; unclaimed, this tab becomes the second side itself. */
    private relayOrAdopt(peer: string, via: string | null): void {
        if (!relay) {
            void this.startSide(peer, via);
            return;
        }
        let claimed = false;
        relay.addEventListener('message', (event) => {
            const message = event.data as { type: string; payload?: string };
            if (message.type === 'claimed' && message.payload === peer) {
                claimed = true;
                note('an open tab claimed the link payload — pairing continues there');
                this.step = 'handed';
                this.hint = 'Handed to your open tab — the connection continues there.';
                this.resetLabel = 'start a fresh pairing here';
            }
        });
        relay.postMessage({ type: 'peer', payload: peer, via });
        setTimeout(() => {
            if (!claimed) {
                void this.startSide(peer, via);
            }
        }, CLAIM_WINDOW_MS);
    }

    private async startSide(peerAtHand: string | null, peerVia: string | null): Promise<void> {
        if (this.starting) {
            return;
        }
        this.starting = true;
        this.hint = 'Gathering candidates…';
        let side: PairSwap;
        try {
            side = await swap(iceConfig());
        } catch (error) {
            this.step = 'failed';
            this.hint = `Pairing failed: ${error}`;
            this.resetLabel = 'start over';
            return;
        }
        this.side = side;
        note('side ready — own payload:', side.payload);
        if (BROKER) {
            this.topic = newTopic();
            this.listen(side);
        }
        this.link = linkFor(side.payload, this.topic);
        this.token = side.payload;
        this.qr = qrSvg(this.link);
        this.resetLabel = 'start over';
        this.step = 'invite';

        // A link opened elsewhere in this browser lands here over the
        // relay. A spent side stays silent — the opening tab then adopts
        // the payload itself instead of handing it to a swap that can no
        // longer pair. Claiming answers with this side's own payload — over
        // the BroadcastChannel for a tab of this browser, and through the
        // relay topic the link carried for the remote side that made it.
        relay?.addEventListener('message', (event) => {
            const message = event.data as { type: string; payload?: string; via?: string };
            if (
                message.type === 'peer' &&
                message.payload &&
                message.payload !== side.payload &&
                !side.spent()
            ) {
                note('claimed a peer payload a link handed over in this browser');
                relay.postMessage({ type: 'claimed', payload: message.payload });
                relay.postMessage({ type: 'peer', payload: side.payload });
                const exchanged = !!(message.via && BROKER);
                if (message.via && BROKER) {
                    void this.answerVia(message.via, side);
                }
                void this.pair(side, message.payload, exchanged);
            }
        });

        if (peerAtHand) {
            // Opened through the peer's link: their payload is already here.
            // With a relay topic the reply travels by itself; without one
            // the link must go back by hand — the hint follows the pair()
            // call so the instruction outlasts its "Connecting…".
            const exchanged = !!(peerVia && BROKER);
            if (peerVia && BROKER) {
                void this.answerVia(peerVia, side);
            }
            void this.pair(side, peerAtHand, exchanged);
            if (!exchanged) {
                this.hint = 'Send your link back — you connect the moment they open it.';
            }
        } else {
            this.hint = BROKER
                ? 'Send the invite — the moment they open it on the other device, you connect.'
                : 'Send the invite, then open theirs under "exchange by hand".';
        }
    }

    /** The inviter's ear on the relay: the reply payload arrives on the
     * one-time topic the invite link carried. */
    private listen(side: PairSwap): void {
        const source = new EventSource(`${BROKER}/${this.topic}/sse?since=30m`);
        this.source = source;
        let complained = false;
        source.addEventListener('message', (event) => {
            let body = '';
            try {
                body = String((JSON.parse(event.data as string) as { message?: string }).message ?? '');
            } catch {
                return;
            }
            const peer = body.trim();
            if (payloadRole(peer) === 'offer' && peer !== side.payload && !side.spent()) {
                note('their reply arrived over the relay');
                void this.pair(side, peer, true);
            }
        });
        source.addEventListener('error', () => {
            // EventSource reconnects by itself; one note is enough, and the
            // QR/manual paths keep working without the relay.
            if (!complained && !this.paired) {
                complained = true;
                note('relay subscription hiccup — reconnecting; manual exchange keeps working');
            }
        });
    }

    /** The opener's reply: one POST to the topic the invite carried. */
    private async answerVia(topic: string, side: PairSwap): Promise<void> {
        try {
            const response = await fetch(`${BROKER}/${topic}`, {
                method: 'POST',
                body: side.payload,
            });
            if (!response.ok) {
                throw new Error(`the relay answered ${response.status}`);
            }
            note('reply payload posted to their relay topic');
        } catch (error) {
            note('relay post failed:', error);
            if (this.step === 'answering') {
                this.step = 'invite';
            }
            this.hint =
                'The relay is unreachable — send your link back by hand; you connect the moment they open it.';
        }
    }

    /** One side of the swap pairs with the peer payload whenever it FIRST
     * arrives — over the relay, pasted, scanned, or opened as a link;
     * later arrivals are ignored, the connection stands. Exactly one side
     * greets: the lexically smaller payload. `exchanged` says the peer
     * holds (or is about to receive) this side's payload too — then there
     * is nothing left to show but the wait. */
    private async pair(side: PairSwap, peer: string, exchanged: boolean): Promise<void> {
        if (this.paired) {
            return;
        }
        if (side.spent()) {
            this.hint = 'This pairing already ran — start a new one below.';
            this.resetLabel = 'start a new pairing';
            return;
        }
        this.paired = true;
        if (exchanged) {
            this.step = 'answering';
            this.hint = 'Connecting — nothing else to do.';
        } else {
            this.hint = 'Connecting…';
        }
        // The wait is legitimate — the connection completes only once BOTH
        // sides applied each other's payload, and the peer may take minutes
        // to open this side's link. A slow connect gets a hint, not an
        // abort; truly unreachable peers reject with the pairing's message.
        const slow = setTimeout(() => {
            this.hint =
                "Still connecting — the other device must open this side's link too; if it already did, the networks may not allow a direct route.";
        }, SLOW_HINT_MS);
        note('connecting to peer payload:', peer);
        try {
            const wire = await side.connect(peer);
            this.source?.close();
            this.connect(wire, side.payload < peer);
        } catch (error) {
            this.paired = false;
            note('pairing failed:', error);
            if (side.spent()) {
                this.step = 'failed';
                this.resetLabel = 'start a new pairing';
            }
            this.hint = `Pairing failed: ${error}`;
        } finally {
            clearTimeout(slow);
        }
    }

    private connect(wire: Wire, greet: boolean): void {
        this.dispatchEvent(new CustomEvent('wire', { detail: { wire, greet }, bubbles: true }));
        this.linked = true;
        note('connected', greet ? '— this side greets with its state' : '— waiting for their greeting');
        this.step = 'connected';
        this.hint = 'Connected — one list, two browsers.';
        this.resetLabel = 'invite somebody else';
        wire.onClose(() => {
            this.linked = false;
            note('wire closed');
            this.step = 'dropped';
            this.hint = 'Connection closed — invite somebody else.';
        });
    }

    private invite(): void {
        void this.startSide(null, null);
    }

    private reset(): void {
        // The way out of any session — waiting, connected, dropped or spent
        // — is a fresh page: links are consumed once, so a reload lands on
        // the clean invite page and everything starts over.
        location.reload();
    }

    private copyLink(event: Event): void {
        // The link is for copying — navigating a waiting tab into a payload
        // would tear the session it belongs to.
        event.preventDefault();
        void copied(this.link, 'Link').then((outcome) => {
            this.hint = outcome;
        });
    }

    private copyToken(event: Event): void {
        (event.currentTarget as HTMLTextAreaElement).select();
        void copied(this.token, 'Token').then((outcome) => {
            this.hint = outcome;
        });
    }

    private async scan(): Promise<void> {
        const side = this.side!;
        try {
            this.hint = 'Point the camera at their code…';
            this.scanning = true;
            await this.updateComplete;
            const raw = await scanFor(this.querySelector('video')!, side.payload);
            const via = viaOf(raw);
            const exchanged = !!(via && BROKER);
            if (via && BROKER) {
                void this.answerVia(via, side);
            }
            void this.pair(side, payloadOf(raw), exchanged);
        } catch (error) {
            this.hint = `Camera unavailable (${error}) — paste their link instead.`;
        } finally {
            this.scanning = false;
        }
    }

    private connectPeer(): void {
        const box = this.querySelector('.peer-input') as HTMLTextAreaElement;
        if (!box.value.trim()) {
            this.hint = 'Paste their link (or token) first.';
            return;
        }
        const side = this.side!;
        const via = viaOf(box.value);
        const exchanged = !!(via && BROKER);
        if (via && BROKER) {
            void this.answerVia(via, side);
        }
        void this.pair(side, payloadOf(box.value), exchanged);
    }

    render() {
        return html`<section class="card">
            <div class="card-header d-flex align-items-center justify-content-between">
                pair another browser
                ${this.linked === null
                    ? ''
                    : html`<span class="badge ${this.linked ? 'text-bg-success' : 'text-bg-danger'}"
                          >${this.linked ? 'connected' : 'disconnected'}</span
                      >`}
            </div>
            <div class="card-body">
                <p class="status text-body-secondary">${this.hint}</p>
                ${this.renderStep()}
                ${this.resetLabel
                    ? html`<button class="btn btn-outline-secondary d-block mt-3" @click=${this.reset}>
                          ${this.resetLabel}
                      </button>`
                    : ''}
            </div>
        </section>`;
    }

    private renderStep() {
        switch (this.step) {
            case 'idle':
                return html`<button class="btn btn-primary" ?disabled=${this.starting} @click=${this.invite}>
                    create an invite
                </button>`;
            case 'invite':
                return html`<div class="qr">${unsafeHTML(this.qr)}</div>
                    <a class="copy-link" href=${this.link} title="click to copy" @click=${this.copyLink}
                        >${this.link}</a
                    >
                    <details ?open=${!BROKER}>
                        <summary>exchange by hand</summary>
                        <textarea
                            name="token"
                            class="form-control font-monospace mt-2"
                            readonly
                            title="click to copy"
                            .value=${this.token}
                            @click=${this.copyToken}
                        ></textarea>
                        <h3 class="h6 mt-3">and open theirs</h3>
                        ${this.canScan
                            ? html`<button class="btn btn-outline-secondary mb-2" @click=${this.scan}>
                                  scan their code
                              </button>`
                            : ''}
                        <video ?hidden=${!this.scanning} playsinline></video>
                        <textarea
                            name="peer"
                            class="peer-input form-control font-monospace"
                            placeholder="their link or uics1.…"
                        ></textarea>
                        <button class="btn btn-primary mt-2" @click=${this.connectPeer}>connect</button>
                    </details>`;
            case 'answering':
                return html`<div class="spinner-border spinner-border-sm text-secondary" role="status"></div>`;
            default:
                return html``;
        }
    }
}
customElements.define('pair-wizard', PairWizard);
