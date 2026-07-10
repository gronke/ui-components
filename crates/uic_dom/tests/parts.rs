//! The parts engine: compile once, instantiate by cloning, patch only what
//! changed.

use uic_dom::parts::{CompiledTemplate, EventBinding, PartValue};
use uic_dom::{html, Document};

const TEMPLATE: &str = "<label>${label}</label>\
    <input class=\"a ${cls} b\" ?disabled=${off} .myValue=${value} @change=${on_change}>\
    <template if=${show}><span>${inner}</span></template>";

fn text(value: &str) -> PartValue {
    PartValue::Text(value.to_string())
}

struct Mounted {
    template: CompiledTemplate,
    doc: Document,
    host: uic_dom::NodeId,
    events: Vec<EventBinding>,
    instance: uic_dom::parts::TemplateInstance,
}

fn mount() -> Mounted {
    let template = CompiledTemplate::compile(TEMPLATE).expect("compiles");
    let mut doc: Document = Document::new();
    let host = doc.create_element(html::Div);
    let root = doc.root();
    doc.append_child(root, host);
    let (instance, events) = template.instantiate(&mut doc, host);
    Mounted {
        template,
        doc,
        host,
        events,
        instance,
    }
}

#[test]
fn compile_numbers_the_holes_in_source_order() {
    let template = CompiledTemplate::compile(TEMPLATE).expect("compiles");
    assert_eq!(
        template.holes(),
        ["label", "cls", "off", "value", "show", "inner"]
    );
}

#[test]
fn compile_strips_bindings_and_plants_markers() {
    let mut m = mount();
    let html = m.doc.outer_html(m.host);
    // The label's text hole became a comment marker; the bound attributes
    // are gone from the clone until a commit writes them.
    assert!(html.contains("<label><!--uic-part--></label>"), "{html}");
    assert!(
        html.contains("<input>"),
        "bound attributes stripped: {html}"
    );
    assert!(
        html.contains("<template></template>"),
        "empty anchor: {html}"
    );
    let _ = &mut m;
}

#[test]
fn events_surface_at_instantiation() {
    let m = mount();
    assert_eq!(m.events.len(), 1);
    assert_eq!(m.events[0].event, "change");
    assert_eq!(m.events[0].handler, "on_change");
    assert_eq!(
        m.doc.tag_name(m.events[0].node).map(|t| &**t),
        Some("input")
    );
}

#[test]
fn commit_fills_text_attributes_and_booleans() {
    let mut m = mount();
    let values = [
        text("Amount"),
        text("wide"),
        PartValue::Bool(true),
        text("42"),
        PartValue::Bool(false),
        PartValue::NoChange,
    ];
    let effects = m.template.commit(&mut m.instance, &mut m.doc, &values);

    let html = m.doc.outer_html(m.host);
    assert!(
        html.contains("<label><!--uic-part-->Amount</label>"),
        "{html}"
    );
    assert!(html.contains("class=\"a wide b\""), "{html}");
    assert!(html.contains("disabled=\"\""), "{html}");
    assert!(!html.contains("<span>"), "branch stays off: {html}");
    assert_eq!(effects.property_writes.len(), 1);
    assert_eq!(effects.property_writes[0].name, "myValue");
    assert_eq!(effects.property_writes[0].value, text("42"));
    assert!(effects.added_events.is_empty());
}

#[test]
fn recommit_patches_in_place_and_nothing_clears() {
    let mut m = mount();
    let on = [
        text("Amount"),
        text("wide"),
        PartValue::Bool(true),
        text("42"),
        PartValue::Bool(false),
        PartValue::NoChange,
    ];
    m.template.commit(&mut m.instance, &mut m.doc, &on);

    let update = [
        text("Total"),
        PartValue::Nothing,
        PartValue::Bool(false),
        PartValue::NoChange,
        PartValue::Bool(false),
        PartValue::NoChange,
    ];
    let effects = m.template.commit(&mut m.instance, &mut m.doc, &update);

    let html = m.doc.outer_html(m.host);
    assert!(html.contains(">Total</label>"), "text patched: {html}");
    // A hole inside static chunks renders empty on nothing; the attribute
    // itself stays (only a single-hole nothing removes it).
    assert!(html.contains("class=\"a  b\""), "{html}");
    assert!(!html.contains("disabled"), "boolean cleared: {html}");
    assert!(
        effects.property_writes.is_empty(),
        "NoChange produces no property write"
    );

    let clear = [
        PartValue::Nothing,
        PartValue::NoChange,
        PartValue::NoChange,
        PartValue::NoChange,
        PartValue::NoChange,
        PartValue::NoChange,
    ];
    m.template.commit(&mut m.instance, &mut m.doc, &clear);
    let html = m.doc.outer_html(m.host);
    assert!(
        html.contains("<label><!--uic-part--></label>"),
        "cleared: {html}"
    );
}

#[test]
fn a_single_hole_attribute_removes_on_nothing() {
    let template = CompiledTemplate::compile("<div title=${tip}></div>").expect("compiles");
    let mut doc: Document = Document::new();
    let root = doc.root();
    let host = doc.create_element(html::Div);
    doc.append_child(root, host);
    let (mut instance, _) = template.instantiate(&mut doc, host);

    template.commit(&mut instance, &mut doc, &[text("hello")]);
    assert!(doc.outer_html(host).contains("title=\"hello\""));
    template.commit(&mut instance, &mut doc, &[PartValue::Nothing]);
    assert!(!doc.outer_html(host).contains("title"));
}

#[test]
fn dirty_checks_skip_untouched_parts() {
    let mut m = mount();
    let values = [
        text("Amount"),
        text("wide"),
        PartValue::Bool(true),
        text("42"),
        PartValue::Bool(false),
        PartValue::NoChange,
    ];
    m.template.commit(&mut m.instance, &mut m.doc, &values);

    // Out-of-band vandalism: a recommit with EQUAL values must not repair
    // it, proving the parts skipped their writes.
    let input = m
        .doc
        .descendants(m.host)
        .find(|&n| m.doc.tag_name(n).map(|t| &**t) == Some("input"))
        .expect("the input");
    m.doc.remove_attribute(input, "class");
    m.template.commit(&mut m.instance, &mut m.doc, &values);
    assert!(!m.doc.outer_html(m.host).contains("class="), "skipped");

    // A changed value writes again.
    let mut changed = values.clone();
    changed[1] = text("narrow");
    m.template.commit(&mut m.instance, &mut m.doc, &changed);
    assert!(m.doc.outer_html(m.host).contains("class=\"a narrow b\""));
}

#[test]
fn conditionals_instantiate_patch_and_tear_down() {
    let mut m = mount();
    let base = [
        text("Amount"),
        text("c"),
        PartValue::Bool(false),
        PartValue::NoChange,
        PartValue::Bool(true),
        text("first"),
    ];
    let effects = m.template.commit(&mut m.instance, &mut m.doc, &base);
    let html = m.doc.outer_html(m.host);
    assert!(
        html.contains("<template></template><span><!--uic-part-->first</span>"),
        "the branch renders after its anchor: {html}"
    );
    assert!(effects.added_events.is_empty(), "no events in this branch");

    // Patching with the branch on updates only the inner part.
    let mut patched = base.clone();
    patched[5] = text("second");
    m.template.commit(&mut m.instance, &mut m.doc, &patched);
    assert!(m.doc.outer_html(m.host).contains(">second</span>"));

    // Off tears the branch down, on re-instantiates it fresh.
    let mut off = patched.clone();
    off[4] = PartValue::Bool(false);
    m.template.commit(&mut m.instance, &mut m.doc, &off);
    assert!(!m.doc.outer_html(m.host).contains("<span>"));
    let mut on_again = off.clone();
    on_again[4] = PartValue::Bool(true);
    on_again[5] = text("third");
    m.template.commit(&mut m.instance, &mut m.doc, &on_again);
    assert!(m.doc.outer_html(m.host).contains(">third</span>"));
}

#[test]
fn camel_case_property_names_recover_from_the_source() {
    let m = mount();
    let mut m = m;
    let values = [
        PartValue::NoChange,
        PartValue::NoChange,
        PartValue::NoChange,
        text("x"),
        PartValue::NoChange,
        PartValue::NoChange,
    ];
    let effects = m.template.commit(&mut m.instance, &mut m.doc, &values);
    // The parser lowercased `.myValue`; the plan got it back by index.
    assert_eq!(effects.property_writes[0].name, "myValue");
}

#[test]
fn escaped_holes_stay_literal() {
    let template = CompiledTemplate::compile("<p>price \\${amount}</p>").expect("compiles");
    assert!(template.holes().is_empty(), "no part for the escaped hole");
    let mut doc: Document = Document::new();
    let root = doc.root();
    let (instance, _) = template.instantiate(&mut doc, root);
    let _ = instance;
    assert!(doc.outer_html(root).contains("price ${amount}"));
}

#[test]
fn two_instances_stay_independent() {
    let template = CompiledTemplate::compile("<b>${x}</b>").expect("compiles");
    let mut doc: Document = Document::new();
    let root = doc.root();
    let first = doc.create_element(html::Div);
    let second = doc.create_element(html::Div);
    doc.append_child(root, first);
    doc.append_child(root, second);
    let (mut one, _) = template.instantiate(&mut doc, first);
    let (mut two, _) = template.instantiate(&mut doc, second);

    template.commit(&mut one, &mut doc, &[text("left")]);
    template.commit(&mut two, &mut doc, &[text("right")]);

    assert!(doc.outer_html(first).contains("left"));
    assert!(!doc.outer_html(first).contains("right"));
    assert!(doc.outer_html(second).contains("right"));
}

#[test]
fn compile_errors_are_reported() {
    use uic_dom::parts::CompileError;
    assert_eq!(
        CompiledTemplate::compile("<p>${open</p>")
            .map(|_| ())
            .unwrap_err(),
        CompileError::UnterminatedHole
    );
    assert!(matches!(
        CompiledTemplate::compile("<input ?disabled=\"x ${a} y\">")
            .map(|_| ())
            .unwrap_err(),
        CompileError::CompositeBinding(_)
    ));
}

#[test]
fn nested_conditionals_tear_down_with_their_parent() {
    let template = CompiledTemplate::compile(
        "<template if=${outer}>a<template if=${inner}><b>${text}</b></template></template>",
    )
    .expect("compiles");
    let mut doc: Document = Document::new();
    let root = doc.root();
    let (mut instance, _) = template.instantiate(&mut doc, root);

    let on = [PartValue::Bool(true), PartValue::Bool(true), text("deep")];
    template.commit(&mut instance, &mut doc, &on);
    assert!(doc.outer_html(root).contains("<b><!--uic-part-->deep</b>"));

    // The outer teardown must take the INNER branch's sibling nodes along.
    let off = [
        PartValue::Bool(false),
        PartValue::NoChange,
        PartValue::NoChange,
    ];
    template.commit(&mut instance, &mut doc, &off);
    assert_eq!(
        doc.outer_html(root),
        "<template></template>",
        "only the anchor remains"
    );

    // And the round trip re-renders cleanly.
    template.commit(&mut instance, &mut doc, &on);
    assert!(doc.outer_html(root).contains(">deep</b>"));
}
