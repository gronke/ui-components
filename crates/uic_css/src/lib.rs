//! CSS for the terminal: a closed dialect parsed with servo's cssparser
//! and selectors crates, cascaded over the retained `uic_dom` document;
//! parsing with a drop report, selector matching with component scoping
//! (`:host`, clamped ancestor walks), four cascade origins, custom
//! properties with `var()` and additive `calc()`, and the computed-style
//! table the terminal's layout and paint consume.

mod cascade;
mod computed;
mod parse;
mod select;
mod value;

pub use cascade::{resolve_document, ElementStyles, Origin, SheetRef, StyleTable};
pub use computed::{ComputedStyle, Dimension, Display, FlexDirection, TextAlign};
pub use parse::{parse_stylesheet, Declaration, DropReport, Rule, Stylesheet};
pub use select::{
    matches, parse_selector_list, CssString, El, PseudoClass, PseudoElement, TuiSelectors,
};
pub use value::{AnsiColor, Axis, Color, Length};
