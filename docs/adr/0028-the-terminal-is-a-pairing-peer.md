# ADR 0028: The terminal is a pairing peer

## Decision

The lit-demo's `p2p` mode makes the terminal a first-class WebRTC peer: it generates an invite (the shared
panel shows the link and a scannable QR — ADR 0029, ADR 0030) and opens someone else's invite (`p2p
'<link-or-token>'`), pairing with a browser over the same compact `uics1.` payloads the page uses.
The payload codec is cross-pinned in Rust (`uic_sync::pair`) beside the TypeScript it must match byte for
byte — `Compact`, `encode_payload`/`decode_payload`, `parse_sdp`/`build_sdp`, `payload_role`, `link_payload`
— guarded by a captured browser vector, so the two languages never drift. The WebRTC stack (`webrtc-rs` 0.17,
pinned — its master is a sans-io rewrite) lives in the app, not the library.
A terminal-generated invite is answered by hand: pairing is a mutual exchange (ADR 0031), so the browser
opens the invite and its reply token pastes back into the terminal's panel. The existing `LiveBridge`
publish/apply loop carries the synced state unchanged — the data channel is just another string wire.

The terminal runs as an **ICE-lite** agent. The browser's `swap()` always stays ICE-controlling (it applies a
locally synthesized answer), and webrtc-rs does not resolve two controlling agents — it ignores a same-role
peer's connectivity checks outright — so the terminal must take the controlled side, and lite is the one
arrangement that yields it while still publishing an offer-role (`actpass`) payload the browser accepts. The
terminal's payload carries no `a=ice-lite`, so the browser still sees a full peer and stays controlling; the
terminal knows it is lite locally, and that is enough.

## Why

The pairing was browser-only: the wizard is page code, and Boa has no WebRTC. But the `uics1.` payload was
designed engine-agnostic (ice credentials, DTLS fingerprint, setup role, `[address, port]` candidates), so a
Rust peer can emit and consume the exact same strings — the terminal becomes the thing it always described,
a peer that generates and opens invites.

## Consequences

- Lite gathers host candidates only (no srflx), so the terminal pairs on a **shared network** — the same LAN,
  a personal hotspot — while crossing NATs stays out of scope, consistent with the demo shipping no TURN.
  Loopback candidates ride along so a terminal and a browser on one host pair directly.
- The Rust↔Rust loopback test pairs one lite (controlled) side against one full (controlling) side — the
  production roles exactly (browser = full/controlling, terminal = lite/controlled) — and proves the codec,
  the synthesized answer, DTLS, SCTP, the data channel, the greet rule and bidirectional sync end to end.
  A real browser↔terminal pairing needs a network the two share; a headless CI browser isolated from the
  process cannot exercise it, so that leg is verified on a real LAN, not in CI.
- `webrtc-rs` is a heavy (~50-crate) tree; it lands in the demo app only, never the libraries, and is pinned
  to 0.17.
- Demo-grade scope holds: no reconnect (a dropped wire ends the session), the reply comes back by hand — the
  browser's token pastes into the terminal's panel (ADR 0031) — and `UIC_LIT_DEMO_ICE_DEBUG` traces the
  candidates when a pairing will not come up.
