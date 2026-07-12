# Architecture

One Rust definition per UI component, rendered two ways: generated LitElement-flavored TypeScript for the browser and a retained-DOM runtime for the terminal (native and wasm).
The decisions behind every load-bearing shape live in [adr/](adr/README.md); this page is the map.

## Crates

| Crate | Role |
|-------|------|
| `uic_template` | The lit-flavored template dialect: parser, IR, chrome splicing, and the JS naming rules (`names`) both sides of the toolchain share. |
| `uic_macros` | `#[derive(CustomElement)]` and `#[input_shared]`: parses the options (`component/args`), models the properties (`component/props`), validates the template, and emits the static `ComponentDef`. |
| `uic_core` | The component model: `Value` (a closed set, ADR 0005), `PropertyMeta`/`ComponentDef` with the per-type capability table, `PropertyStore`, the `Behavior` lifecycle contract, notify semantics, the inventory-backed registry, JSON conversion (feature `json`, ADR 0013), and the test-cycle helpers (feature `testing`). |
| `uic_dom` | The retained document (ADR 0008): arena DOM with per-node payloads, html5ever parsing and serialization, event dispatch, and the parts engine templates compile to (ADR 0010). |
| `uic_tui` | The terminal runtime: components mount on the document (`dom/host`, ADR 0011), composites route children and events (`dom/composite`) — commits through `@change` and live text through `@input` — holes resolve against the store (`dom/resolve`), one `WidgetAdapter` per rat widget (`dom/widget/`, built-in or registered via `WidgetRegistration`, ADR 0015), taffy layout and ratatui paint (`dom/layout`, `dom/render`), `App` hosts it all (ADR 0012), and `lint` gates template TUI-compatibility over the linked registry (ADR 0016). |
| `uic_tui_web` | The browser host (ADR 0007): the same runtime compiled to wasm, an ANSI backend for xterm.js, and the `TuiSession` JSON boundary. |
| `uic_codegen_web` | The web output: one Lit class per component, the Custom Elements Manifest, impl-partial checks (exports and arity), the shared impl helpers, and the npm dist build (feature `dist`, ADR 0004) which honors `dist = false`. |
| `ui_components` | The catalog: the input components, the `nav_tabs` bar (ADR 0017) and the demo composition (`demo/app_root`), each a `.rs` definition beside its `.mhtml` template and `.impl.ts` twin — new components as one directory per component including the terminal widget twin (`tui.rs`, feature `tui`, ADR 0015) — plus the data connectors (`connect`, ADR 0014). |
| `apps/web-demo` | The split view: the DOM components and the wasm terminal pane side by side, synchronized over the state bridge (ADR 0013). |
| `apps/dist`, `apps/tui-demo` | The npm tree builder and the native terminal demo. |

## The update lifecycle

Both targets run ReactiveElement's order behind the shared names of ADR 0002.
A trigger (attribute, property write, routed event) collects a `Changed` batch; `will_update` joins the same batch over its snapshot; notify events emit and bubble; reflected properties land on the host attributes; the commit resolves the template holes and patches the parts; `updated` runs on the committed state and its writes drive a converging follow-up cycle.
`uic_core::testing::cycle` replays the single-step core of this contract for component unit tests.

## The component trio

A component is one Rust struct (`#[derive(CustomElement)]`, properties as fields), one `.mhtml` template (the closed expression language of ADR 0001), and — when it has browser behavior — one hand-written `.impl.ts` twin mirroring the Rust `Logic` implementation.
Codegen enforces the twin's export set and simple-signature arity, the shared idioms live in the emitted `uic-impl-helpers.ts`, and the parity harness (`uic_codegen_web/tests/parity.rs` + `scripts/parity-check.mjs`) replays Rust-generated fixtures against the compiled twin.

## Load-bearing runtime semantics

- Writes suppress equal values at the `PropertyStore`: an unchanged property joins no batch and notifies nobody — components need no equality guards of their own (`Ctx::set` documents the contract).
- A template hole that resolves null or undefined still WRITES into a bound child property (as `Value::Null`, the browser's `el.prop = null`); only an unchanged hole skips. `dom/resolve::part_value_to_value` documents it, the `null-flow` test in `uic_tui/tests/dom_host.rs` pins it.
- `Mount::commit` re-resolves every hole per commit (computed getters included) and relies on per-part dirty checks for minimal tree writes; the cost note on `commit` names the future changed-props filter.
- Listeners registered through `App::on` and `TuiSession::on_notify` run under the update's `&mut` borrow: callbacks must hand data on and return, never call back in. Asynchronous delivery (the BroadcastChannel pattern) is the sanctioned way back into the session.
- Widget mouse handling never routes raw pointer events into rat's own handlers: rat's click arming reads the system clock, which wasm32 does not have. Picks resolve against the published geometry instead.
