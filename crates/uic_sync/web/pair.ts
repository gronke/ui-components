// Serverless WebRTC pairing: the offer and answer travel as compact text —
// small enough for a QR code — instead of full SDP through a signaling
// server. Candidates gather completely before encoding (no trickle), and
// their addresses ride verbatim: browsers hand out mDNS hostnames, which
// resolve between peers on one network. Hostile NATs would need iceServers
// (STUN/TURN); the default is none.

import { DataChannelWire } from './wire.js';
import type { Wire } from './wire.js';

const PREFIX = 'uics1.';
const GATHER_TIMEOUT_MS = 3000;

export interface PairOptions {
    iceServers?: RTCIceServer[];
}

export interface PairHost {
    /** The compact offer — put it in a QR code or hand it over any way. */
    offer: string;
    /** Feeds the guest's compact answer back; resolves on channel open. */
    complete(answer: string): Promise<Wire>;
    close(): void;
}

export interface PairGuest {
    /** The compact answer to travel back to the host. */
    answer: string;
    /** Resolves when the host's data channel arrives and opens. */
    wire: Promise<Wire>;
    close(): void;
}

export interface PairSwap {
    /** The symmetric payload — both sides exchange theirs blindly. */
    payload: string;
    /** Feeds the peer's swap payload; resolves when the channel opens. */
    connect(peer: string): Promise<Wire>;
    /** True once a connect attempt consumed the offer — a spent swap can
     * never pair again (succeed or fail), only a fresh one can. */
    spent(): boolean;
    close(): void;
}

/** The compact payload: ice credentials, DTLS fingerprint, setup role and
 * the candidate [address, port] tuples — everything a minimal
 * data-channel-only SDP rebuilds from. */
interface Compact {
    u: string;
    p: string;
    f: string;
    s: string;
    c: [string, number][];
}

/** WebKit hides RTCPeerConnection from in-app browsers and non-HTTPS
 * pages — a ReferenceError names the variable, this names the way out. */
function requireRtc(): void {
    if (typeof RTCPeerConnection === 'undefined') {
        throw new Error(
            'uic-sync pair: WebRTC is unavailable in this browser context — in-app browsers and non-HTTPS pages often hide it; open the page in a regular browser over HTTPS',
        );
    }
}

/** The role a compact payload plays — offers negotiate (`actpass`), answers
 * commit to a side. `null` for text that is no payload at all; the guards
 * below and scanners deciding whether to keep looking both branch on it. */
export function payloadRole(text: string): 'offer' | 'answer' | null {
    let compact: Compact;
    try {
        compact = decodePayload(text);
    } catch {
        return null;
    }
    if (compact.s === 'actpass') {
        return 'offer';
    }
    if (compact.s === 'active' || compact.s === 'passive') {
        return 'answer';
    }
    return null;
}

export async function createHost(options?: PairOptions): Promise<PairHost> {
    requireRtc();
    const pc = new RTCPeerConnection({ iceServers: options?.iceServers ?? [] });
    const channel = pc.createDataChannel('uic-sync');
    await pc.setLocalDescription(await pc.createOffer());
    await gatheringComplete(pc);
    const offer = encodePayload(parseSdp(pc.localDescription!.sdp));
    return {
        offer,
        async complete(answer: string): Promise<Wire> {
            if (payloadRole(answer) !== 'answer') {
                throw new Error(
                    'uic-sync pair: expected an answer payload — the other device answers after opening the offer link',
                );
            }
            const compact = decodePayload(answer);
            await pc.setRemoteDescription({ type: 'answer', sdp: buildSdp(compact) });
            await openOrFail(pc, channel);
            return new DataChannelWire(channel);
        },
        close(): void {
            pc.close();
        },
    };
}

export async function join(offer: string, options?: PairOptions): Promise<PairGuest> {
    requireRtc();
    if (payloadRole(offer) !== 'offer') {
        throw new Error('uic-sync pair: expected an offer payload');
    }
    const pc = new RTCPeerConnection({ iceServers: options?.iceServers ?? [] });
    const wire = new Promise<Wire>((resolve, reject) => {
        pc.addEventListener('datachannel', (event) => {
            openOrFail(pc, event.channel).then(
                () => resolve(new DataChannelWire(event.channel)),
                reject,
            );
        });
        pc.addEventListener('connectionstatechange', () => {
            if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
                reject(new Error(UNREACHABLE));
            }
        });
    });
    const compact = decodePayload(offer);
    await pc.setRemoteDescription({ type: 'offer', sdp: buildSdp(compact) });
    await pc.setLocalDescription(await pc.createAnswer());
    await gatheringComplete(pc);
    const answer = encodePayload(parseSdp(pc.localDescription!.sdp));
    return {
        answer,
        wire,
        close(): void {
            pc.close();
        },
    };
}

/** Symmetric, order-free pairing: BOTH sides create offers over a
 * negotiated data channel (stream 0 on either end, no in-band
 * announcement), exchange payloads blindly, and each synthesizes the
 * peer's ANSWER locally — the DTLS roles derive deterministically from the
 * fingerprints (the lower one plays the client), and ICE resolves its own
 * role conflict. Neither side needs to know who "started". */
export async function swap(options?: PairOptions): Promise<PairSwap> {
    requireRtc();
    const pc = new RTCPeerConnection({ iceServers: options?.iceServers ?? [] });
    const channel = pc.createDataChannel('uic-sync', { negotiated: true, id: 0 });
    await pc.setLocalDescription(await pc.createOffer());
    await gatheringComplete(pc);
    const mine = parseSdp(pc.localDescription!.sdp);
    const payload = encodePayload(mine);
    let consumed = false;
    return {
        payload,
        async connect(peer: string): Promise<Wire> {
            const compact = decodePayload(peer);
            if (compact.s !== 'actpass') {
                throw new Error("uic-sync pair: swap expects the peer's own swap payload");
            }
            if (compact.f === mine.f) {
                throw new Error(
                    'uic-sync pair: that is this side’s own payload — send it to the peer and open theirs',
                );
            }
            // A second payload after the exchange would tear at the live
            // connection — one swap pairs exactly once.
            if (pc.signalingState !== 'have-local-offer') {
                throw new Error('uic-sync pair: this swap already paired');
            }
            const peerRole = compact.f < mine.f ? 'active' : 'passive';
            consumed = true;
            await pc.setRemoteDescription({
                type: 'answer',
                sdp: buildSdp({ ...compact, s: peerRole }),
            });
            await openOrFail(pc, channel);
            return new DataChannelWire(channel);
        },
        spent(): boolean {
            return consumed;
        },
        close(): void {
            pc.close();
        },
    };
}

function gatheringComplete(pc: RTCPeerConnection): Promise<void> {
    if (pc.iceGatheringState === 'complete') {
        return Promise.resolve();
    }
    return new Promise((resolve) => {
        const timer = setTimeout(resolve, GATHER_TIMEOUT_MS);
        pc.addEventListener('icegatheringstatechange', () => {
            if (pc.iceGatheringState === 'complete') {
                clearTimeout(timer);
                resolve();
            }
        });
    });
}

const UNREACHABLE =
    'uic-sync pair: the peers could not reach each other — on one network, check that devices may talk to each other; across networks this demo ships no TURN relay';

/** Resolves when the channel opens; rejects when the connection gives up —
 * a forever-hanging "Connecting…" tells nobody anything. The listener
 * outlives the promise on purpose: a transport that dies LATER (the peer's
 * tab closed, the network went away) closes the channel, because the
 * browser leaves an abruptly abandoned channel dangling "open" and the
 * wire's onClose would otherwise never hear the end. */
function openOrFail(pc: RTCPeerConnection, channel: RTCDataChannel): Promise<void> {
    return new Promise((resolve, reject) => {
        if (channel.readyState === 'open') {
            resolve();
        } else if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
            reject(new Error(UNREACHABLE));
            return;
        } else {
            channel.addEventListener('open', () => resolve(), { once: true });
        }
        pc.addEventListener('connectionstatechange', () => {
            if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
                reject(new Error(UNREACHABLE));
                // close() on a dead transport never finishes its closing
                // procedure, so the close event would never fire on its
                // own — dispatch it so the wire hears the end either way.
                channel.close();
                channel.dispatchEvent(new Event('close'));
            }
        });
    });
}

function required(sdp: string, pattern: RegExp, what: string): string {
    const match = pattern.exec(sdp);
    if (!match) {
        throw new Error(`uic-sync pair: no ${what} in the local description`);
    }
    return match[1]!;
}

function parseSdp(sdp: string): Compact {
    const candidates: [string, number][] = [];
    const seen = new Set<string>();
    const pattern = /^a=candidate:\S+ 1 (?:udp|UDP) \S+ (\S+) (\d+) typ (?:host|srflx|relay)/gm;
    for (let match = pattern.exec(sdp); match; match = pattern.exec(sdp)) {
        const key = `${match[1]}:${match[2]}`;
        if (!seen.has(key)) {
            seen.add(key);
            candidates.push([match[1]!, Number(match[2])]);
        }
    }
    if (candidates.length === 0) {
        throw new Error('uic-sync pair: no usable candidates gathered');
    }
    return {
        u: required(sdp, /^a=ice-ufrag:(.+)$/m, 'ice-ufrag'),
        p: required(sdp, /^a=ice-pwd:(.+)$/m, 'ice-pwd'),
        f: required(sdp, /^a=fingerprint:sha-256 (.+)$/m, 'sha-256 fingerprint'),
        s: required(sdp, /^a=setup:(.+)$/m, 'setup role'),
        c: candidates,
    };
}

function buildSdp(compact: Compact): string {
    const candidates = compact.c.map(
        ([address, port], index) =>
            `a=candidate:${index + 1} 1 udp ${2113937151 - index} ${address} ${port} typ host`,
    );
    return [
        'v=0',
        'o=- 0 0 IN IP4 127.0.0.1',
        's=-',
        't=0 0',
        'a=group:BUNDLE 0',
        'm=application 9 UDP/DTLS/SCTP webrtc-datachannel',
        'c=IN IP4 0.0.0.0',
        ...candidates,
        `a=ice-ufrag:${compact.u}`,
        `a=ice-pwd:${compact.p}`,
        `a=fingerprint:sha-256 ${compact.f}`,
        `a=setup:${compact.s}`,
        'a=mid:0',
        'a=sctp-port:5000',
        'a=max-message-size:262144',
        '',
    ].join('\r\n');
}

function encodePayload(compact: Compact): string {
    const bytes = new TextEncoder().encode(JSON.stringify(compact));
    let binary = '';
    for (const byte of bytes) {
        binary += String.fromCharCode(byte);
    }
    return PREFIX + btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function decodePayload(text: string): Compact {
    const trimmed = text.trim();
    if (!trimmed.startsWith(PREFIX)) {
        throw new Error(`uic-sync pair: payload does not start with ${JSON.stringify(PREFIX)}`);
    }
    const base64 = trimmed.slice(PREFIX.length).replace(/-/g, '+').replace(/_/g, '/');
    const binary = atob(base64);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return JSON.parse(new TextDecoder().decode(bytes)) as Compact;
}
