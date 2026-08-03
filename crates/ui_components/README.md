# ui_components

The component catalog: one Rust definition per custom element.

Each component module co-locates its Rust definition (`date.rs`), lit-flavored template (`date.html`), stylesheet (`date.scss`), and web partial (`date.impl.ts`) for behavior the browser cannot derive.
A single definition drives both targets: `uic_codegen_web` emits the browser's Lit class from it, and `uic_tui` mounts it directly on the terminal.

The crate is pure web/definition: it depends on `uic_core` and carries no terminal runtime.
The terminal widget twins for its `data-tui` components live in the path-mirrored companion crate `ui_components_tui` (ADR 0002), and the demo composition `<app-root>` lives in `ui_components_demo`.

```sh
cargo test -p ui_components
```
