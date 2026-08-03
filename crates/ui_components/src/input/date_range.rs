//! `<input-date-range>`: one bordered group around two `<input-date>`
//! children, the catalog's `[ from | - | to | tz ▼ ]`.
//!
//! The composite listens to both children's `value-changed` events, keeps
//! the ends ordered and derives the combined `value` in `will_update`, and
//! reflects `complete` after the commit in `updated`: the ReactiveElement
//! flow, identical on both render targets. The group owns the timezone
//! select; the children receive the picked zone as their default.

use uic_core::{input_shared, Changed, Ctx, CustomElement, PropertyStore, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-date-range",
    template_file = "date_range.html",
    scss_file = "date_range.scss",
    web_impl_file = "date_range.impl.ts"
)]
pub struct InputDateRange {
    /// The committed interval as `start/end` (ISO 8601), empty until both
    /// ends are in.
    #[property(notify)]
    pub value: String,
    /// First day, `YYYY-MM-DD` or empty.
    #[property(notify)]
    pub start: String,
    /// Last day, `YYYY-MM-DD` or empty; never before `start`.
    #[property(notify)]
    pub end: String,
    /// Reflects once both ends are committed; set post-commit in `updated`.
    #[property(reflect)]
    pub complete: bool,
    /// IANA timezone override the group's select committed.
    #[property(notify)]
    pub timezone: Option<String>,
    /// Fallback timezone when `timezone` is unset.
    #[property]
    pub default_timezone: Option<String>,
    /// Renders the timezone select at the group's end.
    #[property(reflect)]
    pub show_timezone: bool,
    /// Hides the time on both ends: the interval carries dates only.
    #[property(reflect)]
    pub hide_time: bool,
    /// Hides the seconds on both ends.
    #[property(reflect)]
    pub hide_seconds: bool,
}

/// Both ends in → the ISO interval; anything less commits empty.
fn interval(start: &str, end: &str) -> String {
    if start.is_empty() || end.is_empty() {
        String::new()
    } else {
        format!("{start}/{end}")
    }
}

impl InputDateRangeLogic for InputDateRange {
    /// The zone the children interpret bare dates in: the picked timezone,
    /// falling back to the default; null clears the children's attribute.
    fn range_timezone(&self, store: &PropertyStore) -> Value {
        for prop in ["timezone", "default_timezone"] {
            if let Value::Str(id) = store.get(prop) {
                if !id.is_empty() {
                    return Value::Str(id.clone());
                }
            }
        }
        Value::Null
    }

    /// The embedded timezone select's `default` binding: the catalog passes
    /// `defaultTimezone ?? ""` so its null option always exists.
    fn timezone_default(&self, store: &PropertyStore) -> Value {
        match store.get("default_timezone") {
            Value::Str(id) => Value::Str(id.clone()),
            _ => Value::Str(String::new()),
        }
    }

    /// Routes the group select's `value-changed` into the `timezone`
    /// property.
    fn on_timezone_changed(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("timezone", event.detail.clone().unwrap_or(Value::Null));
    }

    /// Routed from the start child's `value-changed` binding.
    fn on_start_changed(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("start", event.target_value.clone().unwrap_or_default());
    }

    /// Routed from the end child's `value-changed` binding.
    fn on_end_changed(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("end", event.target_value.clone().unwrap_or_default());
    }

    /// The synchronization: the edited end pulls the other along when the
    /// range would invert (ISO dates order lexicographically), then the
    /// combined `value` derives from the ends; an external `value` write
    /// decomposes instead. Mirrored for the browser in `date_range.impl.ts`.
    fn will_update(&mut self, ctx: &mut Ctx, changed: &Changed) {
        if changed.has("start") || changed.has("end") {
            let start = ctx.text("start");
            let end = ctx.text("end");
            if !start.is_empty() && !end.is_empty() && end < start {
                if changed.has("start") {
                    ctx.set("end", start.clone());
                } else {
                    ctx.set("start", end.clone());
                }
            }
            let value = interval(&ctx.text("start"), &ctx.text("end"));
            ctx.set("value", value);
        } else if changed.has("value") {
            let raw = ctx.text("value");
            let (start, mut end) = match raw.split_once('/') {
                Some((start, end)) => (start.to_string(), end.to_string()),
                None => (String::new(), String::new()),
            };
            if !start.is_empty() && !end.is_empty() && end < start {
                end.clone_from(&start);
            }
            ctx.set("start", start.clone());
            ctx.set("end", end.clone());
            // Normalizes malformed or inverted external writes.
            ctx.set("value", interval(&start, &end));
        }
    }

    /// Post-commit: reflect whether the committed range is complete. The
    /// write requests a follow-up cycle, like setting a reactive property
    /// in Lit's `updated`; the store's equal-write suppression keeps that
    /// follow-up quiet.
    fn updated(&mut self, ctx: &mut Ctx, changed: &Changed) {
        if !(changed.has("start") || changed.has("end") || changed.has("value")) {
            return;
        }
        let complete = !ctx.text("start").is_empty() && !ctx.text("end").is_empty();
        if ctx.get("complete") != &Value::Bool(complete) {
            ctx.set("complete", complete);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::testing::{cycle, setup};

    #[test]
    fn the_group_timezone_falls_back_and_notifies() {
        let (mut store, mut behavior) = setup(InputDateRange::definition());
        assert_eq!(
            behavior.compute(&store, "range_timezone"),
            Value::Null,
            "no zone set, the children's attribute clears"
        );

        store.set("default_timezone", "Europe/Berlin");
        assert_eq!(
            behavior.compute(&store, "range_timezone"),
            Value::Str("Europe/Berlin".into())
        );
        assert_eq!(
            behavior.compute(&store, "timezone_default"),
            Value::Str("Europe/Berlin".into())
        );

        // The select's commit routes into `timezone` and wins the fallback.
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(
                ctx,
                "on_timezone_changed",
                &UiEvent {
                    name: "value-changed".to_string(),
                    target_value: Some("America/New_York".to_string()),
                    detail: Some(Value::Str("America/New_York".into())),
                    dataset: Default::default(),
                },
            )
        });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "timezone-changed");
        assert_eq!(
            behavior.compute(&store, "range_timezone"),
            Value::Str("America/New_York".into())
        );
    }
}
