//! `<input-suggestion>` — typeahead text input: the live text leaves through
//! `query-changed` on every keystroke, a host answers by writing matching
//! rows into the `suggestions` property (ADR 0014), a popup shows them, and
//! picking one commits like typed text. Commit semantics follow the text
//! family: trimmed, empty becoming null when `allow-null` is set.
//!
//! The shared template and logic live here, the browser popup in
//! `suggestion.impl.ts`; the terminal popup is the path-mirrored twin in
//! `ui_components_tui` (ADR 0002).

use uic_core::{input_shared, Ctx, CustomElement, SelectOption, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-suggestion",
    template_file = "suggestion.html",
    scss_file = "suggestion.scss",
    web_impl_file = "suggestion.impl.ts"
)]
pub struct InputSuggestion {
    /// Committed value; `allow-null` commits null for empty input.
    #[property(notify, default = "")]
    pub value: Option<String>,
    /// The live text, one write per keystroke — `query-changed` is the
    /// connector hook a host answers by setting `suggestions`.
    #[property(notify, default = "")]
    pub query: String,
    /// The rows the host resolved for the current query (ADR 0014).
    #[property]
    pub suggestions: Vec<SelectOption>,
    /// Commit null instead of the empty string for empty input.
    #[property(reflect)]
    pub allow_null: bool,
    #[property(default = "")]
    pub placeholder: String,
}

impl InputSuggestionLogic for InputSuggestion {
    /// Mirrored for the browser in `suggestion.impl.ts` — keep both in sync.
    fn on_input(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("query", event.target_value.clone().unwrap_or_default());
    }

    /// The text family's commit rule (`input-text`'s), mirrored in
    /// `suggestion.impl.ts`.
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
        let (mut store, mut behavior) = setup(InputSuggestion::definition());
        store.set("allow_null", allow_null);
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(input))
        });
        (store, events)
    }

    #[test]
    fn typing_notifies_the_live_query() {
        let (mut store, mut behavior) = setup(InputSuggestion::definition());
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_input", &UiEvent::input("gl"))
        });
        assert_eq!(store.get("query"), &Value::Str("gl".into()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "query-changed");
        assert_eq!(events[0].value, Value::Str("gl".into()));
    }

    #[test]
    fn commits_trimmed_text_and_notifies() {
        let (store, events) = commit(false, "  glacier  ");
        assert_eq!(store.get("value"), &Value::Str("glacier".into()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
    }

    #[test]
    fn allow_null_commits_null_for_empty_input() {
        let (store, events) = commit(true, "   ");
        assert_eq!(store.get("value"), &Value::Null);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, Value::Null);
    }

    #[test]
    fn empty_input_commits_the_empty_string_by_default() {
        let (store, events) = commit(false, "   ");
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        // Unchanged from the default: no notify event.
        assert!(events.is_empty());
    }

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = InputSuggestion::definition();
        assert_eq!(def.tag_name, "input-suggestion");
        assert_eq!(def.shared_style_id, Some("input-default"));
        assert!(def.scss.is_some());

        // The rows are data, property-only (ADR 0005).
        let suggestions = def.property("suggestions").expect("suggestions");
        assert_eq!(suggestions.js_type, uic_core::JsType::Options);
        assert_eq!(suggestions.attribute, None);

        let allow_null = def.property("allow_null").expect("allow_null");
        assert_eq!(allow_null.attribute, Some("allow-null"));

        let template = def.template();
        assert!(template.referenced_idents().contains("label"));
        assert!(template.referenced_idents().contains("suggestions"));
    }
}
