# uic_sync

State sync tooling for lit components: the listed reactive properties of a root component travel as one canonical snapshot over whatever wire carries text — the envelope-less, last-writer-wins protocol of ADR 0013.

The TypeScript modules under `web/` carry the browser runtime; the Rust `pair` module (`src/pair.rs`) is the compact-payload codec in Rust too, byte-compatible with `web/pair.ts`, so a native peer (ADR 0028) exchanges the same pairing codes.

| Module | Provides |
| --- | --- |
| `codec` | `encode`/`decode`: tagged structured-clone JSON (Date, Map, Set beside plain JSON), canonical — object keys sort at every depth, so byte equality is state equality |
| `wire` | The `Wire` seam (`send`/`onMessage`/`onOpen`/`onClose`/`close`) with `WebSocketWire`, `DataChannelWire`; `onClose` fires once when the wire goes away, however it goes |
| `sync` | `attach(root, { fields, wire, greet? })`: snapshots out on the root's `state-changed`, inbound snapshots assign back; one `last` slot plus ready/applying flags keep echo and boot quiet |
| `pair` | Serverless WebRTC pairing over compact binary payloads (a declared layout in bare base64url, QR-sized — ADR 0028) instead of full SDP through a signaling server. `swap()` is symmetric and order-free: both sides create offers over a negotiated channel, exchange payloads blindly, and each synthesizes the peer's answer with fingerprint-derived DTLS roles. `payloadRole` classifies payloads, a wrong kind is refused with a plain message before any RTC call, and a swap whose offer is consumed says so via `spent()` |
| `session` | Cross-tab organization for p2p wires (ADR 0032): `TabSessions` routes an opened link to the tab owning its session (reply digests address replies exactly), `TakeoverPoint` hands a session to another tab by re-signaling through the standing wire, and `ControlWire` carries the `uicc1.` control plane on any `Wire`, filtered off before state application. Framework-free — BroadcastChannel is the only platform API |

Exactly one side greets (`greet: true`): the party holding the canonical state — a server, a pairing host — announces it on open, and the other side waits.
Pairing gathers candidates completely before encoding and carries their addresses verbatim (browsers hand out mDNS hostnames, which resolve between peers on one network); crossing networks takes `iceServers` (STUN puts server-reflexive addresses into the payload, TURN relayed ones), the default is none.
When the peers cannot reach each other the pending wire rejects with a plain message once the connection reports failure — no forever-hanging "Connecting…".
Demo-grade by scope: no reconnect, no arbiter beyond last-writer-wins, plain-JSON state on the terminal side.

Consumers integrate one of two ways: hand `web_root()` to a `web_modules` build as an extra source root, or emit the compiled npm tree with `npm_tree(out, version)` and install `@gronke/uic-sync` like any package.
The published tree has no dependencies; QR rendering stays with the consumer (the payloads are plain text).

A native peer imports the Rust `pair` module: `Compact`, `encode_payload`/`decode_payload` (the declared binary layout in bare base64url), `parse_sdp`/`build_sdp` (the minimal data-channel SDP), `payload_role`, `link_payload`/`invite_link`, `reply_digest` and the `uicc1.` control-frame codec (`Ctrl`, `encode_ctrl`/`decode_ctrl`) — the same contracts `web/pair.ts` and `web/session.ts` run, cross-pinned so the bytes never drift.
The Rust `session` module is that peer's pairing lifecycle as a pure state machine: events in, effects out, every wire tagged by a monotone generation so a superseded wire's close is a no-op by construction; it owns the `<pair-panel>` view contract and every pairing status.
The WebRTC stack stays with the consumer (the lit-demo drives `webrtc-rs` and executes the machine's effects); this crate ships no transport.

```sh
cargo test -p uic_sync
```
