# ADR 0013: State synchronizes as one canonical snapshot over one wire

## Decision

App state travels as an envelope-less full-state snapshot per message: every change sends the whole state, the last writer wins, and byte-equal canonical text is the dedupe identity.
`crates/uic_sync` ships the tooling as `@gronke/uic-sync` in the twin-entry crate shape of the worker host (ADR 0007): `web_root()` hands the TypeScript sources to a consumer's `web_modules` build as an extra source root, `npm_tree()` emits the compiled publish-ready npm tree, dependency-free on the published side.

Five modules carry it, entered through `sync`:

- `codec` — tagged structured-clone JSON: `Date`, `Map` and `Set` travel as `{"$uic": tag, "v": …}` wrappers beside plain JSON, and a plain key that could be mistaken for the tag gains one leading `$` on encode and loses it on decode.
  The emitted text is canonical with object keys sorted lexicographically at every depth, so byte equality is state equality; the serializer is hand-rolled because rebuilding sorted objects for `JSON.stringify` would reorder integer-like keys numerically.
- `wire` — the one seam, string payloads so every carrier qualifies: `WebSocketWire` and `DataChannelWire` behind `Wire { send, onMessage, onOpen, onClose, close }`.
  Demo-grade by design: one listener per event, sends before open drop silently, and a dead wire stays dead.
- `sync` — `attach(root, { fields, wire, greet? })` mirrors the listed reactive properties: a snapshot leaves on the root's `state-changed` announcement, and an inbound snapshot assigns straight back onto the root.
  Three echo brakes keep the loop quiet without protocol machinery: one shared `last` slot dedupes both directions, the `applying` flag mutes the announcement an inbound assignment re-fires (held until `updateComplete` settles), and the `ready` flag keeps a booting side silent until the first exchange.
- `pair` and `session` — the serverless pairing (ADR 0028) and the session lifecycle (ADR 0032), the two modules that also exist as Rust twins inside the crate; `codec`, `wire` and `sync` stay TypeScript-only.

Exactly one side greets: the party holding the canonical state announces its snapshot on open (a server, a re-pairing session's state holder — ADR 0032) and the other waits; a fresh pairing with no canonical holder yet breaks the tie lexically, the smaller payload greeting.

The native side speaks the same protocol through its own glue (`apps/lit-demo/src/live.rs`): snapshots serialize per property over the shared `STATE_FIELDS` list and dedupe by byte equality against a `latest` slot.
npm-utils enables `serde_json`'s `preserve_order`, and cargo features are additive, so the demo binary sorts explicitly (`sort_all_objects`) to keep byte-equality dedupe honest — recorded because the default looks alphabetical until a nested object arrives.

The lit-demo carries the two harnesses around the unchanged todo app.
`live` makes the terminal the server: an axum `/ws` route greets every client with the canonical snapshot, and the page probes `/live` (staying quiet under plain `serve`) before attaching a `WebSocketWire` without greeting.
`/p2p` pairs peers with no server between them (ADR 0028): the page attaches a `DataChannelWire` the moment the wizard hands one over, detaching any previous attachment so exactly one wire mirrors the app (ADR 0032).

## Why

Two hosts render the same app from one definition, and a full snapshot that trickles both ways over one string wire is the shared-data half — the simplest protocol a WebSocket server and a WebRTC pairing reuse byte for byte.
A snapshot protocol over a string wire is transport-blind by construction, which is what makes the WebRTC harness a page of glue rather than a second stack.
Byte equality only works as state identity when every writer serializes canonically, so the codec sorts at every depth and the native side is held to the same discipline.
Two greeters would swap states and settle crossed; one greeter — the canonical holder — makes boot deterministic and free of traffic.

## Consequences

- Last-writer-wins stays the arbiter; concurrent edits can cross.
  A merge discipline (revisions, CRDTs) would be a codec-level extension, not a wire change.
- A dead wire stays dead: no reconnect, by scope.
  The demo restarts by reloading a page or minting a fresh pairing.
- The `fields` list is the contract: outbound snapshots carry exactly the listed properties, and the native applier ignores unknown members.
- The terminal host speaks plain-JSON state; the codec's tagged types are browser-to-browser until a native decoder is worth its weight.
- Numbers ride each side's serializer: exotic floats may spell differently across hosts (`1e21` is `"1e+21"` in JS); integer-bearing state is byte-stable.
