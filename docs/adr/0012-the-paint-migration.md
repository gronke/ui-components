# 12. The paint migration

Date: 2026-07-10

## Status

Accepted (complete)

## Context

Since ADR 0011 the retained DOM ran beside the renderer: components mounted on the `uic_dom::Document` with full LitElement semantics, while layout and paint still consumed the per-frame expansion (`expand.rs` → `RNode`), with widget state in slots keyed by template order.
The migration's goal is one source of truth: the document IS what renders.

## Decision

The DOM render pipeline (`uic_tui::dom`) replaces the expansion pipeline, driven by `App`:

- **Widget state lives in the node payload.**
  `DomDocument = uic_dom::Document<WidgetPayload>`; every `data-tui` element carries its rat widget in `ElementData::data`, created idempotently whenever nodes appear (fresh instantiation or a conditional branch).
  `.value`/`.options` property writes from the parts engine sync the widget with the lit-style dirty check; node identity replaces slot-by-template-order bookkeeping.
- **Attributes are the runtime's stylesheet selectors.**
  What the old pipeline resolved from expressions per frame reads straight off the tree: placeholders and `disabled` are committed attributes on the widget node, and component state reaches paint through reflection in the glue — `reflect` properties land on the host element as attributes during the update cycle (ReactiveElement's reflection), so the error outline reads `[error]` off the component exactly like the browser's stylesheet, replacing the synthetic `is-invalid` class.
  A `seamless` component's group renders borderless through the same mechanism.
- **Layout and paint walk the document.**
  `dom::layout` builds the taffy tree from nodes (classes from the `class` attribute, whitespace-only text skipped, comment markers and conditional anchors invisible, mounted roots stacking with a one-row margin); `dom::render` carries the paint semantics — borders, hints, focus ring, placeholder and resting-alignment overpaints, the select's closed label, the caret, and the overlays (calendar, option list) painted after all content off the focused node's widget.
- **Focus is a node.**
  `App` walks `data-tui` elements in document order; disabled widgets are skipped, and unrendered conditional branches are unfocusable BY CONSTRUCTION — their nodes do not exist, retiring the old pipeline's `?disabled` guard workaround.
- **Commits are events on the tree.**
  A widget commit routes into the `@change` binding its template declares (descending into the owning child mount) and dispatches a bubbling DOM `change` event — both halves of the browser's change-on-commit.
- **The pointer travels the tree.**
  Hit-testing resolves clicks against the widget areas rat records at paint, keyed by node; clicks focus, place the caret and pick from overlays via published geometry (rat's own mouse path stays unused — its click arming reads the system clock, absent on wasm32), drags select, the wheel pages, and a click into nothing blurs with change-on-blur.

## Consequences

- `uic_tui::App` is the one application host, on the OS event loop natively and driven by the wasm `TuiSession` in the browser; mounts address roots by index, and `expand.rs`, the slot machinery, `ElementInstance` and the flat focus index are gone.
- The full test suite (render, select, nested, range, mouse, lifecycle, ANSI) runs against the DOM pipeline; the lifecycle order's mid-cycle observer is a DOM event listener, because notify events dispatch as bubbling events during the update cycle — the browser's timing.
- Widget state (`WidgetState`, the calendar popup) lives in `dom::widget` as the document payload.
