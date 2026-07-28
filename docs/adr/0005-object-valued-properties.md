# ADR 0005: Object-valued properties are a closed set

## Decision

Object-valued property types are a closed set, one deliberate `JsType`/`Value` variant per shape: `Zoned`, `Options`, `Object`, `Array`.
Every member is property-only — the derive rejects `reflect`, `attribute` and `default` and emits `attribute: None`; the generated Lit declaration is `{ attribute: false }` with no converter, so the default `!==` change detection applies (the catalog's reference semantics).
Notify events fall back to the JS name (`date` → `date-changed`).

- `Zoned` (`uic_core::Zoned`, a newtype over `chrono::DateTime<chrono_tz::Tz>`; browser type `Temporal.ZonedDateTime | null`): the Rust field must be `Option<Zoned>`, and equality is (instant, timezone id), so true no-op writes stay suppressed while a same-instant re-zoning still counts as a change.
- `Options` (`Vec<SelectOption>`; browser `SelectOption[]`, exported from the generated runtime module): option rows `{ value, short?, label? }`; the list always starts empty.
- `Object` (`uic_core::ObjectMap`, a string-keyed map with deterministic key order; browser `Record<string, unknown>`): the carrier of state-shaped properties (ADR 0013); starts empty.
- `Array` (`Vec<Value>`; browser `Record<string, unknown>[]`): the carrier of iterated rows behind `<template for>` (ADR 0001); starts empty.

### Select options are data, not template structure

Option lists never appear as `<option>` children in authored templates: a `<select .options=${…}>` takes no children — the web generator expands the binding into the `<option>` map inside the emitted html literal, and the terminal runtime feeds the resolved list to its dropdown widget (`data-tui="select"`, rat-widget `Choice`).
The derive validates placement: `.options` bindings belong on `<select>` elements, on custom elements (which receive the list as a property), or on `data-tui` widgets, whose co-located adapters store the rows through `set_options` (ADR 0002).
Label precedence follows the catalog's falsy `||` chains: an `input-front`-classed select renders `short || label || value` per option (the compact closed layer), every other select renders `label || value`; the terminal shows `short || label || value` in the closed line and full labels in the popup.
`?selected` compares each option's value against the select's own `.value` binding expression, so the first render is correct before Lit assigns the value property; components bind a computed (`form_value`) that renders null as the empty string.
The `default`-controlled empty option is component logic (the `select_options` computed on both targets), not generator magic.

## Why

The catalog's `date` property carries a `Temporal.ZonedDateTime` next to the `value` string, and its selects model options as an `options` property of `{value, short?, label?}` objects — the single `options.map(...)` lives inside the select base class render, never in consumer templates.
Porting both needs object values that behave identically on the two targets, with the same change and notify semantics.
A closed set of deliberate variants (rather than a generic TS-type escape hatch) keeps that invariant checkable; a new object shape gets its own variant with its own rules.
The terminal widget equally consumes options as data: its rows are widget items, not layout blocks, so the option list rides a property write on both targets.

## Consequences

- The generated class only type-imports Temporal (`import type { Temporal } from 'temporal-polyfill'`), which the TypeScript compilation erases from the runtime JS; the real import lives in the hand-written `.impl.ts`, and `temporal-polyfill ^0.3` joins the dist peer dependencies whenever a registered component declares a Zoned property.
- `uic_core` depends on chrono and chrono-tz (the bundled tz database costs about 1 MB per binary); date arithmetic (parsing, start-of-day, formatting) stays in component behavior, not in `uic_core`.
- Lifecycle hooks wire by export discovery: when a component's impl partial exports `willUpdate` (likewise `updated` and `connected`), the generated class overrides the hook and delegates; the Rust side is `Logic::will_update`.
- `<input-select>` ports the catalog's two-layer overlay: a visible, inert front select showing compact labels over the transparent interactive back select listing full labels; the front layer's static `tabindex="-1"` opts it out of mounting as a terminal widget.
- The terminal option popup: F4/Down/Space open it, arrows, paging and first-character type-ahead browse silently, Enter commits through the regular change path, Esc reverts to the bound value, Tab commits and advances focus; a click picks and commits, an outside press dismisses.
- Deviations from the catalog: no JSON parsing of an `options` attribute (property assignment only), no object-valued `default` (author the row into `options` instead), no mirroring of host `text-*` classes onto the front layer, and the terminal's closed default row renders plain (no muted italic styling).
