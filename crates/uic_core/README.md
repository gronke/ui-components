# uic_core

The component model at the center of the framework: `ComponentDef`, `PropertyMeta`, `Behavior`, notify semantics, and the `inventory`-backed custom-element registry.

The vocabulary mirrors vanilla Web Components (`customElements.define`, the HTMLElement/ReactiveElement lifecycle), so a definition is target-agnostic.
LitElement is one output variant with fixed assumptions, produced by `uic_codegen_web`; the terminal runtime (`uic_tui`) consumes the same definitions directly.

Components reach the registry through `inventory`, the linker-collected analog of `customElements.define`: a component registers itself, and any binary that links it can look it up by tag.
`uic_macros`'s derive builds each `ComponentDef` and submits it here.

```sh
cargo test -p uic_core
```
