//! `<input-date>` — date input (`YYYY-MM-DD`) with the shared input chrome
//! (label, input group, hint/error line below).
//!
//! Temporal parity with the source catalog: next to the `value` string the
//! element carries a `date` object property (`Temporal.ZonedDateTime` in the
//! browser, [`Zoned`] here), kept in sync during `will_update` — a `date`
//! change wins, otherwise `value` is parsed as start of day in
//! `timezone ?? default_timezone ?? "UTC"`.
//! Timezone-only changes deliberately re-derive nothing (catalog behavior).

use chrono::NaiveDate;
use chrono_tz::Tz;
use uic_core::{input_shared, Changed, Ctx, CustomElement, PropertyStore, UiEvent, Value, Zoned};

#[input_shared]
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
    /// The committed date as a zoned timestamp (start of day in the
    /// current timezone); `Temporal.ZonedDateTime | null` in the browser.
    #[property(notify)]
    pub date: Option<Zoned>,
    /// IANA timezone override, e.g. `Europe/Berlin`.
    #[property(notify)]
    pub timezone: Option<String>,
    /// Fallback timezone when `timezone` is unset.
    #[property]
    pub default_timezone: Option<String>,
    /// Earliest accepted date, `YYYY-MM-DD`.
    #[property]
    pub min: Option<String>,
    /// Latest accepted date, `YYYY-MM-DD`.
    #[property]
    pub max: Option<String>,
    /// Placeholder override; defaults to the date format.
    #[property]
    pub placeholder: Option<String>,
    /// Renders the timezone select inline after the date input.
    #[property(reflect)]
    pub show_timezone: bool,
}

fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()
}

/// `timezone ?? default_timezone ?? UTC`; unknown identifiers fall back to
/// UTC (the browser impl throws on construction instead — both surface the
/// misconfiguration without breaking the value string).
fn current_timezone(ctx: &Ctx) -> Tz {
    for prop in ["timezone", "default_timezone"] {
        if let Value::Str(id) = ctx.get(prop) {
            if !id.is_empty() {
                return id.parse().unwrap_or(chrono_tz::UTC);
            }
        }
    }
    chrono_tz::UTC
}

/// The current timezone's display name, same fallback chain as
/// [`current_timezone`] but without the chrono parse (the placeholder shows
/// whatever identifier is configured).
fn timezone_name(store: &PropertyStore) -> String {
    for prop in ["timezone", "default_timezone"] {
        if let Value::Str(id) = store.get(prop) {
            if !id.is_empty() {
                return id.clone();
            }
        }
    }
    "UTC".to_string()
}

/// Start of day in `tz`, stepping over DST gaps where midnight is skipped
/// (Temporal's "compatible" disambiguation moves forward too).
fn start_of_day(date: NaiveDate, tz: Tz) -> Zoned {
    use chrono::offset::LocalResult;
    use chrono::TimeZone;
    let mut naive = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
    for _ in 0..48 {
        match tz.from_local_datetime(&naive) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => return Zoned::new(dt),
            LocalResult::None => naive += chrono::Duration::minutes(30),
        }
    }
    Zoned::new(tz.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight is valid")))
}

impl InputDateLogic for InputDate {
    /// With the timezone select shown, the placeholder hints at the zone a
    /// bare date is interpreted in (the catalog's `defaultPlaceholder`).
    fn placeholder_text(&self, store: &PropertyStore) -> Value {
        let base = match store.get("placeholder") {
            Value::Str(s) if !s.is_empty() => s.clone(),
            _ => "YYYY-MM-DD".to_string(),
        };
        if store.get("show_timezone").truthy() {
            format!("{base} · {}", timezone_name(store)).into()
        } else {
            base.into()
        }
    }

    /// The embedded timezone select's `default` binding: the catalog passes
    /// `defaultTimezone ?? ""` so its null option always exists.
    fn timezone_default(&self, store: &PropertyStore) -> Value {
        match store.get("default_timezone") {
            Value::Str(id) => Value::Str(id.clone()),
            _ => Value::Str(String::new()),
        }
    }

    /// Routes the embedded select's `value-changed` into the `timezone`
    /// property (the catalog binds it via `LitSync`).
    fn on_timezone_changed(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("timezone", event.detail.clone().unwrap_or(Value::Null));
    }

    /// Port of the catalog's `onUpdateDateOrTimezone`: `date` wins over
    /// `value`; parse failures surface on the error line (the catalog only
    /// logs them). Mirrored for the browser in `date.impl.ts`.
    fn will_update(&mut self, ctx: &mut Ctx, changed: &Changed) {
        if changed.has("date") {
            let value = match ctx.get("date").as_zoned() {
                Some(zoned) => zoned.date_naive().format("%Y-%m-%d").to_string(),
                None => String::new(),
            };
            ctx.set("value", value);
        } else if changed.has("value") {
            let raw = ctx.get("value").as_str().unwrap_or("").to_string();
            if raw.is_empty() {
                ctx.set("date", Value::Null);
                ctx.set("error_message", Value::Undefined);
                ctx.set("error", false);
            } else if let Some(date) = parse_date(&raw) {
                let tz = current_timezone(ctx);
                ctx.set("date", start_of_day(date, tz));
                ctx.set("error_message", Value::Undefined);
                ctx.set("error", false);
            } else {
                ctx.set("error_message", format!("Invalid date: {raw}"));
                ctx.set("error", true);
            }
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
            ctx.set("date", Value::Null);
            ctx.set("error_message", Value::Undefined);
            ctx.set("error", false);
            return;
        }
        let Some(date) = parse_date(&raw) else {
            ctx.set("error_message", format!("Invalid date: {raw}"));
            ctx.set("error", true);
            return;
        };
        let min = ctx.get("min").as_str().map(str::to_string);
        if let Some(min) = min {
            if parse_date(&min).is_some_and(|m| date < m) {
                ctx.set("error_message", format!("Date before minimum {min}"));
                ctx.set("error", true);
                return;
            }
        }
        let max = ctx.get("max").as_str().map(str::to_string);
        if let Some(max) = max {
            if parse_date(&max).is_some_and(|m| date > m) {
                ctx.set("error_message", format!("Date after maximum {max}"));
                ctx.set("error", true);
                return;
            }
        }
        // Normalized (zero-padded) form, whatever spelling was typed; the
        // zoned date pins start of day in the current timezone.
        ctx.set("value", date.format("%Y-%m-%d").to_string());
        let tz = current_timezone(ctx);
        ctx.set("date", start_of_day(date, tz));
        ctx.set("error_message", Value::Undefined);
        ctx.set("error", false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::testing::{cycle, setup};

    fn commit(
        store: &mut PropertyStore,
        behavior: &mut Box<dyn uic_core::Behavior>,
        input: &str,
    ) -> Vec<uic_core::NotifyEvent> {
        cycle(store, behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(input))
        })
    }

    #[test]
    fn valid_commit_sets_value_and_date_and_notifies_both() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        let events = commit(&mut store, &mut behavior, "2026-8-1");
        assert_eq!(store.get("value"), &Value::Str("2026-08-01".into()));
        assert_eq!(store.get("error"), &Value::Bool(false));
        let date = store.get("date").as_zoned().expect("zoned date");
        assert_eq!(date.iso(), "2026-08-01T00:00:00+00:00[UTC]");

        let names: Vec<_> = events.iter().map(|e| e.event_name.as_str()).collect();
        assert!(names.contains(&"value-changed"), "events: {names:?}");
        assert!(names.contains(&"date-changed"), "events: {names:?}");
    }

    #[test]
    fn timezone_fallback_chain_applies() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("default_timezone", "Europe/Berlin");
        commit(&mut store, &mut behavior, "2026-07-07");
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2026-07-07T00:00:00+02:00[Europe/Berlin]"
        );

        // An explicit timezone wins over the default.
        store.set("timezone", "America/New_York");
        commit(&mut store, &mut behavior, "2026-07-08");
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2026-07-08T00:00:00-04:00[America/New_York]"
        );
    }

    #[test]
    fn external_value_write_derives_the_date() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("value", "2026-07-07");
        });
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2026-07-07T00:00:00+00:00[UTC]"
        );

        // Clearing the value nulls the date.
        cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("value", "");
        });
        assert_eq!(store.get("date"), &Value::Null);
    }

    #[test]
    fn external_date_write_derives_the_value() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        let zoned = start_of_day(NaiveDate::from_ymd_opt(2026, 7, 9).unwrap(), chrono_tz::UTC);
        let events = cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("date", zoned.clone());
        });
        assert_eq!(store.get("value"), &Value::Str("2026-07-09".into()));
        let names: Vec<_> = events.iter().map(|e| e.event_name.as_str()).collect();
        assert!(names.contains(&"value-changed"));
        assert!(names.contains(&"date-changed"));
    }

    #[test]
    fn timezone_only_change_is_inert() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        commit(&mut store, &mut behavior, "2026-07-07");
        let before = store.get("date").clone();
        let events = cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("timezone", "Europe/Berlin");
        });
        assert_eq!(store.get("date"), &before, "date not re-derived");
        assert_eq!(store.get("value"), &Value::Str("2026-07-07".into()));
        let names: Vec<_> = events.iter().map(|e| e.event_name.as_str()).collect();
        assert_eq!(names, vec!["timezone-changed"]);
    }

    #[test]
    fn invalid_external_value_sets_the_error_message() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("value", "not-a-date");
        });
        assert_eq!(
            store.get("error_message"),
            &Value::Str("Invalid date: not-a-date".into())
        );
        assert_eq!(store.get("error"), &Value::Bool(true));
        assert_eq!(store.get("date"), &Value::Undefined, "date untouched");

        // A subsequent valid external write clears the error state.
        cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("value", "2026-07-07");
        });
        assert_eq!(store.get("error_message"), &Value::Undefined);
        assert_eq!(store.get("error"), &Value::Bool(false));
    }

    #[test]
    fn commit_batch_is_echo_free() {
        // on_change sets value AND date; will_update prefers date and derives
        // the identical value string — nothing oscillates.
        let (mut store, mut behavior) = setup(InputDate::definition());
        let events = commit(&mut store, &mut behavior, "2026-07-07");
        assert_eq!(store.get("value"), &Value::Str("2026-07-07".into()));
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event_name == "value-changed")
                .count(),
            1
        );
    }

    #[test]
    fn invalid_input_sets_error_and_keeps_value() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        let events = commit(&mut store, &mut behavior, "2026-13-99");
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        assert_eq!(
            store.get("error_message"),
            &Value::Str("Invalid date: 2026-13-99".into())
        );
        assert_eq!(store.get("error"), &Value::Bool(true));
        assert!(events.is_empty());
    }

    #[test]
    fn empty_input_clears_value_date_and_error() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        commit(&mut store, &mut behavior, "2026-07-07");
        commit(&mut store, &mut behavior, "  ");
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        assert_eq!(store.get("date"), &Value::Null);
        assert_eq!(store.get("error_message"), &Value::Undefined);
        assert_eq!(store.get("error"), &Value::Bool(false));
    }

    #[test]
    fn valid_commit_clears_a_previous_error() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        commit(&mut store, &mut behavior, "nonsense");
        assert_eq!(store.get("error"), &Value::Bool(true));
        commit(&mut store, &mut behavior, "2026-06-15");
        assert_eq!(store.get("error"), &Value::Bool(false));
        assert_eq!(store.get("error_message"), &Value::Undefined);
    }

    #[test]
    fn min_max_are_enforced() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("min", "2026-01-01");
        store.set("max", "2026-12-31");
        commit(&mut store, &mut behavior, "2025-12-31");
        assert_eq!(
            store.get("error_message"),
            &Value::Str("Date before minimum 2026-01-01".into())
        );
        commit(&mut store, &mut behavior, "2027-01-01");
        assert_eq!(
            store.get("error_message"),
            &Value::Str("Date after maximum 2026-12-31".into())
        );
        commit(&mut store, &mut behavior, "2026-06-15");
        assert_eq!(store.get("error_message"), &Value::Undefined);
        assert_eq!(store.get("value"), &Value::Str("2026-06-15".into()));
    }

    #[test]
    fn show_timezone_extends_the_placeholder() {
        let def = InputDate::definition();
        let behavior = (def.new_behavior)();
        let mut store = PropertyStore::new(def.properties);
        store.set("show_timezone", true);
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD · UTC".into())
        );
        store.set("default_timezone", "Europe/Berlin");
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD · Europe/Berlin".into())
        );
        store.set("timezone", "America/New_York");
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD · America/New_York".into())
        );
    }

    #[test]
    fn timezone_default_falls_back_to_the_empty_string() {
        let def = InputDate::definition();
        let behavior = (def.new_behavior)();
        let mut store = PropertyStore::new(def.properties);
        assert_eq!(
            behavior.compute(&store, "timezone_default"),
            Value::Str(String::new())
        );
        store.set("default_timezone", "Europe/Berlin");
        assert_eq!(
            behavior.compute(&store, "timezone_default"),
            Value::Str("Europe/Berlin".into())
        );
    }

    #[test]
    fn timezone_changed_events_route_into_the_property() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        let picked = UiEvent {
            name: "value-changed".to_string(),
            target_value: Some("Europe/Berlin".to_string()),
            detail: Some(Value::Str("Europe/Berlin".into())),
        };
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_timezone_changed", &picked)
        });
        assert_eq!(store.get("timezone"), &Value::Str("Europe/Berlin".into()));
        let names: Vec<_> = events.iter().map(|e| e.event_name.as_str()).collect();
        assert_eq!(names, vec!["timezone-changed"]);

        // The child's null commit clears the timezone.
        let cleared = UiEvent {
            name: "value-changed".to_string(),
            target_value: Some(String::new()),
            detail: Some(Value::Null),
        };
        cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_timezone_changed", &cleared)
        });
        assert_eq!(store.get("timezone"), &Value::Null);
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
        assert_eq!(def.computed, &["placeholder_text", "timezone_default"]);
        let date = def.property("date").expect("date property");
        assert_eq!(date.attribute, None);
        let show = def.property("show_timezone").expect("show_timezone");
        assert_eq!(show.attribute, Some("show-timezone"));
        assert!(show.reflect);
    }
}
