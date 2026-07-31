# tui-demo

Terminal demo: any registered component by tag (default `input-date`), rendered by `uic_tui`.

It links the catalog (`ui_components_tui`, which chains `ui_components`) and mounts one component so you can drive it in a real terminal — Tab/Enter commit, Esc quits.

```sh
cargo run -p uic_tui_demo               # <input-date>
cargo run -p uic_tui_demo input-text    # <input-text>
```
