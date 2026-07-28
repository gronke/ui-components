// Cross-tab organization for p2p wires: which tab owns a pairing session,
// how a link opened in a fresh tab reaches it, and how a session hands over
// to another tab by re-signaling THROUGH its own live wire (a peer
// connection cannot move between documents, but a new one can be negotiated
// over the old one). Framework-free on purpose — BroadcastChannel is the
// only platform API, the wire stays the `Wire` seam — so this module can
// graduate into a package of its own.
//
// Three pieces:
// - `ControlWire` wraps a live wire with a control plane: `uicc1.`-prefixed
//   frames (canonical-codec JSON) carry protocol messages, everything else
//   passes through as state. The app attaches to the wrapper — `attach`'s
//   decode throws on non-JSON, so control frames must never reach it.
// - `TabSessions` is the same-browser handover: an opened link offers its
//   payload to the tabs, the owning tab claims it (a reply only in the tab
//   whose invite it answers), and a connected owner claims re-opened
//   replies so the opener can offer a takeover instead of a dead end.
// - `TakeoverPoint` is the per-session takeover handshake between the new
//   tab and the owning tab; the owner forwards it over the wire as `repair`
//   control frames, the remote answers with a fresh swap payload, and the
//   connection re-forms in the new tab.

import { decode, encode } from './codec.js';
import { replyDigest } from './pair.js';
import type { Wire } from './wire.js';

/** The control-frame marker on a state wire. State
 * snapshots are canonical JSON and always start with `{`, so the prefix is
 * unambiguous. Rust twin: `uic_sync::pair::CTRL_PREFIX`. */
const CTRL_PREFIX = 'uicc1.';

/** A protocol message riding the control plane. */
export interface ControlMessage {
    t: string;
    payload?: string;
}

/** How long an opened link waits for a claim before adopting or refusing. */
const CLAIM_WINDOW_MS = 400;

/** How long a takeover waits for the answer payload before giving up. */
export const TAKEOVER_TIMEOUT_MS = 15000;

/** A live wire with a control plane: protocol frames filter off before the
 * state consumer sees them. Exactly one underlying message listener fans
 * out internally, because some transports keep a single `onmessage` slot. */
export class ControlWire implements Wire {
    private state: ((text: string) => void) | null = null;
    private control: ((message: ControlMessage) => void) | null = null;

    constructor(private wire: Wire) {
        wire.onMessage((text) => {
            if (text.startsWith(CTRL_PREFIX)) {
                let message: ControlMessage;
                try {
                    message = decode(text.slice(CTRL_PREFIX.length)) as ControlMessage;
                } catch {
                    return;
                }
                this.control?.(message);
            } else {
                this.state?.(text);
            }
        });
    }

    sendControl(message: ControlMessage): void {
        this.wire.send(CTRL_PREFIX + encode(message));
    }

    onControl(callback: (message: ControlMessage) => void): void {
        this.control = callback;
    }

    send(text: string): void {
        this.wire.send(text);
    }

    onMessage(callback: (text: string) => void): void {
        this.state = callback;
    }

    onOpen(callback: () => void): void {
        this.wire.onOpen(callback);
    }

    onClose(callback: () => void): void {
        this.wire.onClose(callback);
    }

    close(): void {
        this.wire.close();
    }
}

/** What a claim tells the opening tab about the claiming session. */
export type ClaimStatus = 'waiting' | 'connected';

/** The session a waiting or connected tab serves to the handover channel —
 * all functions, because a takeover replaces the tab's swap in place. */
export interface ServedSession {
    /** This side's own swap payload. */
    payload(): string;
    /** The reply digests this session answers to: its own payload's, plus
     * an inherited one after a takeover (links in the wild keep naming the
     * original invite). */
    digests(): string[];
    /** True once a connect attempt consumed the swap. */
    spent(): boolean;
    /** True while the wire stands. */
    connected(): boolean;
    /** An unspent claim adopted this peer payload — pair with it. */
    onPeer(peer: string): void;
}

/** What became of an offered link payload. */
export interface OfferOutcomes {
    /** A tab claimed it; `status` says whether it paired or already stands. */
    handed(status: ClaimStatus): void;
    /** A plain invite nobody claimed — this tab becomes the second side. */
    adopt(): void;
    /** A reply whose inviting session no open tab holds. */
    orphan(): void;
}

/** The same-browser handover: one shared channel, module-scoped by the
 * consumer (Chrome garbage-collects unreferenced BroadcastChannels,
 * listeners and all). Absent BroadcastChannel (older WebKit), links always
 * adopt in their own tab and replies refuse. */
export class TabSessions {
    private channel: BroadcastChannel | null;

    constructor(name = 'uic-pair') {
        this.channel = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel(name);
    }

    /** Offers an opened link's payload to the tabs; a reply (`replyTo`) only
     * the tab whose invite it answers may claim. */
    offer(peer: string, replyTo: string | null, outcomes: OfferOutcomes): void {
        if (!this.channel) {
            if (replyTo) {
                outcomes.orphan();
            } else {
                outcomes.adopt();
            }
            return;
        }
        let claimed = false;
        this.channel.addEventListener('message', (event) => {
            const message = event.data as { type: string; payload?: string; status?: ClaimStatus };
            if (message.type === 'claimed' && message.payload === peer && !claimed) {
                claimed = true;
                outcomes.handed(message.status === 'connected' ? 'connected' : 'waiting');
            }
        });
        this.channel.postMessage({ type: 'peer', payload: peer, replyTo });
        setTimeout(() => {
            if (claimed) {
                return;
            }
            if (replyTo) {
                outcomes.orphan();
            } else {
                outcomes.adopt();
            }
        }, CLAIM_WINDOW_MS);
    }

    /** Serves a session to the channel: an unspent side claims and pairs; a
     * connected side claims its own replies so the opener can offer a
     * takeover instead of dead-ending. The claimer's answering payload is
     * addressed to the claimed payload, so no third tab grabs it. */
    serve(session: ServedSession): void {
        this.channel?.addEventListener('message', (event) => {
            const message = event.data as { type: string; payload?: string; replyTo?: string };
            if (message.type !== 'peer' || !message.payload || message.payload === session.payload()) {
                return;
            }
            if (message.replyTo && !session.digests().includes(message.replyTo)) {
                return;
            }
            if (!session.spent()) {
                this.channel?.postMessage({
                    type: 'claimed',
                    payload: message.payload,
                    status: 'waiting',
                });
                this.channel?.postMessage({
                    type: 'peer',
                    payload: session.payload(),
                    replyTo: replyDigest(message.payload),
                });
                session.onPeer(message.payload);
            } else if (session.connected() && message.replyTo) {
                // The reply answers THIS session and the session stands: tell
                // the opener, which then offers to take the session over.
                this.channel?.postMessage({
                    type: 'claimed',
                    payload: message.payload,
                    status: 'connected',
                });
            }
        });
    }
}

/** The per-session takeover handshake, keyed by the session digest both
 * sides already share (the reply-routing digest of the owner's payload).
 * The new tab `request`s with its fresh payload and resolves on the
 * remote's answer; the owning tab `onRequest`s, forwards over its wire as a
 * `repair` control frame, and `answer`s back what the remote returned. */
export class TakeoverPoint {
    private channel: BroadcastChannel;

    constructor(digest: string) {
        this.channel = new BroadcastChannel('uic-take-' + digest);
    }

    /** The new tab's half: offer the fresh payload, await the remote's. The
     * request re-posts until answered — BroadcastChannel keeps no history,
     * and the owner may still be connecting when the first post fires; the
     * owner ignores duplicates while a takeover is in flight. */
    request(ownPayload: string, timeoutMs = TAKEOVER_TIMEOUT_MS): Promise<string> {
        return new Promise((resolve, reject) => {
            const post = (): void => {
                this.channel.postMessage({ type: 'takeover', payload: ownPayload });
            };
            const again = setInterval(post, 2000);
            const timer = setTimeout(() => {
                clearInterval(again);
                reject(new Error('uic-session: the takeover timed out'));
            }, timeoutMs);
            this.channel.addEventListener('message', (event) => {
                const message = event.data as { type: string; payload?: string };
                if (message.type === 'takeover-answer' && message.payload) {
                    clearTimeout(timer);
                    clearInterval(again);
                    resolve(message.payload);
                }
            });
            post();
        });
    }

    /** The new tab reports the fresh wire opened; the old owner retires on
     * it. Carries the new owner's payload — the new tab serves this very
     * channel the moment it connects, and must not retire on its own cue. */
    done(byPayload: string): void {
        this.channel.postMessage({ type: 'takeover-done', payload: byPayload });
    }

    /** The owning tab's half. */
    onRequest(callback: (payload: string) => void): void {
        this.channel.addEventListener('message', (event) => {
            const message = event.data as { type: string; payload?: string };
            if (message.type === 'takeover' && message.payload) {
                callback(message.payload);
            }
        });
    }

    answer(payload: string): void {
        this.channel.postMessage({ type: 'takeover-answer', payload });
    }

    onDone(callback: (byPayload: string) => void): void {
        this.channel.addEventListener('message', (event) => {
            const message = event.data as { type: string; payload?: string };
            if (message.type === 'takeover-done') {
                callback(message.payload ?? '');
            }
        });
    }

    close(): void {
        this.channel.close();
    }
}
