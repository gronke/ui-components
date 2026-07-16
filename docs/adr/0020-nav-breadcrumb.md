# ADR 0020: A static breadcrumb trail

## Context

Hierarchical locations — file trees, organizational units, nested categories — read best as a breadcrumb trail, and hosts keep rebuilding one ad hoc from spans and separators.
Bootstrap's breadcrumb draws its dividers with CSS `::before` content, which the terminal target cannot paint, so its markup renders as bare labels with no separation there.
The catalog needs one definition that paints the same trail on both targets.

## Decision

`<nav-breadcrumb>` renders a static trail from property-only `items` rows (`{label, href?}`) — the options-as-data contract (ADR 0006) applied to navigation — plus a `divider: String` (default `›`).
A computed `crumbs` decorates the rows for display (`{label, href, sep, plain}`): `sep` is empty on the first crumb and the divider afterwards, `plain` complements `href` because loop members cannot be negated (ADR 0018).
The divider therefore travels as data-decorated text nodes on a `d-flex flex-row flex-wrap gap-2` line, not as CSS content, so both targets paint identical separators; the flex utilities map in the terminal.
The component carries its own `uic-breadcrumb` classes instead of Bootstrap's `.breadcrumb`, whose CSS dividers would double the explicit ones in the browser.
The trail is static content: crumbs with an href render as anchors (degrading to plain text in the terminal), the rest as spans — no widget twin, no events, no focus.
The decoration is mirrored in `nav_breadcrumb.impl.ts` (ADR 0002) and held to the Rust behavior by the parity fixtures.

## Consequences

- The first separator is suppressed by data (`<template if=${c.sep}>` skips the empty string), so no target renders a leading divider or a stray flex gap.
- Hosts supply complete hrefs; the component never fabricates or joins URLs, and a crumb without an href is presentation-only.
- Non-object items and missing members degrade to empty plain crumbs instead of failing the render.
- Per-crumb `aria-current` stays out until something needs it; the `<nav aria-label="Breadcrumb">` landmark is the accessibility surface for now.
- No overflow, truncation or collapse behavior: a long trail wraps (`flex-wrap`), which is the acceptable default for both targets.
