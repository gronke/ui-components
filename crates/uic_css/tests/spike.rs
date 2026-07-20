//! The dialect's ground proofs: the servo crates carry it over the arena
//! document.

use cssparser::ToCss as _;
use uic_css::{matches, parse_selector_list, parse_stylesheet};
use uic_dom::Document;

const JSON_VIEWER_CSS: &str = include_str!("fixtures/json-viewer.css");

#[test]
fn json_viewer_stylesheet_parses_with_a_drop_report() {
    let (sheet, report) = parse_stylesheet(JSON_VIEWER_CSS);
    assert!(sheet.rules.len() >= 8, "kept rules: {}", sheet.rules.len());
    // The custom-property block survives.
    let custom = sheet
        .rules
        .iter()
        .flat_map(|rule| &rule.declarations)
        .filter(|decl| decl.name.starts_with("--"))
        .count();
    assert!(custom >= 10, "custom properties kept: {custom}");
    // The terminal-meaningless declarations land in the report.
    for expected in ["font-family", "cursor", "user-select", "transition"] {
        assert!(
            report.declarations.iter().any(|name| name == expected),
            "{expected} should be reported dropped: {:?}",
            report.declarations
        );
    }
    println!(
        "kept {} rules; dropped {} declarations, {} selectors, {} at-rules",
        sheet.rules.len(),
        report.declarations.len(),
        report.selectors.len(),
        report.at_rules.len()
    );
}

#[test]
fn dir_matches_ltr_and_never_rtl() {
    let doc: Document<()> = Document::parse_html("<x-a><span id='t'>x</span></x-a>");
    let root = doc.root();
    let target = doc
        .descendants(root)
        .find(|&n| doc.attribute(n, "id") == Some("t"))
        .unwrap();
    let ltr = parse_selector_list(":dir(ltr)").expect(":dir(ltr)");
    let rtl = parse_selector_list(":dir(rtl)").expect(":dir(rtl)");
    assert!(matches(&doc, target, &ltr, None, None));
    assert!(!matches(&doc, target, &rtl, None, None));
    assert!(parse_selector_list(":dir(sideways)").is_err());
}

#[test]
fn where_host_parses() {
    let list = parse_selector_list(":where(:host)").expect(":where(:host)");
    assert_eq!(list.slice().len(), 1);
    // Specificity of :where() is zero — the custom-property block must lose
    // to any host-targeting override.
    assert_eq!(list.slice()[0].specificity(), 0);
}

#[test]
fn descendant_and_child_combinators_match_over_the_arena() {
    let doc: Document<()> = Document::parse_html(
        "<json-viewer><ul><li>a<ul><li id='target'>b</li></ul></li></ul></json-viewer>",
    );
    let root = doc.root();
    let target = doc
        .descendants(root)
        .find(|&node| doc.attribute(node, "id") == Some("target"))
        .expect("target li");

    let list = parse_selector_list("li ul > li").expect("selector");
    assert!(matches(&doc, target, &list, None, None));

    let outer = doc
        .descendants(root)
        .find(|&node| doc.tag_name(node).map(|t| &**t == "li") == Some(true))
        .expect("outer li");
    assert!(!matches(&doc, outer, &list, None, None));
}

#[test]
fn class_attribute_and_focus_selectors_match() {
    let doc: Document<()> =
        Document::parse_html("<div class='key collapsable' role='treeitem' data-path='a'>x</div>");
    let node = doc
        .descendants(doc.root())
        .find(|&n| doc.tag_name(n).map(|t| &**t == "div") == Some(true))
        .expect("div");

    for selector in [".key", "[role=\"treeitem\"]", "div.collapsable[data-path]"] {
        let list = parse_selector_list(selector).expect(selector);
        assert!(matches(&doc, node, &list, None, None), "{selector}");
    }

    let focus = parse_selector_list(":focus").expect(":focus");
    assert!(!matches(&doc, node, &focus, None, None));
    assert!(matches(&doc, node, &focus, None, Some(node)));
}

#[test]
fn component_scope_clamps_ancestor_walks() {
    let doc: Document<()> = Document::parse_html(
        "<ul><li><json-viewer><span class='inner'>x</span></json-viewer></li></ul>",
    );
    let root = doc.root();
    let scope = doc
        .descendants(root)
        .find(|&n| doc.tag_name(n).map(|t| &**t == "json-viewer") == Some(true))
        .expect("component");
    let inner = doc
        .descendants(scope)
        .find(|&n| doc.tag_name(n).map(|t| &**t == "span") == Some(true))
        .expect("span");

    // Unscoped, the li ancestor is visible; scoped to the component, the
    // walk clamps and the selector no longer reaches it.
    let list = parse_selector_list("li span").expect("selector");
    assert!(matches(&doc, inner, &list, None, None));
    assert!(!matches(&doc, inner, &list, Some(scope), None));
}

#[test]
fn specificity_orders_sanely() {
    let id = parse_selector_list("#a").unwrap().slice()[0].specificity();
    let class = parse_selector_list(".a").unwrap().slice()[0].specificity();
    let tag = parse_selector_list("li").unwrap().slice()[0].specificity();
    let compound = parse_selector_list("li.a[b]").unwrap().slice()[0].specificity();
    assert!(id > class && class > tag);
    assert!(compound > class);
}

#[test]
fn the_bootstrap_filter_keeps_a_meaningful_subset() {
    let source = std::fs::read_to_string(env!("UIC_CSS_BOOTSTRAP")).expect("vendored bootstrap");
    let (sheet, report) = parse_stylesheet(&source);
    let declarations: usize = sheet.rules.iter().map(|rule| rule.declarations.len()).sum();
    println!(
        "bootstrap filter: kept {} rules / {} declarations; dropped {} declarations, {} selectors, {} at-rules",
        sheet.rules.len(),
        declarations,
        report.declarations.len(),
        report.selectors.len(),
        report.at_rules.len()
    );
    // The utilities the terminal map cares about must survive.
    let kept_selector = |needle: &str| {
        sheet.rules.iter().any(|rule| {
            rule.selectors
                .slice()
                .iter()
                .any(|s| s.to_css_string().contains(needle))
        })
    };
    for needle in [".d-flex", ".mt-3", ".w-100", ".flex-column", ".gap-2"] {
        assert!(kept_selector(needle), "{needle} should survive the filter");
    }
    assert!(sheet.rules.len() > 300, "kept {}", sheet.rules.len());
    assert!(report.at_rules.iter().any(|r| r == "@media"));
}

#[test]
fn translucent_tints_leave_the_inherited_background_alone() {
    use uic_css::{resolve_document, Origin, SheetRef};

    // Bootstrap's card cap: a 3% body-color tint over the card background.
    // A terminal cell cannot composite, so the tint declaration drops and
    // the header keeps the card's own background instead of painting the
    // alpha-stripped near-black.
    let (sheet, _) = parse_stylesheet(
        ":root { --bs-body-bg: #fff; --bs-body-color-rgb: 33, 37, 41; }\n         .card { background-color: var(--bs-body-bg); }\n         .card-header { background-color: rgba(var(--bs-body-color-rgb), 0.03); }\n",
    );
    let doc: Document<()> =
        Document::parse_html("<div class='card'><div class='card-header'>Head</div></div>");
    let root = doc.root();
    let sheets = [SheetRef {
        origin: Origin::Target,
        sheet: &sheet,
        scope: None,
    }];
    let table = resolve_document(&doc, &sheets, None);

    let card = doc
        .descendants(root)
        .find(|&n| doc.attribute(n, "class") == Some("card"))
        .unwrap();
    let header = doc
        .descendants(root)
        .find(|&n| doc.attribute(n, "class") == Some("card-header"))
        .unwrap();
    let card_bg = table.get(&card).unwrap().style.background;
    assert_eq!(card_bg, Some(uic_css::Color::Rgb(255, 255, 255)));
    assert_eq!(
        table.get(&header).unwrap().style.background,
        card_bg,
        "the dropped tint leaves the inherited card background"
    );
}

#[test]
fn pseudo_elements_resolve_with_content_and_rotation() {
    use uic_css::{resolve_document, Origin, SheetRef};

    let (sheet, _) = parse_stylesheet(
        ":where(:host) { --accent: #ff0000; }\n\
         .collapsable::before { content: '\u{25b6}'; transform: rotate(90deg); font-size: 0.8em; color: var(--accent); }\n\
         .collapsable--collapsed::before { transform: rotate(0); }\n",
    );
    let doc: Document<()> = Document::parse_html(
        "<x-demo><span class='collapsable'>open</span><span class='collapsable collapsable--collapsed'>closed</span></x-demo>",
    );
    let root = doc.root();
    let host = doc
        .descendants(root)
        .find(|&n| doc.tag_name(n).map(|t| &**t == "x-demo") == Some(true))
        .unwrap();
    let sheets = [SheetRef {
        origin: Origin::Component,
        sheet: &sheet,
        scope: Some(host),
    }];
    let table = resolve_document(&doc, &sheets, None);

    let spans: Vec<_> = doc
        .descendants(root)
        .filter(|&n| doc.tag_name(n).map(|t| &**t == "span") == Some(true))
        .collect();
    let open = table.get(&spans[0]).unwrap().before.as_ref().unwrap();
    assert_eq!(open.content.as_deref(), Some("\u{25b6}"));
    assert_eq!(open.rotation, 90, "expanded marker rotates");
    assert!(open.dim, "sub-em font-size reads dim");
    assert_eq!(
        open.color,
        Some(uic_css::Color::Rgb(0xff, 0, 0)),
        "var() resolves in the pseudo cascade"
    );
    let closed = table.get(&spans[1]).unwrap().before.as_ref().unwrap();
    assert_eq!(closed.rotation, 0, "collapsed marker stays unrotated");
    // No ::after rules — no generated box.
    assert!(table.get(&spans[0]).unwrap().after.is_none());
}
