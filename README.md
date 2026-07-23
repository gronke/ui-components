# ui-components

This project assembles custom web components from Rust code.

One Rust definition per component — reactive properties, a lit-flavored template (inline or `.html`), co-located `.scss`, named behavior hooks — renders to two targets:

- Browser: generated TypeScript web components (LitElement variant: plain class, `static properties`, light DOM, no decorators), vendored, compiled and served by [web_modules](https://github.com/gronke/web_modules).
- Terminal: a runtime interpreting the same template IR with ratatui, laid out by taffy (real CSS flexbox/block over terminal cells) and rat-widget input primitives.

Component registration mirrors `customElements.define` through the `inventory` crate; properties follow the catalog's `LitNotify` vocabulary (`notify` → `<name>-changed` events).
The crate map and runtime overview live in [docs/architecture.md](docs/architecture.md), the decisions in [docs/adr](docs/adr); the plan and milestones in [issue #1](https://github.com/schuhkarton/ui-components/issues/1).

## Defining a component

```rust
#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-date",
    template_file = "date.html",
    scss_file = "date.scss",
    web_impl_file = "date.impl.ts"
)]
pub struct InputDate {
    /// Committed value in the variant's format
    /// (`YYYY-MM-DD[ HH:mm[:ss]]`) or empty.
    #[property(notify)]
    pub value: String,
    #[property]
    pub min: Option<String>,
    // …
}

impl InputDateLogic for InputDate {
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) { /* chrono validation */ }
    fn placeholder_text(&self, store: &PropertyStore) -> Value { /* … */ }
}
```

`#[input_shared]` injects the shared input contract (label, hint, error_message, disabled, name, required) and wraps the component's template in the shared chrome (`input/_shared/chrome.html`, spliced at its `<slot/>`) — see ADR 0003.
The template references properties, computed getters and handlers by name; richer expressions are rejected at compile time (see ADR 0001).
Browser-side behavior lives in the co-located `date.impl.ts` under the same names (see ADR 0002).

Templates can nest registered custom elements (`<input-date …>` inside another component's template).
In the terminal, children mount recursively: `.prop=${…}` and `?attr=${…}` bindings sync down on parent updates, `@value-changed=${handler}` bindings route child notify events into the parent behavior, and Tab traverses parent and child widgets in template order.

Select options are data, not template structure (ADR 0006): a `Vec<SelectOption>` property or computed binds as `<select .options=${…}>`, which the web generator expands into the `<option>` children and the terminal feeds to its dropdown widget — `<input-select>` is the generic element; `data-tui` widgets take the same binding through their adapters.

New components group every asset in one directory — definition, template, stylesheet, browser twin AND terminal widget twin (`tui.rs`, behind the catalog's `tui` feature, registered through `uic_tui::WidgetRegistration`) — see ADR 0015 and `input/suggestion/` as the reference.
Async data reaches a component through connectors (ADR 0014): `<input-suggestion>` raises `query-changed` per keystroke and renders whatever rows land in its `suggestions` property; `QuerySource` implementations ship in `ui_components::connect` (in-memory pool, method wrapper, browser `FetchSource`) with `connectSuggestions(el, source)` as the browser glue.
`<nav-tabs>` is the first non-input component: a value-driven tab bar (rows as `options` data, `value-changed` on pick) whose terminal twin wraps rat's `Tabbed` — the demo composes it with a Bootstrap card and two `<template if>` panes, and the terminal renders the `card` class as its bordered block (ADR 0017).
`<nav-breadcrumb>` renders a static breadcrumb trail from `items` rows (`{label, href?}`): a computed decorates the rows with the `divider` so both targets paint the same text separators, and linked crumbs stay anchors in the browser while degrading to plain text in the terminal (ADR 0020).

Plain HTML translates to the closest terminal representation: unknown tags render as generic blocks, text wraps, and a Bootstrap-class subset maps to layout and text styling (margins, flex, `w-100`, `form-label`, `card`, …) while unmapped classes degrade silently.
Interactivity does not degrade silently: `uic_tui::lint` (ADR 0016) fails on template bindings the terminal can never serve and warns on web-only markup — see "Authoring components in your own crate".

The same `<input-date>` renders as a Lit element with Bootstrap chrome in the browser, and as this frame in a terminal:

```
Date of purchase
┌──────────────────────────────────────────┐
│2026-07-07                                │
└──────────────────────────────────────────┘

Format: YYYY-MM-DD
```

## Using the components in any web app

The generated components are regular web components; no Rust toolchain is needed to consume them.
`cargo run -p uic_dist` builds the npm package tree in `dist/npm`: one plain-ESM lit module per component, `.d.ts` declarations, `elements.css`, a Custom Elements Manifest (`custom-elements.json`, IDE completion) and a `package.json` whose exports map serves each `./<tag>.js` plus the connectors module.
The package is not on npm yet (the real publish stays gated until the registry decision, ADR 0004), so consume the built tree directly — `npm install <path to dist/npm>` or a tarball from `npm pack`; the generated package README carries the exact install and import lines.

- `lit` (^3) is a peer dependency, plus `temporal-polyfill` for the date components; nothing is bundled.
- The components render light DOM styled by global stylesheets: load Bootstrap 5 and the package's `elements.css` beside the modules.
- With a bundler, `import '<package>/input-date.js'` registers the element; without one, an import map that resolves `lit` serves the same modules (the demo page works exactly this way).
- Scalar properties mirror attributes, object-valued ones (option lists, state) assign as properties, and every `notify` property raises its `<name>-changed` event.

## Authoring components in your own crate

The catalog is not special: any crate can define components against `uic_core` (it re-exports the derive, `#[input_shared]` and the registry) and render them through both targets.

- Model each component as one directory (ADR 0015): `mod.rs` beside its `.html`, `.scss` and `.impl.ts`; the derive registers the definition at link time.
- Provide a link anchor so the registrations survive the linker — `#[inline(never)] pub fn link() {}` — and have consumers call it before touching the registry (the pattern behind `ui_components::link()`).
- Generate the web side in the consuming app's `build.rs`: call `your_crate::link()` and run `uic_codegen_web::WebCodegen::new(out)` (see `apps/web-demo/build.rs`); `DistBuild` wraps the same output as an npm tree.
- A terminal-interactive component registers its widget twin from a co-located `tui.rs` — `inventory::submit! { uic_tui::WidgetRegistration { kind: "…", build } }` behind your own `tui` cargo feature; the runtime needs no edit (`nav_tabs/tui.rs` is the reference).
- Gate terminal compatibility with one integration test (ADR 0016); it fails on bindings the terminal can never serve — unknown `data-tui` kinds, undispatched widget events, notify-event typos — and prints warnings for web-only markup:

```rust
#[test]
fn tui_compatible() {
    your_crate::link();
    uic_tui::lint::assert_tui_compatible();
}
```

## Workspace

| Crate | Role |
| --- | --- |
| `crates/uic_template` | Lit-flavored template string parser and IR, shared by the derive macro, codegen and TUI |
| `crates/uic_core` | Component model: `ComponentDef`, `PropertyMeta`, `Behavior`, notify semantics, custom-element registry |
| `crates/uic_macros` | `#[derive(CustomElement)]` |
| `crates/uic_codegen_web` | Emits the TypeScript/SCSS/manifest web components for `web_modules` builds |
| `crates/uic_tui` | Terminal runtime (ratatui + taffy + rat-widget) |
| `crates/uic_tui_web` | Browser host for the terminal runtime: wasm sessions rendered through xterm.js (ADR 0007) |
| `crates/uic_js` | Boa host running real npm lit elements on the terminal runtime (ADR 0023) |
| `crates/ui_components` | The component catalog (inputs plus the `<app-root>` demo composition) |
| `apps/web-demo` | Browser demo served via axum/`web_modules::Frontend` |
| `apps/tui-demo` | Terminal demo |
| `apps/lit-demo` | One hand-written Lit todo app: the terminal runs it on `uic_js`/Boa, `web_modules` serves it to the browser |

## Development

```sh
cargo run -p uic_web_demo             # http://127.0.0.1:8080, live reload for web/ (UIC_WEB_DEMO_ADDR=host:port moves it)
cargo run -p uic_tui_demo             # terminal demo (Enter commits, F4/Down or a click opens pickers, clicks focus, Esc quits)
cargo run -p uic_tui_demo input-text  # any registered tag
cargo run -p uic_tui_demo nav-tabs    # the tab bar standalone (Left/Right or a click switches)
cargo run -p uic_tui_demo app-root    # the tabbed demo card, incl. the live word-pool typeahead
cargo run -p uic_lit_demo             # a hand-written Lit todo app in this terminal (uic_js/Boa)
cargo run -p uic_lit_demo -- serve    # the same app on real lit, http://127.0.0.1:8090
cargo run -p uic_tui --example screen # print one rendered terminal frame
cargo run -p uic_dist                 # npm package tree in dist/npm (ADR 0004)
scripts/build-wasm.sh                 # browser TUI for the web demo's split view (ADR 0007), then restart the demo
```

With the wasm build in place the web demo is a gallery (ADR 0022): the root groups `/demo/` (the composed form), `/components/<tag>/` (one page per catalog component) and `/examples/` (maintained end-to-end examples — foreign npm elements whose terminal pane runs on the browser's own engine in a dedicated worker, ADR 0023); each page shows the element twice — the real web component beside the same element in a terminal, rendered by the TUI runtime.
The pages are responsive: side by side above the `md` breakpoint following the width slider (the terminal recomputes its columns live), tabs below it.
The terminal pane follows the page theme — the OS scheme or the toggle choice — in its xterm palette and in the mounted document's cascade alike.
The form example keeps the one-`state`-object story: both panes synchronize over a BroadcastChannel — edit either pane and the other follows, with the state messages landing in the shared log (ADR 0013); the component pages sync per notify property instead.

The dist tree is plain lit ESM + `.d.ts` + `elements.css` + `custom-elements.json` with `lit` as peer dependency — usable from any bundler or import map without Rust.

Releases: bump `workspace.package.version`, merge, tag `vX.Y.Z` — the release workflow rebuilds the tree, checks the tag against the package version and rehearses `npm publish --dry-run` (ADR 0004; the real publish is gated until the registry decision).

`web-demo/build.rs` regenerates the TypeScript from the Rust catalog on every build; the generated tree (including `custom-elements.json`) lands in `$OUT_DIR/gen_web`.
Refresh the codegen snapshot after intentional output changes with `UPDATE_EXPECTED=1 cargo test -p uic_codegen_web`.

QA before committing (the CI gauntlet):

```sh
rustup update stable
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy -p uic_tui_web -p uic_dom --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
cargo test -p uic_codegen_web --features dist
node scripts/parity-check.mjs
./scripts/build-wasm.sh
```
