# ADR 0028: Pairing is a serverless mutual exchange

## Decision

Two peers pair with no third party: each side creates an invite and opens the other's — a link, a QR or a pasted code — and the two connect once each has applied the other's payload.
Both sides create WebRTC offers over one negotiated data channel (stream 0 on either end, no in-band announcement), exchange compact payloads blindly, and each synthesizes the peer's ANSWER locally — the DTLS roles derive deterministically from the fingerprints (the lower one plays the client), so neither side needs to be "first".
Candidates gather completely before encoding (no trickle), and their addresses travel verbatim: browsers hand out mDNS hostnames, and rewriting them breaks the handshake.
Guards keep the both-ways step honest: a swap accepts only an offer-role payload ("the peer's own swap payload"), refuses this side's own by fingerprint, and pairs exactly once — a spent swap never pairs again, only a fresh one.

An invite is one link with the payload as its bare fragment (`#<payload>`, no query parameters), so a chat app linkifies the whole URL and the payload never reaches a server.
A link is consumed exactly once: the page reads the fragment, then `history.replaceState` strips it, so a reload lands on the clean invite page and nothing lingers in the address bar or history.
A link answering an opened invite appends the reply digest — `#<payload>.<digest8>`, fnv1a-32 of the invite payload, still one URL-safe token with every parser cutting before the dot — so the same-browser handover (ADR 0032) routes a reply to the exact tab whose invite it answers, and several pairings can wait in different tabs at once.
The digest is routing only; the payload's own credential guards stay the security.

The payload's wire form is a binary layout in bare base64url — no prefix, no JSON.
The shape is declared once per language as a layout table the twins mirror verbatim (`uic_sync::pair` `LAYOUT`, `web/pair.ts` `LAYOUT`), and a small interpreter walks it: `u`/`p` (ice credentials) as `str8`, `f` (the fingerprint) as `hex32`, `s` (the setup role) as `enum("actpass", "active", "passive")`, `c` (the candidates) as `addrs8`.
The kinds: `str8` is a u8 length plus ASCII, `hex32` the sha-256 fingerprint's 32 raw bytes shown as colon-hex, `enum` a u8 index into its values, and `addrs8` a u8 count of tagged address plus big-endian u16 port entries — IPv4 as 4 bytes, IPv6 as 16, an mDNS `<uuid>.local` name as its 16 uuid bytes, anything else length-prefixed ASCII.
There is no version marker: structural validation replaces it — length prefixes must fit, the setup byte and address tags are constrained, every byte must be consumed and at least one candidate be present — and in a link the fragment position is the discriminator.
A golden vector pins the bytes across the twins: the captured Chrome-shaped payload must decode to the same `Compact` and re-encode to the exact same 120 characters in both languages, and the reply digest rides the same discipline (`reply_digest("abc") == "1a47e90b"`, pinned in both).
The whole codec surface is cross-pinned this way: `Compact`, `encode_payload`/`decode_payload`, `parse_sdp`/`build_sdp`, `payload_role`, `link_payload`, `invite_link`, `reply_digest`, and the `uicc1.` `Ctrl` frame codec (ADR 0032).

The lit-demo's terminal is a first-class peer (`p2p [link-or-code]`): it generates invites through the shared panel (ADR 0029) — linking to the published pairing page, `UIC_LIT_DEMO_P2P_PAGE` pointing elsewhere — and opens a browser's invite, whose reply link pastes back into the panel by hand.
`uic_sync::pair` is codec only; the WebRTC stack (`webrtc-rs`, pinned to 0.17 — its master is a sans-io rewrite) lives in the demo app, never the libraries.
The terminal runs as an ICE-LITE agent: the browser's swap always stays ICE-controlling (it applies a locally synthesized answer), and webrtc-rs does not resolve two controlling agents — it ignores a same-role peer's connectivity checks outright — so the terminal must take the controlled side, and lite is the one arrangement that yields it while still publishing an offer-role (`actpass`) payload the browser accepts.
The terminal's payload carries no `a=ice-lite`, so the browser still sees a full peer and stays controlling; the terminal knows it is lite locally, and that is enough.
The `Setup` enum (`ActPass`/`Active`/`Passive`) spells the role vocabulary in wire order and makes an invalid role unrepresentable, which keeps `encode_payload` infallible.

`iceServers` defaults to none: host candidates connect peers on one network without STUN, TURN or a signaling server.
The `/p2p` page opts into a public STUN server on top, and the `uic-ice` localStorage knob appends any further `RTCIceServer` list — a TURN server with credentials makes hostile NATs reachable without putting a server in the repo.
The terminal keeps the empty default: a lite agent gathers host candidates only, so STUN would give it nothing.

## Why

A rendezvous relay (ntfy.sh) once carried the reply leg and was removed in favor of the mutual exchange.
No third party sees a connection offer, and the both-ways step people lose track of is guided by the UI instead: the shared panel shows "share your invite" and "open their invite" as equal halves (ADR 0029).
The compact payload is engine-agnostic by design — ice credentials, a DTLS fingerprint, a setup role, `[address, port]` candidates — so a Rust peer emits and consumes the exact same strings as a browser, and the terminal is the thing the payload always described: a peer that generates and opens invites.
The binary layout exists because a JSON form spends its characters on representation, not entropy: the fingerprint wore 95 hex-and-colon characters for 32 bytes, an mDNS uuid 42 characters for 16, and a format marker another 6 — the pinned payload drops from ~330 base64 characters to 120, short enough for chat apps that refused to linkify the long form, and a QR several versions smaller, which the terminal grid feels directly.
Compression was the rejected alternative: the ice credentials and the fingerprint are high-entropy and barely deflate, a compressor lands in both languages, and non-deterministic output would break the byte-pinning.
A standardized container (CBOR and kin) was rejected too: browsers ship no native codec, a vendored library plus a Rust dependency would wrap five fields, and the tuple headers cost back part of the shortening — the declared layout table gives the write-it-once shape without either price.

## Consequences

- Every pairing is two-way: both sides share a datum — the deliberate tradeoff for keeping payloads off any third party.
- Lite gathers host candidates only (no srflx), so the terminal pairs on a shared network — the same LAN, a personal hotspot — while crossing NATs stays out of scope, consistent with the demo shipping no TURN relay.
  Loopback candidates ride along so a terminal and a browser on one host pair directly.
- `payload_role`/`payloadRole` classify by decoding — the structural checks make random text fail reliably (bad base64, unfitting lengths, alien setup and tag bytes, trailing bytes) — which is what the camera scanner and the paste guards lean on.
- IPv6 addresses canonicalize through the decode (`::1` for `0:0:0:0:0:0:0:1`); each side feeds its locally decoded strings into its own SDP rebuild, so cross-language string identity is not required — the encoded bytes are, and the vector pins them.
- The connected channel is just another string wire: the browser attaches it, the terminal pumps its live bridge through it, and the state protocol is ADR 0013's unchanged.
- The Rust↔Rust loopback test pairs one lite (controlled) side against one full (controlling) side — the production roles exactly — and proves the codec, the synthesized answer, DTLS, SCTP, the data channel, the greet rule and the control-plane split end to end; a real browser↔terminal pairing needs a network the two share, so that leg is verified on a real LAN, not in CI.
- Camera scanning (`BarcodeDetector` + `getUserMedia`) needs a secure context; the paste path is the always-present one, and paste boxes accept links and bare codes alike.
- mDNS candidate resolution assumes one non-isolated network; AP-isolated Wi-Fi needs `iceServers` and possibly more.
- A reply whose inviting tab is gone refuses with guidance instead of adopting into a swap that could never satisfy the peer's credentials — a session lives in its tab, because WebRTC state cannot be persisted (ADR 0032).
- Demo-grade scope holds: no reconnect, the terminal's reply leg goes by hand, and `UIC_LIT_DEMO_ICE_DEBUG` traces the candidates when a pairing will not come up.
