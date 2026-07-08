# ui-components

This project assembles custom web components from Rust code.

One Rust definition per component — reactive properties, a lit-flavored template (inline or `.mhtml`), co-located `.scss`, named behavior hooks — renders to two targets:

- Browser: generated TypeScript web components (LitElement variant: plain class, `static properties`, light DOM, no decorators), vendored, compiled and served by [web_modules](https://github.com/gronke/web_modules).
- Terminal: a runtime interpreting the same template IR with ratatui, laid out by taffy (real CSS flexbox/block over terminal cells) and rat-widget input primitives.

Component registration mirrors `customElements.define` through the `inventory` crate; properties follow the catalog's `LitNotify` vocabulary (`notify` → `<name>-changed` events).
Design decisions live in [docs/adr](docs/adr); the plan and milestones in [issue #1](https://github.com/schuhkarton/ui-components/issues/1).

## Defining a component

```rust
#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-date",
    template_file = "date.mhtml",
    scss_file = "date.scss",
    web_impl_file = "date.impl.ts"
)]
pub struct InputDate {
    /// Committed value, `YYYY-MM-DD` or empty.
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

`#[input_shared]` injects the shared input contract (label, hint, error_message, disabled, name, required) and wraps the component's template in the shared chrome (`input/_shared/chrome.mhtml`, spliced at its `<slot/>`) — see ADR 0003.
The template references properties, computed getters and handlers by name; richer expressions are rejected at compile time (see ADR 0001).
Browser-side behavior lives in the co-located `date.impl.ts` under the same names (see ADR 0002).

Templates can nest registered custom elements (`<input-date …>` inside another component's template).
In the terminal, children mount recursively: `.prop=${…}` and `?attr=${…}` bindings sync down on parent updates, `@value-changed=${handler}` bindings route child notify events into the parent behavior, and Tab traverses parent and child widgets in template order.

Select options are data, not template structure (ADR 0006): a `Vec<SelectOption>` property or computed binds as `<select .options=${…}>`, which the web generator expands into the `<option>` children and the terminal feeds to its dropdown widget — `<input-select>` is the generic element.

The same `<input-date>` renders as a Lit element with Bootstrap chrome in the browser, and as this frame in a terminal:

```
Date of purchase
┌──────────────────────────────────────────┐
│2026-07-07                                │
└──────────────────────────────────────────┘

Format: YYYY-MM-DD
```

## Workspace

| Crate | Role |
| --- | --- |
| `crates/uic_template` | Lit-flavored template string parser and IR, shared by the derive macro, codegen and TUI |
| `crates/uic_core` | Component model: `ComponentDef`, `PropertyMeta`, `Behavior`, notify semantics, custom-element registry |
| `crates/uic_macros` | `#[derive(CustomElement)]` |
| `crates/uic_codegen_web` | Emits the TypeScript/SCSS/manifest web components for `web_modules` builds |
| `crates/uic_tui` | Terminal runtime (ratatui + taffy + rat-widget) |
| `crates/ui_components` | The component catalog |
| `apps/web-demo` | Browser demo served via axum/`web_modules::Frontend` |
| `apps/tui-demo` | Terminal demo |

## Development

```sh
cargo run -p uic_web_demo             # http://127.0.0.1:8080, live reload for web/
cargo run -p uic_tui_demo             # terminal demo (Enter commits, F4/Down opens a date's calendar or a select's list, Esc quits)
cargo run -p uic_tui_demo input-text  # any registered tag
cargo run -p uic_tui --example screen # print one rendered terminal frame
cargo run -p uic_dist                 # npm package tree in dist/npm (ADR 0004)
```

The dist tree is plain lit ESM + `.d.ts` + `elements.css` + `custom-elements.json` with `lit` as peer dependency — usable from any bundler or import map without Rust.

Releases: bump `workspace.package.version`, merge, tag `vX.Y.Z` — the release workflow rebuilds the tree, checks the tag against the package version and rehearses `npm publish --dry-run` (ADR 0004; the real publish is gated until the registry decision).

`web-demo/build.rs` regenerates the TypeScript from the Rust catalog on every build; the generated tree (including `custom-elements.json`) lands in `$OUT_DIR/gen_web`.
Refresh the codegen snapshot after intentional output changes with `UPDATE_EXPECTED=1 cargo test -p uic_codegen_web`.

QA before committing:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```
