# uic_sync

State sync tooling for lit components (ADR 0024): the listed reactive properties of a root component travel as one canonical snapshot over whatever wire carries text — the envelope-less, last-writer-wins protocol of ADR 0013.

| Module | Provides |
| --- | --- |
| `codec` | `encode`/`decode`: tagged structured-clone JSON (Date, Map, Set beside plain JSON), canonical — object keys sort at every depth, so byte equality is state equality |
| `wire` | The `Wire` seam (`send`/`onMessage`/`onOpen`/`close`) with `WebSocketWire`, `DataChannelWire`, `BroadcastWire` |
| `sync` | `attach(root, { fields, wire, greet?, event? })`: snapshots out on the root's `state-changed`, inbound snapshots assign back; one `last` slot plus ready/applying flags keep echo and boot quiet |
| `pair` | Serverless WebRTC pairing: `createHost()`/`join(offer)` exchange compact `uics1.` payloads (QR-sized) instead of full SDP through a signaling server |

Exactly one side greets (`greet: true`): the party holding the canonical state — a server, a pairing host — announces it on open, and the other side waits.
Pairing gathers candidates completely before encoding and carries their addresses verbatim (browsers hand out mDNS hostnames, which resolve between peers on one network); hostile NATs would need `iceServers`, the default is none.
Demo-grade by scope: no reconnect, no arbiter beyond last-writer-wins, plain-JSON state on the terminal side.

Consumers integrate one of two ways: hand `web_root()` to a `web_modules` build as an extra source root, or emit the compiled npm tree with `npm_tree(out, version)` and install `@schuhkarton/uic-sync` like any package.
The published tree has no dependencies; QR rendering stays with the consumer (the payloads are plain text).

```sh
cargo test -p uic_sync
```
