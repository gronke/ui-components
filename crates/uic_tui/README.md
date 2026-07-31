# uic_tui

The terminal runtime: components mount as element nodes on a retained DOM (`uic_dom`), taffy computes layout over terminal cells, paint hosts rat-widget input primitives living in the node payloads, and keys and the pointer travel the tree.

It runs the same `uic_core` definitions the browser renders through generated Lit, so a component's structure and lifecycle behave identically on both targets.
The built-in widget kinds (date, text, number, textarea, select) live here; a component's own `data-tui` widget twins register through the `inventory` `WidgetRegistration` seam (ADR 0026), the analog of the element registry.

```rust
ui_components_tui::link();
let mut app = uic_tui::App::new()?;
let el = app.mount("input-date")?;
app.set_attr(el, "label", "Date of purchase");
app.run()?;
```

`uic_tui_web` compiles this stack to WebAssembly for the browser, and `uic_js` hosts real npm lit elements on top of it through Boa.

```sh
cargo test -p uic_tui
```
