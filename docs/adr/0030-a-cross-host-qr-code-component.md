# ADR 0030: A cross-host `<qr-code>` component

## Decision

The QR code is a shared component, `<qr-code data="…">` (baked into `@schuhkarton/lit-todo`, loaded by both hosts), rendering alike in the browser and the terminal.
In the browser it draws an SVG with an external library (`qrcode-generator`), loaded through a dynamic `import()`; on the terminal the same element mounts a native Rust QR widget, selected by a `data-tui="qr"` marker through the widget registry (ADR 0027) and fed the data on the `value` attribute.
Both hosts paint it black on white explicitly — a camera wants dark modules on a light ground whatever the theme — the browser on its own white card, the widget with explicit cell colors.
The panel renders it inline, completing the QR the shared panel had to leave out (ADR 0029); the terminal hides that inline copy through the panel's terminal-only `static styles` and instead composes a `<p2p-deck>` — a flex-wrap container whose stylesheet docks the QR to the right of the todo card and the panel on a wide terminal and wraps it below them under about 200 columns, real taffy flexbox instead of host rect math.
The half-block renderer (`qrcode` crate, Dense1x2) is one `render_qr` function shared by the widget and the live-mode join pane.

## Why

The QR existed twice, and in neither shared place: a browser-only SVG in `pair-wizard`, and a separate Rust `qr_pane` painted beside the app in live mode.
ADR 0029 kept the QR out of the shared panel because Boa has no SVG path, which left the terminal peer with no scannable code at all.
ADR 0027 already lets a custom element mount a host-drawn terminal widget through the `data-tui` inventory registry — the seam `<nav-tabs>` uses — so a `<qr-code>` can be a real component: an SVG in the browser, a native grid on the terminal, one element with two renderers, the `todo-item` `[x]`-span precedent at QR scale.

## Consequences

- The browser QR library must never enter Boa's static module graph, which would fail the terminal at link time; it loads through a dynamic `import()` executed only in the browser (guarded, and swallowed if it ever runs under Boa), so the terminal renders the native widget and nothing else.
- The QR data rides the widget's `value` channel — the one attribute the scripted host mirrors — and flows to the nested `<qr-code>` as an attribute, since a `.prop=` binding would not survive the serialize commit.
- The terminal QR is as large as its payload: a long invite link is a high-version grid, so the deck wraps it below the panes on narrow terminals, where the link and token text remain the working fallback; shrinking the payload is a separate concern (ADR 0031 leaves the codec byte-stable).
- The deck's breakpoint is flex-basis arithmetic (the stack's `width` beside the QR's intrinsic columns), not a media query — `uic_css` maps `display: flex`, `flex-wrap`, `min-width` and `ch` lengths onto taffy, but has no `@media`/container conditions and no `flex-basis` property (the recorded follow-ups if a literal breakpoint is ever wanted).
- The deck carries no reactive properties, so it commits exactly once — a re-commit would swap the composed `<todo-app>` fresh and lose its live state; the hosts drive the nested elements directly by node.
- The live-mode join pane keeps its placement and now shares the one `render_qr` helper with the widget.
