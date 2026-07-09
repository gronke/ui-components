//! html5ever parses into the arena — and the lit-flavored binding dialect
//! survives the trip.

use uic_dom::{Document, NodeData};

#[test]
fn a_fragment_parse_has_no_implied_scaffolding() {
    let doc: Document = Document::parse_fragment("<div>x</div><span>y</span>", "body");
    let tags: Vec<String> = doc
        .children(doc.root())
        .filter_map(|node| doc.tag_name(node).map(|name| name.to_string()))
        .collect();
    assert_eq!(tags, ["div", "span"]);
    assert_eq!(doc.outer_html(doc.root()), "<div>x</div><span>y</span>");
}

#[test]
fn a_document_parse_implies_the_scaffolding() {
    let doc: Document = Document::parse_html("<!doctype html><p>hi");
    assert_eq!(doc.doctype.as_deref(), Some("html"));
    let html = doc.first_child(doc.root()).expect("the html element");
    assert_eq!(doc.tag_name(html).map(|n| &**n), Some("html"));
    let tags: Vec<String> = doc
        .children(html)
        .filter_map(|node| doc.tag_name(node).map(|name| name.to_string()))
        .collect();
    assert_eq!(tags, ["head", "body"]);
}

#[test]
fn the_binding_dialect_survives_the_parser() {
    let doc: Document = Document::parse_fragment(
        r#"<input ?disabled=${disabled} .value=${display_value} @change=${commit} placeholder="pre ${hint} post">"#,
        "body",
    );
    let input = doc.first_child(doc.root()).expect("the input element");
    assert_eq!(doc.attribute(input, "?disabled"), Some("${disabled}"));
    assert_eq!(doc.attribute(input, ".value"), Some("${display_value}"));
    assert_eq!(doc.attribute(input, "@change"), Some("${commit}"));
    assert_eq!(
        doc.attribute(input, "placeholder"),
        Some("pre ${hint} post")
    );
}

#[test]
fn text_holes_pass_through_untouched() {
    let doc: Document = Document::parse_fragment("<label>${label}</label>tail ${hole}", "body");
    let label = doc.first_child(doc.root()).expect("the label element");
    assert_eq!(doc.text_content(label), "${label}");
    assert_eq!(
        doc.outer_html(doc.root()),
        "<label>${label}</label>tail ${hole}"
    );
}

#[test]
fn attribute_names_lowercase_like_a_browser() {
    // The parts compiler recovers the case from source by index, like lit;
    // the tree itself holds what a browser would.
    let doc: Document = Document::parse_fragment("<div .camelProp=${x} ID=headline></div>", "body");
    let div = doc.first_child(doc.root()).expect("the div element");
    assert_eq!(doc.attribute(div, ".camelprop"), Some("${x}"));
    assert_eq!(doc.attribute(div, ".camelProp"), None);
    assert_eq!(doc.attribute(div, "id"), Some("headline"));
}

#[test]
fn template_children_parse_into_the_contents_fragment() {
    let doc: Document = Document::parse_fragment(
        "<template if=${show_timezone}><span>tz</span></template>",
        "body",
    );
    let template = doc.first_child(doc.root()).expect("the template element");
    assert_eq!(doc.attribute(template, "if"), Some("${show_timezone}"));
    // The element itself has no children; the fragment carries them.
    assert_eq!(doc.children(template).count(), 0);
    let contents = doc
        .element(template)
        .and_then(|el| el.template_contents)
        .expect("templates carry a contents fragment");
    assert!(matches!(doc.node(contents), Some(NodeData::Fragment)));
    let span = doc.first_child(contents).expect("the span inside");
    assert_eq!(doc.tag_name(span).map(|n| &**n), Some("span"));
    assert_eq!(
        doc.outer_html(template),
        "<template if=\"${show_timezone}\"><span>tz</span></template>",
    );
}

#[test]
fn comments_become_comment_nodes() {
    // lit's child markers are bogus comments (`<?…>`) plus regular comments;
    // both must survive as addressable nodes.
    let doc: Document = Document::parse_fragment("<!-- marker --><?lit$1$>", "body");
    let kinds: Vec<String> = doc
        .children(doc.root())
        .map(|node| match doc.node(node) {
            Some(NodeData::Comment(text)) => format!("comment:{text}"),
            _ => "other".to_string(),
        })
        .collect();
    assert_eq!(kinds, ["comment: marker ", "comment:?lit$1$"]);
}

#[test]
fn malformed_input_recovers_with_diagnostics() {
    let doc: Document = Document::parse_fragment("<div a=1 a=2><b>bold</div> tail", "body");
    // Duplicate attributes are a tokenizer parse error; the tree still builds.
    assert!(!doc.parse_errors.is_empty());
    let div = doc.first_child(doc.root()).expect("the div element");
    assert_eq!(doc.attribute(div, "a"), Some("1"));
    assert_eq!(doc.text_content(div), "bold");
}

#[test]
fn a_div_closes_an_open_paragraph() {
    // The spec's implied-end-tag fix-ups run silently; worth knowing when a
    // hand-written template gets restructured.
    let doc: Document = Document::parse_fragment("<p>a<div>b</div>", "body");
    let tags: Vec<String> = doc
        .children(doc.root())
        .filter_map(|node| doc.tag_name(node).map(|name| name.to_string()))
        .collect();
    assert_eq!(tags, ["p", "div"]);
}

#[test]
fn parsing_into_a_payload_carrying_document_defaults_the_payload() {
    #[derive(Default, PartialEq, Debug)]
    struct Payload {
        touched: bool,
    }
    let mut doc: Document<Payload> = Document::parse_fragment("<input>", "body");
    let input = doc.first_child(doc.root()).expect("the input element");
    assert_eq!(
        doc.element(input).map(|el| &el.data),
        Some(&Payload { touched: false }),
    );
    doc.element_mut(input)
        .expect("still an element")
        .data
        .touched = true;
    assert!(doc.element(input).expect("still an element").data.touched);
}
