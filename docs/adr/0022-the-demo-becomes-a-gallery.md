# ADR 0022: The demo becomes a gallery

## Context

The web demo was one page: `<app-root>` beside its terminal twin.
The form is just one example, the terminal pane wore a hardcoded dark theme against a light page
(so `Color::Reset` cells painted black behind the inputs while the Rust cascade filled the card
light), and nothing could resize the terminal.

## Decision

One manifest in `apps/web-demo/build.rs` generates the site: a gallery index at the root and one
page per entry under its route — `/demo/` for the composed form, `/components/<tag>/` for every
catalog component, `/examples/<name>/` for maintained end-to-end examples (foreign npm
elements in both panes; the terminal side runs on the browser's own engine in a worker,
ADR 0023) — seeded like
`apps/tui-demo`, with the notify wiring derived from the registry.
The per-property pane sync carries JSON-faithful scalars only; rich types (a Zoned crossing JSON is
a string, not a Temporal instance) stay per pane, their scalar twins carrying the information.
A manifest entry may declare a word pool: the page renders it as an editable textarea and answers
both panes' `query-changed` from it — the standalone host role, same `InMemorySource` semantics as
the form's live pool.
Pages anchor themselves at the site root with a depth-derived `<base>`, so the single
document-relative importmap web_modules emits serves every depth, on the dev server, the embedded
binary and a GitHub project page alike.

The terminal pane follows the page theme.
The page resolves its Bootstrap color mode from the stored toggle choice or the OS scheme (applied
inline before first paint); the xterm palette derives from the custom properties resolved on the
screen element, and the session hands the same mode to the mounted document
(`App::set_dom_attr(index, "data-bs-theme", …)` — a plain DOM attribute write the cascade sees, the
browser's setAttribute for names outside observedAttributes).
Both halves repaint on change: the palette covers `Reset` cells and rings, the cascade the
truecolor fills.
Translucent colors below one half alpha drop instead of turning opaque (`uic_css`), so Bootstrap's
low-alpha tints stop painting near-black strips.

The panes respond to width.
Above the `md` breakpoint they sit side by side and follow the width slider through one CSS
variable; below it they become tabs.
The terminal resizes for real: `TuiSession::resize` mutates the backend size and returns the
full-repaint ANSI (ratatui's autoresize clears and repaints), and the pane boot opens the terminal
lazily on its first nonzero width, calibrates the cell width once the canvas measures plausibly,
subtracts the pane chrome from the column math — pane width feeds back into columns otherwise, a
few columns per observation up to the clamp — and funnels slider, tab and window widths through one
debounced ResizeObserver.

## Consequences

- A new catalog component joins the gallery by one manifest entry; the page, seeds and notify
  wiring follow from it.
- The example boot (`web/example.ts`, `tui-pane.ts`, `sync.ts`) is the reusable pane pair: per-notify
  property sync with a canonical-JSON echo brake, while the form keeps its whole-state broadcast
  channel and cross-tab story.
- Without the wasm bundle every page degrades to the web pane alone; the Terminal tab hides.
- Rows stay fixed per example; only columns follow the width for now.
