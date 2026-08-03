# uic_tui_web

Browser host for the terminal runtime: the `uic_tui` stack compiled to WebAssembly, rendering frames as ANSI for xterm.js and feeding DOM keyboard and pointer events back in.

Two sessions share the `XtermBackend`:

- `TuiSession`, the catalog path: mounts registered Rust components (`ui_components`), replays attributes and properties, routes notify events out, resizes, themes.
- `DomSession`, the native engine's host boundary (ADR 0007): exposes the shared host operations (`uic_tui::dom::HostState`) so the unchanged mocked-lit runtime runs on the browser's own JS engine in a dedicated worker.
  A foreign npm lit element mounts against the retained document, its `static styles` parsed into the cascade, keys and clicks delivered by the worker through `__uicDeliver`.

The same host operations back the Boa natives on real terminals (`uic_js`): one body per operation, two thin wrappers.

```sh
cargo test -p uic_tui_web        # sessions and backend, natively
./scripts/build-wasm.sh          # the browser bundle (served as /tui)
```
