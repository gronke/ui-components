//! The element operations behave like their web namesakes.

use uic_dom::{html, Document, NodeData};

#[test]
fn an_appended_child_moves_from_its_old_parent() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let first = doc.create_element(html::Div);
    let second = doc.create_element(html::Div);
    let child = doc.create_element(html::Span);
    doc.append_child(root, first);
    doc.append_child(root, second);
    doc.append_child(first, child);

    doc.append_child(second, child);

    assert_eq!(doc.children(first).count(), 0);
    assert_eq!(doc.parent(child), Some(second));
}

#[test]
fn insert_before_after_and_replace_keep_sibling_order() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let list = doc.create_element(html::Div);
    doc.append_child(root, list);
    let a = doc.create_text_node("a");
    let c = doc.create_text_node("c");
    doc.append_child(list, a);
    doc.append_child(list, c);

    let b = doc.create_text_node("b");
    doc.insert_before(b, c);
    let d = doc.create_text_node("d");
    doc.insert_after(d, c);
    assert_eq!(doc.inner_html(list), "abcd");

    let x = doc.create_text_node("x");
    doc.replace_child(x, c);
    assert_eq!(doc.inner_html(list), "abxd");
    // The replaced node is detached, not destroyed.
    assert_eq!(doc.text(c), Some("c"));
    assert_eq!(doc.parent(c), None);
}

#[test]
fn detach_keeps_the_subtree_alive_for_reinsertion() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let div = doc.create_element(html::Div);
    let span = doc.create_element(html::Span);
    doc.append_child(root, div);
    doc.append_child(div, span);

    doc.detach(div);
    assert_eq!(doc.children(root).count(), 0);
    assert_eq!(doc.parent(span), Some(div));

    doc.append_child(root, div);
    assert_eq!(doc.outer_html(root), "<div><span></span></div>");
}

#[test]
fn remove_reclaims_the_subtree_and_its_template_contents() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let div = doc.create_element(html::Div);
    let template = doc.create_element(html::Template);
    doc.append_child(root, div);
    doc.append_child(div, template);
    let contents = doc
        .element(template)
        .and_then(|el| el.template_contents)
        .expect("templates carry a contents fragment");

    doc.remove(div);

    assert!(doc.node(div).is_none());
    assert!(doc.node(template).is_none());
    assert!(doc.node(contents).is_none());
    assert_eq!(doc.children(root).count(), 0);
}

#[test]
fn stale_ids_stay_absent_after_slot_reuse() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let old = doc.create_element(html::Div);
    doc.append_child(root, old);
    doc.remove(old);

    // The next creation may recycle the freed slot; the stale id must not
    // alias the new node.
    let new = doc.create_element(html::Span);
    doc.append_child(root, new);

    assert!(doc.node(old).is_none());
    assert_eq!(doc.children(old).count(), 0);
    assert_eq!(doc.parent(old), None);
    assert_eq!(doc.tag_name(new).map(|n| &**n), Some("span"));
}

#[test]
fn attributes_upsert_in_insertion_order() {
    let mut doc: Document = Document::new();
    let input = doc.create_element(html::Input);
    doc.set_attribute(input, "type", "text");
    doc.set_attribute(input, "placeholder", "free text");
    doc.set_attribute(input, "type", "number");

    assert_eq!(doc.attribute(input, "type"), Some("number"));
    let names: Vec<&str> = doc
        .element(input)
        .expect("input is an element")
        .attrs()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["type", "placeholder"]);

    doc.remove_attribute(input, "type");
    assert_eq!(doc.attribute(input, "type"), None);

    // Non-elements ignore attribute writes instead of failing.
    let text = doc.create_text_node("x");
    doc.set_attribute(text, "class", "nope");
    assert_eq!(doc.attribute(text, "class"), None);
}

#[test]
fn the_class_list_adds_removes_and_toggles() {
    let mut doc: Document = Document::new();
    let div = doc.create_element(html::Div);
    doc.add_class(div, "input-group");
    doc.add_class(div, "is-invalid");
    doc.add_class(div, "input-group");
    assert_eq!(doc.attribute(div, "class"), Some("input-group is-invalid"));
    assert!(doc.has_class(div, "is-invalid"));

    doc.remove_class(div, "input-group");
    assert_eq!(doc.attribute(div, "class"), Some("is-invalid"));

    assert!(!doc.toggle_class(div, "is-invalid"));
    // Like classList.remove, dropping the last class keeps the attribute.
    assert_eq!(doc.attribute(div, "class"), Some(""));
    assert!(doc.toggle_class(div, "is-invalid"));
    assert!(doc.has_class(div, "is-invalid"));
}

#[test]
fn text_content_concatenates_descendants() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let p = doc.create_element(html::P);
    let b = doc.create_element(html::Span);
    let hello = doc.create_text_node("hello ");
    let world = doc.create_text_node("world");
    doc.append_child(root, p);
    doc.append_child(p, hello);
    doc.append_child(p, b);
    doc.append_child(b, world);

    assert_eq!(doc.text_content(p), "hello world");

    doc.set_text(world, "there");
    assert_eq!(doc.text_content(p), "hello there");
}

#[test]
fn typed_kinds_create_the_expected_tags() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    for (kind, tag) in [
        (doc.create_element(html::Div), "div"),
        (doc.create_element(html::TextArea), "textarea"),
        (doc.create_element(html::OptionEl), "option"),
        (doc.create_element(html::H3), "h3"),
    ] {
        doc.append_child(root, kind);
        assert_eq!(doc.tag_name(kind).map(|name| &**name), Some(tag));
    }
    let dynamic = doc.create_element_named("input-date");
    assert_eq!(
        doc.tag_name(dynamic).map(|name| &**name),
        Some("input-date")
    );
}

#[test]
fn the_serializer_round_trips_markup() {
    let mut doc: Document = Document::new();
    let root = doc.root();
    let label = doc.create_element(html::Label);
    let text = doc.create_text_node("Amount <€> & \"quotes\"");
    let input = doc.create_element(html::Input);
    doc.append_child(root, label);
    doc.append_child(label, text);
    doc.append_child(label, input);
    doc.set_attribute(input, "value", "a\"b");

    // Text and attribute values escape; the void element has no end tag.
    assert_eq!(
        doc.outer_html(label),
        "<label>Amount &lt;€&gt; &amp; \"quotes\"<input value=\"a&quot;b\"></label>",
    );

    match doc.node(text) {
        Some(NodeData::Text(content)) => assert_eq!(content, "Amount <€> & \"quotes\""),
        _ => panic!("expected the text node"),
    }
}
