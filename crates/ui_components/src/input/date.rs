//! `<input-date>` — date input (`YYYY-MM-DD`) with the Schuhkarton input
//! chrome (label, input group, hint/error line below).

use chrono::NaiveDate;
use uic_core::{Ctx, CustomElement, PropertyStore, UiEvent, Value};

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-date",
    template_file = "date.mhtml",
    scss_file = "date.scss",
    web_impl_file = "date.impl.ts"
)]
pub struct InputDate {
    /// Committed value, `YYYY-MM-DD` or empty.
    #[property(notify)]
    pub value: String,
    /// Label rendered above the input.
    #[property]
    pub label: Option<String>,
    /// Hint rendered below the input while there is no error.
    #[property]
    pub hint: Option<String>,
    /// Validation error rendered below the input.
    #[property]
    pub error_message: Option<String>,
    #[property(reflect)]
    pub disabled: bool,
    /// Earliest accepted date, `YYYY-MM-DD`.
    #[property]
    pub min: Option<String>,
    /// Latest accepted date, `YYYY-MM-DD`.
    #[property]
    pub max: Option<String>,
    /// Placeholder override; defaults to the date format.
    #[property]
    pub placeholder: Option<String>,
}

fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()
}

impl InputDateLogic for InputDate {
    fn placeholder_text(&self, store: &PropertyStore) -> Value {
        match store.get("placeholder") {
            Value::Str(s) if !s.is_empty() => s.clone().into(),
            _ => "YYYY-MM-DD".into(),
        }
    }

    /// Mirrored for the browser in `date.impl.ts` — keep both in sync.
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let raw = event
            .target_value
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();
        if raw.is_empty() {
            ctx.set("value", "");
            ctx.set("error_message", Value::Undefined);
            return;
        }
        let Some(date) = parse_date(&raw) else {
            ctx.set("error_message", format!("Invalid date: {raw}"));
            return;
        };
        let min = ctx.get("min").as_str().map(str::to_string);
        if let Some(min) = min {
            if parse_date(&min).is_some_and(|m| date < m) {
                ctx.set("error_message", format!("Date before minimum {min}"));
                return;
            }
        }
        let max = ctx.get("max").as_str().map(str::to_string);
        if let Some(max) = max {
            if parse_date(&max).is_some_and(|m| date > m) {
                ctx.set("error_message", format!("Date after maximum {max}"));
                return;
            }
        }
        // Normalized (zero-padded) form, whatever spelling was typed.
        ctx.set("value", date.format("%Y-%m-%d").to_string());
        ctx.set("error_message", Value::Undefined);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::{notify_events, Changed};

    fn commit(input: &str) -> (PropertyStore, Changed) {
        let def = InputDate::definition();
        let mut behavior = (def.new_behavior)();
        let mut store = PropertyStore::new(def.properties);
        let mut changed = Changed::default();
        let mut ctx = Ctx::new(&mut store, &mut changed);
        behavior.handle(&mut ctx, "on_change", &UiEvent::change(input));
        (store, changed)
    }

    #[test]
    fn valid_input_normalizes_and_notifies() {
        let def = InputDate::definition();
        let (store, changed) = commit("2026-8-1");
        assert_eq!(store.get("value"), &Value::Str("2026-08-01".into()));
        assert_eq!(store.get("error_message"), &Value::Undefined);
        let events = notify_events(def, &changed, &store);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
        assert_eq!(events[0].value, Value::Str("2026-08-01".into()));
    }

    #[test]
    fn invalid_input_sets_error_and_keeps_value() {
        let def = InputDate::definition();
        let (store, changed) = commit("2026-13-99");
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        assert_eq!(
            store.get("error_message"),
            &Value::Str("Invalid date: 2026-13-99".into())
        );
        assert!(notify_events(def, &changed, &store).is_empty());
    }

    #[test]
    fn empty_input_clears_value_and_error() {
        let (store, _) = commit("  ");
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        assert_eq!(store.get("error_message"), &Value::Undefined);
    }

    #[test]
    fn min_max_are_enforced() {
        let def = InputDate::definition();
        let mut behavior = (def.new_behavior)();
        let mut store = PropertyStore::new(def.properties);
        store.set("min", "2026-01-01");
        store.set("max", "2026-12-31");
        let mut changed = Changed::default();
        let mut ctx = Ctx::new(&mut store, &mut changed);
        behavior.handle(&mut ctx, "on_change", &UiEvent::change("2025-12-31"));
        assert_eq!(
            ctx.get("error_message"),
            &Value::Str("Date before minimum 2026-01-01".into())
        );
        behavior.handle(&mut ctx, "on_change", &UiEvent::change("2027-01-01"));
        assert_eq!(
            ctx.get("error_message"),
            &Value::Str("Date after maximum 2026-12-31".into())
        );
        behavior.handle(&mut ctx, "on_change", &UiEvent::change("2026-06-15"));
        assert_eq!(ctx.get("error_message"), &Value::Undefined);
        assert_eq!(ctx.get("value"), &Value::Str("2026-06-15".into()));
    }

    #[test]
    fn placeholder_computed_prefers_the_property() {
        let def = InputDate::definition();
        let behavior = (def.new_behavior)();
        let mut store = PropertyStore::new(def.properties);
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD".into())
        );
        store.set("placeholder", "when?");
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("when?".into())
        );
    }

    #[test]
    fn definition_carries_the_co_located_assets() {
        let def = InputDate::definition();
        assert_eq!(def.tag_name, "input-date");
        assert!(def.scss.expect("scss").contains(".el-input-date"));
        assert!(def
            .web_impl
            .expect("impl")
            .contains("export function onChange"));
        assert_eq!(def.computed, &["placeholder_text"]);
    }
}
