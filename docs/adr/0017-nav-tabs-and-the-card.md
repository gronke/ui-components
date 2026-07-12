# ADR 0017: nav-tabs is a value-driven bar; the card is the bordered block

## Decision

`<nav-tabs>` is the catalog's first non-input component: a `value: String` (notify, `value-changed`) plus property-only `options: Vec<SelectOption>` — the select's options-as-data contract (ADR 0006) applied to a tab bar.
Panes are the host's job: two sibling `<template if>` branches beside the bar, switched by the bound value (the demo's Form/About card).
The terminal twin `data-tui="tab-bar"` (co-located `tui.rs`, ADR 0015) wraps rat's `Tabbed` in its glued form — one caption row, no content block; the browser twin builds Bootstrap `nav-link` button rows imperatively in `updated()`.
The `card` class becomes the terminal's generic bordered container: layout reserves a one-cell border plus one cell of horizontal padding (the `input-group` treatment), paint draws a static dark-gray `Block::bordered()`; `card-body` maps to one row of top padding and `card-header` stays a plain block.

## Why

Only `data-tui` widgets receive focus and clicks in the terminal, so Bootstrap tab markup alone cannot switch anything — the bar needs a widget twin, and a value-driven one keeps it reusable instead of demo-local.
Picks travel the existing `@input` route on both targets: the adapter records the chosen value and `take_input` flushes it after the same event, a browser button dispatches a bubbling `input` event — one shared template binding, zero runtime changes.
`@change` stays the commit event of text-bearing widgets; a tab pick is not a commit, it is live selection, which is exactly what the input route carries.
The card mapping answers "write HTML, get the closest terminal representation" for structure: the border is static because focus and error dressing describe editable groups (`input-group`), not passive containers.

## Consequences

- The highlighted tab always derives from the bound value with a fallback to the first row (`Math.max(0, findIndex)` in the browser, `position(..).unwrap_or(0)` in the terminal), so external state writes move the selection and unknown values degrade predictably.
- Hosts should bind the bar to a value that defaults to the first tab's key only on interaction: a mount-time push of a non-default value notifies `value-changed` like any LitNotify property, and the demo's `on_tab` would write the member into every boot state — app-root therefore computes `tab` with an empty-string fallback.
- Left/Up and Right/Down switch tabs when the bar has focus (rat's own binding); Down is free because the bar opens no overlay.
- `<template if>` panes re-create their children on each switch and values re-sync from state, so uncommitted edits do not survive a tab switch in either target — matching a browser re-render.
- The card border consumes four columns and rows in total; the demo's terminal geometry grew accordingly (tests 72×60, the web pane 72×58).
