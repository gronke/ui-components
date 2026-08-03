//! The web-demo's browser-TUI wasm entry (ADR 0007): it binds the reusable,
//! catalog-agnostic terminal host (`uic_tui_web`) to this demo's catalog
//! (`ui_components` plus the `<app-root>` composition), so the host itself
//! stays free of any one catalog.
//!
//! `uic_tui_web`'s `TuiSession` is a wasm-bindgen export that rides along into
//! this cdylib; [`link_catalog`] anchors the host and the catalog into the
//! bundle so the linker keeps their inventory constructors, which
//! `TuiSession::new`'s `__wasm_call_ctors` then runs.

use wasm_bindgen::prelude::*;

/// Anchors the host and the catalog into this bundle. As a wasm-bindgen export
/// it is a linker root, so wasm-ld keeps `uic_tui_web`, `ui_components` and
/// `ui_components_demo`'s objects (and their inventory constructors) rather
/// than dropping them by lazy archive extraction.
#[wasm_bindgen]
pub fn link_catalog() {
    uic_tui_web::link();
    ui_components_tui::link();
    ui_components_demo::link();
}
