# ADR 0023: The browser hosts its own runtime

## Context

The first foreign-element pane for the gallery shipped Boa — a JavaScript VM — compiled to
WebAssembly, into a browser that is a JavaScript VM.
That doubled the engine, tripled the bundle (8.6 MB beside the catalog's 3.1 MB), and inherited
Boa's language gaps, for no benefit the browser did not already provide.

## Decision

The mocked-lit runtime is host-agnostic TypeScript; only the retained document, the cascade and
the paint need Rust.
In the browser, a foreign lit element runs on the native engine inside a **dedicated module
worker**, against `uic_tui_web::DomSession` — a wasm export of the shared host operations
(`uic_tui::dom::HostState`, the same bodies the Boa natives wrap on real terminals).
The worker shims each operation onto its `__uic_*` global, imports the unchanged runtime modules
and the component, mounts through `create_root` + `__uicMount`, delivers keys and clicks through
`__uicDeliver` (picking pointer targets with `hit_test`), and streams ANSI to the page per message.
One settled turn follows each entry call — the browser's own job draining, where the Boa host
needed `run_jobs()`.

The worker is not just isolation: it is the module-resolution boundary.
The page's importmap must keep `lit` resolving to the real lit for the browser pane, and import
maps do not reach workers at all — so the build emits a worker module tree: the runtime compiled
to files (the same per-module TypeScript pass the Boa host bakes) beside the vendored package with
its bare `lit*` imports rewritten to relative paths.
The rewrite covers the quoted-specifier grammar of ES modules; web_modules' AST readers are not
public yet (an upstream proposal), and the browser resolves loudly on anything missed.

Boa remains what it always really was: the host for native terminals, where no engine exists.
Its full path stays proven by `uic_js`'s native test suite and the real-terminal examples; the
runtime TypeScript is byte-identical across both hosts.

## Consequences

- One wasm bundle serves the catalog panes and the foreign panes; the examples pages reuse the
  already-cached artifact.
- The `__uic_*` surface is now an explicit contract with two implementations — a conformance seam
  the lit-test harness can also drive.
- Foreign code runs isolated from the page (a worker has no DOM) and cannot collide with the
  browser pane's real lit.
- The per-message hop between page and worker makes frames pushed rather than returned; the pane
  scaffolding accepts both flows.
