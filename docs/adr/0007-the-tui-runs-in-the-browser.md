# ADR 0007: The TUI runs in the browser

## Decision

The terminal runtime compiles to `wasm32-unknown-unknown` and renders the whole demo page beside the DOM components: `crates/uic_tui_web` hosts a `TuiSession` (wasm-bindgen) whose xterm.js pane shows the same six elements the page markup declares, in one terminal.

- crossterm 0.29.0 is pinned to `schuhkarton/mirror-crossterm` (`[patch.crates-io]`), a private staging mirror carrying additive `cfg(not(any(unix, windows)))` stubs so the crate compiles on neutral targets; terminal control reports `Unsupported` at runtime and the event reader starts without a source.
  rat-widget's entire input API is written against crossterm's event types, and the browser host constructs those values from DOM `KeyboardEvent`s — nothing ever reads a TTY.
  The `impl App<CrosstermBackend<Stdout>>` block (raw mode, blocking read) is gated to native targets.
- `App` hosts multiple roots: mounts stack vertically like block elements in a document, Tab hands focus to the next root when a tree's cycle wraps, and the active root's popup paints after all content so overlays win over the roots below.
- `XtermBackend` implements ratatui's `Backend` by appending ANSI (cursor moves plus reset-then-set SGR runs) into a buffer the session drains into `term.write`, and keeps a plain-text shadow grid (`screen_text()`) as the assertion surface for tests.
- `translate_key` maps `KeyboardEvent.key` names to crossterm key events in Rust, so the mapping is unit-tested natively; the TS glue stays dumb.
- The page markup is the single source of truth: `tui.ts` mounts every page-level component element (light-DOM children of other components excluded), replays its attributes and the `input-select` options property, and forwards notify events into the shared `#events` log with a `[tui]` prefix.
- The wasm artifact is built by `scripts/build-wasm.sh` (dedicated `wasm` cargo profile) into the gitignored `apps/web-demo/web-tui/`, served read-through at `/tui` in every serving mode; when absent the page degrades to the DOM demo alone.

## Why

One Rust definition rendering two ways is the point of this repo, and the split view makes the claim inspectable: the same attributes, validation, notify events and popup semantics, DOM left, terminal cells right.
xterm.js was chosen over DOM-rendering alternatives (ratzilla) because it is a real terminal emulator fed by the same ANSI a terminal would receive, ships a proper ES module (`@xterm/xterm` 6.0.0) that vendors through web_modules, and adds no second ratatui version to the tree; the hand-rolled backend is ~150 lines against a stable trait.
crossterm cannot compile for wasm as published (its platform `sys` layers have no neutral arm) and no feature flag escapes it, so the mirror pin is the enabling move — the same pattern as the web_modules pin.

## Constraints found on the way

- `inventory` registrations are linker constructors: on wasm nothing calls them unless the module does (`__wasm_call_ctors`, invoked once in the session's link step), and wasm-ld's lazy archive extraction drops the registration objects entirely unless they share their object file with code the session references — hence `codegen-units = 1` in the `wasm` profile.
- The focused text widget places the terminal caret (rat-text's `screen_cursor()` forwarded into the frame), the focused element's group border wears a focus ring, and an `[error]` element's border turns red — the browser's caret, focus outline and invalid state, in cells; selects show no caret, matching the browser.
- State colors are plain ANSI palette slots (bright blue ring, red error border, bright red error text), so a real terminal keeps the user's own scheme; the web pane maps those slots to Bootstrap's dark-theme custom properties (`--bs-primary-text-emphasis`, `--bs-danger`, …) read from the vendored stylesheet at boot.
- Shift+Tab deliberately leaves the pane — the runtime has no reverse focus traversal — and function keys other than F4 stay with the browser.
- Pointer input skips terminal mouse protocols in the pane: pixels convert to cells and feed the session directly, while the native runner enables crossterm's mouse capture.
  Clicks focus the widget under them (committing the one they leave), pick calendar days and options, a press outside an overlay dismisses it, and a click outside every element — or leaving the pane — blurs with the browser's change-on-blur semantics.
- crossterm's `event-stream` feature remains unavailable on neutral targets (its waker is platform-bound); nothing in this tree enables it.

## Consequences

- Drop the crossterm pin when upstream ships the stubs and ratatui-crossterm and the rat crates adopt that release; whether the patch moves from the private mirror to a public fork and upstream is a separate decision.
- The release binary does not embed `/tui`; baking the wasm output in as an extra build root is the recorded follow-up if the split view should ship self-contained.
- The wasm binary weighs a few MB (chrono-tz's table dominates); acceptable for a demo, `wasm-opt` deliberately not part of the pipeline yet.
- Driving the exposed `window.__tui` session directly desynchronizes the visible xterm (its ANSI goes to the caller, and later diffs skip unchanged cells); `screen_text()` stays authoritative, which is exactly what the browser tests assert against.
