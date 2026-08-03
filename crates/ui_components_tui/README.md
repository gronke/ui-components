# ui_components_tui

The catalog's terminal widget twins (ADR 0002): the `tui.rs` adapters that back `ui_components`'s `data-tui` widgets (the tab bar and the suggestion popup) in a companion crate whose directories mirror the catalog's.

Splitting the twins out keeps `ui_components` a pure web/definition crate with no `uic_tui` dependency, while the mirrored paths keep each twin legible beside the definition it serves.
Drift stays guarded structurally rather than by co-location.
A twin keeps its mirrored path, registers the same `data-tui` string kind through `uic_tui`'s `inventory` `WidgetRegistration`, and runs under the same TestBackend suites.

`ui_components_tui::link()` anchors the widget registrations past the linker and chains `ui_components::link()`, so a terminal consumer reaches both the element and the widget registries through this one call.

```sh
cargo test -p ui_components_tui
```
