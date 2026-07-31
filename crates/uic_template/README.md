# uic_template

The Lit-flavored template parser and IR, shared by every render target.

One parser serves both authoring forms — inline template strings and `.html` files — and both targets that consume a template: the generated Lit TypeScript (`uic_codegen_web`) and the terminal runtime (`uic_tui`).
Parsing the dialect once, here, is what keeps the two targets rendering the same structure.

The dialect is an HTML subset with lit-html's binding sigils: `${name}` text holes, `attr=${name}` and `?attr=${name}` attribute and boolean bindings, `.prop=${name}` element properties, `@event=${handler}` handlers, and `<template if=${name}>` conditional subtrees.
Hole expressions are deliberately closed to bare identifiers and `!ident`; anything richer belongs in a named computed property or handler on the component, which keeps every template executable by both targets.

```sh
cargo test -p uic_template
```
