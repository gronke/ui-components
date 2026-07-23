//! `<input-textarea>` — multi-line text input with CSS-driven auto-grow
//! (`field-sizing: content` up to `max-lines`), in the shared input chrome.
//!
//! The commit behavior matches `<input-text>` (trim; empty becomes null with
//! `allow-null`); the trim logic is duplicated rather than inherited — the
//! component model is flat, unlike the catalog's mixin chain.
//! In the terminal, Enter inserts a newline and Tab (focus leave) commits,
//! matching the browser's `@change`-on-blur.

use uic_core::{input_shared, Ctx, CustomElement, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-textarea",
    template_file = "textarea.html",
    scss_file = "textarea.scss",
    web_impl_file = "textarea.impl.ts"
)]
pub struct InputTextarea {
    /// Committed text, trimmed; null for empty commits with `allow-null`.
    #[property(notify, default = "")]
    pub value: Option<String>,
    /// An empty commit becomes null instead of the empty string.
    #[property(reflect)]
    pub allow_null: bool,
    #[property(default = "")]
    pub placeholder: String,
    /// Password managers stay out of free-text fields.
    #[property(reflect, default = "one-time-code")]
    pub autocomplete: String,
    /// Auto-grow limit in lines (CSS `--textarea-max-lines`).
    #[property(default = 10)]
    pub max_lines: f64,
}

impl InputTextareaLogic for InputTextarea {
    /// Mirrored for the browser in `textarea.impl.ts` — keep both in sync.
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let raw = event.target_value.as_deref().unwrap_or("");
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

    fn commit(input: &str, allow_null: bool) -> (PropertyStore, Vec<NotifyEvent>) {
        let (mut store, mut behavior) = setup(InputTextarea::definition());
        if allow_null {
            store.set("allow_null", true);
        }
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(input))
        });
        (store, events)
    }

    #[test]
    fn commits_trimmed_multiline_text() {
        let (store, events) = commit("  line one\nline two  ", false);
        assert_eq!(store.get("value"), &Value::Str("line one\nline two".into()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
    }

    #[test]
    fn empty_commit_is_empty_string_or_null() {
        let (store, _) = commit("   ", false);
        assert_eq!(store.get("value"), &Value::Str(String::new()));

        let (store, _) = commit("   ", true);
        assert_eq!(store.get("value"), &Value::Null);
    }

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = InputTextarea::definition();
        assert_eq!(def.tag_name, "input-textarea");
        let max_lines = def.property("max_lines").expect("max_lines");
        assert_eq!(max_lines.attribute, Some("max-lines"));
        assert_eq!(max_lines.default, uic_core::DefaultValue::Num(10.0));
        let autocomplete = def.property("autocomplete").expect("autocomplete");
        assert_eq!(
            autocomplete.default,
            uic_core::DefaultValue::Str("one-time-code")
        );
    }
}
