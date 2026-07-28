# uic_worker

The browser worker host as a reusable artifact (ADR 0007): the dedicated module worker that runs
a foreign lit element on the browser's own engine against `uic_tui_web::DomSession`, and the
page-side client whose session surface matches the wasm sessions', so the same pane scaffolding
drives either.

Two TypeScript sources live in `web/`:

- `tui-worker.ts` — the worker: shims the host operations onto their `__uic_*` globals, imports
  the runtime module tree and the component entry, mounts, and streams ANSI frames per message.
- `client.ts` — `connectWorkerSession`: the postMessage facade a page wires into its terminal
  pane; frames arrive pushed through `onAnsi`.
  Key events carry their modifier flags across the boundary, so the delivered keydown matches
  the native Boa host's contract (`uic_tui::keys`).

Consumers integrate one of two ways:

```rust
// As an extra source root of a web_modules build (the demo's path):
build(&BuildOptions { roots: &[web, uic_worker::web_root()], .. })

// Or as a publish-ready npm tree, installable like any package:
uic_worker::npm_tree(&out, version)?; // @schuhkarton/uic-worker
```

The worker expects the catalog wasm bundle served as `/tui` and the runtime module tree as
`/tui-worker/modules/` — `apps/web-demo/build.rs` shows the full wiring, including the build-time
rewrite of the vendored package's bare `lit*` imports (import maps do not reach workers).

```sh
cargo test -p uic_worker   # the npm tree emits compiled ESM and a package manifest
```
