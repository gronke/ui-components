//! `<input-text>` — plain text input with the shared input chrome.
//! Commits on change: trimmed, empty becoming null when `allow-null` is set.

use uic_core::{input_shared, Ctx, CustomElement, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-text",
    template_file = "text.html",
    web_impl_file = "text.impl.ts"
)]
pub struct InputText {
    /// Committed value; `allow-null` commits null for empty input.
    #[property(notify, default = "")]
    pub value: Option<String>,
    /// Inner input type passthrough (`text`, `email`, …).
    #[property(reflect, default = "text")]
    pub r#type: String,
    /// Commit null instead of the empty string for empty input.
    #[property(reflect)]
    pub allow_null: bool,
    #[property(default = "")]
    pub placeholder: String,
    /// Anti-autofill by default, like the catalog.
    #[property(reflect, default = "one-time-code")]
    pub autocomplete: String,
}

impl InputTextLogic for InputText {
    /// Mirrored for the browser in `text.impl.ts` — keep both in sync.
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let raw = event.target_value.clone().unwrap_or_default();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            if ctx.get("allow_null").truthy() {
                ctx.set("value", Value::Null);
            } else {
                ctx.set("value", "");
            }
            return;
        }
        ctx.set("value", trimmed.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::testing::{cycle, setup};
    use uic_core::{NotifyEvent, PropertyStore};

    fn commit(allow_null: bool, input: &str) -> (PropertyStore, Vec<NotifyEvent>) {
        let (mut store, mut behavior) = setup(InputText::definition());
        store.set("allow_null", allow_null);
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(input))
        });
        (store, events)
    }

    #[test]
    fn commits_trimmed_text_and_notifies() {
        let (store, events) = commit(false, "  hello world  ");
        assert_eq!(store.get("value"), &Value::Str("hello world".into()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
        assert_eq!(events[0].value, Value::Str("hello world".into()));
    }

    #[test]
    fn empty_input_commits_the_empty_string_by_default() {
        let (store, events) = commit(false, "   ");
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        // Unchanged from the default: no notify event.
        assert!(events.is_empty());
    }

    #[test]
    fn allow_null_commits_null_for_empty_input() {
        let (store, events) = commit(true, "   ");
        assert_eq!(store.get("value"), &Value::Null);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, Value::Null);
        assert_eq!(events[0].old_value, Value::Str(String::new()));
    }

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = InputText::definition();
        assert_eq!(def.tag_name, "input-text");
        assert_eq!(def.shared_style_id, Some("input-default"));
        assert!(def.shared_scss.is_some());
        assert!(def.wraps_src.is_some());

        let ty = def.property("type").expect("type property");
        assert_eq!(ty.js_name, "type");
        assert_eq!(ty.default, uic_core::DefaultValue::Str("text"));
        let autocomplete = def.property("autocomplete").expect("autocomplete");
        assert_eq!(
            autocomplete.default,
            uic_core::DefaultValue::Str("one-time-code")
        );
        let allow_null = def.property("allow_null").expect("allow_null");
        assert_eq!(allow_null.attribute, Some("allow-null"));

        // The chrome supplies label/hint/error; no computed properties needed.
        assert!(def.computed.is_empty());
        let template = def.template();
        assert!(template.referenced_idents().contains("label"));
    }
}
