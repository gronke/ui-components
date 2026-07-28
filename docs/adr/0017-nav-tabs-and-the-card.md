# ADR 0017: Structural HTML maps to the terminal: the tab bar, the card, tables and the breadcrumb

## Decision

### nav-tabs is a value-driven bar; the card is the bordered block

`<nav-tabs>` is a `value: String` (notify, `value-changed`) plus property-only `options: Vec<SelectOption>` — the select's options-as-data contract (ADR 0005) applied to a tab bar.
Panes are the host's job: sibling `<template if>` branches beside the bar, switched by the bound value (the demo's Form/About card).
The terminal twin `data-tui="tab-bar"` (a co-located `tui.rs`, ADR 0002) wraps rat's `Tabbed` in its glued form — one caption row, no content block; the browser twin builds Bootstrap `nav-link` button rows imperatively in its `updated` hook.
Picks travel the `@input` route on both targets (ADR 0014): the adapter records the chosen value and `take_input` flushes it after the same event, a browser button dispatches a bubbling `input` event — one shared template binding.
The `card` class is the terminal's generic bordered container: the terminal stylesheet reserves the cells (`border-width: 1px; padding: 0 1ch`, the `input-group` treatment, ADR 0021) and paint draws a static dark-gray `Block::bordered()`; `card-body` maps to one row of top padding and `card-header` stays a plain block.

### Tables lay out as shared column tracks

The terminal lays a `<table>` element out as a CSS grid — the one taffy mechanism whose `auto` tracks size to the largest contribution across all rows.
`<thead>`, `<tbody>`, `<tfoot>` and `<tr>` are structural, not layout boxes (taffy has no table display types and no `display: contents`); rows are collected in document order — direct `<tr>` children and `<tr>` under the section elements.
The cells (`<th>`/`<td>`) become the grid items, placed explicitly by row and column line, so a short row never shifts the placement of later rows; the column count is the maximum cell count across rows.
Tracks are `minmax(auto, 1fr)` when the table carries Bootstrap's `table` class or `w-100` — the table fills its row and shares the surplus, like the browser's `width: 100%` table; plain `auto` tracks hug the content otherwise, and the shrink-to-fit idiom `table w-auto` opts a styled table back into hugging.
Columns are separated by a one-cell gap.
`<th>` reads bold from the terminal's user-agent sheet, and the `fw-bold` class maps to bold on both targets (ADR 0021).

### The breadcrumb trail

`<nav-breadcrumb>` renders a static trail from property-only `items` rows (`{label, href?}`, riding the `Array` carrier of ADR 0005) plus a `divider: String` (default `›`).
A computed `crumbs` decorates the rows for display (`{label, href, sep, plain}`): `sep` is empty on the first crumb and the divider afterwards, `plain` complements `href` because loop members cannot be negated (ADR 0001).
The divider therefore travels as data-decorated text nodes on a `d-flex flex-row flex-wrap gap-2` line, not as CSS `::before` content — which the terminal cannot paint — so both targets render identical separators.
The component carries its own `uic-breadcrumb` classes instead of Bootstrap's `.breadcrumb`, whose CSS dividers would double the explicit ones in the browser.
The trail is static content: crumbs with an href render as anchors (degrading to plain text in the terminal), the rest as spans — no widget twin, no events, no focus.
The decoration is mirrored in `nav_breadcrumb.impl.ts` (ADR 0002) and held to the Rust behavior by the parity fixtures.

## Why

"Write HTML, get the closest terminal representation" needs an answer for structure, not just for inputs.
Only mounted widgets receive focus and clicks in the terminal, and a plain `<ul>` mounts none, so Bootstrap tab markup alone cannot switch anything — the bar needs a widget twin, and a value-driven one keeps it reusable instead of demo-local.
`@change` stays the commit event of text-bearing widgets; a tab pick is not a commit, it is live selection, which is exactly what the input route carries.
The card border is static because focus and error dressing describe editable groups (`input-group`), not passive containers.
Row-and-column data rendered as one flex container per row cannot align across rows: `flex-basis` stays `auto`, every row divides its width by its own content, and separate flex containers cannot share column tracks by design — real tables need widest-cell-wins columns that hold across all rows.
The browser has that for free (`<table>` markup is valid throughout the stack, and Bootstrap's `table` class styles it); the grid mapping gives the terminal the same alignment.
Hierarchical locations read best as a breadcrumb trail, and Bootstrap's own breadcrumb draws its dividers with CSS `::before` content, which leaves bare unseparated labels in the terminal — one definition with dividers as data paints the same trail on both targets.

## Consequences

- The highlighted tab always derives from the bound value with a fallback to the first row (`Math.max(0, findIndex)` in the browser, `position(..).unwrap_or(0)` in the terminal), so external state writes move the selection and unknown values degrade predictably.
- Hosts should bind the bar to a value that defaults to the first tab's key only on interaction: a mount-time push of a non-default value notifies `value-changed` like any LitNotify property, so the demo's app-root computes `tab` with an empty-string fallback.
- Left/Up and Right/Down switch tabs when the bar has focus (rat's own binding); Down is free because the bar opens no overlay.
- `<template if>` panes re-create their children on each switch and values re-sync from state, so uncommitted edits do not survive a tab switch in either target — matching a browser re-render.
- One template renders aligned tables on both targets; the per-row flex pattern remains available for non-tabular layout.
- `<template for>` anchors sit invisibly beside the rows they instantiate (the instances are ordinary `<tr>` siblings), so iterated tables work unchanged, and a table nested inside a cell lays out as its own independent grid through the ordinary recursion.
- The row and section elements own no rectangle in the laid tree; classes meant for text styling belong on the table or the cells.
- Out of scope for tables: `colspan`/`rowspan` (cells always span one track), `<caption>`, and per-column sizing attributes; a cell's longest word still floors its column's width (the text measure's min-content), exactly as in the browser.
- The first breadcrumb separator is suppressed by data (`<template if=${c.sep}>` skips the empty string), so no target renders a leading divider or a stray flex gap.
- Hosts supply complete hrefs; the breadcrumb never fabricates or joins URLs, a crumb without an href is presentation-only, and non-object items degrade to empty plain crumbs instead of failing the render.
- Per-crumb `aria-current` stays out until something needs it; the `<nav aria-label="Breadcrumb">` landmark is the accessibility surface, and a long trail wraps (`flex-wrap`) with no overflow, truncation or collapse behavior.
