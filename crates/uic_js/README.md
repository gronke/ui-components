# uic_js

A Boa-embedded JS engine hosting real LitElement components on the terminal runtime.

Components import a mocked `lit` — TypeScript modules under js/src/ mirroring the specifiers (`lit.ts`, `lit/decorators.ts`, …), compiled per module by the build script through web_modules and served by the in-memory loader: `LitElement` installs per-property accessors that schedule microtask updates, `html` captures template strings and values, and `performUpdate` commits the rendered subtree through the `__uic_*` natives into the retained `uic_tui::dom::DomDocument` — the existing taffy layout and ratatui paint draw it unchanged (`uic_tui::dom::paint_document`).

Events travel the other way: the host synthesizes bubbling `keydown`/`click`/`focusin`/`focusout` DOM events (`__uicDeliver`), template `@event` bindings resolve through render-scoped listener markers with lit's host-`this` contract, and the DOM focus bridges into the paint (a focused plain node reads as a one-row selection bar); focus survives each subtree swap by re-resolving its `data-path`.

A component's `static styles` reach the terminal too: `customElements.define` hands the collected css`` text to `uic_tui::dom::adopt_component_sheet`, and the cascade scopes it per instance — json-viewer's own palette, `calc()` indentation and `ul` reset style the pane with no hardcoded entries.
Its `.collapsable::before` marker renders as a generated box (▶ turning ▼ through `transform: rotate(90deg)`), keys and values flow on one row through the anonymous inline rows, and clicking the marker cell hits the owning key span.

The demo component runs byte-unmodified: `build.rs` vendors the packages declared in `package.json` (the component, xterm.js, and the real lit family for the split view's DOM pane), and json-viewer's own LitElement code — decorators, directives, roving-tabindex keyboard navigation, click-to-toggle — drives the terminal.

```sh
cargo test -p uic_js
cargo run -p uic_js --example json_viewer        # interactive terminal demo
cargo run -p uic_js --example json_viewer_web    # browser split view on :8091
cargo test -p uic_js --release --test measure -- --ignored --nocapture
```

The render path is a deliberate simplification: a subtree swap (serialize, `parse_fragment`, `import_node`), not per-part diffing — instant at form scale, measurably slow on very wide documents; per-part commits are the recorded follow-up.

`tests/boa_quirks.rs` is the canary for a Boa 0.21 engine bug the runtime works around (a closure created inside a class constructor capturing a local lexical binding panics the VM); when it starts failing, Boa fixed the bug — drop the module-level accessor installation with it.
