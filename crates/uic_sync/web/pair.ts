// Serverless WebRTC pairing: the offer and answer travel as compact text
// (small enough for a QR code) instead of full SDP through a signaling
// server. Candidates gather completely before encoding (no trickle), and
// their addresses ride verbatim: browsers hand out mDNS hostnames, which
// resolve between peers on one network. Hostile NATs would need iceServers
// (STUN/TURN); the default is none.

import { DataChannelWire } from './wire.js';
import type { Wire } from './wire.js';

// A four-byte `uic1` magic head prefixes the payload, so any reader tells a
// uic:p2p credential for certain; the binary layout self-validates, and in a
// link the fragment position is the discriminator.
const GATHER_TIMEOUT_MS = 3000;

// How long a connect waits for the channel to open before giving up. The
// return link travels by hand, so the peer may take many seconds to open it;
// a transient `failed` along the way is not the end (the browser can report
// it just before the channel comes up), so the wait is bounded by this clock
// rather than by the first `failed`.
const CONNECT_TIMEOUT_MS = 45000;

/** The reply-routing digest (fnv1a-32, 8 hex chars) of an invite payload. A
 * return link answering an invite carries `.{digest}` after its own payload,
 * so the same-browser handover routes the reply to the exact tab that
 * invited. A routing hint only; the payload's own credential guards stay
 * the security. Rust twin: `src/pair.rs` `reply_digest`; the pinned vector
 * is replyDigest('abc') === '1a47e90b'. */
export function replyDigest(payload: string): string {
    let hash = 0x811c9dc5;
    for (const byte of new TextEncoder().encode(payload)) {
        hash ^= byte;
        hash = Math.imul(hash, 0x01000193) >>> 0;
    }
    return hash.toString(16).padStart(8, '0');
}

export interface PairOptions {
    iceServers?: RTCIceServer[];
}

/** The payload carried by a link, pasted text or scanned code: the invite
 * is the whole fragment, so the payload sits after `#` in a link and at
 * the front of a bare code: the base64url run from there (a reply link's
 * `.{digest}` suffix is cut with the rest). Text with no payload passes
 * through for the decode to reject. Rust twin: `src/pair.rs`
 * `link_payload`. */
export function linkPayload(text: string): string {
    const trimmed = text.trim();
    const match = /#([A-Za-z0-9_-]+)/.exec(trimmed) ?? /^([A-Za-z0-9_-]+)/.exec(trimmed);
    return match ? match[1]! : trimmed;
}

/** The reply-routing digest riding a return link (`#<payload>.<digest>`):
 * the invite it answers, so the same-browser handover reaches the exact
 * tab that sent it. Null on a plain invite. */
export function linkReply(text: string): string | null {
    const match = /#[A-Za-z0-9_-]+\.([A-Za-z0-9_-]{4,16})/.exec(text);
    return match ? match[1]! : null;
}

/** Builds an invite link the pairing page opens: the payload as a single
 * URL-safe fragment (base64url needs no escaping), so a chat app linkifies
 * the whole URL; `replyTo` appends the reply digest. Rust twin:
 * `src/pair.rs` `invite_link`. */
export function inviteLink(pageHref: string, payload: string, replyTo?: string): string {
    return replyTo ? `${pageHref}#${payload}.${replyTo}` : `${pageHref}#${payload}`;
}

export interface PairSwap {
    /** The symmetric payload; both sides exchange theirs blindly. */
    payload: string;
    /** Feeds the peer's swap payload; resolves when the channel opens. */
    connect(peer: string): Promise<Wire>;
    /** True once a connect attempt consumed the offer; a spent swap can
     * never pair again (succeed or fail), only a fresh one can. */
    spent(): boolean;
    close(): void;
}

/** The compact payload: ice credentials, DTLS fingerprint, setup role and
 * the candidate [address, port] tuples: everything a minimal
 * data-channel-only SDP rebuilds from. */
interface Compact {
    u: string;
    p: string;
    f: string;
    s: string;
    c: [string, number][];
}

/** WebKit hides RTCPeerConnection from in-app browsers and non-HTTPS
 * pages; a ReferenceError names the variable, this names the way out. */
function requireRtc(): void {
    if (typeof RTCPeerConnection === 'undefined') {
        throw new Error(
            'uic-sync pair: WebRTC is unavailable in this browser context — in-app browsers and non-HTTPS pages often hide it; open the page in a regular browser over HTTPS',
        );
    }
}

/** The role a compact payload plays: offers negotiate (`actpass`), answers
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

/** Symmetric, order-free pairing: BOTH sides create offers over a
 * negotiated data channel (stream 0 on either end, no in-band
 * announcement), exchange payloads blindly, and each synthesizes the
 * peer's ANSWER locally; the DTLS roles derive deterministically from the
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
            // connection; one swap pairs exactly once.
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

/** Resolves when the channel opens; rejects on a bounded timeout so a
 * forever-hanging "Connecting…" cannot happen. A transient `failed` is
 * tolerated on the way up; the peer opens the return link by hand, and a
 * browser (Safari especially) can flag `failed` a moment before the channel
 * comes up, so the clock bounds the wait, not the first `failed`; only a
 * deliberate `close()` (`connectionState === 'closed'`) is terminal at once.
 * The state listener outlives the promise on purpose: a transport that dies
 * AFTER we connected (the peer's tab closed, the network went away) closes
 * the channel, because the browser leaves an abruptly abandoned channel
 * dangling "open" and the wire's onClose would otherwise never hear the end. */
function openOrFail(pc: RTCPeerConnection, channel: RTCDataChannel): Promise<void> {
    return new Promise((resolve, reject) => {
        let settled = false;
        // A dead transport never finishes close()'s procedure, so its close
        // event would never fire on its own; dispatch it so the wire hears
        // the end either way.
        const shutTheChannel = () => {
            channel.close();
            channel.dispatchEvent(new Event('close'));
        };
        const timer = setTimeout(() => {
            if (settled) {
                return;
            }
            settled = true;
            reject(new Error(UNREACHABLE));
            shutTheChannel();
        }, CONNECT_TIMEOUT_MS);
        const succeed = () => {
            if (settled) {
                return;
            }
            settled = true;
            clearTimeout(timer);
            resolve();
        };

        if (channel.readyState === 'open') {
            succeed();
        } else if (pc.connectionState === 'closed') {
            settled = true;
            clearTimeout(timer);
            reject(new Error(UNREACHABLE));
            shutTheChannel();
            return;
        } else {
            channel.addEventListener('open', succeed, { once: true });
        }

        pc.addEventListener('connectionstatechange', () => {
            if (pc.connectionState !== 'failed' && pc.connectionState !== 'closed') {
                return;
            }
            if (settled) {
                // Already connected: a LATER death, so close the channel and
                // let the wire hear it.
                shutTheChannel();
            } else if (pc.connectionState === 'closed') {
                // A deliberate teardown before we connected is terminal; a
                // bare `failed` is not; the timer bounds that wait.
                settled = true;
                clearTimeout(timer);
                reject(new Error(UNREACHABLE));
                shutTheChannel();
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

// The wire layout, declaratively: the single place the payload's shape
// lives. The Rust twin mirrors this table verbatim (`src/pair.rs`,
// `LAYOUT`): field order, kinds and enum values must match byte for byte,
// and the Rust golden vector pins them. Kinds: `str8` is u8 length + ASCII,
// `hex32` 32 raw bytes shown as colon-hex, `enum` a u8 index into its
// values, `addrs8` a u8 count of tagged address + big-endian u16 port
// entries (IPv4 = 4 bytes, IPv6 = 16, an mDNS `<uuid>.local` = its 16 uuid
// bytes, anything else length-prefixed ASCII).
const LAYOUT = [
    ['u', 'str8'],
    ['p', 'str8'],
    ['f', 'hex32'],
    ['s', 'enum', ['actpass', 'active', 'passive']],
    ['c', 'addrs8'],
] as const;

const ADDR_V4 = 0;
const ADDR_V6 = 1;
const ADDR_MDNS = 2;
const ADDR_NAME = 3;

// The payload's magic head ("uic1"): four bytes so any reader tells a
// uic:p2p credential for certain instead of guessing from structure;
// decodePayload rejects anything without it. Rust twin: `MAGIC` in
// `src/pair.rs`.
const MAGIC = [0x75, 0x69, 0x63, 0x31];

function encodePayload(compact: Compact): string {
    const out: number[] = [...MAGIC];
    for (const entry of LAYOUT) {
        const [name, kind] = entry;
        const value = compact[name];
        if (kind === 'str8') {
            pushShort(out, value as string);
        } else if (kind === 'hex32') {
            for (const pair of (value as string).split(':')) {
                out.push(parseInt(pair, 16) || 0);
            }
        } else if (kind === 'enum') {
            const index = (entry[2] as readonly string[]).indexOf(value as string);
            if (index < 0) {
                throw new Error(`uic-sync pair: unknown ${name} value ${JSON.stringify(value)}`);
            }
            out.push(index);
        } else {
            const addrs = value as [string, number][];
            out.push(addrs.length);
            for (const [address, port] of addrs) {
                pushAddr(out, address);
                out.push((port >> 8) & 0xff, port & 0xff);
            }
        }
    }
    let binary = '';
    for (const byte of out) {
        binary += String.fromCharCode(byte);
    }
    return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function decodePayload(text: string): Compact {
    const base64 = text.trim().replace(/-/g, '+').replace(/_/g, '/');
    const binary = atob(base64);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    let at = 0;
    const take = (len: number): Uint8Array => {
        if (at + len > bytes.length) {
            throw new Error('uic-sync pair: payload is truncated');
        }
        const slice = bytes.subarray(at, at + len);
        at += len;
        return slice;
    };
    const byte = (): number => take(1)[0]!;
    const short = (): string => new TextDecoder().decode(take(byte()));

    const head = take(MAGIC.length);
    if (!MAGIC.every((expected, i) => head[i] === expected)) {
        throw new Error('uic-sync pair: not a uic:p2p payload');
    }

    const fields: Record<string, unknown> = {};
    for (const entry of LAYOUT) {
        const [name, kind] = entry;
        if (kind === 'str8') {
            fields[name] = short();
        } else if (kind === 'hex32') {
            fields[name] = [...take(32)]
                .map((b) => b.toString(16).padStart(2, '0').toUpperCase())
                .join(':');
        } else if (kind === 'enum') {
            const value = (entry[2] as readonly string[])[byte()];
            if (!value) {
                throw new Error(`uic-sync pair: unknown ${name} byte`);
            }
            fields[name] = value;
        } else {
            const count = byte();
            const addrs: [string, number][] = [];
            for (let i = 0; i < count; i++) {
                const address = takeAddr(take, byte, short);
                const port = take(2);
                addrs.push([address, (port[0]! << 8) | port[1]!]);
            }
            fields[name] = addrs;
        }
    }
    // Structural validation: every byte consumed, at least one candidate.
    if (at !== bytes.length) {
        throw new Error('uic-sync pair: payload has trailing bytes');
    }
    const compact = fields as unknown as Compact;
    if (compact.c.length === 0) {
        throw new Error('uic-sync pair: payload carries no candidates');
    }
    return compact;
}

/** One tagged address entry. */
function pushAddr(out: number[], address: string): void {
    const uuid = mdnsUuidBytes(address);
    const v4 = ipv4Bytes(address);
    const v6 = uuid || v4 ? null : ipv6Bytes(address);
    if (uuid) {
        out.push(ADDR_MDNS, ...uuid);
    } else if (v4) {
        out.push(ADDR_V4, ...v4);
    } else if (v6) {
        out.push(ADDR_V6, ...v6);
    } else {
        out.push(ADDR_NAME);
        pushShort(out, address);
    }
}

function takeAddr(
    take: (len: number) => Uint8Array,
    byte: () => number,
    short: () => string,
): string {
    const tag = byte();
    if (tag === ADDR_V4) {
        return [...take(4)].join('.');
    }
    if (tag === ADDR_V6) {
        return ipv6String(take(16));
    }
    if (tag === ADDR_MDNS) {
        const hex = [...take(16)].map((b) => b.toString(16).padStart(2, '0')).join('');
        return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}.local`;
    }
    if (tag === ADDR_NAME) {
        return short();
    }
    throw new Error(`uic-sync pair: unknown address tag ${tag}`);
}

function pushShort(out: number[], text: string): void {
    const bytes = new TextEncoder().encode(text);
    out.push(bytes.length, ...bytes);
}

/** The 16 uuid bytes of an mDNS `<uuid>.local` candidate, or null. */
function mdnsUuidBytes(address: string): number[] | null {
    if (!address.endsWith('.local')) {
        return null;
    }
    const uuid = address.slice(0, -'.local'.length);
    const hex = uuid.replace(/-/g, '');
    if (uuid.length !== 36 || !/^[0-9a-fA-F]{32}$/.test(hex)) {
        return null;
    }
    const out: number[] = [];
    for (let i = 0; i < 32; i += 2) {
        out.push(parseInt(hex.slice(i, i + 2), 16));
    }
    return out;
}

function ipv4Bytes(address: string): number[] | null {
    const match = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(address);
    if (!match) {
        return null;
    }
    const octets = match.slice(1).map(Number);
    return octets.every((o) => o <= 255) ? octets : null;
}

/** IPv6 text into its 16 bytes: `::` expansion plus an optional embedded
 * IPv4 tail. Null when the text is no IPv6 address. */
function ipv6Bytes(address: string): number[] | null {
    if (!address.includes(':')) {
        return null;
    }
    let head = address;
    let v4tail: number[] = [];
    const lastColon = address.lastIndexOf(':');
    if (address.includes('.', lastColon)) {
        const v4 = ipv4Bytes(address.slice(lastColon + 1));
        if (!v4) {
            return null;
        }
        v4tail = v4;
        head = address.slice(0, lastColon) + ':0:0';
    }
    const halves = head.split('::');
    if (halves.length > 2) {
        return null;
    }
    const parse = (part: string): number[] | null => {
        if (part === '') {
            return [];
        }
        const groups = part.split(':');
        const out: number[] = [];
        for (const group of groups) {
            if (!/^[0-9a-fA-F]{1,4}$/.test(group)) {
                return null;
            }
            const value = parseInt(group, 16);
            out.push(value >> 8, value & 0xff);
        }
        return out;
    };
    const left = parse(halves[0]!);
    const right = halves.length === 2 ? parse(halves[1]!) : [];
    if (left === null || right === null) {
        return null;
    }
    const v4pad = v4tail.length; // replaces the trailing ':0:0' placeholder
    const total = left.length + right.length - (v4pad ? 4 : 0) + v4pad;
    if (halves.length === 1 && total !== 16) {
        return null;
    }
    if (total > 16) {
        return null;
    }
    const bytes = [...left, ...Array(16 - total).fill(0), ...right] as number[];
    if (v4pad) {
        bytes.splice(12, 4, ...v4tail);
    }
    return bytes.length === 16 ? bytes : null;
}

/** RFC 5952-style text for 16 IPv6 bytes: lowercase hex groups, the first
 * longest run of two or more zero groups compressed to `::`. */
function ipv6String(bytes: Uint8Array): string {
    const groups: number[] = [];
    for (let i = 0; i < 16; i += 2) {
        groups.push((bytes[i]! << 8) | bytes[i + 1]!);
    }
    let bestAt = -1;
    let bestLen = 0;
    for (let i = 0; i < 8; i++) {
        if (groups[i] !== 0) {
            continue;
        }
        let len = 0;
        while (i + len < 8 && groups[i + len] === 0) {
            len++;
        }
        if (len > bestLen) {
            bestAt = i;
            bestLen = len;
        }
        i += len;
    }
    if (bestLen < 2) {
        return groups.map((g) => g.toString(16)).join(':');
    }
    const left = groups.slice(0, bestAt).map((g) => g.toString(16));
    const right = groups.slice(bestAt + bestLen).map((g) => g.toString(16));
    return `${left.join(':')}::${right.join(':')}`;
}
