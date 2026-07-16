# ADR 0019: Tables lay out as shared column tracks

## Context

Row-and-column data rendered as one flex container per row (`d-flex` with
`flex-grow-1` cells) does not align across rows: `flex-basis` stays `auto`, so
every row divides its width by its own content, and separate flex containers
cannot share column tracks by design.
Real tables need widest-cell-wins columns that hold across all rows.

The browser has this for free: `<table>` markup is valid throughout the stack
(parser, derive validation, codegen, retained DOM), the generated Lit iterates
rows with the proven `map` shape (ADR 0018), and Bootstrap's `table` class
styles it.
Only the terminal lacked a layout: unknown elements are blocks, so cells
stacked vertically.

## Decision

The terminal lays a `<table>` element out as a CSS **grid** — the one taffy
mechanism whose `auto` tracks size to the largest contribution across all
rows.

- `<thead>`, `<tbody>`, `<tfoot>` and `<tr>` are structural, not layout boxes
  (taffy has no table display types and no `display: contents`).
  Rows are collected in document order: direct `<tr>` children and `<tr>`
  under the section elements.
- The cells (`<th>`/`<td>`) become the grid items, placed **explicitly** by
  row and column line, so a short row never shifts the placement of later
  rows.
- The column count is the maximum cell count across rows.
- Tracks: `minmax(auto, 1fr)` when the table carries Bootstrap's `table`
  class or `w-100` — the table fills its row and shares the surplus, like the
  browser's `width: 100%` table; plain `auto` tracks otherwise, hugging the
  content.
  Bootstrap's shrink-to-fit idiom `table w-auto` opts a styled table back
  into hugging.
  Columns are separated by a one-cell gap.
- `<th>` paints bold through the text-style hints, which now read the element
  tag beside the classes; the `fw-bold` class maps to bold for both targets.

## Consequences

- One template renders aligned tables on both targets; the per-row flex
  pattern remains available for non-tabular layout.
- `<template for>` anchors sit invisibly beside the rows they instantiate
  (the instances are ordinary `<tr>` siblings), so iterated tables work
  unchanged.
- A table nested inside a cell lays out as its own independent grid through
  the ordinary recursion.
- Out of scope: `colspan`/`rowspan` (cells always span one track),
  `<caption>`, and per-column sizing attributes.
  A cell's longest word still floors its column's width (the text measure's
  min-content), exactly as in the browser.
- The row and section elements own no rectangle in the laid tree; classes
  meant for text styling belong on the table or the cells.
