# uic_npm

One npm-tree emitter, shared by the crates that publish compiled TypeScript.
`emit_tree` compiles every `*.ts` under a source root to `*.js` and writes a dependency-free, publish-ready `package.json`.

Three trees ride it — `@schuhkarton/uic-sync`, `@schuhkarton/uic-worker`, and the lit-demo's `@schuhkarton/lit-todo` — differing only in name, description, exports and (the app alone) peer dependencies.
It replaced three hand-written copies of the same read-dir → compile → scaffold sequence, and the emitted bytes are unchanged from those copies.

```sh
cargo test -p uic_npm
```
