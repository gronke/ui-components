//! The catalog's terminal widget twins (ADR 0002): the `tui.rs` adapters that
//! back `ui_components`'s `data-tui` widgets (the tab bar and the suggestion
//! popup) in a companion crate whose directories mirror the catalog's, so the
//! twins stay legible beside their definitions while `ui_components` keeps no
//! `uic_tui` dependency. A terminal consumer links this crate, which chains the
//! catalog's own `link`.

// The twins register through `inventory` at their mirrored paths; they only
// need to be compiled, so lib includes them by path as private modules.
#[path = "nav_tabs/tui.rs"]
mod nav_tabs_tui;
#[path = "input/suggestion/tui.rs"]
mod suggestion_tui;

/// Anchors this crate's widget registrations and the catalog's element
/// registrations past the linker. A terminal consumer calls this in place of
/// `ui_components::link()`.
#[inline(never)]
pub fn link() {
    ui_components::link();
}
