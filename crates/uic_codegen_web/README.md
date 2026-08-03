# uic_codegen_web

The web codegen backend: it turns registered component definitions into readable TypeScript (the LitElement variant) plus SCSS partials and an aggregator, laid out as an extra source root for a `web_modules::build`.

Where `uic_tui` runs the definitions directly, this crate emits the browser's half of each twin: one Lit class per component, the co-located `.impl.ts` behavior partial copied through, the shared runtime port emitted once, and the connectors.
The generated output is meant to read like hand-written, dependency-light Lit (ADR 0002): no transpiler, no wasm in the page.

Generated root layout:

```text
<out>/
├── components/<tag>.ts          one Lit class per component
├── components/<tag>.impl.ts     co-located behavior partial, copied
├── components/uic-runtime.ts    LitNotify port, emitted once
├── components/uic-*.ts          extra shared modules (the connectors)
├── components/_<tag>.scss       component stylesheet (grass partial)
├── elements.scss                aggregator, compiled to /elements.css
└── custom-elements.json         optional Custom Elements Manifest
```

The `dist` feature adds the npm-distributable build: compiled ESM, `.d.ts` and CSS through `web_modules`.

```sh
cargo test -p uic_codegen_web
cargo test -p uic_codegen_web --features dist
```
