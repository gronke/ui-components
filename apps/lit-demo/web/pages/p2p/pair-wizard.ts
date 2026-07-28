// The browser's pairing transport controller. It owns the swap lifecycle
// and hands the finished wire to the page through a single 'wire' event.
// Its reactive state speaks the panel's own vocabulary (PanelMode et al),
// so the render below is a plain pass-through — no rename layer;
// the cross-tab organization — which tab holds a session, how an opened
// link reaches it, how a session hands over to another tab — lives in
// @schuhkarton/uic-sync's session module (ADR 0032). Pairing is a mutual
// exchange with no third party (ADR 0028): each side sends its invite and
// opens the other's. The UI itself is the shared <pair-panel> (ADR 0029).
// Browser-only by nature (WebRTC), so it never runs under the mocked
// terminal lit — the panel does.
//
// A tab plays up to three roles in a takeover (ADR 0032): the NEW tab that
// opened a reply and asks to take the session over; the OLD tab that owns
// the wire and re-signals a fresh pairing through it; and the REMOTE end
// that answers a repair request with a fresh swap of its own. Every live
// wire wears the control plane (ControlWire), because any connected tab
// can be asked to repair later.

import { html, LitElement } from 'lit';
import {
    inviteLink,
    linkPayload,
    linkReply,
    replyDigest,
    swap,
} from '../@schuhkarton/uic-sync/pair.js';
import type { PairSwap } from '../@schuhkarton/uic-sync/pair.js';
import {
    ControlWire,
    TabSessions,
    TakeoverPoint,
    TAKEOVER_TIMEOUT_MS,
} from '../@schuhkarton/uic-sync/session.js';
import type { Wire } from '../@schuhkarton/uic-sync/wire.js';
import '../@schuhkarton/lit-todo/pair-panel.js';
import type { PanelMode } from '../@schuhkarton/lit-todo/pair-panel.js';
import { iceConfig } from './ice.js';
import { scanFor } from './scan.js';

const SLOW_HINT_MS = 15000;

// Module-scoped on purpose: Chrome garbage-collects unreferenced
// BroadcastChannels, listeners and all — a function-local channel goes
// silently deaf once its scope ends.
const sessions = new TabSessions();

/** Important pairing events go to the console — the hint line shows one
 * state, the log keeps the history (and the payloads, for debugging). */
function note(...parts: unknown[]): void {
    console.info('[p2p]', ...parts);
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

export class PairWizard extends LitElement {
    static properties = {
        mode: { state: true },
        status: { state: true },
        connected: { state: true },
        scanning: { state: true },
        starting: { state: true },
        resetLabel: { state: true },
        actionLabel: { state: true },
    };

    declare mode: PanelMode;
    declare status: string;
    declare connected: boolean | null;
    declare scanning: boolean;
    declare starting: boolean;
    declare resetLabel: string;
    declare actionLabel: string;

    private side: PairSwap | null = null;
    private paired = false;
    private link = '';
    private canScan = 'BarcodeDetector' in window && !!navigator.mediaDevices;
    private booted = false;
    /** The live wire's control plane; null until connected or after retire. */
    private ctrlWire: ControlWire | null = null;
    /** The served takeover endpoint while this tab owns the session. */
    private point: TakeoverPoint | null = null;
    /** The session digest a reply link named — the takeover channel key,
     * inherited across a takeover so chained handovers keep working. */
    private takeDigest: string | null = null;
    /** True while a deliberate re-wire is in flight: the old wire's close
     * must not paint "connection closed". */
    private handingOver = false;
    /** True once this tab handed its session away. */
    private retired = false;
    /** True while this (owning) tab forwards one takeover. */
    private takingOver = false;
    private served = false;

    constructor() {
        super();
        this.mode = 'idle';
        this.status =
            'Share this list over WebRTC — no server carries the state. Each side sends one invite; open theirs, they open yours, done.';
        this.connected = null;
        this.scanning = false;
        this.starting = false;
        this.resetLabel = '';
        this.actionLabel = '';
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
            this.mode = 'nortc';
            this.status =
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
            const peer = linkPayload(carried);
            const replyTo = linkReply(carried);
            sessions.offer(peer, replyTo, {
                handed: (status) => {
                    note('an open tab claimed the link payload —', status);
                    this.mode = 'handed';
                    this.takeDigest = replyTo;
                    this.status =
                        status === 'connected'
                            ? 'Already connected in your other tab.'
                            : 'Handed to your open tab — the connection continues there.';
                    this.resetLabel = 'start a fresh pairing here';
                    if (replyTo) {
                        this.actionLabel = 'take the session over in this tab';
                    }
                },
                adopt: () => void this.startSide(peer),
                orphan: () => this.refuseOrphanReply(),
            });
            return;
        }
        if (['localhost', '127.0.0.1'].includes(location.hostname)) {
            this.status = 'Open this page through your LAN address so the links reach other devices.';
        }
    }

    /** A reply whose inviting session no open tab holds. */
    private refuseOrphanReply(): void {
        note('a reply arrived for an invite no open tab holds');
        this.mode = 'failed';
        this.status =
            'This reply answers an invite this browser no longer holds — the tab that created it was closed or reloaded. Start a fresh pairing and send a new invite.';
        this.resetLabel = 'start a fresh pairing';
    }

    /** Serves this tab's session to the handover channel exactly once; the
     * closures read the live fields, so a takeover replacing the swap keeps
     * the served identity current. */
    private ensureServed(): void {
        if (this.served) {
            return;
        }
        this.served = true;
        sessions.serve({
            payload: () => this.side?.payload ?? '',
            digests: () => {
                const own = this.side ? [replyDigest(this.side.payload)] : [];
                return this.takeDigest ? [...own, this.takeDigest] : own;
            },
            spent: () => this.side?.spent() ?? true,
            connected: () => this.connected === true && !this.retired,
            onPeer: (peer) => {
                if (this.side) {
                    void this.pair(this.side, peer);
                }
            },
        });
    }

    private async startSide(peerAtHand: string | null): Promise<void> {
        if (this.starting) {
            return;
        }
        this.starting = true;
        this.status = 'Gathering candidates…';
        let side: PairSwap;
        try {
            side = await swap(iceConfig());
        } catch (error) {
            this.mode = 'failed';
            this.status = `Pairing failed: ${error}`;
            this.resetLabel = 'start over';
            return;
        }
        this.side = side;
        note('side ready — own payload:', side.payload);
        // A link built in reply to an opened invite names it: the `.{digest}`
        // suffix routes the reply to the exact tab that invited (still one
        // URL-safe token — every parser cuts before the dot).
        const page = new URL(location.pathname, location.href).href;
        this.link = inviteLink(page, side.payload, peerAtHand ? replyDigest(peerAtHand) : undefined);
        this.resetLabel = 'start over';
        this.mode = 'invite';
        this.ensureServed();

        if (peerAtHand) {
            // Opened through the peer's link: their payload is already here,
            // so this side connects at once — but the return leg goes by
            // hand, so send your invite back and they connect on opening it.
            void this.pair(side, peerAtHand);
            this.status = 'Send your invite back — you connect the moment they open it.';
        } else {
            this.status = 'Send your invite, then open theirs below to connect.';
        }
    }

    /** One side of the swap pairs with the peer payload whenever it FIRST
     * arrives — pasted, scanned, or opened as a link; later arrivals are
     * ignored, the connection stands. Exactly one side greets: the lexically
     * smaller payload, unless a takeover forces the roles (the remote holds
     * the canonical state, the fresh tab must not greet with an empty one). */
    private async pair(side: PairSwap, peer: string, forcedGreet?: boolean): Promise<void> {
        if (this.paired) {
            return;
        }
        if (side.spent()) {
            this.status = 'This pairing already ran — start a new one below.';
            this.resetLabel = 'start a new pairing';
            return;
        }
        this.paired = true;
        this.status = 'Connecting…';
        // The wait is legitimate — the connection completes only once BOTH
        // sides applied each other's payload, and the peer may take a while
        // to open this side's link. A slow connect gets a hint, not an abort;
        // truly unreachable peers reject with the pairing's message.
        const slow = setTimeout(() => {
            this.status =
                "Still connecting — the other device must open this side's link too; if it already did, the networks may not allow a direct route.";
        }, SLOW_HINT_MS);
        note('connecting to peer payload:', peer);
        try {
            const wire = await side.connect(peer);
            this.connect(wire, forcedGreet ?? side.payload < peer);
        } catch (error) {
            this.paired = false;
            note('pairing failed:', error);
            if (side.spent()) {
                this.mode = 'failed';
                this.resetLabel = 'start a new pairing';
            }
            this.status = `Pairing failed: ${error}`;
        } finally {
            clearTimeout(slow);
        }
    }

    /** A wire opened: wear the control plane, serve the takeover endpoint,
     * hand the wrapped wire to the page, and show the connected state. */
    private connect(raw: Wire, greet: boolean): void {
        const wire = new ControlWire(raw);
        this.ctrlWire = wire;
        wire.onControl((message) => {
            if (message.t === 'repair' && message.payload) {
                void this.repair(wire, message.payload);
            } else if (message.t === 'repair-answer' && message.payload) {
                // The remote's fresh payload, relayed to the requesting tab;
                // the wire switch follows its takeover-done.
                this.point?.answer(message.payload);
            }
        });
        this.ensureServed();
        this.point?.close();
        this.point = new TakeoverPoint(this.takeDigest ?? replyDigest(this.side!.payload));
        this.point.onRequest((payload) => this.forwardTakeover(payload));
        this.point.onDone((byPayload) => {
            // The new owner serves this same channel — only the OLD one
            // retires on the cue.
            if (byPayload !== this.side?.payload) {
                this.retire();
            }
        });

        this.dispatchEvent(new CustomEvent('wire', { detail: { wire, greet }, bubbles: true }));
        this.connected = true;
        note('connected', greet ? '— this side greets with its state' : '— waiting for their greeting');
        this.mode = 'connected';
        this.status = 'Connected — one list, two browsers.';
        this.resetLabel = 'invite somebody else';
        wire.onClose(() => {
            // Only the CURRENT wire's death is a drop: a handover supersedes
            // the old wire (and a retire nulls it) before or while it closes,
            // and its close event must not paint over the live state.
            if (this.ctrlWire !== wire || this.retired) {
                return;
            }
            this.connected = false;
            note('wire closed');
            this.mode = 'dropped';
            this.status = 'Connection closed — invite somebody else.';
        });
    }

    /** The OWNING tab's half of a takeover: another tab of this browser
     * asked for the session; forward its fresh payload to the remote over
     * the live wire as a repair request. One at a time. */
    private forwardTakeover(payload: string): void {
        if (!this.ctrlWire || this.connected !== true || this.takingOver || this.retired) {
            return;
        }
        this.takingOver = true;
        note('another tab requests this session — re-signaling through the wire');
        this.ctrlWire.sendControl({ t: 'repair', payload });
        // A takeover that never completes unblocks the next attempt.
        setTimeout(() => {
            this.takingOver = false;
        }, TAKEOVER_TIMEOUT_MS);
    }

    /** The REMOTE end's half: the other side moves to a new tab — answer
     * with a fresh swap and re-form the connection on it. The old wire
     * stays up until the new one opened, so a failed handover loses
     * nothing. */
    private async repair(ctrl: ControlWire, peer: string): Promise<void> {
        if (this.handingOver) {
            return;
        }
        this.handingOver = true;
        note('the other side moves to a new tab — re-pairing through the wire');
        let fresh: PairSwap;
        try {
            fresh = await swap(iceConfig());
        } catch (error) {
            note('re-pairing setup failed:', error);
            this.handingOver = false;
            return;
        }
        ctrl.sendControl({ t: 'repair-answer', payload: fresh.payload });
        try {
            // This side holds the canonical state: it greets the fresh tab.
            const wire = await fresh.connect(peer);
            const previous = this.ctrlWire;
            this.side = fresh;
            this.connect(wire, true);
            previous?.close();
            this.status = 'The other side moved to a new tab — reconnected.';
        } catch (error) {
            note('the re-pairing failed, the old wire stands:', error);
            this.status = `The handover failed (${error}) — still on the previous connection.`;
        } finally {
            this.handingOver = false;
        }
    }

    /** The NEW tab's half: mint a fresh swap and ask the owning tab to
     * re-signal it through the session's own wire. */
    private async takeOver(): Promise<void> {
        if (!this.takeDigest || this.starting) {
            return;
        }
        this.starting = true;
        this.actionLabel = '';
        this.status = 'Taking the session over — re-signaling through your other tab…';
        let side: PairSwap;
        try {
            side = await swap(iceConfig());
        } catch (error) {
            this.mode = 'failed';
            this.status = `Takeover failed: ${error}`;
            this.starting = false;
            return;
        }
        const point = new TakeoverPoint(this.takeDigest);
        try {
            const answer = await point.request(side.payload);
            this.side = side;
            // The remote greets with the canonical state; this fresh tab
            // must not greet with its empty one.
            await this.pair(side, answer, false);
            point.done(side.payload);
        } catch (error) {
            note('the takeover did not complete:', error);
            this.status =
                'The takeover timed out — the other side may not support handover, or the tabs lost each other. The session stays in your other tab.';
            this.actionLabel = 'take the session over in this tab';
            this.starting = false;
            side.close();
        } finally {
            point.close();
        }
    }

    /** This tab handed its session to another one. */
    private retire(): void {
        if (this.retired) {
            return;
        }
        this.retired = true;
        note('the session moved to another tab');
        this.point?.close();
        this.point = null;
        this.ctrlWire?.close();
        this.ctrlWire = null;
        this.connected = null;
        this.mode = 'moved';
        this.status = 'Session moved to your other tab — this one is done.';
        this.actionLabel = '';
        this.resetLabel = 'start a fresh pairing here';
    }

    private invite(): void {
        void this.startSide(null);
    }

    private reset(): void {
        // The way out of any session — waiting, connected, dropped, spent or
        // moved — is a fresh page: links are consumed once, so a reload
        // lands on the clean invite page and everything starts over.
        location.reload();
    }

    private onCopyLink(): void {
        // The panel already cancels the anchor's navigation; here we just
        // put the link on the clipboard.
        void copied(this.link, 'Link').then((outcome) => {
            this.status = outcome;
        });
    }

    private onAction(): void {
        void this.takeOver();
    }

    private async scan(): Promise<void> {
        const side = this.side!;
        try {
            this.status = 'Point the camera at their code…';
            this.scanning = true;
            await this.updateComplete;
            const raw = await scanFor(this.querySelector('video')!, side.payload);
            void this.pair(side, linkPayload(raw));
        } catch (error) {
            this.status = `Camera unavailable (${error}) — paste their link instead.`;
        } finally {
            this.scanning = false;
        }
    }

    private onConnect(event: CustomEvent<string>): void {
        const pasted = (event.detail ?? '').trim();
        if (!pasted) {
            this.status = 'Paste their link (or token) first.';
            return;
        }
        const side = this.side!;
        void this.pair(side, linkPayload(pasted));
    }

    render() {
        // The shared panel is the whole UI; this element only feeds it state
        // and answers its intents. The camera video stays here — a
        // browser-only surface the panel does not carry.
        return html`<pair-panel
                .mode=${this.mode}
                .link=${this.link}
                .status=${this.status}
                .connected=${this.connected}
                .resetLabel=${this.resetLabel}
                .actionLabel=${this.actionLabel}
                .canScan=${this.canScan}
                @invite=${this.invite}
                @reset=${this.reset}
                @connect=${this.onConnect}
                @copy-link=${this.onCopyLink}
                @action=${this.onAction}
                @scan=${this.scan}
            ></pair-panel>
            <video ?hidden=${!this.scanning} playsinline></video>`;
    }
}
customElements.define('pair-wizard', PairWizard);
