//! `<uic-icon>`: a Material Symbols icon from one vendored SVG source
//! (`uic_icons`).
//!
//! In the browser the named SVG is injected inline (themed through
//! `currentColor`, sized by CSS); in the terminal the `data-tui="icon"` twin in
//! `ui_components_tui` rasterizes the same SVG to Braille cells. Display-only:
//! no value, no commit, so the generated logic trait is empty.

use uic_core::CustomElement;

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "uic-icon",
    template_file = "icon.html",
    scss_file = "icon.scss",
    web_impl_file = "icon.impl.ts"
)]
pub struct UicIcon {
    /// The icon name — a Material Symbols name such as `visibility` or
    /// `content_copy`. Reflected so `<uic-icon name="visibility">` works from
    /// markup and the terminal twin reads it.
    #[property(reflect, default = "")]
    pub name: String,
}

impl UicIconLogic for UicIcon {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = UicIcon::definition();
        assert_eq!(def.tag_name, "uic-icon");
        let name = def.property("name").expect("name property");
        assert_eq!(name.js_name, "name");
        assert_eq!(name.attribute, Some("name"));
        // Display-only: no computed properties, and it ships in the package.
        assert!(def.computed.is_empty());
        assert!(def.dist, "the icon ships in the npm package");
    }
}
