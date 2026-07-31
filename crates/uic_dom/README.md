# uic_dom

A retained DOM for the terminal runtime — LitElement's substrate, in Rust.

One arena-backed tree carries the web's element operations, spec-grade parsing through html5ever's `TreeSink`, and the WHATWG event-dispatch subset with capture/bubble propagation.
This is the architecture lit-html itself uses in the browser: real HTML parsing, with the binding dialect riding through as plain attributes and text.

This is the foundation layer of the terminal side (ADR 0008).
The template-parts compiler, the reactive update lifecycle, the CSS cascade (`uic_css`) and the TUI runtime (`uic_tui`) all build on it.

```sh
cargo test -p uic_dom
```
