# uic_sync

State sync tooling for lit components (ADR 0024): the listed reactive properties of a root component travel as one canonical snapshot over whatever wire carries text — the envelope-less, last-writer-wins protocol of ADR 0013.

The TypeScript modules under `web/` carry the browser runtime; the Rust `pair` module (`src/pair.rs`) is the compact-payload codec in Rust too, byte-compatible with `web/pair.ts`, so a native peer (ADR 0028) exchanges the same `uics1.` strings.

| Module | Provides |
| --- | --- |
| `codec` | `encode`/`decode`: tagged structured-clone JSON (Date, Map, Set beside plain JSON), canonical — object keys sort at every depth, so byte equality is state equality |
| `wire` | The `Wire` seam (`send`/`onMessage`/`onOpen`/`onClose`/`close`) with `WebSocketWire`, `DataChannelWire`, `BroadcastWire`; `onClose` fires once when the wire goes away, however it goes |
| `sync` | `attach(root, { fields, wire, greet?, event? })`: snapshots out on the root's `state-changed`, inbound snapshots assign back; one `last` slot plus ready/applying flags keep echo and boot quiet |
| `pair` | Serverless WebRTC pairing over compact `uics1.` payloads (QR-sized) instead of full SDP through a signaling server. `swap()` is the symmetric, order-free form: both sides create offers over a negotiated channel, exchange payloads blindly, and each synthesizes the peer's answer with fingerprint-derived DTLS roles. `createHost()`/`join(offer)` remain the classic directed form; `payloadRole` classifies payloads, every entry refuses the wrong kind with a plain message before any RTC call, and a swap whose offer is consumed says so via `spent()` |

Exactly one side greets (`greet: true`): the party holding the canonical state — a server, a pairing host — announces it on open, and the other side waits.
Pairing gathers candidates completely before encoding and carries their addresses verbatim (browsers hand out mDNS hostnames, which resolve between peers on one network); crossing networks takes `iceServers` (STUN puts server-reflexive addresses into the payload, TURN relayed ones), the default is none.
When the peers cannot reach each other the pending wire rejects with a plain message once the connection reports failure — no forever-hanging "Connecting…".
Demo-grade by scope: no reconnect, no arbiter beyond last-writer-wins, plain-JSON state on the terminal side.

Consumers integrate one of two ways: hand `web_root()` to a `web_modules` build as an extra source root, or emit the compiled npm tree with `npm_tree(out, version)` and install `@schuhkarton/uic-sync` like any package.
The published tree has no dependencies; QR rendering stays with the consumer (the payloads are plain text).

A native peer imports the Rust `pair` module: `Compact`, `encode_payload`/`decode_payload` (the `uics1.` base64url), `parse_sdp`/`build_sdp` (the minimal data-channel SDP), `payload_role` and `link_payload` — the same functions `web/pair.ts` runs, cross-pinned by a captured browser vector so the bytes never drift. The WebRTC stack stays with the consumer (the lit-demo drives `webrtc-rs`); this crate is codec only.

```sh
cargo test -p uic_sync
```
