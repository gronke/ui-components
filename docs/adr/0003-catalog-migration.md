# ADR 0003: Migrating the Schuhkarton catalog

## Goal

The Schuhkarton frontend's element catalog (`packages/frontend/web/src/elements/`, ~30 inputs built on `InputDefault`/`InputGroup`) migrates to Rust definitions in `ui_components`, one component at a time; `<input-date>` is the pilot.

## Conventions carried over

- Light DOM everywhere; Bootstrap classes; the `ExternalStyles` pattern (`el-<name>` host class targeted by per-component `.scss`, aggregated into `elements.scss` → `/elements.css`).
- `LitNotify` semantics: `notify` properties fire `<attribute || property>-changed` events with `{ property, value, oldValue }`.
- `static properties` + `customElements.define` (no decorators) in the generated classes.

## Composition plan (deferred until a second component exists)

`<input-date>` v1 inlines the shared `InputDefault` contract (label, hint, error_message, disabled) in its own struct and template.
For the catalog at scale, two mechanisms are planned:

1. `#[input_shared]` — an attribute macro running before the derive that injects the shared contract fields into the struct.
2. A chrome template partial: the shared label/input-group/message markup as a template whose slot is spliced with the component's own `renderInput`-equivalent at IR level.

Generated classes stay flat (no reproduction of the catalog's 4-level JS mixin hierarchy); the hierarchy is an authoring device, and Rust is the authoring layer here.

## Known parity gaps of `<input-date>` v1

Date-only (`YYYY-MM-DD`): no time/seconds, no timezone sub-select, no `date: ZonedDateTime` object property, no calendar icon, no `compact`/`seamless`/`suggested`, no `input-id` label association.
The mechanisms all exist (computed properties, lifecycle hooks, nested custom elements are registry-checked), so parity is incremental work, starting with temporal-polyfill vendoring and a `Value::Object` variant.
