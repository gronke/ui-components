# ADR 0009: Composites synchronize in will_update

## Context

Components need to compose: one element surrounding child components, reading their notify events and keeping shared state consistent.
The natural home for that synchronization is LitElement's reactive update — derive and correct state in `willUpdate(changedProperties)`, observe the committed result in `updated(changedProperties)` — and both render targets must run the same flow.

Before this change the TUI's update cycle ran `updated` *before* the commit (the widget and child sync) and discarded its writes, and the generated LitElement classes only knew the `willUpdate` impl-export; `connected` and `updated` had no browser wiring at all.

## Decision

- **The TUI update cycle follows ReactiveElement's order**: the mutating trigger collects the change batch (old values, first change wins) → `will_update` (its writes join the same batch, like Lit) → notify events → the commit (widget and child sync stand in for Lit's render) → `updated` on the committed state.
  Writes inside `updated` request a follow-up cycle, exactly like setting a reactive property in Lit's `updated()`; the store's no-change suppression makes it converge, with a debug guard against runaway loops.
- **`updated` and `connected` join `willUpdate` as impl-export lifecycle hooks** in the generated TypeScript: when the component's `.impl.ts` exports them, the generated class calls `impl.updated(this, changed)` from its `updated()` override and `impl.connected(this)` from `connectedCallback`.
  The per-target rule stays: the Rust `Logic` impl and the `.impl.ts` partial express the same behavior, kept in sync by hand and by tests.
- **`input-date-range` establishes the composite pattern**: one element around two `<input-date>` children.
  The children's `@value-changed` bindings route into plain property writes (`on_start_changed`/`on_end_changed`); `will_update` owns the rules — the edited end pulls the other along when the range would invert, the combined `value` derives from the ends, an external `value` write decomposes and normalizes; `updated` reflects `complete` post-commit; `connected` opts the shared chrome into `seamless`, because the children draw their own borders.
  The shared input chrome takes free-form slot content, so a composite gets label, hint and error rows like any input.

## Consequences

- Hook order is now byte-comparable across targets: `will_update` sees the batch before anything paints, `updated` sees the world after, on both the LitElement and the terminal runtime.
- A composite's synchronization writes cascade into its children through the existing bindings; echo loops die on the no-change suppression, as before.
- `first_updated` remains unimplemented on the Rust side (Lit's remaining lifecycle hook); it joins when a component needs it.
- The demo pages carry the range beside the single inputs, and the event log filters to page-level elements so composites report once.
