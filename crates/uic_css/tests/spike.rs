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
    for expected in ["font-family", "cursor", "user-select", "transform"] {
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
