# ADR 0027: Element types select their terminal widgets

## Decision

A plain form element mounts its terminal widget because of what it is: `<input>` (and its textual types),
`<input type="number">`, `<input type="date">`, `<input type="datetime-local">`, `<textarea>` and `<select>`
resolve to their rat adapters by tag and type, through one shared table (`uic_template::native`) that the
runtime mount, the template lint and the macro checks all consult — the three cannot drift.
Detected date variants are type-implied: `date` is date-only, `datetime-local` carries minutes.
Control-type inputs (checkbox, radio, submit, …) stay plain elements, and a negative `tabindex` opts a
presentation twin out of detection — the platform's own "out of the focus order" signal.
`data-tui` remains beside the table as the explicit override and extension point: it wins over detection,
mounts on any tag (the tab-bar is a `<ul>`), and stays the discriminator inside the framework's own input
templates.
Every consumer beyond the mount keys on widget presence in the node payload, not on markup.

## Why

The model is a TUI representation of browser UI: a user writes `<input type="date" />` and the terminal
provides the widget that resembles that element's behavior, exactly as the browser provides its native
control — framework plumbing must not leak into user markup.
The framework's own input components cannot ride the same detection: date, number, suggestion and text all
deliberately render `<input type="text">` (they replace native controls with their own parsing, masking and
overlays), so `(input, text)` names four different kinds — for them `data-tui` is not an opt-in but the
discriminator, and it doubles as the registration hook `inventory` kinds already use.

## Consequences

- The lint's widget contract now covers detected elements: `@click` on a plain `<input>` is an error (it
  truly never dispatches — widget clicks become focus and caret placement), and a bound `type=${…}` warns
  as not statically checkable while its `@change`/`@input` stay legal.
- A chrome template must not contain plain form elements any more than `data-tui` markers; a `for` body
  rejects both alike.
- `.options` on a plain `<select>` was always legal markup and now actually reaches a widget under the
  scripted host.
- A `data-tui` element whose kind fails to resolve now renders as a generic container instead of a blank
  widget leaf (only reachable from scripted hosts; the lint blocks it for Rust templates).
- A kind change on one `data-path` — a bound `type` landing after the bind-time mount — recreates the
  widget and resets typed state, like a variant flip: kind flips are configuration.
