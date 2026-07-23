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

export async function createHost(options?: PairOptions): Promise<PairHost> {
    const pc = new RTCPeerConnection({ iceServers: options?.iceServers ?? [] });
    const channel = pc.createDataChannel('uic-sync');
    await pc.setLocalDescription(await pc.createOffer());
    await gatheringComplete(pc);
    const offer = encodePayload(parseSdp(pc.localDescription!.sdp));
    return {
        offer,
        async complete(answer: string): Promise<Wire> {
            const compact = decodePayload(answer);
            await pc.setRemoteDescription({ type: 'answer', sdp: buildSdp(compact) });
            await channelOpen(channel);
            return new DataChannelWire(channel);
        },
        close(): void {
            pc.close();
        },
    };
}

export async function join(offer: string, options?: PairOptions): Promise<PairGuest> {
    const pc = new RTCPeerConnection({ iceServers: options?.iceServers ?? [] });
    const wire = new Promise<Wire>((resolve) => {
        pc.addEventListener('datachannel', (event) => {
            channelOpen(event.channel).then(() => resolve(new DataChannelWire(event.channel)));
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

function channelOpen(channel: RTCDataChannel): Promise<void> {
    if (channel.readyState === 'open') {
        return Promise.resolve();
    }
    return new Promise((resolve) => {
        channel.addEventListener('open', () => resolve(), { once: true });
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
    const pattern = /^a=candidate:\S+ 1 (?:udp|UDP) \S+ (\S+) (\d+) typ (?:host|srflx)/gm;
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
