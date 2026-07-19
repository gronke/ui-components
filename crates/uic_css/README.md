# uic_css

CSS for the terminal (ADR 0021): a closed dialect parsed with servo's cssparser and selectors crates, cascaded over the retained `uic_dom` document into computed styles the layout and paint consume.

The dialect keeps what cells can express — display, box spacing, flex, sizes, colors (ANSI palette, 24-bit, `Highlight` as reverse video), text styling, `content` with right-angle `transform: rotate(...)`, custom properties with `var()` and additive `calc()` — and drops everything else into a report.
Unknown pseudo-classes parse and never match; `:focus` matches the host's focused node; component scoping clamps ancestor walks at the dash-tag boundary with `:host` as the component element.
Rules targeting `::before`/`::after` resolve in a pseudo pass per element, inheriting from the owner; an element's resolved entry carries its own style plus the pseudo styles whose cascade produced `content`.

Units calibrate to the repo's own cells: one column = 0.75rem = 12px = 1ch, one row = 1.5rem = 24px = 1lh, rounded half away from zero with borders and gaps flooring at one cell.

`gen-tui-css` filters the vendored compiled Bootstrap into the utility sheet `uic_tui` ships:

```sh
cargo run -p uic_css --bin gen-tui-css   # regenerate crates/uic_tui/css/bootstrap-tui.gen.css
cargo test -p uic_css
```
