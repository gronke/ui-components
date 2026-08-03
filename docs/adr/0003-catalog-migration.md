# ADR 0003: The catalog ports carry the upstream conventions, with recorded deviations

## Decision

`ui_components` defines the element catalog in Rust; the upstream frontend's catalog (`packages/frontend/web/src/elements/`, ~30 inputs built on `InputDefault`/`InputGroup`) is the reference every port is held against.
The upstream conventions carry over:

- Light DOM everywhere; Bootstrap classes; the `ExternalStyles` pattern (`el-<name>` host class targeted by per-component `.scss`, aggregated into `elements.scss` → `/elements.css`).
- `LitNotify` semantics: `notify` properties fire `<attribute || property>-changed` events with `{ property, value, oldValue }`.
- `static properties` + `customElements.define` (no decorators) in the generated classes.

Input components compose through the shared contract instead of the catalog's mixin chain:

1. `#[input_shared]`, an attribute macro above the derive, injects the contract properties (label, hint, error_message, disabled, name, required and the reflected state booleans error, suggested, seamless) and wires the shared chrome and stylesheet by appending a second `#[custom_element(...)]` attribute.
2. The chrome template partial `input/_shared/chrome.html` holds the label/input-group/message markup with a `<slot/>` where the component's own input goes; `uic_template::splice` merges the trees at compile time (validation) and runtime.
3. The shared stylesheet `input/_shared/input-default.scss` backs the stacked `el-input-default` host class, emitted once per style id.

Generated classes stay flat (no reproduction of the catalog's 4-level JS mixin hierarchy); the hierarchy is an authoring device, and Rust is the authoring layer here.
The `.el-input-default` rules style the state booleans (`[error]` danger outline, `[suggested]` accent outline, `[seamless]` borderless flush); the terminal stylesheet gives `[seamless]` the same effect by zeroing the chrome's border and inset (ADR 0021).

### Deviations: validation state

- Our inputs couple the `error` flag to their own validation (`input-date` and `input-number` set and clear it together with `error_message`), while the catalog drives `error` externally only; external writes still win until the next commit.
- Parse failures surface on the error line and the `error` attribute; the catalog throws uncaught (numbers) or only logs (dates).

### Deviations of the `input-number`/`input-textarea` ports

- `value` on `input-number` is honestly number-typed (`Option<f64>`, `number | null`); the catalog declares String but stores a number.
- `allow-null` defaults to false on both: a reflected boolean attribute cannot default to true (absence must mean the default in the attribute model); the catalog's number input defaults it to true.
- The dead `separator` option and the private `__inputmode` are not ported; `inputmode` emits the standard `decimal`/`numeric` tokens instead of the catalog's invalid `number`.
- The browser impl keeps the catalog's extra native `change` CustomEvent; the terminal relies on `value-changed`.
- `input-textarea` duplicates the trim-on-change of `input-text` instead of inheriting it (the component model is flat), and its `seamless` handling comes from the shared contract instead of the catalog's `_option_seamless` shadow property.

### Deviations of the `input-select`/`input-timezone` ports

- The catalog hardcodes a curated ~430-entry zone array; our targets each ask their platform: Rust iterates chrono-tz (597 zones, including legacy aliases like `US/Pacific`), the browser `Intl.supportedValuesOf('timeZone')` (the ICU set).
  Both pin UTC first, the rest in the platform's order.
  The two lists differ slightly by design: the implementations are specialized per target but kept side by side (`timezone.rs` next to `timezone.impl.ts`) so they stay comparable.
- `input-timezone` shares `select.html` and the `el-input-select` styles via the `style` override instead of subclassing (the component model is flat); the shared select computeds live once in `select.rs`/`select.impl.ts` and are delegated to.
- The remaining select-family deviations (no `options` attribute, no object default, the popup semantics) are recorded in ADR 0005.

### Deviations of the `show-timezone` embedding

- The `language` material icon next to the input is not ported (no icon element exists here); the placeholder suffix ` · <current timezone>` is.
- The embedded select carries `seamless` from the shared contract instead of the catalog's `input-class="form-select border-square"` passthrough, so only the wrapper's border renders.
- The hidden branch also disables the child (`?disabled=${!show_timezone}`): the binding keeps terminal focus out of the unrendered branch.
- The terminal's embedded option popup clips to the select's own width; widening it to the longest label is an open follow-up.

### Deviations of the `input-date` port

- The stored `date` IS normalized to UTC, and `timezone`/`default-timezone` only interpret input and render output; the catalog merely comments that `.date` "is always UTC" while actually storing the zone's instant.
- Remaining parity gaps against the catalog: no calendar icon, no `compact`, no `input-id` label association; the mechanisms (computed properties, lifecycle hooks, nested custom elements) all exist, so closing them is incremental work.

## Why

One definition renders on two targets, and consumers keep the catalog's markup and events, so the ports must observe the upstream semantics precisely where behavior is contractual.
Where a port deviates, the deviation is deliberate (type honesty, the attribute model, standard tokens, platform-owned data), and this record is where each one lives.
An undocumented mismatch with the catalog is a bug; a documented one here is a decision.

## Consequences

- Behavior shared inside a component family lives in delegated helper functions beside the components (`select.rs`/`select.impl.ts`), not in base classes.
- The state attributes make validation externally observable: hosts read and may write `error`, and the value/error pair stays consistent because the components manage both together.
- The zone lists drift with their platforms (chrono-tz releases, ICU updates), which is accepted; UTC stays pinned first on both.
- Every new input port starts by reading the upstream element and recording its deviations here.
