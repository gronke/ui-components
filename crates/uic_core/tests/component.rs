//! End-to-end exercise of `#[derive(CustomElement)]` against the runtime
//! model: metadata, registry, store, handler/computed dispatch, notify pass.

use uic_core::{
    attribute_to_value, notify_events, Changed, Ctx, CustomElement, CustomElementRegistry, JsType,
    PropertyStore, UiEvent, Value, Zoned,
};

/// Exercises every derive feature in one component.
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "test-box",
    template = "<template if=${label}><label>${label}</label></template>\
                <input type=\"text\" .value=${value} placeholder=${placeholder_text} \
                ?disabled=${disabled} @change=${on_change}>"
)]
struct TestBox {
    /// Committed value.
    #[property(notify)]
    value: String,
    #[property]
    label: Option<String>,
    #[property(reflect)]
    disabled: bool,
    #[property(attribute = "max-len", notify = "max-len-picked")]
    max_len: f64,
    /// Keyword field: maps to the JS name `type` via unraw.
    #[property(reflect, default = "text")]
    r#type: String,
}

impl TestBoxLogic for TestBox {
    fn placeholder_text(&self, store: &PropertyStore) -> Value {
        if store.get("disabled").truthy() {
            "off".into()
        } else {
            "type…".into()
        }
    }

    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let value = event.target_value.clone().unwrap_or_default();
        if value.len() > 5 {
            ctx.set("disabled", true);
        }
        ctx.set("value", value);
    }
}

/// Wrapped by the chrome fixture; proves splice + merged validation.
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "test-wrapped",
    template = "<input data-tui=\"date-input\" .value=${value} @change=${on_change}>",
    wraps_file = "fixtures/chrome.mhtml",
    shared_style = "input-default"
)]
struct TestWrapped {
    #[property(notify)]
    value: String,
    #[property]
    label: Option<String>,
}

impl TestWrappedLogic for TestWrapped {
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("value", event.target_value.clone().unwrap_or_default());
    }

    fn status_note(&self, _store: &PropertyStore) -> Value {
        "ready".into()
    }
}

/// An object-valued (Zoned) property next to a plain one.
#[derive(CustomElement, Default)]
#[custom_element(tag = "test-zoned", template = "<span>${value}</span>")]
struct TestZoned {
    #[property(notify)]
    value: String,
    /// The zoned timestamp mirroring `value`.
    #[property(notify)]
    date: Option<Zoned>,
}

impl TestZonedLogic for TestZoned {}

#[test]
fn zoned_property_is_property_only_and_notifies_by_js_name() {
    let def = TestZoned::definition();
    let date = def.property("date").expect("date property");
    assert_eq!(date.js_type, JsType::Zoned);
    assert_eq!(date.attribute, None, "no observed attribute");
    assert!(date.optional);
    assert!(!date.reflect);
    assert_eq!(date.notify_event_name().as_deref(), Some("date-changed"));

    // The store starts undefined and reports real changes only.
    let mut store = PropertyStore::new(def.properties);
    assert_eq!(store.get("date"), &Value::Undefined);
    let zoned = Zoned::new(
        chrono::TimeZone::with_ymd_and_hms(&chrono_tz::Europe::Berlin, 2026, 7, 7, 0, 0, 0)
            .unwrap(),
    );
    assert!(store.set("date", zoned.clone()).is_some());
    assert!(store.set("date", zoned.clone()).is_none(), "same value");

    // The notify pass carries the object value.
    let mut changed = Changed::default();
    changed.record("date", Value::Null);
    store.set("date", Value::Null);
    store.set("date", zoned.clone());
    let events = notify_events(def, &changed, &store);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_name, "date-changed");
    assert_eq!(events[0].value, Value::Zoned(zoned));
}

#[test]
fn wrapped_component_merges_chrome_and_inner_template() {
    let def = TestWrapped::definition();
    assert!(def.wraps_src.is_some());
    assert_eq!(def.shared_style_id, Some("input-default"));
    assert!(def.shared_scss.is_none());
    // The chrome's undeclared ident became a computed property; the chrome's
    // `label` resolved against the declared field.
    assert_eq!(def.computed, &["status_note"]);
    let handlers: Vec<_> = def.handlers.iter().map(|h| h.name).collect();
    assert_eq!(handlers, vec!["on_change"]);

    // The runtime template is the merged tree: the slot is gone, the inner
    // input sits inside the chrome's wrap div.
    let template = def.template();
    assert!(template.custom_tags().is_empty());
    let wrap = template
        .roots
        .iter()
        .find_map(|node| match node {
            uic_template::Node::Element(el) if el.tag == "div" => Some(el),
            _ => None,
        })
        .expect("chrome wrap div");
    assert!(wrap
        .children
        .iter()
        .any(|node| matches!(node, uic_template::Node::Element(el) if el.tag == "input")));
}

#[test]
fn definition_metadata() {
    let def = TestBox::definition();
    assert_eq!(def.tag_name, "test-box");
    assert_eq!(TestBox::TAG_NAME, "test-box");
    assert_eq!(def.class_name, "TestBox");
    assert_eq!(def.style_id, "test-box");
    assert!(def.scss.is_none());
    assert!(def.web_impl.is_none());
    assert_eq!(def.properties.len(), 5);

    let value = def.property("value").expect("value property");
    assert_eq!(value.js_name, "value");
    assert_eq!(value.attribute, Some("value"));
    assert_eq!(value.js_type, JsType::String);
    assert_eq!(value.notify_event_name().as_deref(), Some("value-changed"));
    assert_eq!(value.doc, "Committed value.");
    assert!(!value.reflect);

    let max_len = def.property("max_len").expect("max_len property");
    assert_eq!(max_len.js_name, "maxLen");
    assert_eq!(max_len.attribute, Some("max-len"));
    assert_eq!(max_len.js_type, JsType::Number);
    assert_eq!(
        max_len.notify_event_name().as_deref(),
        Some("max-len-picked")
    );

    let disabled = def.property("disabled").expect("disabled property");
    assert!(disabled.reflect);
    assert_eq!(disabled.notify_event_name(), None);

    // r#type unraws to plain `type` everywhere, with the custom default.
    let ty = def.property("type").expect("type property");
    assert_eq!(ty.js_name, "type");
    assert_eq!(ty.attribute, Some("type"));
    assert!(ty.reflect);
    assert!(!ty.optional);
    assert_eq!(ty.default, uic_core::DefaultValue::Str("text"));

    assert_eq!(def.computed, &["placeholder_text"]);
    let handlers: Vec<_> = def.handlers.iter().map(|h| h.name).collect();
    assert_eq!(handlers, vec!["on_change"]);
}

#[test]
fn registry_resolves_the_component() {
    let def = CustomElementRegistry::get("test-box").expect("registered");
    assert_eq!(def.tag_name, "test-box");
    assert!(CustomElementRegistry::iter().any(|d| d.tag_name == "test-box"));
    CustomElementRegistry::assert_valid().expect("registry is consistent");
}

#[test]
fn template_parses_lazily() {
    let template = TestBox::definition().template();
    let handlers: Vec<_> = template.referenced_handlers().into_iter().collect();
    assert_eq!(handlers, vec!["on_change"]);
}

#[test]
fn handler_dispatch_and_notify_pass() {
    let def = TestBox::definition();
    let mut behavior = (def.new_behavior)();
    let mut store = PropertyStore::new(def.properties);
    let mut changed = Changed::default();

    let mut ctx = Ctx::new(&mut store, &mut changed);
    behavior.handle(&mut ctx, "on_change", &UiEvent::change("2026"));

    let events = notify_events(def, &changed, &store);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].property, "value");
    assert_eq!(events[0].event_name, "value-changed");
    assert_eq!(events[0].value, Value::Str("2026".into()));
    assert_eq!(events[0].old_value, Value::Str(String::new()));
}

#[test]
fn non_notify_changes_produce_no_events() {
    let def = TestBox::definition();
    let mut behavior = (def.new_behavior)();
    let mut store = PropertyStore::new(def.properties);
    let mut changed = Changed::default();

    // A long value flips `disabled` (no notify) and sets `value` (notify).
    let mut ctx = Ctx::new(&mut store, &mut changed);
    behavior.handle(&mut ctx, "on_change", &UiEvent::change("2026-08-01"));

    assert!(changed.has("disabled"));
    let events = notify_events(def, &changed, &store);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_name, "value-changed");
    assert_eq!(events[0].value, Value::Str("2026-08-01".into()));
}

#[test]
fn computed_dispatch() {
    let def = TestBox::definition();
    let behavior = (def.new_behavior)();
    let mut store = PropertyStore::new(def.properties);

    assert_eq!(
        behavior.compute(&store, "placeholder_text"),
        Value::Str("type…".into())
    );
    store.set("disabled", true);
    assert_eq!(
        behavior.compute(&store, "placeholder_text"),
        Value::Str("off".into())
    );
    assert_eq!(behavior.compute(&store, "nope"), Value::Undefined);
}

#[test]
fn custom_defaults_seed_the_store() {
    let def = TestBox::definition();
    let store = PropertyStore::new(def.properties);
    assert_eq!(store.get("type"), &Value::Str("text".into()));
}

#[test]
fn attributes_convert_by_declared_type() {
    let def = TestBox::definition();
    let max_len = def.property_by_attribute("max-len").expect("by attribute");
    assert_eq!(
        attribute_to_value(max_len.js_type, Some("12")),
        Value::Num(12.0)
    );
    let disabled = def.property_by_attribute("disabled").expect("by attribute");
    assert_eq!(
        attribute_to_value(disabled.js_type, None),
        Value::Bool(false)
    );
}
