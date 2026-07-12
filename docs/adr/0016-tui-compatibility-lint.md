# ADR 0016: A linked lint validates TUI compatibility

## Decision

`uic_tui::lint` walks every registered component's parsed template and reports `Finding`s with two severities.
Errors are bindings the terminal can never serve: a static `data-tui` kind that neither a built-in widget nor a `WidgetRegistration` resolves (checked through the runtime's own `WidgetBox::new`, so the lint and the mount cannot drift), an `@event` other than `change`/`input` on a `data-tui` widget, and an `@event` on a custom element that is not one of the child definition's notify events.
Warnings mark web-only markup that is legal but inert in the terminal: an `@event` on a plain element (only widgets receive events) and a bound `data-tui=${…}` kind (not statically checkable).
`assert_tui_compatible()` prints warnings and panics listing all errors; component crates gate themselves with one integration test — `your_crate::link(); uic_tui::lint::assert_tui_compatible();`.

## Why

The macros validate grammar and placement at compile time, but the two facts these checks need only exist once a binary links: `inventory` collects widget registrations from any crate, and a referenced child's notify events live in another crate's definition — both unknowable at macro expansion.
Before the lint, both failure modes were silent: `mount_widgets` swallows an unknown kind (the node renders as dead space) and an undispatched event binding simply never fires.
A linked test binary is the earliest point the full registry exists, and as a plain `cargo test` the gate rides the existing workspace-test CI step with no new plumbing.
Web-only handlers warn instead of fail because the browser legitimately has richer interaction; a hard error would outlaw valid per-target markup (ADR 0002).

## Consequences

- The catalog gates itself in `crates/uic_tui/tests/lint.rs`; the same two lines serve any external component crate.
- Registry-level defects (nothing linked, duplicate tags, unresolved custom tags) short-circuit as one error via `CustomElementRegistry::assert_valid` — the per-template walk assumes a sane registry.
- Registry-backed kind resolution cannot be unit-tested inside `uic_tui` itself: a `cfg(test)` lib build and the catalog's dependency are two copies of the crate with separate inventories, so that coverage lives in the integration gate.
- Candidate future checks, deliberately out of v1: property bindings the terminal ignores on plain elements, and coverage reporting for classes without a terminal mapping (the styling contract stays silent degradation by design — not all Bootstrap applies to cells).
