# ADR 0006: Select options are data, not template structure

## Decision

Option lists are a closed object-valued property type: `uic_core::SelectOption { value, short, label }`, `Value::Options(Vec<SelectOption>)`, `JsType::Options`, browser type `SelectOption[]` (exported from the generated runtime module).
Options properties are property-only in the Zoned mold (ADR 0005) — the derive rejects `reflect`, `attribute` and `default`, requires a plain `Vec<SelectOption>`, and the list always starts empty (`DefaultValue::EmptyOptions`); the Lit declaration is `{ attribute: false }` with reference change detection.
Templates bind the list with the existing property syntax: a `<select .options=${name}>` takes no children — the web generator expands the binding into the `<option>` map inside the emitted html literal, and the terminal runtime feeds the resolved list to its dropdown widget (`data-tui="select"`, rat-widget `Choice`).
The derive validates placement: `.options` bindings belong on `<select>` elements or custom elements, nowhere else.

## Why

The deferral note on the first select port assumed a template-iteration construct; recon showed the catalog itself models options as data — an `options` property of `{value, short?, label?}` objects, with the single `options.map(...)` living inside the select base class render, never in consumer templates.
The terminal widget equally consumes options as data (its rows are widget items, not layout blocks), and the slot model requires widget counts to be static in template position, which a general loop would break.
So the closed grammar (ADR 0001) stays untouched: iteration exists only as one fixed, well-defined emission for `<select>`, and a general `<template for>` remains deferred until something structurally needs repetition.

## Emission contract

- Label precedence follows the catalog's falsy `||` chains: an `input-front`-classed select renders `short || label || value` per option (the compact closed layer), every other select renders `label || value`.
- `?selected` compares each option's value against the select's own `.value` binding expression, so the first render is correct before Lit assigns the value property; components bind a computed (`form_value`) that renders null as the empty string.
- The `default`-controlled empty option is component logic (`select_options` computed on both targets), not generator magic.

## Consequences

- `<input-select>` ports the catalog's two-layer overlay: a visible, inert front select showing compact labels over the transparent interactive back select listing full labels; the terminal shows the compact label in the closed line and full labels in the popup (which also drives its first-character type-ahead).
- The terminal popup is keyboard-only like the calendar: F4/Down/Space open, arrows and paging browse silently, Enter commits through the regular change path, Esc reverts to the bound value, Tab commits and advances focus.
- Deviations from the catalog: no JSON parsing of an `options` attribute (property assignment only), no object-valued `default` (author the row into `options` instead), no mirroring of host `text-*` classes onto the front layer, and the terminal's closed default row renders plain (no muted italic styling).
- If Lit's commit order (element properties before children) ever produces a stale select value when options and value change in one update, the fix is an optional `updated` impl export symmetric to `willUpdate` — not built preemptively.
- `input-select-int` (the catalog's number-valued select) is a follow-up, not part of this port.
