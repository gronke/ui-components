# lit-demo

One hand-written Lit todo app — `<todo-app>` composing `<todo-item>` rows, plain `lit` and nothing else — run by two hosts from one crate:

```sh
cargo run -p uic_lit_demo             # the app in this terminal (uic_js/Boa + ratatui)
cargo run -p uic_lit_demo -- serve    # the same sources on real lit, http://127.0.0.1:8090
```

Both targets speak the same keys: type to draft a new entry and Enter adds it, Space toggles the selected row, Enter edits the selected row in place (typing changes it live, Enter finishes, emptying the text deletes the row), ArrowUp/ArrowDown select, a click toggles.
Esc quits the terminal.
The component's `static styles` drive both renderings — the `[x]`/`[ ]` markers are `::before` generated content from one stylesheet.

## One tree, two hosts

`build.rs` compiles `web/src/*.ts` once into an npm-shaped package (`$OUT_DIR/npm/@schuhkarton/lit-todo`):

- the terminal host loads it like any installed package (`uic_js::JsHost::load_package`) and runs it against the mocked lit;
- the browser build takes the same tree as a source root beside the Tera-rendered page, with the real lit family vendored from `web/package.json` — vendoring is not transitive, so the manifest names lit's own channels too.

The app sticks to the idioms both engines serve (see `crates/uic_js/README.md`, Runtime mechanics): composition data flows down as attributes, keyboard input is a plain `keydown` listener on the host element, and template event values are method references with row context in a `data-*` attribute.
Module names must not shadow the terminal runtime's specifiers — never name app files `main.ts`, `runtime.ts` or `lit*.ts`.

## Knobs

- `UIC_LIT_DEMO_ADDR=host:port` moves the server (default `127.0.0.1:8090`).
- `WEB_MODULES_EMBEDDED=1` forces the fully-embedded dist (no filesystem reads).
- Dev serving recompiles `web/pages/` live; a change under `web/src/` re-bakes through cargo — restart the server.
