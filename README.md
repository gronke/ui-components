# ui-components

This project aims to assemble custom web components from Rust code.

One Rust definition per component — properties, events, a lit-flavored template (inline or `.mhtml`), co-located `.scss` — is rendered to two targets:

- Browser: generated TypeScript web components (LitElement variant: plain class, `static properties`, light DOM, no decorators), vendored, compiled and served by [web_modules](https://github.com/gronke/web_modules).
- Terminal: a TUI renderer interpreting the same template IR with ratatui, laid out by taffy (CSS flexbox/grid over terminal cells) and rat-widget input primitives.

Component registration mirrors `customElements.define` through the `inventory` crate.
The pilot component is `<input-date>`; plan and milestones live in [issue #1](https://github.com/schuhkarton/ui-components/issues/1).

## Workspace

| Crate | Role |
| --- | --- |
| `crates/uic_template` | Lit-flavored template string parser and IR, shared by the derive macro, codegen and TUI |
| `crates/uic_core` | Component model: `ComponentDef`, `PropertyMeta`, `Behavior`, notify semantics, custom-element registry |
| `crates/uic_macros` | `#[derive(CustomElement)]` |
| `crates/uic_codegen_web` | Emits the TypeScript/SCSS web components for `web_modules` builds |
| `crates/uic_tui` | Terminal runtime (ratatui + taffy + rat-widget) |
| `crates/ui_components` | The component catalog |
| `apps/web-demo` | Browser demo served via axum/`web_modules::Frontend` |
| `apps/tui-demo` | Terminal demo |

## Development

```sh
cargo build -p uic_web_demo   # bakes the frontend (vendors npm deps at build time)
cargo run -p uic_web_demo     # serves http://127.0.0.1:8080 with live reload
cargo run -p uic_tui_demo     # terminal demo
```

QA before committing:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```
