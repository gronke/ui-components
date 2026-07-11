//! `<input-date-range>` — one element around two `<input-date>` children.
//!
//! The composite listens to both children's `value-changed` events, keeps
//! the ends ordered and derives the combined `value` in `will_update`, and
//! reflects `complete` after the commit in `updated` — the ReactiveElement
//! flow, identical on both render targets.

use uic_core::{input_shared, Changed, Ctx, CustomElement, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-date-range",
    template_file = "date_range.mhtml",
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
    fn connected(&mut self, ctx: &mut Ctx) {
        // The children draw their own borders; the shared chrome's group
        // renders borderless around them.
        ctx.set("seamless", true);
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
