# ui_components_demo

The catalog's demo composition (ADR 0013): `<app-root>` assembles the input components around one `state` object, rather than being an input itself.

It ships out of the published npm tree (`dist = false`) yet rides the generated web catalog and both runtimes, so it lives beside the catalog rather than in it — the reusable inputs stay in `ui_components`, and the one demo screen that wires them together stays here.
The demo's word-pool, the static query source behind the suggestion input, rides along.

A terminal or browser host links `ui_components_demo` to get `<app-root>` on top of the catalog it already links.

```sh
cargo test -p ui_components_demo
```
