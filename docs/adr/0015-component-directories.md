# ADR 0015: A component's directory holds all of its targets

## Decision

New components live in one directory holding every asset: the Rust definition and logic (`mod.rs`), the template (`.html`), the stylesheet (`.scss`), the browser twin (`.impl.ts`) and the terminal twin (`tui.rs`) — `input/suggestion/` is the first.

The terminal twin registers itself through `uic_tui::WidgetRegistration { kind, build }`, collected by `inventory` like the custom-element registry; `WidgetBox::new` consults the registry after the built-in kinds, so a new `data-tui` kind edits nothing in the runtime.
`uic_tui` exposes the implementation surface for such twins: `WidgetAdapter` and `OverlayOutcome` are public, and the crate re-exports `crossterm`, `ratatui`, `rat_widget` and `unicode_width` so adapters build against the runtime's own dependency versions.

The catalog's terminal half sits behind the `tui` cargo feature (`ui_components = { workspace = true, features = ["tui"] }`).
TUI consumers enable it — uic_tui's own integration tests, uic_tui_web, tui-demo; web-only consumers compile no terminal stack — the web-demo build script, apps/dist and uic_codegen_web's tests stay lean, which cargo's resolver guarantees because build-dependency features never unify with normal ones.

Macro asset paths resolve beside the source file and then upward through its ancestors, stopping at the crate root, the nearest match winning — so `#[input_shared]`'s `_shared/chrome.html` reaches components at any directory depth (and the trybuild fixtures keep their local copies).

`.options` bindings gain a third valid placement: `data-tui` widgets, whose adapters store the rows through `set_options` (amends ADR 0006).

## Why

Per-target behavior twins belong side by side (ADR 0002): the browser popup in `suggestion.impl.ts` and the terminal popup in `tui.rs` are the same feature spelled twice, and splitting them across crates hides drift.
Adding a component must not affect existing ones or central runtime files; registration through `inventory` gives widget kinds the property the element registry already has — the match arm was the last per-component edit in the runtime.
The feature gate exists for the consumers that never paint a terminal: the npm tree and the web codegen build scripts should not compile ratatui, crossterm and the rat crates.

## Consequences

- The four built-in widgets (date, text/number, textarea, select) stay in `uic_tui` until a change gives a reason to migrate; flat components migrate to directories opportunistically, never in bulk.
- `WidgetAdapter` is public API now — its methods are the commitment the runtime makes to co-located twins.
- Composition stays template-level (nested custom elements, as in `input-date-range`): `input-suggestion` wraps a raw `<input>` rather than an `<input-text>` child, precisely so an existing component's contract needn't change for a new one.
- A widget registration rides the same inventory/linker mechanics as element registrations (the wasm registry gate and `ui_components::link()` cover both).
