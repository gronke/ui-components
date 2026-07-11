//! `<input-select>` — the generic dropdown of the catalog's select family.
//! Options are data (ADR 0006): a `SelectOption` list assigned as a property
//! or produced by a computed, rendered as the two-layer front/back select in
//! the browser and as a dropdown widget in the terminal.

use uic_core::{input_shared, Changed, Ctx, CustomElement, SelectOption, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-select",
    template_file = "select.mhtml",
    scss_file = "select.scss",
    web_impl_file = "select.impl.ts"
)]
pub struct InputSelect {
    /// Committed value; with a `default` present, empty commits null.
    #[property(notify, default = "")]
    pub value: Option<String>,
    /// Prepended null option: unset means none, a string (possibly empty)
    /// labels it. Its presence also enables committing null (`allow_null`).
    #[property]
    pub r#default: Option<String>,
    /// Flush rendering when embedded in another input's group.
    #[property(reflect)]
    pub embedded: bool,
    /// The option list; property-only, starts empty (ADR 0006).
    #[property]
    pub options: Vec<SelectOption>,
}

/// Whether the empty selection commits null — the catalog couples this to a
/// `default` being present (`default !== undefined`).
pub(crate) fn allow_null(default: &Value) -> bool {
    !matches!(default, Value::Undefined)
}

/// Prepends the `default`-controlled null option, catalog rules: a string
/// default labels it, any other set value leaves it blank, unset adds none.
pub(crate) fn with_default_option(mut options: Vec<SelectOption>, default: &Value) -> Value {
    match default {
        Value::Undefined => {}
        Value::Str(label) => {
            options.insert(0, SelectOption::new("").with_label(label.clone()));
        }
        _ => options.insert(0, SelectOption::new("").with_label("")),
    }
    Value::Options(options)
}

/// The select-facing value: null and undefined render as the empty option's
/// value.
pub(crate) fn form_value(store: &uic_core::PropertyStore) -> Value {
    match store.get("value") {
        Value::Str(value) => Value::Str(value.clone()),
        _ => Value::Str(String::new()),
    }
}

/// Placeholder styling on the visible layer while the null option shows.
pub(crate) fn front_class(store: &uic_core::PropertyStore) -> Value {
    let empty = !matches!(store.get("value"), Value::Str(value) if !value.is_empty());
    if allow_null(store.get("default")) && empty {
        Value::Str("default text-muted fst-italic".into())
    } else {
        Value::Str(String::new())
    }
}

pub(crate) fn embedded_class(store: &uic_core::PropertyStore) -> Value {
    if store.get("embedded").truthy() {
        Value::Str("bg-transparent border-0".into())
    } else {
        Value::Str(String::new())
    }
}

/// The catalog normalizes in the value setter, so external writes get the
/// same rule: with a `default` present, the empty string becomes null.
pub(crate) fn normalize_empty_value(ctx: &mut Ctx, changed: &Changed) {
    if !changed.has("value") {
        return;
    }
    let empty = matches!(ctx.get("value"), Value::Str(value) if value.is_empty());
    if empty && allow_null(ctx.get("default")) {
        ctx.set("value", Value::Null);
    }
}

impl InputSelectLogic for InputSelect {
    /// Mirrored for the browser in `select.impl.ts` — keep both in sync.
    fn select_options(&self, store: &uic_core::PropertyStore) -> Value {
        let options = match store.get("options") {
            Value::Options(options) => options.clone(),
            _ => Vec::new(),
        };
        with_default_option(options, store.get("default"))
    }

    fn form_value(&self, store: &uic_core::PropertyStore) -> Value {
        form_value(store)
    }

    fn front_class(&self, store: &uic_core::PropertyStore) -> Value {
        front_class(store)
    }

    fn embedded_class(&self, store: &uic_core::PropertyStore) -> Value {
        embedded_class(store)
    }

    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let raw = event.target_value.clone().unwrap_or_default();
        if raw.is_empty() && allow_null(ctx.get("default")) {
            ctx.set("value", Value::Null);
        } else {
            ctx.set("value", raw);
        }
    }

    fn will_update(&mut self, ctx: &mut Ctx, changed: &Changed) {
        normalize_empty_value(ctx, changed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::testing::cycle;
    use uic_core::PropertyStore;

    fn store_with(default: Option<&str>, options: Vec<SelectOption>) -> PropertyStore {
        let def = InputSelect::definition();
        let mut store = PropertyStore::new(def.properties);
        if let Some(default) = default {
            store.set("default", default);
        }
        store.set("options", options);
        store
    }

    fn zones() -> Vec<SelectOption> {
        vec![
            SelectOption::new("Europe/Berlin").with_short("Berlin"),
            SelectOption::new("Europe/Amsterdam").with_short("Amsterdam"),
        ]
    }

    fn commit(store: &mut PropertyStore, input: &str) -> Vec<uic_core::NotifyEvent> {
        let mut behavior = (InputSelect::definition().new_behavior)();
        cycle(store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(input))
        })
    }

    #[test]
    fn select_options_prepend_the_default_row() {
        let behavior = InputSelect::default();
        let plain = store_with(None, zones());
        let Value::Options(options) = behavior.select_options(&plain) else {
            panic!("expected options");
        };
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].value, "Europe/Berlin");

        let with_default = store_with(Some("Pick a zone"), zones());
        let Value::Options(options) = behavior.select_options(&with_default) else {
            panic!("expected options");
        };
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].value, "");
        assert_eq!(options[0].full_label(), "Pick a zone");

        let mut null_default = store_with(None, zones());
        null_default.set("default", Value::Null);
        let Value::Options(options) = behavior.select_options(&null_default) else {
            panic!("expected options");
        };
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].value, "");
        assert_eq!(options[0].full_label(), "");
    }

    #[test]
    fn commit_sets_the_picked_value_and_notifies() {
        let mut store = store_with(None, zones());
        let events = commit(&mut store, "Europe/Berlin");
        assert_eq!(store.get("value"), &Value::Str("Europe/Berlin".into()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
    }

    #[test]
    fn empty_commit_is_null_only_with_a_default() {
        let mut plain = store_with(None, zones());
        plain.set("value", "Europe/Berlin");
        commit(&mut plain, "");
        assert_eq!(plain.get("value"), &Value::Str(String::new()));

        let mut with_default = store_with(Some(""), zones());
        with_default.set("value", "Europe/Berlin");
        commit(&mut with_default, "");
        assert_eq!(with_default.get("value"), &Value::Null);
    }

    #[test]
    fn external_empty_writes_normalize_like_the_setter() {
        let mut store = store_with(Some("Pick"), zones());
        store.set("value", "Europe/Berlin");
        let mut behavior = (InputSelect::definition().new_behavior)();
        cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("value", "");
        });
        assert_eq!(store.get("value"), &Value::Null);
    }

    #[test]
    fn front_class_marks_the_placeholder_state() {
        let behavior = InputSelect::default();
        let plain = store_with(None, zones());
        assert_eq!(behavior.front_class(&plain), Value::Str(String::new()));

        let with_default = store_with(Some("Pick"), zones());
        assert_eq!(
            behavior.front_class(&with_default),
            Value::Str("default text-muted fst-italic".into())
        );

        let mut picked = store_with(Some("Pick"), zones());
        picked.set("value", "Europe/Berlin");
        assert_eq!(behavior.front_class(&picked), Value::Str(String::new()));
    }

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = InputSelect::definition();
        assert_eq!(def.tag_name, "input-select");
        assert_eq!(def.shared_style_id, Some("input-default"));

        let options = def.property("options").expect("options property");
        assert_eq!(options.js_type, uic_core::JsType::Options);
        assert_eq!(options.attribute, None);
        assert_eq!(options.default, uic_core::DefaultValue::EmptyOptions);

        let default = def.property("default").expect("default property");
        assert_eq!(default.attribute, Some("default"));
        assert_eq!(default.default, uic_core::DefaultValue::Undefined);

        let value = def.property("value").expect("value property");
        assert_eq!(value.notify_event_name().as_deref(), Some("value-changed"));
    }
}
