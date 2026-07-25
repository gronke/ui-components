# ADR 0026: The scripted host drives native widgets

## Decision

The scripted host (`uic_tui::dom::HostState` — behind Boa on real terminals, behind the browser's own engine
in the wasm TUI session) mounts and drives the same terminal widgets the pure-Rust host always had, so a
plain mocked-lit app uses real `<input data-tui="text-input">` elements instead of hand-rolled keydown
string editing.
Four capabilities close the gap, all in the one shared body:

- The commit mounts a `WidgetBox` on every `data-tui` descendant (the mount walk is now a free function both
  hosts share), and mounted widgets survive the subtree swap keyed by (`data-path`, kind, variant) — the
  same stable key focus survival already uses — plus a one-slot stash for the focused widget of a nested
  input, whose parent commit destroys it one microtask before the child re-renders it.
- `.value=` on the browser's value-carrying elements (`input`, `textarea`, `select`) serializes as the
  `value` attribute (lit-SSR's rule), and the commit syncs each widget from it through an echo-skip: a value
  equal to the widget's live text only records the sync, so the component echoing back what the user just
  typed never moves the caret; genuinely different values re-sync and park the caret at the end, like a
  script assigning `value` in a browser.
- Key routing is browser-exact: the keydown delivers first and bubbles; uncancelled, the host runs the
  focused widget as the editing default action, and a text change synthesizes a bubbling `input` event whose
  `target.value` reads the live text. `preventDefault()` on keydown suppresses the editing — the cancelable
  contract apps use for chords like Space-toggles-in-list-mode.
- Pointer clicks focus a widget node and place the caret under the pointer before the click delivers.

## Why

The lit-demo app emulated text entry (keydown listener, string append, a rendered caret span) because the
scripted host supported nothing better — no widget mount, no value channel, no input events.
The pure-Rust host had the entire contract already; teaching the shared `HostState` the same four moves lets
plain-lit apps write ordinary browser code (`<input>`, `@input`, `target.value`) and get the native caret,
selection and mid-text editing in the browser for free and the rat widget twin in the terminal.
Browser semantics are deliberately the contract: no Enter-commit engine, no tab-order engine — Enter, Tab
and the arrows bubble as keydown and the app decides, exactly as on the web.

## Consequences

- Uncommitted typing rides the widget, not the component: a re-render that echoes the same value leaves the
  caret alone, while a genuinely different value (a remote live-sync snapshot) resets the text and parks the
  caret at the end — external edits win, as intended.
- Text inputs are the first-class kinds; `.options` (select, date) is not serialized yet, so those widgets
  stay with the pure-Rust host's components for now.
- The wasm TUI worker gained the same triage behind capability checks — a stale checked-in glue degrades to
  the old keydown-only behavior instead of throwing.
- The pure-Rust host is untouched: `sync_value` and the adapter trait keep their exact semantics; the only
  addition is a default-implemented `caret_to_end` the scripted sync calls.
