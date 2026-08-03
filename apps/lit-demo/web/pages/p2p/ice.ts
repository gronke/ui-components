// The page's ICE policy. The library defaults to no iceServers (one
// network, mDNS candidates); this page opts into a public STUN server so
// peers on different networks still find a route, and 'uic-ice' in
// localStorage appends any further RTCIceServer list; a TURN relay with
// credentials makes hostile NATs reachable without putting a server in
// the repo.
import type { PairOptions } from '../@gronke/uic-sync/pair.js';

export function iceConfig(): PairOptions {
    const iceServers: RTCIceServer[] = [{ urls: 'stun:stun.l.google.com:19302' }];
    const extra = localStorage.getItem('uic-ice');
    if (extra) {
        try {
            const parsed = JSON.parse(extra) as RTCIceServer[];
            if (Array.isArray(parsed)) {
                iceServers.push(...parsed);
            } else {
                console.warn('[p2p] uic-ice ignored: expected a JSON array of RTCIceServer');
            }
        } catch (error) {
            console.warn('[p2p] uic-ice ignored, not valid JSON:', error);
        }
    }
    return { iceServers };
}
