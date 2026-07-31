# dist

Builds the publish-ready npm tree into `dist/npm/`: compiled Lit ESM components and declarations, `elements.css`, the Custom Elements Manifest, and a `package.json` with lit as a peer dependency.

This is the packaging entry for the generated web catalog — it drives `uic_codegen_web`'s `dist` feature to emit what a browser project would `npm install`, rather than serving anything itself.

```sh
cargo run -p uic_dist    # writes dist/npm/
```
