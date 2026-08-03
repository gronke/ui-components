# ADR 0002: Behavior hooks are implemented once per target behind shared names

## Decision

A component's behavior surface is the set of handler and computed-property names its template references.
Each name is implemented twice, deliberately:

- Rust: the derive-generated `<Name>Logic` trait (required methods per referenced name, defaulted lifecycle hooks) drives the terminal runtime on every host.
- Browser: a co-located `.impl.ts` partial exports one function per name (`export function onChange(el: InputDate, e: Event)`); the generated class delegates through `import * as impl`.

`uic_codegen_web` scans the partial's exports and fails the build listing missing names (and wrong arities), so the two surfaces cannot drift apart structurally.

### One lifecycle, byte-comparable across targets

The lifecycle hooks are `connected`, `will_update` and `updated`: defaulted methods on the Rust `Behavior` trait, and impl-export hooks in the generated TypeScript.
When the partial exports them, the generated class calls `impl.connected(this)` from `connectedCallback` and `impl.willUpdate`/`impl.updated` from its LitElement overrides.
The terminal's update cycle follows ReactiveElement's order: the mutating trigger collects the change batch (old values, first change wins) → `will_update` (its writes join the same batch, like Lit) → notify events → the commit (parts, widgets and child sync stand in for Lit's render) → `updated` on the committed state.
Writes inside `updated` request a follow-up cycle, exactly like setting a reactive property in Lit's `updated()`; the store's equal-write suppression makes it converge, with a debug guard against runaway loops.
Hook order is byte-comparable across targets: `will_update` sees the batch before anything paints, `updated` sees the world after, on both the LitElement and the terminal runtime.

### Composites synchronize in `will_update`

`input-date-range` is the composite pattern: one element around two `<input-date>` children (plus the group's timezone select), wrapped in the shared input chrome like any input, so it gets label, hint and error rows.
The children's `@value-changed` bindings route into plain property writes (`on_start_changed`/`on_end_changed`); `will_update` owns the rules: the edited end pulls the other along when the range would invert, the combined `value` derives from the ends, an external `value` write decomposes and normalizes; `updated` reflects `complete` post-commit.
The children carry `seamless` in the template: the chrome's input-group draws the one border and the children render borderless inside it.
A composite's synchronization writes cascade into its children through the existing bindings; echo loops die on the equal-write suppression.

### One directory holds all of a component's targets

A component's directory holds its web assets: the Rust definition and logic (`mod.rs`), the template (`.html`), the stylesheet (`.scss`) and the browser twin (`.impl.ts`).
The terminal twin (`tui.rs`) belongs beside them by default, but may instead live in a path-mirrored companion crate: `ui_components_tui`'s `nav_tabs/tui.rs` sits at the same directory path as `ui_components`'s `nav_tabs/`, which keeps `ui_components` a pure web/definition crate with no `uic_tui` dependency at all.
That dissolves an edge co-location would otherwise force: the catalog's `tui.rs` builds against `uic_tui`, and `uic_tui`'s own integration tests mount the catalog, so the two would depend on each other.
The guard against drift stays structural, not positional: the twin keeps its mirrored path, registers the same `data-tui` string kind, and is exercised by the same TestBackend suites, so co-location was the convention and never the mechanism that caught drift.
The terminal twin registers itself through `uic_tui::WidgetRegistration { kind, build }`, collected by `inventory` like the custom-element registry; `WidgetBox::new` consults the registry after the built-in kinds, so a new `data-tui` kind edits nothing in the runtime.
`uic_tui` exposes the implementation surface for such twins: `WidgetAdapter` and `OverlayOutcome` are public, and the crate re-exports `crossterm`, `ratatui`, `rat_widget` and `unicode_width` so adapters build against the runtime's own dependency versions.
`.options` bindings are valid on `data-tui` widgets, whose adapters store the rows through `set_options` (ADR 0005).

The catalog's terminal half is the companion crate.
TUI consumers (`uic_tui`'s own integration tests, `uic_tui_web`, tui-demo) depend on `ui_components_tui`, which pulls `ui_components` for the definitions and `uic_tui` for the widget surface, and whose `link` chains the catalog's own.
Web-only consumers depend on `ui_components` alone and compile no terminal stack: the web-demo build script, `apps/dist` and `uic_codegen_web`'s tests stay lean by construction, with no feature to leave unset.
Macro asset paths resolve beside the source file and then upward through its ancestors, stopping at the crate root, the nearest match winning, so `#[input_shared]`'s `_shared/chrome.html` reaches components at any directory depth.

## Why

Template structure, properties, and events are single-sourced in Rust; imperative logic cannot be auto-translated without a transpiler or a WASM runtime in the browser, both rejected (the generated output must stay readable, dependency-light Lit).
Composition needs a home with LitElement's semantics: derive and correct state in `willUpdate(changedProperties)`, observe the committed result in `updated(changedProperties)`, and both render targets must run the same flow.
Per-target behavior twins belong side by side by default: the browser popup in `suggestion.impl.ts` and the terminal popup in `tui.rs` are the same feature spelled twice.
Splitting them across crates hides drift only when the split also loses the structure that caught it; a path-mirrored companion crate that registers the same kinds and runs the same parity and TestBackend suites keeps drift visible while letting the catalog shed its dependency on the runtime.
Adding a component must not affect existing ones or central runtime files; registration through `inventory` gives widget kinds the property the element registry already has.
The companion crate exists for the consumers that never paint a terminal: the npm tree and the web codegen build scripts depend on `ui_components` alone, so they compile no ratatui, crossterm or rat crates.

## Consequences

Logic like date parsing exists in `date.rs` and `date.impl.ts`; the doc comments cross-reference each other and changes must touch both.
Behavioral parity is pinned by tests exercising the same inputs on both sides: Rust unit tests, TestBackend end-to-end tests, and the parity fixtures the Rust tests stage for CI to replay against the compiled browser partial (`scripts/parity-check.mjs`).
`HandlerKind` on `HandlerMeta` (today only `PerTarget`) is the seam where a future shared-WASM variant plugs in: codegen would emit a wasm-calling stub instead of the impl import, removing the duplication for components that opt in.
`first_updated` is not part of the Rust lifecycle surface (Lit's remaining hook); it joins when a component needs it.
The built-in widget kinds (date, text/number, textarea, select) live in `uic_tui`; flat components migrate to directories opportunistically, never in bulk.
`WidgetAdapter` is public API: its methods are the commitment the runtime makes to co-located twins.
Composition stays template-level (nested custom elements, as in `input-date-range`): `input-suggestion` wraps a raw `<input>` rather than an `<input-text>` child, precisely so an existing component's contract needn't change for a new one.
A widget registration rides the same inventory/linker mechanics as element registrations; `ui_components_tui::link()` anchors the widgets and chains `ui_components::link()` for the elements past the linker.
