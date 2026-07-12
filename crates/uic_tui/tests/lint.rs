//! The catalog's own TUI-compatibility gate (ADR 0016): every registered
//! template must be servable by the terminal. Third-party component crates
//! add the same two lines against their own `link()` anchor.

#[test]
fn the_catalog_is_tui_compatible() {
    ui_components::link();
    uic_tui::lint::assert_tui_compatible();
}
