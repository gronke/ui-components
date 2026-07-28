# ADR 0014: Async data sources are connectors behind one query interface

## Decision

Suggestion-style components obtain data through a connector contract defined once per target and co-located (ADR 0002): `crates/ui_components/src/connect.rs` beside its browser twin `connect.impl.ts`.

```rust
pub type Deliver<'a> = Box<dyn FnOnce(Vec<SelectOption>) + 'a>;
pub trait QuerySource {
    fn query(&self, text: &str, deliver: Deliver<'_>);
}
```

```ts
export interface QuerySource {
  query(text: string): Promise<SelectOption[]>;
}
```

Variants ship with the catalog: `InMemorySource` (a fixed pool; case-insensitive value-prefix match in pool order, capped at `limit`, the empty query resolves empty), `MethodSource` (a provided function answers each query), and — TypeScript only — `FetchSource` (a URL with a colon-notation `:query` parameter, `RequestInit` options, and a JSON-body mapper defaulting to arrays of strings or option-shaped objects).

The component side of the contract is `<input-suggestion>`: the live text is a `query` notify property written once per keystroke (`query-changed`), the answer arrives as the property-only `suggestions: Vec<SelectOption>` (options are data, ADR 0005), and the popup renders whatever the property holds, whenever it arrives.
The glue is a slim wrapper listening to the input's events: in the demo, app-root's `on_word_query` handler delegates to a static pool (`WORD_POOL` in `app_root.rs`, `wordPool` in the twin — its browser half is genuinely `async`); for standalone browser use the connectors module exports `connectSuggestions(el, source)`, which answers `query-changed` events with `el.suggestions` writes and drops out-of-order responses.

The web codegen emits the TS twin as `components/uic-connectors.ts` through the `WebCodegen::extra_module(name, source)` hook (`DistBuild` forwards it and adds a package export); the consuming build scripts pass `ui_components::connect::WEB_TS` explicitly.

### The `@input` route

Live text needs a per-keystroke path that the commit-only `@change` route cannot provide.
A widget adapter reports edited text through `WidgetAdapter::take_input()` (consumed once); after every event the app host drains it (`flush_widget_input`), routes it into the template's `@input` binding (`Mount::dispatch_widget_input`, sharing the `@change` dispatcher) as a `UiEvent::input`, and bubbles a DOM `input` event — typing keeps flowing with the popup open or closed.
In the browser the same `@input=${on_input}` binding is the native per-keystroke event; both targets run the same component logic: `on_input` writes the `query` property, notify does the rest.

## Why

Properties carry only data — `Value` has no function variant, and the update cycle is synchronous — so a query cannot be a function-valued property or an async method on component logic.
Callback delivery is the one shape every host supports: an in-memory source delivers inside the current update cycle, which is the only delivery that can repaint the terminal popup in the same frame (the native loop blocks in `crossterm::event::read()` with no waker, and `App::on` listeners must not re-enter); the browser twin resolves Promises and lands late answers as property writes, the ADR 0013 pattern.
An async trait would buy nothing the deliver callback does not already express: no executor exists anywhere in the terminal stack.
`FetchSource` stays TypeScript-only: the component stack carries no HTTP client, the demo queries the in-memory pool, and `MethodSource` covers native injection.
A native fetch variant (for example `ureq` behind a feature) and a wake-capable native pump for deferred delivery are recorded follow-ups.

### Mixins, the translation path

The catalog's mixin idiom is attribute macros stacked above the derive: `#[input_shared]` injects the shared contract properties and wraps the template, exactly what a Lit mixin's `static properties = { ...super.properties }` plus render wrapping does.
When a second queryable component appears, the hand-wired trio — the `query` notify property, the `suggestions` property, the wrapper handler — lifts into a `#[queryable]` macro the same way, and connector declarations can accumulate like endpoint lists in a mixin chain.
The contract ships hand-wired once; the macro waits for its second consumer.

## Consequences

- The suggestion popup is per-target presentation (ADR 0002): `suggestion.impl.ts` renders a Bootstrap `dropdown-menu` and drives it from `keydown`/`mousedown` listeners, `tui.rs` paints a bordered row list through the overlay protocol; both fill from the same `suggestions` property and commit picks through the regular change path.
- The parity fixtures replay the pool through both implementations (the `suggest` cases in `crates/uic_codegen_web/tests/parity/fixtures.json`); `scripts/parity-check.mjs` also spot-checks `MethodSource` and `FetchSource` against a stubbed `fetch`.
- The commit contract stays the text family's: trimmed, empty committing null under `allow-null`; picking a suggestion fills the input and commits like typed text, and Esc merely closes — the popup only ever moves a highlight, so there is nothing to revert.
