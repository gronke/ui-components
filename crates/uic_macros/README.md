# uic_macros

The derive macros for the catalog: `#[derive(CustomElement)]`.

The derive turns a struct into a registered custom element.
It builds the `&'static ComponentDef` (tag, properties, handlers, computed names, and the embedded template/scss/impl sources) and submits it to the `uic_core` `inventory` registry, the `customElements.define` analog.
It validates the template at compile time with the same `uic_template` parser the runtimes use.
It also generates the per-component `<Name>Logic` trait carrying exactly the handler and computed-property names the template references, plus defaulted lifecycle hooks.

A component always provides an `impl <Name>Logic` block, even an empty one; a template-referenced handler with no implementation surfaces as a plain missing-trait-method error.

```sh
cargo test -p uic_macros
```
