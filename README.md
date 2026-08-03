# ui-components

Build a TUI from HTML.
One component definition, written in Rust, renders as an idiomatic Lit element in the browser and as a real TUI in the terminal: same properties, same events, same keyboard.

A definition carries reactive properties, a lit-flavored template (inline or `.html`), co-located `.scss` and named behavior hooks.
Two targets consume it:

- Browser: generated TypeScript web components (LitElement variant: plain class, `static properties`, light DOM, no decorators), vendored, compiled and served by [web_modules](https://github.com/gronke/web_modules).
- Terminal: a runtime interpreting the same template IR with ratatui, laid out by taffy (real CSS flexbox and block over terminal cells) and rat-widget input primitives.

The terminal host scales with the browser context a page needs.
The template runtime runs without any JavaScript.
For real JS, the scripted host (`uic_js`) runs unmodified lit modules on the Boa engine and grows browser platform APIs as cargo features: `storage`, `sqlite`, `dialogs`, `clipboard`.
[`apps/lit-demo`](apps/lit-demo/README.md) is the proof: a plain, hand-written Lit todo app the terminal runs byte-unmodified.
A frontend designed with a TUI layer in mind gets a second, honest render target for free.
What survives 80 columns tends to be a better web page too.

The stack stays purist on both sides: light-DOM Lit over web standards in the browser; a retained DOM, real CSS (flexbox, the cascade, custom properties) and ratatui in the terminal.
State travels one wire seam (ADR 0013) and two devices pair over WebRTC with no server between them (ADR 0028).
Component registration mirrors `customElements.define` through the `inventory` crate.
Properties follow the catalog's `LitNotify` vocabulary: `notify` raises `<name>-changed` events.

Planned next: the reverse direction.
The retained DOM already exposes the web-shaped mutation API and an HTML serializer, so a Rust program could assemble its TUI programmatically, in a shape ratatui users recognize, and render the same document as HTML.

The crate map and runtime overview live in [docs/architecture.md](docs/architecture.md), the decisions in [docs/adr](docs/adr).

## See it running

```sh
cargo run -p uic_lit_demo             # the hand-written Lit todo app, in this terminal
cargo run -p uic_lit_demo -- p2p      # the same app as a serverless WebRTC peer, QR invites included
cargo run -p uic_web_demo             # the component gallery: every element web-beside-terminal
```

The demo's own guide (`serve`, `live`, `p2p` and the pairing story) is [apps/lit-demo/README.md](apps/lit-demo/README.md).

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

`#[input_shared]` injects the shared input contract (label, hint, error_message, disabled, name, required) and wraps the template in the shared chrome, spliced at its `<slot/>` (ADR 0003).
The template references properties, computed getters and handlers by name; richer expressions are rejected at compile time (ADR 0001).
Browser-side behavior lives in the co-located `date.impl.ts` under the same names (ADR 0002).

Templates nest registered custom elements.
In the terminal, children mount recursively: `.prop=${…}` and `?attr=${…}` bindings sync down on parent updates, `@value-changed=${handler}` bindings route child notify events into the parent behavior, and Tab traverses parent and child widgets in template order.

Select options are data, not template structure (ADR 0005).
A `Vec<SelectOption>` property or computed binds as `<select .options=${…}>`; the web generator expands the `<option>` children and the terminal feeds its dropdown widget.
`<input-select>` is the generic element, and `data-tui` widgets take the same binding through their adapters.

A new component groups every asset in one directory: definition, template, stylesheet, browser twin and terminal widget twin (ADR 0002; `input/suggestion/` is the reference).
Async data reaches a component through connectors (ADR 0014): `<input-suggestion>` raises `query-changed` per keystroke and renders whatever rows land in its `suggestions` property, with `QuerySource` implementations in `ui_components::connect` and `connectSuggestions(el, source)` as the browser glue.
`<nav-tabs>` is a value-driven tab bar whose terminal twin wraps rat's `Tabbed` (ADR 0017).
`<nav-breadcrumb>` renders a static trail from `items` rows; linked crumbs stay anchors in the browser and degrade to plain text in the terminal (ADR 0017).

Plain HTML translates to the closest terminal representation: unknown tags render as generic blocks, text wraps, and a Bootstrap-class subset maps to layout and text styling while unmapped classes degrade silently.
Interactivity does not degrade silently.
`uic_tui::lint` (ADR 0026) fails on template bindings the terminal can never serve and warns on web-only markup; see "Authoring components in your own crate".

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
`cargo run -p uic_dist` builds the npm package tree in `dist/npm`: one plain-ESM lit module per component, `.d.ts` declarations, `elements.css`, a Custom Elements Manifest and a `package.json` whose exports map serves each `./<tag>.js` plus the connectors module.
The package is not on npm yet (the publish stays gated until the registry decision, ADR 0004).
Consume the built tree directly: `npm install <path to dist/npm>` or a tarball from `npm pack`; the generated package README carries the exact install and import lines.

- `lit` (^3) is a peer dependency, plus `temporal-polyfill` for the date components; nothing is bundled.
- The components render light DOM styled by global stylesheets: load Bootstrap 5 and the package's `elements.css` beside the modules.
- With a bundler, `import '<package>/input-date.js'` registers the element; without one, an import map that resolves `lit` serves the same modules.
- Scalar properties mirror attributes, object-valued ones assign as properties, and every `notify` property raises its `<name>-changed` event.

## Authoring components in your own crate

The catalog is not special: any crate can define components against `uic_core` (it re-exports the derive, `#[input_shared]` and the registry) and render them through both targets.

- Model each component as one directory (ADR 0002): `mod.rs` beside its `.html`, `.scss` and `.impl.ts`; the derive registers the definition at link time.
- Provide a link anchor so the registrations survive the linker: `#[inline(never)] pub fn link() {}`, called by consumers before touching the registry (the pattern behind `ui_components::link()`).
- Generate the web side in the consuming app's `build.rs`: call `your_crate::link()` and run `uic_codegen_web::WebCodegen::new(out)` (see `apps/web-demo/build.rs`); `DistBuild` wraps the same output as an npm tree.
- A terminal-interactive component registers its widget twin from a co-located `tui.rs` through `inventory::submit! { uic_tui::WidgetRegistration { kind: "…", build } }`; the runtime needs no edit (`nav_tabs/tui.rs` is the reference).
- Gate terminal compatibility with one integration test (ADR 0026); it fails on bindings the terminal can never serve and prints warnings for web-only markup:

```rust
#[test]
fn tui_compatible() {
    your_crate::link();
    uic_tui::lint::assert_tui_compatible();
}
```

## Workspace

The crate map, the runtime overview and the load-bearing semantics live in [docs/architecture.md](docs/architecture.md).

## Development

```sh
cargo run -p uic_web_demo             # http://127.0.0.1:8080, live reload for web/ (UIC_WEB_DEMO_ADDR=host:port moves it)
cargo run -p uic_tui_demo             # terminal demo (Enter commits, F4/Down or a click opens pickers, Esc quits)
cargo run -p uic_tui_demo input-text  # any registered tag
cargo run -p uic_tui_demo nav-tabs    # the tab bar standalone (Left/Right or a click switches)
cargo run -p uic_tui_demo app-root    # the tabbed demo card, incl. the live word-pool typeahead
cargo run -p uic_lit_demo             # the Lit todo app in this terminal; serve/live/p2p in its README
cargo run -p uic_tui --example screen # print one rendered terminal frame
cargo run -p uic_dist                 # npm package tree in dist/npm (ADR 0004)
scripts/build-wasm.sh                 # browser TUI for the web demo's split view (ADR 0007), then restart the demo
```

With the wasm build in place the web demo is a gallery (ADR 0007).
The root groups `/demo/` (the composed form), `/components/<tag>/` (one page per catalog component) and `/examples/` (foreign npm elements whose terminal pane runs on the browser's own engine in a dedicated worker).
Each page shows the element twice: the real web component beside the same element in a terminal, rendered by the TUI runtime.
The pages are responsive: side by side above the `md` breakpoint following the width slider, tabs below it.
The terminal pane follows the page theme in its xterm palette and in the mounted document's cascade alike.
The form example keeps the one-`state`-object story: both panes synchronize over a BroadcastChannel, and the state messages land in the shared log (ADR 0013); the component pages sync per notify property instead.

Releases: bump `workspace.package.version`, merge, tag `vX.Y.Z`.
The release workflow rebuilds the tree, checks the tag against the package version and rehearses `npm publish --dry-run` (ADR 0004).

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
