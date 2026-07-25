# uic_js

A Boa-embedded JS engine hosting real LitElement components on the terminal runtime — any npm lit element, byte-unmodified.
Boa is the host for real terminals, where no JS engine exists; in the browser the same runtime modules run on the native engine in a worker against `uic_tui_web::DomSession` (ADR 0023), and the host operations behind both are one shared implementation (`uic_tui::dom::HostState`).

Loading is generic: `JsHost::load_package(vendor_root, "@scope/name")` derives the package's ESM entry from its own manifest (`exports` ".", then `module`, then `main`), registers the whole dist tree under path-preserving specifiers, and evaluates the entry; `mount(tag, attrs)` takes any tag.
Packages arrive through the same registry-read-only vendoring the rest of the repo uses (ADR 0004): `build.rs` vendors whatever `package.json` declares for the tests and examples, and the `third_party` example vendors any `name@range` at runtime.

```sh
cargo test -p uic_js
cargo run -p uic_js --example third_party        # the vendored test component, offline
cargo run -p uic_js --example third_party -- 'some-pkg@^1' some-tag --prop 'data={"a":1}'
cargo run -p uic_js --example json_viewer        # the pinned reference, interactive
cargo run -p uic_js --example json_viewer_web    # browser split view on :8091
cargo test -p uic_js --release --test measure -- --ignored --nocapture
```

## The mocked lit, shaped like upstream's channels

Components import a mocked `lit` — TypeScript modules under `js/src/`, compiled per module by the build script and served by the in-memory loader.
The tree mirrors who produces each feature upstream, and `lit` is pure re-exports:

| Channel | Provides |
| --- | --- |
| `lit-html` | `html`, `svg`, `nothing`, and every directive under `lit-html/directives/*` |
| `@lit/reactive-element` | the `css` tag; the decorators under `@lit/reactive-element/decorators` |
| `lit-element` | the `LitElement` base |
| `lit`, `lit/decorators.js`, `lit/directives/*` | re-export shims over the producers |
| `@lit-labs/*` | reserved for mocked labs features |

Both import spellings resolve (`lit/directives/when.js` and `lit-html/directives/when.js`); every module also registers under its extension-less stem.
A missing module reports itself: the error names the specifier beside everything the runtime provides — extending the surface is adding a file here.

Behind the channels, `runtime.ts` is a pure re-export barrel over one module per polyfilled platform concept, each beside its own test suite:

| `runtime/` module | Polyfills | Tests |
| --- | --- | --- |
| `state` | the singleton leaf: registry, instances, listener table | (data only) |
| `serialize` | template tags and the render-to-string commit | `tests/serialize.rs` |
| `properties` | property options, converters, accessors | `tests/converters.rs` |
| `element` | the node facade (`value`, `closest`, `dataset`, …) | `tests/facade.rs` |
| `events` | bubbling dispatch, the stop family, preventDefault | `tests/events.rs` |
| `focus` | focusout/focusin in WHATWG order | `tests/focus.rs` |
| `custom-elements` | define/upgrade over the retained tree | `tests/composition.rs` |

## Directives

Supported with full semantics: `classMap`, `map`, `when`, `repeat` (unkeyed — the subtree-swap commit rebuilds either way; focus survives by `data-path`), `ifDefined` (the attribute renders empty rather than absent under the serialize commit), `choose`, `join`, `range`, `keyed` (degrades to its value), `styleMap`.
Identities where the renderer's model makes memoization moot: `guard` (recomputes), `cache`, `live`.
Not provided (async model, raw HTML injection): `until`, `asyncAppend`, `asyncReplace`, `ref`, `unsafeHTML`, `unsafeSVG` — importing one reports the gap.

## Runtime mechanics

`LitElement` installs per-property accessors that schedule microtask updates, `html` captures template strings and values, and `performUpdate` commits the rendered subtree through the `__uic_*` natives into the retained `uic_tui::dom::DomDocument` — the existing taffy layout and ratatui paint draw it unchanged (`uic_tui::dom::paint_document`).
A committed subtree upgrades the nested custom elements it names: components compose, and a parent's re-commit swaps its children in fresh, re-synced from their attributes (the serialize commit drops `.prop=` bindings, so composition data flows as attributes).

Plain `input`/`textarea`/`select` elements are first-class: the shared commit mounts each one's terminal widget by element type (ADR 0027; `data-tui` overrides), `.value=` on the browser's value-carrying elements serializes as the `value` attribute and syncs the widget echo-skipped — a component echoing back what the user just typed never moves the caret — and the focused widget survives the subtree swap keyed by the same `data-path` that keys focus survival (plus a one-slot stash for a nested input whose parent commit re-renders it a beat later).
An uncancelled keydown then runs the focused widget as the browser's editing default action; a text change synthesizes a bubbling `input` event whose `target.value` reads the live text (the node facade's `value` accessor), so `preventDefault()` on keydown suppresses the editing exactly like in a browser.
Text inputs are the first-class kinds; `.options` (select, date) is not serialized yet.

Events travel the other way: the host synthesizes bubbling `keydown`/`click`/`dblclick`/`focusin`/`focusout` DOM events (`__uicDeliver`), template `@event` bindings resolve through render-scoped listener markers with lit's host-`this` contract, and the DOM focus bridges into the paint; focus survives each subtree swap by re-resolving its `data-path`.
Synthesized events carry the modifier flags the host hands in — `JsHost::dispatch` takes the shared `uic_tui::KeyStroke` (the DOM key name plus all four flags; `dispatch_key`/`dispatch_key_shift` stay as shorthands) — and their `target` exposes `matches(selector)` and `closest(selector)`: the hit test lands on text nodes, so click discrimination walks up with closest.
Template `@event` values want to be method references (`@click=${this.onPick}`) — the marker binding supplies the host `this`, compiled lit's own shape — with row context travelling as a `data-*` attribute read off `event.currentTarget`; an inline closure over render locals trips the second Boa 0.21 capture bug pinned in `tests/boa_quirks.rs`.

A component's `static styles` reach the terminal too: `customElements.define` hands the collected css`` text to `uic_tui::dom::adopt_component_sheet`, and the cascade scopes it per instance — no Bootstrap assumed, the element's own stylesheet drives colors, indentation and generated content (json-viewer's `.collapsable::before` marker renders as a generated box, ▶ turning ▼ through `transform: rotate(90deg)`).

## The pinned test component

`@alenaksu/json-viewer` is the crate's pinned integration fixture: the `json_viewer*` tests and examples prove the runtime against a real published element — its decorators, directives, roving-tabindex keyboard navigation and click-to-toggle drive the terminal, and the browser split view renders the same bytes against the real lit family for comparison.

The render path is a deliberate simplification: a subtree swap (serialize, `parse_fragment`, `import_node`), not per-part diffing — instant at form scale, measurably slow on very wide documents; per-part commits are the recorded follow-up.

`tests/boa_quirks.rs` is the canary for a Boa 0.21 engine bug the runtime works around (a closure created inside a class constructor capturing a local lexical binding panics the VM); when it starts failing, Boa fixed the bug — drop the module-level accessor installation with it.
