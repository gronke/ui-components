//! The runtime's stylesheet set (ADR 0021): the user-agent defaults, the
//! generated Bootstrap utility layer and the terminal overrides, parsed once
//! and cascaded per layout pass into computed styles the layout and paint
//! read instead of matching classes in code.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use uic_css::{
    resolve_document, AnsiColor, ComputedStyle, Origin, SheetRef, StyleTable, Stylesheet, TextAlign,
};
use uic_dom::{NodeData, NodeId};

use super::DomDocument;

const UA: &str = include_str!("../../css/ua.css");
const BOOTSTRAP: &str = include_str!("../../css/bootstrap-tui.gen.css");
const OVERRIDES: &str = include_str!("../../css/tui-overrides.css");

struct Stylist {
    ua: Stylesheet,
    bootstrap: Stylesheet,
    overrides: Stylesheet,
}

fn stylist() -> &'static Stylist {
    static STYLIST: OnceLock<Stylist> = OnceLock::new();
    STYLIST.get_or_init(|| {
        let (ua, _) = uic_css::parse_stylesheet(UA);
        let (bootstrap, _) = uic_css::parse_stylesheet(BOOTSTRAP);
        let (overrides, _) = uic_css::parse_stylesheet(OVERRIDES);
        Stylist {
            ua,
            bootstrap,
            overrides,
        }
    })
}

thread_local! {
    /// Component sheets adopted by tag (ADR 0021 stage 2): a foreign
    /// component's `static styles`, parsed once, scoped per instance at
    /// resolve time. Thread-local like the JS host owning the document.
    static COMPONENT_SHEETS: RefCell<HashMap<String, Rc<Stylesheet>>> =
        RefCell::new(HashMap::new());
}

/// Adopts a component's stylesheet for a tag: every mounted instance of the
/// tag gets the sheet scoped to its subtree, `:host` matching the instance.
/// Returns the dropped-declaration count (the degradation contract's
/// measure).
pub fn adopt_component_sheet(tag: &str, css_text: &str) -> usize {
    let (sheet, report) = uic_css::parse_stylesheet(css_text);
    let dropped = report.declarations.len() + report.selectors.len() + report.at_rules.len();
    COMPONENT_SHEETS.with(|sheets| {
        sheets.borrow_mut().insert(tag.to_string(), Rc::new(sheet));
    });
    dropped
}

/// Resolves the whole document against the runtime's sheet set plus the
/// adopted component sheets, scoped to their instances.
pub(crate) fn resolve(doc: &DomDocument, focused: Option<NodeId>) -> StyleTable {
    let stylist = stylist();
    let component_sheets: Vec<(NodeId, Rc<Stylesheet>)> = COMPONENT_SHEETS.with(|sheets| {
        let sheets = sheets.borrow();
        if sheets.is_empty() {
            return Vec::new();
        }
        doc.descendants(doc.root())
            .filter_map(|node| match doc.node(node) {
                Some(NodeData::Element(el)) => sheets
                    .get(&**el.tag())
                    .map(|sheet| (node, Rc::clone(sheet))),
                _ => None,
            })
            .collect()
    });

    let mut sheets = vec![
        SheetRef {
            origin: Origin::Ua,
            sheet: &stylist.ua,
            scope: None,
        },
        SheetRef {
            origin: Origin::Target,
            sheet: &stylist.bootstrap,
            scope: None,
        },
    ];
    for (instance, sheet) in &component_sheets {
        sheets.push(SheetRef {
            origin: Origin::Component,
            sheet,
            scope: Some(*instance),
        });
    }
    sheets.push(SheetRef {
        origin: Origin::App,
        sheet: &stylist.overrides,
        scope: None,
    });
    resolve_document(doc, &sheets, focused)
}

/// The text style a computed style paints with, plus its centering flag.
pub(crate) fn text_style(computed: &ComputedStyle) -> (Style, bool) {
    let mut style = Style::default();
    if let Some(color) = computed.color {
        style = style.fg(convert_color(color));
    }
    match computed.background {
        Some(uic_css::Color::Highlight) => style = style.add_modifier(Modifier::REVERSED),
        Some(background) => style = style.bg(convert_color(background)),
        None => {}
    }
    if computed.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if computed.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if computed.dim {
        style = style.add_modifier(Modifier::DIM);
    }
    if computed.underlined {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if computed.crossed_out {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    (style, computed.text_align == TextAlign::Center)
}

pub(crate) fn convert_color(color: uic_css::Color) -> Color {
    match color {
        uic_css::Color::Ansi(ansi) => match ansi {
            AnsiColor::Black => Color::Black,
            AnsiColor::Red => Color::Red,
            AnsiColor::Green => Color::Green,
            AnsiColor::Yellow => Color::Yellow,
            AnsiColor::Blue => Color::Blue,
            AnsiColor::Magenta => Color::Magenta,
            AnsiColor::Cyan => Color::Cyan,
            AnsiColor::Gray => Color::Gray,
            AnsiColor::DarkGray => Color::DarkGray,
            AnsiColor::LightRed => Color::LightRed,
            AnsiColor::LightGreen => Color::LightGreen,
            AnsiColor::LightYellow => Color::LightYellow,
            AnsiColor::LightBlue => Color::LightBlue,
            AnsiColor::LightMagenta => Color::LightMagenta,
            AnsiColor::LightCyan => Color::LightCyan,
            AnsiColor::White => Color::White,
        },
        uic_css::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
        // Highlight is handled as reverse video at the call sites.
        uic_css::Color::Highlight => Color::Reset,
    }
}
