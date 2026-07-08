# ADR 0003: Migrating the upstream catalog

## Goal

The upstream frontend's element catalog (`packages/frontend/web/src/elements/`, ~30 inputs built on `InputDefault`/`InputGroup`) migrates to Rust definitions in `ui_components`, one component at a time; `<input-date>` is the pilot.

## Conventions carried over

- Light DOM everywhere; Bootstrap classes; the `ExternalStyles` pattern (`el-<name>` host class targeted by per-component `.scss`, aggregated into `elements.scss` → `/elements.css`).
- `LitNotify` semantics: `notify` properties fire `<attribute || property>-changed` events with `{ property, value, oldValue }`.
- `static properties` + `customElements.define` (no decorators) in the generated classes.

## Composition (implemented with the input-text port)

1. `#[input_shared]` — an attribute macro above the derive that injects the contract properties (label, hint, error_message, disabled, name, required) and wires the shared chrome and stylesheet by appending a second `#[custom_element(...)]` attribute.
2. The chrome template partial `input/_shared/chrome.mhtml`: the label/input-group/message markup with a `<slot/>` where the component's own input goes; `uic_template::splice` merges the trees at compile time (validation) and runtime.
3. The shared stylesheet `input/_shared/input-default.scss` backs the stacked `el-input-default` host class, emitted once per style id.

Generated classes stay flat (no reproduction of the catalog's 4-level JS mixin hierarchy); the hierarchy is an authoring device, and Rust is the authoring layer here.

## State attributes (`error`, `suggested`, `seamless`)

The shared contract carries the catalog's reflected state booleans; the ported `.el-input-default` rules style them (`[error]` danger outline, `[suggested]` accent outline, `[seamless]` borderless flush).

Deviation from the catalog: our inputs couple the `error` flag to their own validation (`input-date` sets and clears it together with `error_message`), while the catalog drives `error` externally only.
External writes still win until the next commit.

## Deviations of the `input-number`/`input-textarea` ports

- `value` on `input-number` is honestly number-typed (`Option<f64>`, `number | null`); the catalog declares String but stores a number.
- `allow-null` defaults to false on both — a reflected boolean attribute cannot default to true (absence must mean the default in the attribute model); the catalog's number input defaults it to true.
- Parse failures surface on the error line and the `error` attribute; the catalog throws uncaught.
- The dead `separator` option and the private `__inputmode` are not ported; `inputmode` emits the standard `numeric` token instead of the catalog's invalid `number`.
- The browser impl keeps the catalog's extra native `change` CustomEvent; the terminal relies on `value-changed`.
- `input-textarea` duplicates the trim-on-change of `input-text` instead of inheriting it (the component model is flat), and its `seamless` handling comes from the shared contract instead of the catalog's `_option_seamless` shadow property.

## Deviations of the `input-select`/`input-timezone` ports

- The catalog hardcodes a curated ~430-entry zone array; our targets each ask their platform — Rust iterates chrono-tz (597 zones, including legacy aliases like `US/Pacific`), the browser `Intl.supportedValuesOf('timeZone')` (the ICU set) — both pinning UTC first, the rest in the platform's order.
  The two lists differ slightly by design: the implementations are specialized per target but kept side by side (`timezone.rs` next to `timezone.impl.ts`) so they stay comparable.
- `input-timezone` shares `select.mhtml` and the `el-input-select` styles via the `style` override instead of subclassing (the component model is flat); the shared select computeds live once in `select.rs`/`select.impl.ts` and are delegated to.
- The remaining select-family deviations (no `options` attribute, no object default, keyboard-only terminal popup) are recorded in ADR 0006.

## Deviations of the `show-timezone` embedding

- The `language` material icon next to the input is not ported (no icon element exists here); the placeholder suffix ` · <current timezone>` is.
- The embedded select carries `seamless` from the shared contract instead of the catalog's `input-class="form-select border-square"` passthrough; the terminal maps a seamless chrome's `input-group` class to plain flex so only the host border renders.
- The hidden branch also disables the child (`?disabled=${!show_timezone}`): terminal focus has no notion of unrendered branches yet, so the binding keeps Tab out of the invisible select.
- The terminal's embedded option popup clips to the select's own width; widening it to the longest label is an open follow-up.

## Known parity gaps of `<input-date>` v1

Date-only (`YYYY-MM-DD`): no time/seconds, no calendar icon, no `compact`, no `input-id` label association.
The mechanisms all exist (computed properties, lifecycle hooks, nested custom elements), so parity is incremental work; the `date`/`timezone` object properties landed with ADR 0005, the `show-timezone` sub-select with ADR 0006's select family.
