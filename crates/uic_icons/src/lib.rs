//! Material Symbols icons for the catalog.
//!
//! One vendored SVG source (`svg/`, from `@material-symbols/svg-400`,
//! Apache-2.0) feeds both render targets: the browser links [`WEB_SPRITE`] and
//! references icons by name (`<use href="…#visibility">`, themed through
//! `currentColor`); the terminal — behind the `raster` feature — downsamples a
//! build-time alpha mask to Braille cells (see [`raster`]).

/// The assembled SVG sprite: one `<symbol id="…">` per icon. The host serves it
/// and the `<uic-icon>` web component references a symbol by name.
pub static WEB_SPRITE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/material-symbols-sprite.svg"));

/// A generated TS module exporting `ICON_SVGS` (name → inline SVG). Wired into
/// a web build via `WebCodegen::extra_module("uic-icons.ts", uic_icons::WEB_TS)`
/// and imported by the `<uic-icon>` component to inject the icon markup.
pub static WEB_TS: &str = include_str!(concat!(env!("OUT_DIR"), "/uic-icons.ts"));

// `pub static ICON_NAMES: &[&str]`, generated from the `svg/` directory.
include!(concat!(env!("OUT_DIR"), "/icon_names.rs"));

/// Whether `name` names a known icon.
pub fn has(name: &str) -> bool {
    ICON_NAMES.contains(&name)
}

#[cfg(feature = "raster")]
pub mod raster;
