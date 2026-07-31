//! The catalog's own TUI-compatibility gate (ADR 0026): every registered
//! template must be servable by the terminal. Third-party component crates
//! add the same two lines against their own `link()` anchor — the demo
//! composition (`<app-root>`) rides along here.

#[test]
fn the_catalog_is_tui_compatible() {
    ui_components_tui::link();
    ui_components_demo::link();
    uic_tui::lint::assert_tui_compatible();
}
