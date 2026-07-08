# ADR 0005: Object-valued properties are a closed set, starting with `Zoned`

## Decision

`uic_core::Zoned` (a newtype over `chrono::DateTime<chrono_tz::Tz>`) is the first and only object-valued property type: `JsType::Zoned`, `Value::Zoned`, browser type `Temporal.ZonedDateTime | null`.
Zoned properties are property-only — the derive rejects `reflect`, `attribute` and `default`, requires `Option<Zoned>`, and emits `attribute: None`; the generated Lit declaration is `{ attribute: false }` with no converter, so the default `!==` change detection applies (the catalog's reference semantics).
Notify events fall back to the JS name (`date` → `date-changed`), and `Zoned` equality is (instant, timezone id), so true no-op writes stay suppressed while a same-instant re-zoning still counts as a change.

## Why

The catalog's `date` property carries a `Temporal.ZonedDateTime` next to the `value` string; porting it needs an object value on both targets with the same change and notify semantics.
A closed enum variant (rather than a generic TS-type escape hatch) keeps the invariant that every property works identically on both targets; new object types get their own deliberate variant.

## Consequences

- The generated class only type-imports Temporal (`import type { Temporal } from 'temporal-polyfill'`), which oxc erases from the runtime JS; the real import lives in the hand-written `.impl.ts`, and `temporal-polyfill ^0.3` joins the dist peer dependencies whenever a registered component declares a Zoned property.
- The optional `willUpdate` lifecycle hook is wired by export discovery: when a component's impl partial exports `willUpdate`, the generated class overrides `willUpdate` and delegates (the Rust side already had `Logic::will_update`).
- `uic_core` now depends on chrono and chrono-tz (the bundled tz database costs about 1 MB per binary).
- Date arithmetic (parsing, start-of-day, formatting) stays in component behavior, not in `uic_core`.
