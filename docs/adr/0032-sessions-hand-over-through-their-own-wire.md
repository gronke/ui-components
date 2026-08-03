# ADR 0032: Sessions hand over through their own wire

## Decision

A pairing session can move to another tab by re-signaling a fresh pairing **through its own live wire**.
Opening a reply link while the session stands offers a choice in the new tab: stay in the owning tab, or take the session over.
Taking over, the new tab mints a fresh swap; its payload travels new tab → owning tab (`BroadcastChannel`) → remote peer (`uicc1.` control frames on the established data channel).
The remote answers with a fresh swap of its own; the connection re-forms in the new tab; the old tab retires with "session moved to your other tab".
The old wire closes only after the new one opened, so a failed handover loses nothing.

The cross-tab organization lives in `@gronke/uic-sync`'s `session.js` (framework-free, `BroadcastChannel` the only platform API, ready to graduate into a package of its own):

- `ControlWire` wraps a live wire with the control plane: `uicc1.`-prefixed frames (canonical-codec JSON) filter off *before* the state consumer; the sync `attach`'s decode throws on non-JSON, so the app attaches to the wrapper.
  Exactly one underlying message listener fans out internally, because some transports keep a single `onmessage` slot.
- `TabSessions` is the same-browser handover: an opened link offers its payload to the tabs, an unspent session claims it and pairs, a reply may be claimed only by the tab whose invite it answers, and a connected owner claims its own re-opened replies so the opener can offer a takeover instead of a dead end.
  Absent `BroadcastChannel`, links adopt in their own tab and replies refuse.
- `TakeoverPoint` is the per-session handshake between the new tab and the owning tab, keyed by the reply digest both already share: the owner forwards a request over its wire as a `repair` control frame and answers back what the remote returned, and the done cue carries the new owner's payload so the new tab (serving the same channel by then) never retires on its own signal.

The native side of the lifecycle is `uic_sync::session`, a pure state machine: transport events go in (`Minted`, `Connected`, `Closed`, `Command`, `Repair` and the failure twins), effects come out (`Present`, `Mint`, `Connect`, `SendCtrl`, `Close`), and the WebRTC stack stays with the consumer.
Every wire carries a monotone `Gen` tag, and events from superseded wires are no-ops by construction: a renewed, replaced or already-failed wire is stale by gen, so its close falls through silently; no per-wire flags, and no drop UI painted over a live handover.
The machine also owns every user-facing pairing status and the `PanelState` it presents, the shared `<pair-panel>` property contract (ADR 0029).
The demo's `drive_session` (`apps/lit-demo/src/pair.rs`) executes the effects with real swaps, keyed by `Gen` in a map.
Mints await inline while a connect is spawned off the loop, so a disconnect or a modal accept never queues behind the long wait for both sides to apply each other's payloads; the completion returns as a `Gen`-tagged event, a superseded connect's a stale no-op.
`SendCtrl` rides the standing wire's outbound pump, and one bridge serves the session's every wire, so a handover's fresh wire pumps the same channels.

A repair round runs beside the standing wire, one at a time: the `repair` frame's fresh peer payload mints a fresh gen, the `repair-answer` rides the standing wire back, and the fresh connect greets FORCED; this side holds the canonical state, whatever the lexical order says.
Only on the fresh wire's `Connected` does the machine emit the old wire's `Close`; a failed round drops the fresh swap and reports "still on the old wire".
The new owner serves the session's original identity: links in the wild keep naming the original invite's digest, so the takeover inherits it, and claims and chained handovers keep routing to whichever tab currently owns the session.

## Why

A session is pinned to the tab holding its `RTCPeerConnection`: ICE credentials, the DTLS certificate and the negotiated transport die with the page, and browsers offer no transfer.
But the *connection* is not what the user needs to move: a new connection signaled over the old one serves the same end, and the established data channel is the one private, already-authenticated path both ends share.
No third party gets involved and no manual exchange interrupts the takeover, consistent with the mutual exchange (ADR 0028).

Three constraints shape the mechanics:

- **State snapshots and protocol frames share one channel.**
  Control frames carry the `uicc1.` prefix (state is canonical JSON and always starts with `{`), and both ends split them off before state application: `ControlWire` in the browser, the swap's message pump in the terminal.
- **Greet is forced on a re-paired wire.**
  The remote holds the canonical state and greets; the fresh tab must not greet with its empty one (the plain lexical rule of ADR 0013 could pick either).
- **Supersession is structural, not flagged.**
  The machine mints a new gen for every wire and simply no longer knows a superseded one when its close arrives, so "was this close deliberate?" never needs asking.
  The browser end answers the same question by identity, painting the drop only when the closing wire is still the current one and the tab has not retired.

## Consequences

- The takeover request re-posts until answered (`BroadcastChannel` keeps no history and the owner may still be connecting); the owner forwards one takeover at a time and unblocks after the shared 15-second timeout.
- A takeover against a peer that does not speak the control plane times out honestly ("the session stays in your other tab"); an unrecognized `uicc1.` frame is dropped as ignored garbage, never corruption.
- The page glue keeps the `Attachment` handle and detaches it before attaching a replacement wire, so exactly one wire mirrors the app.
- Retired tabs are done: they keep their message and the fresh-pairing button; live mirroring of retired tabs stays a recorded follow-up.
- The machine is pure, so the whole lifecycle (invite, connect, renew, drop, repair rounds, stale closes) unit-tests without a network, and the loopback handover test pins the forced greet and the close-after-open ordering with real swaps.
