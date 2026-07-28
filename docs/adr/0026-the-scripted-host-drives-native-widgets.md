# ADR 0026: The scripted host drives native widgets

## Decision

The terminal serves real widgets for browser markup: the scripted host (`uic_tui::dom::HostState` — behind Boa on real terminals, behind the browser's own engine in the worker session, ADR 0007) mounts and drives the same terminal widgets the pure-Rust host always had, so a plain mocked-lit app uses a real `<input>` instead of hand-rolled keydown string editing.

A plain form element mounts its widget because of what it is:

- `<input>` (and its textual types), `<input type="number">`, `<input type="date">`, `<input type="datetime-local">`, `<textarea>` and `<select>` resolve to their rat adapters by tag and type, through one shared table (`uic_template::native`) that the runtime mount, the template lint and the macro checks all consult — the three cannot drift.
  An unknown `type` value falls back to the text state, HTML's own rule.
- Detected date variants are type-implied: `date` is date-only, `datetime-local` carries minutes; seconds want the explicit override.
- Control-type inputs (checkbox, radio, submit, …) stay plain elements, and a negative `tabindex` opts a presentation twin out of detection — the platform's own "out of the focus order" signal.
- `data-tui` remains beside the table as the explicit override and extension point: it wins over detection, mounts on any tag (the tab-bar is a `<ul>`), and stays the discriminator inside the framework's own input templates.
  It doubles as the registration hook: components register widget adapters through `WidgetRegistration` (`inventory`), the way uic_tui's `qr` cargo feature registers `data-tui="qr"` — the terminal twin of the pairing UI's QR element (ADR 0029), kept off the wasm bundle so the browser carries no encoder.
- Every consumer beyond the mount keys on widget presence in the node payload, not on markup.

Four host capabilities close the gap to the browser, all in the one shared body:

- The commit mounts a widget on every descendant that implies one (the mount walk is a free function both hosts share), and mounted widgets survive the subtree swap keyed by (`data-path`, kind, variant) — the same stable key focus survival already uses — plus a one-slot stash for the focused widget of a nested input, whose parent commit destroys it one microtask before the child re-renders it.
- `.value=` on the browser's value-carrying elements (`input`, `textarea`, `select`) serializes as the `value` attribute (lit-SSR's rule), and the commit syncs each widget from it through an echo-skip: a value equal to the widget's live text only records the sync, so the component echoing back what the user just typed never moves the caret; genuinely different values re-sync and park the caret at the end, like a script assigning `value` in a browser.
  No `value` attribute means an uncontrolled input — the transplanted text stays.
- Key routing is browser-exact: the keydown delivers first and bubbles; uncancelled, the host runs the focused widget as the editing default action, and a text change synthesizes a bubbling `input` event whose `target.value` reads the live text.
  `preventDefault()` on keydown suppresses the editing — the cancelable contract apps use for chords like Space-toggles-in-list-mode; disabled inputs swallow no keys.
- Pointer clicks focus a widget node and place the caret under the pointer before the click delivers.

A linked lint is the guardrail: `uic_tui::lint` walks every registered component's parsed template and reports `Finding`s with two severities.

- Errors are bindings the terminal can never serve: a static `data-tui` kind that neither a built-in widget nor a `WidgetRegistration` resolves (checked through the runtime's own `WidgetBox::new`, so the lint and the mount cannot drift), an `@event` other than `change`/`input` on a widget — explicit, bound or detected, so `@click` on a plain `<input>` is an error (it truly never dispatches; widget clicks become focus and caret placement) — and an `@event` on a custom element that is not one of the child definition's notify events.
- Warnings mark web-only markup that is legal but inert in the terminal: a bound `data-tui=${…}` kind (not statically checkable), a bound `type=${…}` on a plain input (the committed value decides at runtime; its `@change`/`@input` stay legal), and a non-click `@event` on a plain element — plain elements receive only `@click`, which the pointer path dispatches natively.
- `assert_tui_compatible()` prints warnings and panics listing all errors; component crates gate themselves with one integration test — `your_crate::link(); uic_tui::lint::assert_tui_compatible();`.

## Why

The model is a TUI representation of browser UI: a user writes `<input type="date" />` and the terminal provides the widget that resembles that element's behavior, exactly as the browser provides its native control — framework plumbing must not leak into user markup.
The framework's own input components cannot ride the same detection: date, number, suggestion and text all deliberately render `<input type="text">` (they replace native controls with their own parsing, masking and overlays), so `(input, text)` names four different kinds — for them `data-tui` is not an opt-in but the discriminator.
The lit-demo app emulated text entry (keydown listener, string append, a rendered caret span) because the scripted host supported nothing better; the pure-Rust host had the entire contract already, and teaching the shared `HostState` the same moves lets plain-lit apps write ordinary browser code (`<input>`, `@input`, `target.value`) and get the native caret, selection and mid-text editing for free, plus the rat widget twin in the terminal.
Browser semantics are deliberately the contract: no Enter-commit engine, no tab-order engine — Enter, Tab and the arrows bubble as keydown and the app decides, exactly as on the web.
The lint exists because the macros validate grammar and placement at compile time, but the two facts these checks need only exist once a binary links: `inventory` collects widget registrations from any crate, and a referenced child's notify events live in another crate's definition — both unknowable at macro expansion.
Before the lint, both failure modes were silent: the mount walk swallows an unknown kind (the node renders as dead space) and an undispatched event binding simply never fires.
A linked test binary is the earliest point the full registry exists, and as a plain `cargo test` the gate rides the existing workspace-test CI step with no new plumbing.
Web-only handlers warn instead of fail because the browser legitimately has richer interaction; a hard error would outlaw valid per-target markup (ADR 0002).

## Consequences

- Uncommitted typing rides the widget, not the component: a re-render that echoes the same value leaves the caret alone, while a genuinely different value (a remote live-sync snapshot) resets the text and parks the caret at the end — external edits win, as intended.
- Text inputs are the first-class kinds under the scripted host: the serializer carries no property binding beyond `value` and `hidden` (the per-part commit path is the recorded follow-up), so a scripted app's select mounts but its option rows stay empty — `.options` on a plain `<select>` reaches the widget through the Rust parts engine, and options remain data rows (ADR 0005).
- A kind change on one `data-path` — a bound `type` landing after the bind-time mount — recreates the widget and resets typed state, like a variant flip: kind flips are configuration.
- A `data-tui` element whose kind fails to resolve renders as a generic container instead of a blank widget leaf (only reachable from scripted hosts; the lint blocks it for Rust templates).
- A chrome template must not contain plain form elements any more than `data-tui` markers, and a `for` body rejects both alike — a loop renders data rows, not widgets (ADR 0001).
- The wasm TUI worker runs the same triage behind capability checks — a stale checked-in glue degrades to the old keydown-only behavior instead of throwing.
- The pure-Rust host is untouched: `sync_value` and the adapter trait keep their exact semantics; the only addition is a default-implemented `caret_to_end` the scripted sync calls.
- The catalog gates itself in `crates/uic_tui/tests/lint.rs`, and the `qr` feature adds its own registry gate; the same two lint lines serve any external component crate.
- Registry-level defects (nothing linked, duplicate tags, unresolved custom tags) short-circuit as one error via `CustomElementRegistry::assert_valid` — the per-template walk assumes a sane registry.
- Registry-backed kind resolution cannot be unit-tested inside `uic_tui` itself: a `cfg(test)` lib build and the catalog's dependency are two copies of the crate with separate inventories, so that coverage lives in the integration gate.
- Candidate future checks, deliberately reserved: property bindings the terminal ignores on plain elements, and coverage reporting for classes without a terminal mapping (the styling contract stays silent degradation by design — not all Bootstrap applies to cells).
