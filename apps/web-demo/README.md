# web-demo

Browser demo: the generated `ui_components` catalog served through `web_modules`.

`build.rs` bakes the frontend: the generated Lit components, the vendored modules, and the `<app-root>` composition (`ui_components_demo`).
The binary serves it.
The `/tui` pane runs the same catalog in the browser through the WebAssembly terminal host (`web-demo-tui`), so the page shows both renderings side by side.

```sh
cargo run -p uic_web_demo             # live-reload dev server on 127.0.0.1:8080
cargo run -p uic_web_demo --release   # everything embedded, no filesystem
```
