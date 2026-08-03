//! `<input-date>`: date (and optionally time) input with the shared input
//! chrome (label, input group, hint/error line below).
//!
//! Temporal parity with the source catalog: partial input auto-completes to
//! the start of its period (`2024` → `2024-01-01 00:00:00`), `hide-time`
//! and `hide-seconds` pick the value format, and next to the `value` string
//! the element carries a `date` object property (`Temporal.ZonedDateTime`
//! in the browser, [`Zoned`] here). The committed instant is interpreted in
//! `timezone ?? default_timezone ?? "UTC"` and STORED normalized to UTC:
//! the zones exist to read input and display output, the date itself is
//! absolute. Timezone-only changes deliberately re-derive nothing (catalog
//! behavior); `end-of` commits the end of the typed period instead of its
//! start (the range's end field).

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use uic_core::{input_shared, Changed, Ctx, CustomElement, PropertyStore, UiEvent, Value, Zoned};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-date",
    template_file = "date.html",
    scss_file = "date.scss",
    web_impl_file = "date.impl.ts"
)]
pub struct InputDate {
    /// Committed value in the variant's format
    /// (`YYYY-MM-DD[ HH:mm[:ss]]`) or empty.
    #[property(notify)]
    pub value: String,
    /// The committed instant, normalized to UTC;
    /// `Temporal.ZonedDateTime | null` in the browser.
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
    /// Placeholder override; defaults to the variant's format.
    #[property]
    pub placeholder: Option<String>,
    /// Renders the timezone select inline after the date input.
    #[property(reflect)]
    pub show_timezone: bool,
    /// Hides the time: the value carries the date only.
    #[property(reflect)]
    pub hide_time: bool,
    /// Hides the seconds from the time.
    #[property(reflect)]
    pub hide_seconds: bool,
    /// Commits the END of the typed period instead of its start: the
    /// range's end field (`2024` → `2024-12-31 23:59:59`... at the
    /// variant's precision).
    #[property(reflect)]
    pub end_of: bool,
}

/// The catalog's `parseDate`: a 1900–2099 year, an optionally dash-joined
/// 1–2 digit month and day, an optional space or `T`, and optionally
/// colon-joined 1–2 digit time parts; every separator is optional, so
/// compact forms (`20240305`, `2024030514`) parse too. Missing parts
/// complete to the start of the period (month 1, day 1, midnight);
/// out-of-range parts clamp (the catalog's Temporal `constrain`); the
/// first unrecognized character drops itself and everything after it
/// (`2024/03` → `2024-01-01 00:00:00`).
fn parse_partial(raw: &str) -> Option<NaiveDateTime> {
    let bytes = raw.as_bytes();
    if bytes.len() < 4 || !bytes[..4].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let year: i32 = raw[..4].parse().ok()?;
    if !(1900..=2099).contains(&year) {
        return None;
    }

    // One optional separator from the set, then up to two digits; an
    // absent part stays None while later parts may still match, exactly
    // the catalog regex's independently optional groups.
    let mut pos = 4;
    let mut part = |seps: &[u8]| -> Option<u32> {
        if pos < bytes.len() && seps.contains(&bytes[pos]) {
            pos += 1;
        }
        let start = pos;
        while pos < bytes.len() && pos - start < 2 && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        (pos > start).then(|| raw[start..pos].parse().expect("digits"))
    };
    let month = part(b"-");
    let day = part(b"-");
    let hour = part(b" T");
    let minute = part(b":");
    let second = part(b":");

    let month = month.unwrap_or(1).clamp(1, 12);
    let day = day.unwrap_or(1).clamp(1, days_in_month(year, month));
    let hour = hour.unwrap_or(0).min(23);
    let minute = minute.unwrap_or(0).min(59);
    let second = second.unwrap_or(0).min(59);
    Some(
        NaiveDate::from_ymd_opt(year, month, day)
            .expect("clamped into range")
            .and_hms_opt(hour, minute, second)
            .expect("clamped into range"),
    )
}

fn days_in_month(year: i32, month: u32) -> u32 {
    (28..=31)
        .rev()
        .find(|&day| NaiveDate::from_ymd_opt(year, month, day).is_some())
        .expect("every month has 28 days")
}

/// The value format of the variant (the catalog's `format` getter).
fn value_pattern(hide_time: bool, hide_seconds: bool) -> &'static str {
    if hide_time {
        "%Y-%m-%d"
    } else if hide_seconds {
        "%Y-%m-%d %H:%M"
    } else {
        "%Y-%m-%d %H:%M:%S"
    }
}

/// Snaps the completed instant to the variant's precision, at the start or
/// the end of the period (the catalog's `_reduceDatePrecision`).
fn reduce_precision(
    local: NaiveDateTime,
    hide_time: bool,
    hide_seconds: bool,
    end_of: bool,
) -> NaiveDateTime {
    if hide_time {
        let time = if end_of {
            NaiveTime::from_hms_opt(23, 59, 59)
        } else {
            NaiveTime::from_hms_opt(0, 0, 0)
        };
        local.date().and_time(time.expect("valid time"))
    } else if hide_seconds {
        let second = if end_of { 59 } else { 0 };
        local
            .date()
            .and_hms_opt(local.time().hour(), local.time().minute(), second)
            .expect("valid time")
    } else {
        local
    }
}

/// `timezone ?? default_timezone ?? UTC`; unknown identifiers fall back to
/// UTC (the browser impl throws on construction instead; both surface the
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

/// The local wall clock in `tz` as a UTC-normalized [`Zoned`], stepping
/// over DST gaps (Temporal's "compatible" disambiguation moves forward
/// too). The stored date is ALWAYS UTC; the zone only interprets the input.
fn zone_local_as_utc(local: NaiveDateTime, tz: Tz) -> Zoned {
    use chrono::offset::LocalResult;
    use chrono::TimeZone;
    let mut naive = local;
    for _ in 0..48 {
        match tz.from_local_datetime(&naive) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => {
                return Zoned::new(dt.with_timezone(&chrono_tz::UTC));
            }
            LocalResult::None => naive += chrono::Duration::minutes(30),
        }
    }
    Zoned::new(tz.from_utc_datetime(&local).with_timezone(&chrono_tz::UTC))
}

use chrono::Timelike;

impl InputDateLogic for InputDate {
    /// The catalog's `defaultPlaceholder`: the variant's format hint (the
    /// minutes token is literally `ii` there), plus the zone a bare date is
    /// interpreted in when the timezone select shows.
    fn placeholder_text(&self, store: &PropertyStore) -> Value {
        let base = match store.get("placeholder") {
            Value::Str(s) if !s.is_empty() => s.clone(),
            _ => {
                let mut base = "YYYY-MM-DD".to_string();
                if !store.get("hide_time").truthy() {
                    base.push_str(" HH:ii");
                    if !store.get("hide_seconds").truthy() {
                        base.push_str(":ss");
                    }
                }
                base
            }
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
    /// `value`; an external `value` write derives the date but keeps the
    /// string as written (partials stay partial); parse failures surface on
    /// the error line (the catalog only logs them). Mirrored for the
    /// browser in `date.impl.ts`.
    fn will_update(&mut self, ctx: &mut Ctx, changed: &Changed) {
        let hide_time = ctx.get("hide_time").truthy();
        let hide_seconds = ctx.get("hide_seconds").truthy();
        if changed.has("date") {
            // The UTC instant renders as the current zone's wall clock.
            let value = match ctx.get("date").as_zoned() {
                Some(zoned) => {
                    let tz = current_timezone(ctx);
                    zoned
                        .datetime()
                        .with_timezone(&tz)
                        .naive_local()
                        .format(value_pattern(hide_time, hide_seconds))
                        .to_string()
                }
                None => String::new(),
            };
            ctx.set("value", value);
        } else if changed.has("value") {
            let raw = ctx.text("value");
            if raw.is_empty() {
                ctx.set("date", Value::Null);
                ctx.set("error_message", Value::Undefined);
                ctx.set("error", false);
            } else if let Some(local) = parse_partial(&raw) {
                let end_of = ctx.get("end_of").truthy();
                let local = reduce_precision(local, hide_time, hide_seconds, end_of);
                let tz = current_timezone(ctx);
                ctx.set("date", zone_local_as_utc(local, tz));
                ctx.set("error_message", Value::Undefined);
                ctx.set("error", false);
            } else {
                ctx.set("error_message", format!("Invalid date: {raw}"));
                ctx.set("error", true);
            }
        }
    }

    /// Typed commits auto-complete (`2024` → `2024-01-01 00:00:00`) and
    /// echo the normalized string. Mirrored for the browser in
    /// `date.impl.ts`; keep both in sync.
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
        let Some(local) = parse_partial(&raw) else {
            ctx.set("error_message", format!("Invalid date: {raw}"));
            ctx.set("error", true);
            return;
        };
        let hide_time = ctx.get("hide_time").truthy();
        let hide_seconds = ctx.get("hide_seconds").truthy();
        let end_of = ctx.get("end_of").truthy();
        let local = reduce_precision(local, hide_time, hide_seconds, end_of);
        let date = local.date();
        let min = ctx.get("min").as_str().map(str::to_string);
        if let Some(min) = min {
            if NaiveDate::parse_from_str(&min, "%Y-%m-%d").is_ok_and(|m| date < m) {
                ctx.set("error_message", format!("Date before minimum {min}"));
                ctx.set("error", true);
                return;
            }
        }
        let max = ctx.get("max").as_str().map(str::to_string);
        if let Some(max) = max {
            if NaiveDate::parse_from_str(&max, "%Y-%m-%d").is_ok_and(|m| date > m) {
                ctx.set("error_message", format!("Date after maximum {max}"));
                ctx.set("error", true);
                return;
            }
        }
        // The completed, zero-padded form, whatever spelling was typed; the
        // zoned date pins the UTC instant of that wall clock.
        ctx.set(
            "value",
            local
                .format(value_pattern(hide_time, hide_seconds))
                .to_string(),
        );
        let tz = current_timezone(ctx);
        ctx.set("date", zone_local_as_utc(local, tz));
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

    /// The catalog's parseDate vectors (date.test.js).
    #[test]
    fn partial_input_completes_to_the_period_start() {
        let dt = |s: &str| parse_partial(s).map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string());
        assert_eq!(dt("2024"), Some("2024-01-01 00:00:00".into()));
        assert_eq!(dt("2024-06"), Some("2024-06-01 00:00:00".into()));
        assert_eq!(dt("2024-3-5"), Some("2024-03-05 00:00:00".into()));
        assert_eq!(dt("2024-01-15 10"), Some("2024-01-15 10:00:00".into()));
        assert_eq!(dt("2024-01-15 10:30"), Some("2024-01-15 10:30:00".into()));
        assert_eq!(
            dt("2024-01-15T10:30:45"),
            Some("2024-01-15 10:30:45".into())
        );
        // Compact forms: the separators are optional.
        assert_eq!(dt("20240305"), Some("2024-03-05 00:00:00".into()));
        assert_eq!(dt("2024030514"), Some("2024-03-05 14:00:00".into()));
        // The first unrecognized character drops itself and the rest.
        assert_eq!(dt("2024/03"), Some("2024-01-01 00:00:00".into()));
        // A skipped part still lets later parts match (the regex's
        // independently optional groups).
        assert_eq!(dt("2024--05"), Some("2024-01-05 00:00:00".into()));
        assert_eq!(dt("2024T10"), Some("2024-01-01 10:00:00".into()));
        // Out of range clamps instead of rejecting (Temporal constrain).
        assert_eq!(
            dt("2024-13-45 25:70:70"),
            Some("2024-12-31 23:59:59".into())
        );
        // Outside the 1900–2099 window, or no leading year: invalid.
        assert_eq!(dt("1899"), None);
        assert_eq!(dt("2100"), None);
        assert_eq!(dt("abc"), None);
        assert_eq!(dt("202"), None);
    }

    #[test]
    fn typed_commit_completes_and_normalizes_utc() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        let events = commit(&mut store, &mut behavior, "2024");
        assert_eq!(
            store.get("value"),
            &Value::Str("2024-01-01 00:00:00".into())
        );
        let date = store.get("date").as_zoned().expect("zoned date");
        assert_eq!(date.iso(), "2024-01-01T00:00:00+00:00[UTC]");
        let names: Vec<_> = events.iter().map(|e| e.event_name.as_str()).collect();
        assert!(names.contains(&"value-changed"), "events: {names:?}");
        assert!(names.contains(&"date-changed"), "events: {names:?}");
    }

    #[test]
    fn the_variants_pick_the_value_format() {
        // hide-time: date only, like the old port.
        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("hide_time", true);
        commit(&mut store, &mut behavior, "2026-8-1");
        assert_eq!(store.get("value"), &Value::Str("2026-08-01".into()));

        // hide-seconds: minutes precision.
        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("hide_seconds", true);
        commit(&mut store, &mut behavior, "2026-08-01 10:30:45");
        assert_eq!(store.get("value"), &Value::Str("2026-08-01 10:30".into()));
    }

    #[test]
    fn end_of_commits_the_period_end() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("hide_time", true);
        store.set("end_of", true);
        commit(&mut store, &mut behavior, "2026-08-01");
        // The value stays the date; the instant pins the day's end.
        assert_eq!(store.get("value"), &Value::Str("2026-08-01".into()));
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2026-08-01T23:59:59+00:00[UTC]"
        );

        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("hide_seconds", true);
        store.set("end_of", true);
        commit(&mut store, &mut behavior, "2026-08-01 10:30");
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2026-08-01T10:30:59+00:00[UTC]"
        );
    }

    #[test]
    fn the_zone_interprets_input_but_the_date_stores_utc() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("default_timezone", "Europe/Berlin");
        commit(&mut store, &mut behavior, "2026-07-07");
        // Berlin midnight in summer is 22:00 UTC the previous day.
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2026-07-06T22:00:00+00:00[UTC]"
        );
        // The value renders the wall clock of the current zone.
        assert_eq!(
            store.get("value"),
            &Value::Str("2026-07-07 00:00:00".into())
        );

        // An explicit timezone wins over the default.
        store.set("timezone", "America/New_York");
        commit(&mut store, &mut behavior, "2026-07-08");
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2026-07-08T04:00:00+00:00[UTC]"
        );
    }

    #[test]
    fn external_value_write_derives_the_date_but_keeps_the_string() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("value", "2024");
        });
        // The catalog completes the date, not the written string.
        assert_eq!(store.get("value"), &Value::Str("2024".into()));
        assert_eq!(
            store.get("date").as_zoned().expect("zoned").iso(),
            "2024-01-01T00:00:00+00:00[UTC]"
        );

        // Clearing the value nulls the date.
        cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("value", "");
        });
        assert_eq!(store.get("date"), &Value::Null);
    }

    #[test]
    fn external_date_write_derives_the_value_in_the_current_zone() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        store.set("default_timezone", "Europe/Berlin");
        let zoned = zone_local_as_utc(
            NaiveDate::from_ymd_opt(2026, 7, 9)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            chrono_tz::Europe::Berlin,
        );
        let events = cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("date", zoned.clone());
        });
        assert_eq!(
            store.get("value"),
            &Value::Str("2026-07-09 00:00:00".into()),
            "the UTC instant renders as the zone's wall clock"
        );
        let names: Vec<_> = events.iter().map(|e| e.event_name.as_str()).collect();
        assert!(names.contains(&"value-changed"));
        assert!(names.contains(&"date-changed"));
    }

    #[test]
    fn timezone_only_change_is_inert() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        commit(&mut store, &mut behavior, "2026-07-07");
        let before = store.get("date").clone();
        let value_before = store.get("value").clone();
        let events = cycle(&mut store, &mut behavior, |_, ctx| {
            ctx.set("timezone", "Europe/Berlin");
        });
        assert_eq!(store.get("date"), &before, "date not re-derived");
        assert_eq!(store.get("value"), &value_before);
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
        // the identical value string; nothing oscillates.
        let (mut store, mut behavior) = setup(InputDate::definition());
        let events = commit(&mut store, &mut behavior, "2026-07-07");
        assert_eq!(
            store.get("value"),
            &Value::Str("2026-07-07 00:00:00".into())
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event_name == "value-changed")
                .count(),
            1
        );
    }

    #[test]
    fn unparseable_input_sets_error_and_keeps_value() {
        let (mut store, mut behavior) = setup(InputDate::definition());
        let events = commit(&mut store, &mut behavior, "next tuesday");
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        assert_eq!(
            store.get("error_message"),
            &Value::Str("Invalid date: next tuesday".into())
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
        assert_eq!(
            store.get("value"),
            &Value::Str("2026-06-15 00:00:00".into())
        );
    }

    #[test]
    fn the_placeholder_follows_the_variant() {
        let def = InputDate::definition();
        let behavior = (def.new_behavior)();
        let mut store = PropertyStore::new(def.properties);
        // The catalog's minutes token is literally `ii`.
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD HH:ii:ss".into())
        );
        store.set("hide_seconds", true);
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD HH:ii".into())
        );
        store.set("hide_time", true);
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD".into())
        );
        store.set("show_timezone", true);
        store.set("default_timezone", "Europe/Berlin");
        assert_eq!(
            behavior.compute(&store, "placeholder_text"),
            Value::Str("YYYY-MM-DD · Europe/Berlin".into())
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
            dataset: Default::default(),
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
            dataset: Default::default(),
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
        let hide_time = def.property("hide_time").expect("hide_time");
        assert_eq!(hide_time.attribute, Some("hide-time"));
        assert!(hide_time.reflect);
        let end_of = def.property("end_of").expect("end_of");
        assert_eq!(end_of.attribute, Some("end-of"));
    }
}
