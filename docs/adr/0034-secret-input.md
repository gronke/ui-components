# ADR 0034: input-secret is a masked field — display or editable, revealed in both targets

## Decision

`input-secret` shows a secret (a token, a key) as a masked value with a reveal affordance in both render targets, and a clipboard copy in the browser only.
It is display-only by default (the host sets `value`, the user reads and copies it) and becomes editable with `editable`: revealing reads as before, and a commit sets a new `value` (so the property carries `notify`).
When the host never discloses the stored secret (`value` is `null`), an editable field is write-only — there is nothing to reveal but the new input.

Reveal is an independent, persistent toggle: the eye button in the browser, a `[x] reveal` checkbox on the right in the terminal (Space at rest, or a click).
The browser edits inline when `editable` (click and type; the eye toggles visibility; commit on change).
The terminal edits modally, so a focused field can be navigated past without being changed: Enter opens edit mode — which reveals the text so it reads while typing — Enter commits and Esc reverts to the value at edit-start; Tabbing away commits.

## Why

Masking a secret and revealing it on demand are meaningful in a terminal — a password prompt does exactly this — so the field earns a real widget twin in both targets rather than degrading to a generic block.
A system clipboard has no terminal equivalent, so copy stays browser-only: the copy button and its `navigator.clipboard` call live in `secret.impl.ts`, and the terminal twin omits it.
Editing is opt-in because the field was born to *show* a one-time token (a minted credential); keeping display-only the default leaves that use untouched, and a `notify` that never fires in display mode costs nothing.
The terminal's modal edit (Enter/Esc) matches how a keyboard-only user moves through a form: Tab selects, Enter commits to editing, Esc backs out — editing while masked over bullets would hide what you type, so edit mode reveals and the prior reveal choice is restored on exit.
Reveal and edit are separate because a user may want to read without editing, or edit a value they keep masked from over-the-shoulder view; the reveal state is display state, not part of the shared object, so it does not travel between the twins.
`navigator.clipboard` needs a secure context and a user gesture; the copy click supplies the gesture, and callers already serve over HTTPS or `localhost`.

## Consequences

- The reveal, copy, and commit wiring lives in `secret.impl.ts`, delegated on the host element in `connected` (so it survives Lit rendering the buttons after `connectedCallback`); the template carries the commit `@change` and the shared `#[input_shared]` chrome, and the icons are inline SVG (no icon-font dependency).
- The terminal twin is a `WidgetAdapter` (`ui_components_tui`, `data-tui="secret-input"`) built on rat's text-input state, masked at rest; a terminal consumer already links that crate.
- The runtime gained a per-frame `readonly` push (`WidgetAdapter::set_readonly`, a default no-op read from the element like `disabled`), so the widget knows when it is display-only and never enters edit mode. No other widget is affected.
- Because editing commits `value`, a host that only displays a secret must not also treat the value as user input — display mode (the default) simply never commits.
