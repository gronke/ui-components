# ADR 0029: The pairing UI is one shared component set

## Decision

The pairing UI is one shared component set — `<pair-panel>`, `<qr-code>` and the terminal's `<p2p-deck>`, baked into `@schuhkarton/lit-todo` and loaded by both hosts; only the transport differs per host, the browser's `pair-wizard` owning the swap, the clipboard and the camera, the terminal's native Rust peer driving the same elements from the demo binary (ADR 0028).

`<pair-panel>` is presentation only: the card, the connection badge, the status line, the invite body, the peer-paste box and the buttons, driven entirely by properties (`mode`, `link`, `status`, `connected`, `resetLabel`, `actionLabel`, `canScan`, `peer`, `command`).
The exported `PanelMode` union is the mode contract — `idle`, `invite`, `connected`, `dropped`, `failed`, `handed`, `moved`, `nortc` — of which the native session machine produces the first five (Rust twin: `uic_sync::session::PanelMode`), the browser wizard adding the takeover roles and the no-WebRTC dead end.
Only `idle` and `invite` render a body; every other mode speaks through the status line, the badge and the action/reset buttons.
Intent flows back through the `command` property: a click writes it, the host reads and clears it, and — where the platform has events — the same `emit()` also dispatches a browser `CustomEvent` of that name so a controller can just listen.
Only `connect` carries a detail (the pasted peer text); `action` stays a deliberately generic name — the panel is presentation-only and the host supplies the label (the tab takeover today, ADR 0032).
The wizard's reactive state speaks the panel's own vocabulary, so its render is a plain pass-through; the terminal loop mirrors the session machine's `PanelState` onto the mounted panel with `set_prop` and polls `command` after each click.

`<qr-code data="…">` renders alike in both hosts: the browser draws an SVG with `qrcode-generator`, loaded through a guarded dynamic `import()`, while the terminal mounts a native Rust QR widget, selected by a `data-tui="qr"` marker through the widget registry (ADR 0026) behind `uic_tui`'s `qr` feature.
The data rides the widget's `value` channel — the one attribute the scripted host mirrors — and flows to the nested element as an attribute, since a `.prop=` binding would not survive the serialize commit.
Both hosts paint it black on white explicitly — a camera wants dark modules on a light ground whatever the theme — the browser's SVG on its own white card, the widget with explicit cell colors; the half-block renderer (`qrcode` crate, Dense1x2) is one `render_qr` function shared by the widget and the live-mode join pane.

`<p2p-deck>` is the terminal's arrangement: the todo card and the panel stack in one column and the QR docks to their right, wrapping below the panes under about 200 columns — real taffy flexbox from the deck's own stylesheet (a 111ch stack width as the flex basis beside the QR's ~87 intrinsic columns and the 2ch gap), no host rect math.
The deck deliberately has no reactive properties: it renders exactly once, so a re-commit can never swap the composed `<todo-app>`'s live state away, and the hosts drive the nested elements directly by node.
It also deliberately imports neither `pair-panel.js` nor `qr-code.js`: the terminal loads those modules explicitly before mounting, and an import here would drag the pairing UI into the browser's todo-app graph.

The invite shows once per host: the browser a compact copy-link (it has a clipboard), the terminal the full link as wrapped, selectable text (`overflow-wrap: anywhere`, honored by the text layout).
The panel's `static styles` are the terminal-only layer — hiding the copy-link and the inline QR (the deck docks its own beside the panes) while the mapped Bootstrap subset draws the card and the badge (ADR 0021) — and the browser page's CSS hides the link text instead.

The panel restores focus across re-renders: `willUpdate` captures the focused control's stable identity (its `name` or leading class), and `updated` puts focus back on the same control or the new body's first one — guarded, because the terminal host has no `document`.

## Why

The pairing UI existed in per-host copies, while the transport is the only part that genuinely cannot be shared: Boa has no WebRTC, no clipboard and no camera.
The mocked terminal lit has no `Event`/`CustomEvent`/`dispatchEvent`, and `JsHost` exposes only `set_prop`/`prop_json`, so a shared component cannot signal intent by dispatching an event — the `command` property is the channel that works everywhere, and the browser event is a convenience on top of the same helper.
Boa has no SVG path either, but the widget registry already lets a custom element mount a host-drawn terminal widget (ADR 0026), so the QR is one real component with two renderers instead of a host-drawn pane — the todo-item checkbox precedent (a `[x]` span beside the real `<input>`) at QR scale.
A structural re-render (the mode body swapping, a label button appearing or vanishing) tears the focused node down and the browser drops focus to `body`, knocking a keyboard user back to the top of the page; the restoration keeps the keyboard walk in place.

## Consequences

- The browser QR library must never enter Boa's static module graph, which would fail the terminal at link time; the dynamic `import()` runs only in the browser, and a run under Boa rejects and is swallowed, leaving the native widget.
- The terminal QR is as large as its payload: a long invite link is a high-version grid, so the deck wraps it below the panes on narrow terminals, where the wrapped link text remains the working fallback (an unbreakable token's min-content drops to one cell and the wrap machinery breaks it).
- The deck's breakpoint is flex-basis arithmetic (the stack's `width` beside the QR's intrinsic columns), not a media query: `uic_css` maps `display: flex`, `flex-wrap`, `min-width` and `ch` lengths onto taffy, but has no `@media`/container conditions and no `flex-basis` property — the recorded follow-ups, issue #98.
- The clipboard and the camera stay host-specific by necessity: the terminal answers `copy-link` and `scan` intents with nothing (the link is selectable text), and `canScan` turns the scan button on only where `BarcodeDetector` exists.
- "One component" holds for the pairing UI proper — link, QR, buttons, status, badge — with the browser-only camera video living in the wizard beside the panel.
