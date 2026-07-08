# ADR 0003: Migrating the Schuhkarton catalog

## Goal

The Schuhkarton frontend's element catalog (`packages/frontend/web/src/elements/`, ~30 inputs built on `InputDefault`/`InputGroup`) migrates to Rust definitions in `ui_components`, one component at a time; `<input-date>` is the pilot.

## Conventions carried over

- Light DOM everywhere; Bootstrap classes; the `ExternalStyles` pattern (`el-<name>` host class targeted by per-component `.scss`, aggregated into `elements.scss` → `/elements.css`).
- `LitNotify` semantics: `notify` properties fire `<attribute || property>-changed` events with `{ property, value, oldValue }`.
- `static properties` + `customElements.define` (no decorators) in the generated classes.

## Composition (implemented with the input-text port)

1. `#[input_shared]` — an attribute macro above the derive that injects the contract properties (label, hint, error_message, disabled, name, required) and wires the shared chrome and stylesheet by appending a second `#[custom_element(...)]` attribute.
2. The chrome template partial `input/_shared/chrome.mhtml`: the label/input-group/message markup with a `<slot/>` where the component's own input goes; `uic_template::splice` merges the trees at compile time (validation) and runtime.
3. The shared stylesheet `input/_shared/input-default.scss` backs the stacked `el-input-default` host class, emitted once per style id.

Generated classes stay flat (no reproduction of the catalog's 4-level JS mixin hierarchy); the hierarchy is an authoring device, and Rust is the authoring layer here.

## Known parity gaps of `<input-date>` v1

Date-only (`YYYY-MM-DD`): no time/seconds, no timezone sub-select, no `date: ZonedDateTime` object property, no calendar icon, no `compact`/`seamless`/`suggested`, no `input-id` label association.
The mechanisms all exist (computed properties, lifecycle hooks, nested custom elements are registry-checked), so parity is incremental work, starting with temporal-polyfill vendoring and a `Value::Object` variant.
