# ADR 0002: Behavior hooks are implemented once per target behind shared names

## Decision

A component's behavior surface is the set of handler and computed-property names its template references.
Each name is implemented twice, deliberately:

- Rust: the derive-generated `<Name>Logic` trait (required methods per referenced name, defaulted lifecycle hooks) drives the terminal and native targets.
- Browser: a co-located `.impl.ts` partial exports one function per name (`export function onChange(el: InputDate, e: Event)`); the generated class delegates through `import * as impl`.

`uic_codegen_web` scans the partial's exports and fails the build listing missing names, so the two surfaces cannot drift apart structurally.
Behavioral parity is pinned by tests exercising the same inputs on both sides (Rust unit tests, browser/TestBackend end-to-end tests).

## Why

Template structure, properties, and events are single-sourced in Rust; imperative logic cannot be auto-translated without a transpiler or a WASM runtime in the browser — both rejected for v1 (the generated output must stay readable, dependency-light Lit).

## Consequences

Logic like date parsing exists in `date.rs` and `date.impl.ts`; the doc comments cross-reference each other and changes must touch both.
`HandlerKind` on `HandlerMeta` is the seam where a future `SharedWasm` variant plugs in: codegen would emit a wasm-calling stub instead of the impl import, removing the duplication for components that opt in.
