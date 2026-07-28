# ADR 0007: The TUI runs in the browser

## Decision

The terminal runtime compiles to `wasm32-unknown-unknown` and the browser hosts it end to end: `crates/uic_tui_web` exposes wasm-bindgen sessions whose ANSI feeds an xterm.js pane, `apps/web-demo` generates a gallery pairing each web component with its terminal twin, and a dedicated module worker runs foreign lit elements on the browser's own engine against the same wasm artifact.

The wasm terminal:

- crossterm 0.29 is pinned to `gronke/fork-crossterm` (`[patch.crates-io]`) — upstream master plus additive `cfg(not(any(unix, windows)))` stubs, the branch behind crossterm-rs/crossterm#1066 — so the crate compiles on neutral targets; terminal control reports `Unsupported` at runtime and nothing ever reads a TTY.
  rat-widget's entire input API is written against crossterm's event types, and the browser host constructs those values from DOM events.
  The `impl App<CrosstermBackend<Stdout>>` block (raw mode, blocking read, mouse capture) is gated to native targets.
- `TuiSession` hosts multiple mounted roots: mounts stack vertically like block elements in a document, Tab and Shift+Tab cycle focus across root boundaries in document order, and the focused widget's overlay paints after all content so it wins over the roots below.
- `XtermBackend` implements ratatui's `Backend` by appending ANSI (cursor moves plus reset-then-set SGR runs) into a buffer the session drains into `term.write`, and keeps a plain-text shadow grid (`screen_text()`) as the assertion surface for tests.
- `translate_key` maps DOM `KeyboardEvent` names to crossterm key events through the shared `uic_tui::keys` vocabulary, so the mapping is unit-tested natively; shifted Tab folds into `BackTab`, bare modifiers translate to nothing, and the TS glue stays dumb.
- `inventory` registrations are linker constructors: on wasm nothing calls them unless the module does (`__wasm_call_ctors`, invoked once in the session constructor), and wasm-ld's lazy archive extraction drops the registration objects entirely unless they share their object file with code the session references — hence `codegen-units = 1` in the dedicated `wasm` profile, and the build script greps the emitted bundle for a component tag as the guard.
- The focused text widget places the terminal caret (rat-text's `screen_cursor()` forwarded into the frame), the focused element's border wears a focus ring, and an `[error]` element's border turns red — the browser's caret, focus outline and invalid state, in cells; selects show no caret, matching the browser.
  State colors are plain ANSI palette slots (bright blue ring, red error border), so a real terminal keeps the user's own scheme.
- Shift+Tab walks the focus backward inside the pane (the keymap turns it into `BackTab`), function keys other than F4 stay with the browser, and Esc or Ctrl-C quits the session — the host blurs the pane in response.
- Pointer input skips terminal mouse protocols in the pane: pixels convert to cells and feed the session directly, while the native runner enables crossterm's mouse capture.
  Clicks focus the widget under them (committing the one they leave), pick calendar days and options, a press outside an overlay dismisses it, drags extend the selection, the wheel browses an open list, and a click outside every element — or leaving the pane — blurs with the browser's change-on-blur semantics.
- The wasm artifact is built by `scripts/build-wasm.sh` (the `wasm` cargo profile) into the gitignored `apps/web-demo/web-tui/`, served from disk at `/tui` in every serving mode; when absent the pages degrade to the web pane alone and the Terminal tab hides.

The gallery:

- One manifest in `apps/web-demo/build.rs` generates the site: a gallery index at the root and one page per entry under its route — `/demo/` for the composed form, `/components/<tag>/` for every catalog component, `/examples/<name>/` for maintained end-to-end examples with foreign npm elements in both panes — with the notify wiring derived from the registry.
- Each page's boot mounts the same element in both panes from the manifest config, replaying attributes, plain property seeds and option rows (ADR 0005) through the session, and records notify traffic in the page's event log tagged by pane.
- The per-property pane sync carries JSON-faithful scalars only, with a canonical-JSON echo brake per property; rich types (a Zoned crossing JSON is a string, not a Temporal instance) stay per pane, their scalar twins carrying the information.
  The form keeps its whole-state broadcast channel and cross-tab story (ADR 0013).
- A manifest entry may declare a word pool: the page renders it as an editable textarea and answers both panes' `query-changed` from it — the standalone host role, the same `InMemorySource` semantics as the form's live pool (ADR 0014).
- Pages anchor themselves at the site root with a depth-derived `<base>`, so the single document-relative importmap web_modules emits serves every depth — the dev server, the embedded binary and a GitHub project page alike.
- The terminal pane follows the page theme: the page resolves its Bootstrap color mode from the stored toggle choice or the OS scheme (applied inline before first paint), the xterm palette derives from the custom properties resolved on the screen element, and the session sets the same mode on every mounted root (`data-bs-theme`, a plain DOM attribute write the cascade sees).
  Both halves repaint on change, and translucent colors below one half alpha drop instead of turning opaque (`uic_css`), so Bootstrap's low-alpha tints stop painting near-black strips.
- The panes respond to width: above the `md` breakpoint they sit side by side and follow the width slider through one CSS variable, below it they become tabs.
  The terminal resizes for real: `resize` mutates the backend size and returns the full-repaint ANSI (ratatui's autoresize clears and repaints), and the shared pane scaffold opens the terminal lazily on its first nonzero width, calibrates the cell width once the canvas measures plausibly, subtracts the pane chrome from the column math — pane width feeds back into columns otherwise — and funnels slider, tab and window widths through one debounced ResizeObserver.

The worker, the browser-hosted runtime for foreign panes:

- The mocked-lit runtime is host-agnostic TypeScript; only the retained document, the cascade and the paint need Rust.
  In the browser a foreign lit element runs on the native engine inside a dedicated module worker, against `uic_tui_web::DomSession` — a wasm export of the shared host operations (`uic_tui::dom::HostState`, the same bodies the Boa natives wrap on real terminals).
- The worker shims each operation onto its `__uic_*` global, imports the unchanged runtime modules and the component, mounts through `create_root` + `__uicMount`, delivers keys and clicks through `__uicDeliver` (picking pointer targets with `hit_test`, running the focused widget's editing default action per ADR 0026), and streams ANSI to the page per message.
  One settled turn follows each entry call — the browser's own job draining, where the Boa host needs `run_jobs()`.
- The worker is the module-resolution boundary: the page's importmap must keep `lit` resolving to the real lit for the browser pane, and import maps do not reach workers at all — so the build emits a worker module tree, the runtime compiled to files (the same per-module TypeScript pass the Boa host bakes) beside the vendored package with its bare `lit*` imports rewritten to relative paths.
  The rewrite covers the quoted-specifier grammar of ES modules; web_modules' AST readers are not public yet (an upstream proposal), and the browser resolves loudly on anything missed.
  A foreign page's entry and module list derive from the vendored tree at build time.
- The worker and its page-side client are a reusable artifact (`crates/uic_worker`): an extra source root for a `web_modules` build or a publish-ready npm tree, with the client's session surface matching the wasm sessions' so the same pane scaffolding drives either.

## Why

One Rust definition rendering two ways is the point of this repo, and the split view makes the claim inspectable: the same attributes, validation, notify events and popup semantics, DOM left, terminal cells right.
xterm.js was chosen over DOM-rendering alternatives (ratzilla) because it is a real terminal emulator fed by the same ANSI a terminal would receive, ships a proper ES module (`@xterm/xterm` 6.0.0) that vendors through web_modules, and adds no second ratatui version to the tree; the hand-rolled backend is a couple hundred lines against a stable trait.
crossterm cannot compile for wasm as published (its platform `sys` layers have no neutral arm) and no feature flag escapes it, so the fork pin is the enabling move.
The gallery exists because the form is just one example: a manifest entry per component keeps page, seeds and notify wiring derived instead of maintained, and the theme and resize work removed the hardcoded dark pane and fixed geometry the first page shipped.
The worker exists because the browser already ships the engine: the first foreign-element pane compiled a JavaScript VM (Boa) to wasm into a browser that is one, and hosting the runtime natively removed the doubled engine and its bundle weight while Boa stays the host for real terminals, where no engine exists — the runtime TypeScript is byte-identical across both hosts.

## Consequences

- Drop the crossterm pin when a release carrying the stubs reaches ratatui-crossterm and the rat crates; re-pin if the upstream PR rebases.
- crossterm's `event-stream` feature remains unavailable on neutral targets (its waker is platform-bound); nothing in this tree enables it.
- The release binary does not embed `/tui` — it serves the directory from disk, and the GitHub Pages deploy copies the wasm bundle beside the baked site; baking the wasm output in as an extra build root is the recorded follow-up if the split view should ship self-contained.
- The wasm binary weighs a few MB; acceptable for a demo, `wasm-opt` deliberately not part of the pipeline yet.
- A new catalog component joins the gallery by one manifest entry; the page, seeds and notify wiring follow from it.
- One wasm bundle serves the catalog panes and the foreign panes; the examples pages reuse the already-cached artifact.
- The `__uic_*` surface is an explicit contract with two implementations — Boa natives on real terminals, the wasm session behind worker shims — a conformance seam the lit-test harness can also drive.
- Foreign code runs isolated from the page (a worker has no DOM) and cannot collide with the browser pane's real lit; its frames arrive pushed rather than returned, and the pane scaffolding accepts both flows.
- Driving the exposed `window.__tui` session directly desynchronizes the visible xterm (its ANSI goes to the caller, and later diffs skip unchanged cells); `screen_text()` stays authoritative, which is exactly what the browser tests assert against.
- Rows stay fixed per example; only columns follow the width.
