# uic_js

The exploration host for issue #65: a Boa-embedded JS engine running real LitElement components against the terminal runtime.

Components import a mocked `lit` (js/bootstrap.js behind an in-memory module loader): `LitElement` installs per-property accessors that schedule microtask updates, `html` captures template strings and values, and `performUpdate` commits the rendered subtree through the `__uic_*` natives into the retained `uic_tui::dom::DomDocument` — the existing taffy layout and ratatui paint draw it unchanged (`uic_tui::dom::paint_document`).

Events travel the other way: the host synthesizes bubbling `keydown`/`click`/`focusin`/`focusout` DOM events (`__uicDeliver`), template `@event` bindings resolve through render-scoped listener markers with lit's host-`this` contract, and the DOM focus bridges into the paint (a focused plain node reads as reverse video); focus survives each subtree swap by re-resolving its `data-path`.

The exploration target runs byte-unmodified: `build.rs` vendors `@alenaksu/json-viewer` from npm through web_modules, and its own LitElement code — decorators, directives, roving-tabindex keyboard navigation, click-to-toggle — drives the terminal.

```sh
cargo test -p uic_js
cargo run -p uic_js --example json_viewer            # interactive demo
cargo test -p uic_js --release --test measure -- --ignored --nocapture
```

The render path is the exploration's deliberate simplification: a subtree swap (serialize, `parse_fragment`, `import_node`), not per-part diffing — measured at ~300 ms per keystroke on a 500-row document (instant at form scale), the recorded motivation for the per-part follow-up.

`tests/boa_quirks.rs` is the canary for the Boa 0.21 bug the bootstrap works around (a closure created inside a class constructor capturing a local lexical binding panics the VM); when it starts failing, Boa fixed the bug — drop the `installAccessors` hoisting with it.
