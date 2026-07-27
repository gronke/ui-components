# ADR 0029: The pairing UI is one component, two transports

## Decision

The pairing UI is a single shared component, `<pair-panel>` (baked into `@schuhkarton/lit-todo`, loaded by
both hosts): the card, the connection badge, the status line, the invite link, the copyable token, the
peer-paste box and the buttons. It renders in the terminal and the browser alike — the framework's thesis,
applied to pairing. Only the WebRTC transport differs per host: the browser's `pair-wizard` (the swap, the
clipboard, the camera) renders a `<pair-panel>` and drives it; the terminal's native Rust peer mounts the
same panel and drives it from `main.rs`. (The relay is since removed — ADR 0031 — and the QR is now the
shared `<qr-code>`, in the panel for the browser and beside the panes in the terminal's deck, ADR 0030.)

The panel is presentation only. State flows in as properties (`mode`, `link`, `token`, `status`,
`connected`, `resetLabel`, `canScan`); intent flows out through a `command` property a host reads and
clears. In the browser the panel additionally dispatches a `CustomEvent` for the same intent, so a browser
controller can just `addEventListener`; the property is the channel that also works under the mocked terminal
lit, which has no `dispatchEvent`.

## Why

The pairing UI existed twice: a heavyweight browser `pair-wizard` and a hand-rolled Rust QR pane + status
line. Sharing the *view* removes the duplication and makes the terminal's pairing look and behave like the
browser's — the invite link, a renew button, a join-another control, the live badge — while the transport,
which genuinely cannot be shared (Boa has no WebRTC), stays where it must.

Two host constraints shaped the seam:
- **No event-out under Boa.** The mocked lit has no `Event`/`CustomEvent`/`dispatchEvent`; `JsHost` exposes
  only `set_prop`/`prop_json`. So a shared component cannot signal intent by dispatching an event — it writes
  a `command` property the terminal loop polls after each click and clears. The browser gets a real event
  too, from the same `emit()` helper, guarded by `typeof CustomEvent`.
- **No SVG/`unsafeHTML` under Boa.** The QR cannot render inside the shared component *as an SVG*; at this
  ADR it stayed host-drawn beside the panel — the browser an SVG, the terminal a block-char pane. This is
  the `.check`/`[x]`-span precedent from `todo-item` at panel scale: one feature, two irreducibly
  host-specific renderings, each drawn where it can be. (ADR 0030 later folds the QR back into the panel as
  the shared `<qr-code>`, giving the terminal a native QR widget, so the SVG is no longer the only path.)

## Consequences

- The terminal mounts `<pair-panel>` as a second sibling root beside `<todo-app>` (`paint_document` renders
  the whole document); a `PanelState` the pairing thread writes is mirrored onto it with `set_prop` each
  tick, and its `command` property is polled after clicks and forwarded to the pairing thread — which can
  now create a fresh `Swap` on renew (`Swap::connect` borrows `&self` so the loop can `close()` the old wire
  first).
- The terminal's invite is the panel's link text (selectable, always current through renew), replacing the
  separate always-static QR pane in p2p mode; the QR is since the shared `<qr-code>` (ADR 0030) — inline in
  the panel for the browser, docked beside the panes by the terminal's p2p deck.
- The browser's `pair-wizard` keeps every transport method unchanged; only its `render()` changed — it now
  renders and drives the panel and answers its intent events. The `'wire'` event to the page is unchanged.
- The clipboard stays host-specific by necessity; "one component" holds for the link, QR, buttons, status
  and badge (the QR since folded in as `<qr-code>`, ADR 0030), which is the pairing UI proper.
