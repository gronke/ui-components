# web-demo-tui

The web-demo's browser-TUI wasm entry (ADR 0007): it binds the reusable, catalog-agnostic terminal host (`uic_tui_web`) to this demo's catalog (`ui_components` plus the `<app-root>` composition), so the host itself stays free of any one catalog.

`uic_tui_web`'s `TuiSession` is a wasm-bindgen export that rides along into this cdylib; `link_catalog` anchors the host and the catalog into the bundle so the linker keeps their inventory constructors, which `TuiSession::new`'s `__wasm_call_ctors` then runs.
`scripts/build-wasm.sh` builds it into `apps/web-demo/web-tui/`, which the web-demo serves under `/tui`.

```sh
./scripts/build-wasm.sh    # builds the wasm bundle into apps/web-demo/web-tui
```
