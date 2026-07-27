// The browser's pairing transport controller. It owns the whole exchange —
// the symmetric swap and the same-browser tab handover — and hands the
// finished wire to the page through a single 'wire' event. Pairing is a
// mutual exchange with no third party (ADR 0031): each side sends its invite
// and opens the other's. The UI itself is the shared <pair-panel> (ADR
// 0029): this element renders one, feeds it state as properties, and listens
// for its intent events; the terminal renders the same panel driven by its
// native peer. Browser-only by nature (WebRTC), so it never runs under the
// mocked terminal lit — the panel does.

import { html, LitElement } from 'lit';
import { payloadRole, swap } from '../@schuhkarton/uic-sync/pair.js';
import type { PairOptions, PairSwap } from '../@schuhkarton/uic-sync/pair.js';
import type { Wire } from '../@schuhkarton/uic-sync/wire.js';
import '../@schuhkarton/lit-todo/pair-panel.js';

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

// A same-browser handover, not a server: a link opened in a fresh tab offers
// its payload to a tab already waiting. Module-scoped on purpose — Chrome
// garbage-collects unreferenced BroadcastChannels, listeners and all, so a
// function-local channel goes silently deaf once its scope ends. Optional
// because older WebKit lacks the API; without it a link simply always adopts
// in its own tab.
const relay = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel('uic-pair');

/** Important pairing events go to the console — the hint line shows one
 * state, the log keeps the history (and the payloads, for debugging). */
function note(...parts: unknown[]): void {
    console.info('[p2p]', ...parts);
}

/** The payload carried by a link, pasted text or scanned code: the invite is
 * a single `#uics1.…` fragment, so find the prefix and take the base64url
 * run that follows; a bare token passes through unchanged. */
function payloadOf(text: string): string {
    const trimmed = text.trim();
    const at = trimmed.indexOf('uics1.');
    if (at < 0) {
        return trimmed;
    }
    const rest = trimmed.slice(at);
    return /^uics1\.[A-Za-z0-9_-]*/.exec(rest)?.[0] ?? rest;
}

/** The invite link: the payload as a single URL-safe fragment (base64url
 * needs no escaping), so a chat app linkifies the whole URL. */
function linkFor(payload: string): string {
    const link = new URL(location.pathname, location.href);
    link.hash = payload;
    return link.href;
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
    private link = '';
    private token = '';
    private canScan = 'BarcodeDetector' in window && !!navigator.mediaDevices;
    private booted = false;

    constructor() {
        super();
        this.step = 'idle';
        this.hint =
            'Share this list over WebRTC — no server carries the state. Each side sends one invite; open theirs, they open yours, done.';
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
        const carried = location.hash.length > 1 ? location.hash : null;
        if (carried) {
            note('opened through a peer link');
            // The link is consumed exactly once — a reload or bookmark lands
            // on the clean idle page instead of re-feeding a stale payload,
            // and the payload never lingers in the address bar or history.
            history.replaceState(null, '', location.pathname);
            this.relayOrAdopt(payloadOf(carried));
            return;
        }
        if (['localhost', '127.0.0.1'].includes(location.hostname)) {
            this.hint = 'Open this page through your LAN address so the links reach other devices.';
        }
    }

    /** A link's payload first offers itself to a waiting tab in this
     * browser; unclaimed, this tab becomes the second side itself. */
    private relayOrAdopt(peer: string): void {
        if (!relay) {
            void this.startSide(peer);
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
        relay.postMessage({ type: 'peer', payload: peer });
        setTimeout(() => {
            if (!claimed) {
                void this.startSide(peer);
            }
        }, CLAIM_WINDOW_MS);
    }

    private async startSide(peerAtHand: string | null): Promise<void> {
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
        this.link = linkFor(side.payload);
        this.token = side.payload;
        this.resetLabel = 'start over';
        this.step = 'invite';

        // A link opened in another tab of this browser lands here over the
        // BroadcastChannel. A spent side stays silent — the opening tab then
        // adopts the payload itself instead of handing it to a swap that can
        // no longer pair. Claiming answers with this side's own payload so
        // the other tab pairs against it.
        relay?.addEventListener('message', (event) => {
            const message = event.data as { type: string; payload?: string };
            if (
                message.type === 'peer' &&
                message.payload &&
                message.payload !== side.payload &&
                !side.spent()
            ) {
                note('claimed a peer payload a link handed over in this browser');
                relay.postMessage({ type: 'claimed', payload: message.payload });
                relay.postMessage({ type: 'peer', payload: side.payload });
                void this.pair(side, message.payload);
            }
        });

        if (peerAtHand) {
            // Opened through the peer's link: their payload is already here,
            // so this side connects at once — but the return leg goes by
            // hand, so send your invite back and they connect on opening it.
            void this.pair(side, peerAtHand);
            this.hint = 'Send your invite back — you connect the moment they open it.';
        } else {
            this.hint = 'Send your invite, then open theirs below to connect.';
        }
    }

    /** One side of the swap pairs with the peer payload whenever it FIRST
     * arrives — pasted, scanned, or opened as a link; later arrivals are
     * ignored, the connection stands. Exactly one side greets: the lexically
     * smaller payload. */
    private async pair(side: PairSwap, peer: string): Promise<void> {
        if (this.paired) {
            return;
        }
        if (side.spent()) {
            this.hint = 'This pairing already ran — start a new one below.';
            this.resetLabel = 'start a new pairing';
            return;
        }
        this.paired = true;
        this.hint = 'Connecting…';
        // The wait is legitimate — the connection completes only once BOTH
        // sides applied each other's payload, and the peer may take a while
        // to open this side's link. A slow connect gets a hint, not an abort;
        // truly unreachable peers reject with the pairing's message.
        const slow = setTimeout(() => {
            this.hint =
                "Still connecting — the other device must open this side's link too; if it already did, the networks may not allow a direct route.";
        }, SLOW_HINT_MS);
        note('connecting to peer payload:', peer);
        try {
            const wire = await side.connect(peer);
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
        void this.startSide(null);
    }

    private reset(): void {
        // The way out of any session — waiting, connected, dropped or spent
        // — is a fresh page: links are consumed once, so a reload lands on
        // the clean invite page and everything starts over.
        location.reload();
    }

    private onCopyLink(): void {
        // The panel already cancels the anchor's navigation; here we just
        // put the link on the clipboard.
        void copied(this.link, 'Link').then((outcome) => {
            this.hint = outcome;
        });
    }

    private onCopyToken(): void {
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
            void this.pair(side, payloadOf(raw));
        } catch (error) {
            this.hint = `Camera unavailable (${error}) — paste their link instead.`;
        } finally {
            this.scanning = false;
        }
    }

    private onConnect(event: CustomEvent<string>): void {
        const pasted = (event.detail ?? '').trim();
        if (!pasted) {
            this.hint = 'Paste their link (or token) first.';
            return;
        }
        const side = this.side!;
        void this.pair(side, payloadOf(pasted));
    }

    render() {
        // The shared panel is the whole UI; this element only feeds it state
        // and answers its intents. The camera video stays here — a
        // browser-only surface the panel does not carry (the QR now lives
        // inside the panel as <qr-code>).
        return html`<pair-panel
                .mode=${this.step}
                .link=${this.link}
                .token=${this.token}
                .status=${this.hint}
                .connected=${this.linked}
                .resetLabel=${this.resetLabel}
                .canScan=${this.canScan}
                @invite=${this.invite}
                @reset=${this.reset}
                @connect=${this.onConnect}
                @copy-link=${this.onCopyLink}
                @copy-token=${this.onCopyToken}
                @scan=${this.scan}
            ></pair-panel>
            <video ?hidden=${!this.scanning} playsinline></video>`;
    }
}
customElements.define('pair-wizard', PairWizard);
