# ADR 0024: State synchronizes through one wire seam

## Decision

`crates/uic_sync` ships the synchronization tooling as `@schuhkarton/uic-sync` — the `web_root()`/`npm_tree()` twin-entry crate shape of ADR 0023's worker host, dependency-free on the published side.

The protocol stays ADR 0013's: an envelope-less full-state snapshot per message, last-writer-wins, dedupe by canonical text.
Four modules carry it:

- `codec` — tagged structured-clone JSON: `Date`, `Map` and `Set` travel as `{"$uic": tag, "v": …}` wrappers beside plain JSON; a plain key that could be mistaken for the tag gains one leading `$` on encode and loses it on decode.
  The emitted text is canonical with object keys sorted lexicographically at every depth, which extends ADR 0013's top-level dedupe below the surface — the serializer is hand-rolled because rebuilding sorted objects for `JSON.stringify` would reorder integer-like keys numerically.
- `wire` — the seam ADR 0013's `Transport` sketched, string-payload so every carrier qualifies: `WebSocketWire`, `DataChannelWire`, `BroadcastWire` behind `Wire { send, onMessage, onOpen, close }`.
- `sync` — `attach(root, { fields, wire, greet? })` mirrors the listed reactive properties: snapshots leave on the root's `state-changed` announcement, inbound snapshots assign back, and one shared `last` slot plus the ready/applying flags brake echo and boot.
  Exactly one side greets — the party holding the canonical state (a server, a pairing host) announces on open and the other waits; two greeters would swap states and settle crossed.
- `pair` — serverless WebRTC pairing: candidates gather completely before encoding (no trickle), and the offer reduces to ice credentials, the DTLS fingerprint, the setup role and `[address, port]` candidate tuples — a `uics1.`-prefixed base64url payload under 300 characters, QR-sized.
  The peer rebuilds a minimal data-channel-only SDP from it.
  Candidate addresses travel verbatim: browsers hand out mDNS hostnames, and rewriting them breaks the handshake.
  `iceServers` defaults to none — host candidates connect peers on one network without STUN, TURN or a signaling server.

The lit-demo carries both showcases around the unchanged todo app: `live` (the terminal is the server; the page attaches a `WebSocketWire`; the terminal renders the join URL as a QR pane and its Rust side sorts snapshots with `serde_json`'s `sort_all_objects`) and `/p2p` (two browsers exchange offer and answer as mutually shown QR codes — the offer rides the page link's fragment so a phone camera opens it — then attach `DataChannelWire`s).

## Why

The web-demo bridge and the live page had grown two parallel implementations of the same ADR 0013 protocol with no shared code; the seam belongs in one publishable artifact that any consumer wires the same way.
A snapshot protocol over a string wire is transport-blind by construction, which is what makes the WebRTC showcase a page of glue rather than a second stack.
QR codes replace the signaling server because the offer fits in one: pairing needs no infrastructure at all between two devices that can see each other's screens.
`npm-utils` enables `serde_json`'s `preserve_order` in the demo binary, so the native side must sort explicitly to keep byte-equality dedupe honest — recorded here because the default looks alphabetical until a nested object arrives.

## Consequences

- Last-writer-wins stays the arbiter; concurrent edits can cross. A merge discipline (revisions, CRDTs) would be a codec-level extension, not a wire change.
- A dead wire stays dead: no reconnect, by scope. The demo restarts a connection by reloading a page.
- The terminal host speaks plain-JSON state; the codec's tagged types are browser-to-browser until a native decoder is worth its weight.
- Camera scanning (`BarcodeDetector` + `getUserMedia`) needs a secure context, which `http://<lan-ip>` is not — the paste path is the always-present, tested one.
- mDNS candidate resolution assumes one non-isolated network; AP-isolated Wi-Fi needs `iceServers` (STUN) and possibly more.
- Numbers ride each side's serializer: exotic floats may spell differently across hosts; integer-bearing state is byte-stable.
