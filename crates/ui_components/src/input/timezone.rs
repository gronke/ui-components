//! `<input-timezone>` — the IANA time-zone select, a thin catalog subclass
//! of `<input-select>`: it shares the select template and styles (`style =
//! "input-select"`), supplies the zone list as its computed options, and the
//! empty selection always commits null.
//!
//! The two targets keep specialized zone lists on purpose, side by side for
//! comparison: this file iterates chrono-tz, `timezone.impl.ts` asks the
//! browser via `Intl.supportedValuesOf('timeZone')`. Both pin UTC first and
//! shorten to the last path segment (ADR 0003 records the divergence).

use std::sync::LazyLock;

use uic_core::{input_shared, Changed, Ctx, CustomElement, SelectOption, UiEvent, Value};

use super::select::{form_value, front_class, normalize_empty_value, with_default_option};

/// Keep in sync with `timezoneOptions` in `timezone.impl.ts`.
static TIMEZONE_OPTIONS: LazyLock<Vec<SelectOption>> = LazyLock::new(|| {
    std::iter::once("UTC")
        .chain(
            chrono_tz::TZ_VARIANTS
                .iter()
                .map(|tz| tz.name())
                .filter(|name| *name != "UTC"),
        )
        .map(|id| {
            let short = id.rsplit('/').next().unwrap_or(id).trim();
            SelectOption::new(id).with_short(short)
        })
        .collect()
});

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-timezone",
    style = "input-select",
    template_file = "select.mhtml",
    web_impl_file = "timezone.impl.ts"
)]
pub struct InputTimezone {
    /// The selected IANA identifier, or null for no selection.
    #[property(notify, default = "")]
    pub value: Option<String>,
    /// Prepended null option: unset means none, a string (possibly empty)
    /// labels it.
    #[property]
    pub r#default: Option<String>,
    /// Flush rendering when embedded in another input's group.
    #[property(reflect)]
    pub embedded: bool,
}

impl InputTimezoneLogic for InputTimezone {
    /// Mirrored for the browser in `timezone.impl.ts` — keep both in sync.
    fn select_options(&self, store: &uic_core::PropertyStore) -> Value {
        with_default_option(TIMEZONE_OPTIONS.clone(), store.get("default"))
    }

    fn form_value(&self, store: &uic_core::PropertyStore) -> Value {
        form_value(store)
    }

    fn front_class(&self, store: &uic_core::PropertyStore) -> Value {
        front_class(store)
    }

    fn embedded_class(&self, store: &uic_core::PropertyStore) -> Value {
        super::select::embedded_class(store)
    }

    /// Unlike the generic select, the empty selection is always null (the
    /// catalog's `InputTimezone.onChange` override).
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let raw = event.target_value.clone().unwrap_or_default();
        if raw.is_empty() {
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
    use uic_core::{notify_events, PropertyStore};

    #[test]
    fn the_zone_list_pins_utc_first_and_shortens_the_last_segment() {
        let options = &*TIMEZONE_OPTIONS;
        assert!(options.len() > 400, "a real zone database");
        assert_eq!(options[0].value, "UTC");
        assert_eq!(
            options.iter().filter(|o| o.value == "UTC").count(),
            1,
            "UTC appears exactly once"
        );

        let berlin = options
            .iter()
            .find(|o| o.value == "Europe/Berlin")
            .expect("Europe/Berlin");
        assert_eq!(berlin.short_label(), "Berlin");
        let buenos_aires = options
            .iter()
            .find(|o| o.value == "America/Argentina/Buenos_Aires")
            .expect("Buenos Aires");
        assert_eq!(buenos_aires.short_label(), "Buenos_Aires");
    }

    #[test]
    fn empty_commits_null_even_without_a_default() {
        let def = InputTimezone::definition();
        let mut store = PropertyStore::new(def.properties);
        store.set("value", "Europe/Berlin");
        let mut behavior = (def.new_behavior)();
        let mut changed = Changed::default();
        let mut ctx = Ctx::new(&mut store, &mut changed);
        behavior.handle(&mut ctx, "on_change", &UiEvent::change(""));
        assert_eq!(store.get("value"), &Value::Null);
        let events = notify_events(def, &changed, &store);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
        assert_eq!(events[0].value, Value::Null);
    }

    #[test]
    fn definition_shares_the_select_template_and_style() {
        let def = InputTimezone::definition();
        assert_eq!(def.tag_name, "input-timezone");
        assert_eq!(def.style_id, "input-select");
        assert_eq!(def.shared_style_id, Some("input-default"));
        assert!(def.scss.is_none(), "styles come from input-select");
        assert_eq!(
            def.template_src,
            crate::input::select::InputSelect::definition().template_src,
            "shares select.mhtml"
        );
        assert!(def.property("options").is_none(), "the list is computed");
    }
}
