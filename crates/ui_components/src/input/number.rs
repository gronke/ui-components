//! `<input-number>` — numeric input with international separator parsing
//! (`1.234,5`, `1,5`, `1234.5`) and comma-decimal display, in the shared
//! input chrome.
//!
//! Deviations from the source catalog, by design: `value` is honestly
//! number-typed (the catalog declares String but stores a number),
//! `allow-null` defaults to false (a reflected boolean cannot default to
//! true in the attribute model), parse failures surface on the error line
//! (the catalog throws uncaught), and the dead `separator` option is not
//! ported. The browser impl additionally dispatches the catalog's native
//! `change` CustomEvent; the terminal relies on `value-changed`.

use uic_core::{input_shared, Ctx, CustomElement, PropertyStore, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-number",
    template_file = "number.html",
    scss_file = "number.scss",
    web_impl_file = "number.impl.ts"
)]
pub struct InputNumber {
    /// Committed numeric value; null only with `allow-null`.
    #[property(notify, default = 0)]
    pub value: Option<f64>,
    /// Digits before the decimal point, for width styling.
    #[property(default = 4)]
    pub digits: f64,
    /// Decimal places shown after the comma.
    #[property(reflect, default = 2)]
    pub decimals: f64,
    /// Whole numbers display without decimals.
    #[property(reflect)]
    pub decimals_optional: bool,
    /// Unit suffix rendered behind the input (e.g. a currency sign).
    #[property]
    pub unit: Option<String>,
    #[property(default = "")]
    pub placeholder: String,
    /// Placeholder alignment: left, right or center.
    #[property]
    pub placeholder_position: Option<String>,
    /// An empty commit becomes null instead of 0.
    #[property(reflect)]
    pub allow_null: bool,
}

/// Port of the catalog's `getFloat`: comma or dot decimals, dots as thousand
/// separators when grouped in threes; `None` for anything malformed.
/// Mirrored in `number.impl.ts` — keep both in sync.
fn get_float(raw: &str) -> Option<f64> {
    let mut dots = 0;
    let mut commas = 0;
    let mut last_dot_distance = 0;
    for (i, c) in raw.chars().enumerate() {
        last_dot_distance += 1;
        match c {
            '.' => {
                if commas > 0 {
                    return None;
                }
                if dots > 0 && last_dot_distance != 4 {
                    return None;
                }
                last_dot_distance = 0;
                dots += 1;
            }
            ',' => commas += 1,
            '0'..='9' => {}
            '-' if i == 0 => {}
            _ => return None,
        }
    }
    if commas > 1 {
        return None;
    }
    let mut normalized = raw.to_string();
    if (commas > 0 && dots > 0) || (commas == 0 && dots > 1) {
        // Dots are thousand separators, e.g. `1.000,50` or `1.000.000`.
        normalized = normalized.replace('.', "");
    }
    let normalized = normalized.replace(',', ".");
    let number: f64 = normalized.parse().ok()?;
    // Normalize -0 to 0 for consistency.
    Some(if number == 0.0 { 0.0 } else { number })
}

/// Port of the catalog's `getFixed`: round half away from zero, comma as the
/// decimal separator, whole numbers plain when decimals are optional.
fn get_fixed(value: f64, decimals: u32, decimals_optional: bool) -> String {
    let factor = 10f64.powi(decimals as i32);
    let rounded = (value * factor).round() / factor;
    if decimals_optional && rounded.fract() == 0.0 {
        return format!("{}", rounded as i64);
    }
    format!("{rounded:.decimals$}", decimals = decimals as usize).replace('.', ",")
}

fn decimals_of(store: &PropertyStore) -> u32 {
    match store.get("decimals") {
        Value::Num(n) if *n >= 0.0 => *n as u32,
        _ => 2,
    }
}

impl InputNumberLogic for InputNumber {
    /// The formatted text the input displays; null renders empty.
    fn display_value(&self, store: &PropertyStore) -> Value {
        match store.get("value") {
            Value::Num(n) => get_fixed(
                *n,
                decimals_of(store),
                store.get("decimals_optional").truthy(),
            )
            .into(),
            _ => "".into(),
        }
    }

    /// Soft-keyboard hint; the catalog emits the invalid `number` token,
    /// this port uses the standard `numeric`.
    fn input_mode(&self, store: &PropertyStore) -> Value {
        if decimals_of(store) > 0 {
            "decimal".into()
        } else {
            "numeric".into()
        }
    }

    /// Mirrored for the browser in `number.impl.ts` — keep both in sync.
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let raw = event
            .target_value
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();
        if raw.is_empty() {
            if ctx.get("allow_null").truthy() {
                ctx.set("value", Value::Null);
            } else {
                ctx.set("value", 0.0);
            }
            ctx.set("error_message", Value::Undefined);
            ctx.set("error", false);
            return;
        }
        let Some(number) = get_float(&raw) else {
            ctx.set("error_message", format!("Invalid number: {raw}"));
            ctx.set("error", true);
            return;
        };
        ctx.set("value", number);
        ctx.set("error_message", Value::Undefined);
        ctx.set("error", false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::testing::{cycle, setup};
    use uic_core::NotifyEvent;

    #[test]
    fn get_float_parses_international_formats() {
        assert_eq!(get_float("1.234,5"), Some(1234.5));
        assert_eq!(get_float("1,5"), Some(1.5));
        assert_eq!(get_float("1234.5"), Some(1234.5));
        assert_eq!(get_float("1.000.000"), Some(1_000_000.0));
        assert_eq!(get_float("-123"), Some(-123.0));
        assert_eq!(get_float("-0"), Some(0.0));
        assert!(get_float("-0").is_some_and(|n| n.is_sign_positive()));

        assert_eq!(get_float("1,000.50"), None, "dot must not follow a comma");
        assert_eq!(get_float("1,2,3"), None, "multiple commas");
        assert_eq!(get_float("1.00.0"), None, "bad thousand grouping");
        assert_eq!(get_float("12x"), None, "invalid character");
        assert_eq!(get_float("1-2"), None, "minus inside");
        assert_eq!(get_float("-"), None);
    }

    #[test]
    fn get_fixed_formats_with_comma_decimals() {
        assert_eq!(get_fixed(1234.5, 2, false), "1234,50");
        assert_eq!(get_fixed(1234.5, 4, false), "1234,5000");
        assert_eq!(get_fixed(42.0, 2, true), "42");
        assert_eq!(get_fixed(42.5, 2, true), "42,50");
        assert_eq!(get_fixed(1.235, 2, false), "1,24", "half away from zero");
        assert_eq!(get_fixed(0.0, 2, false), "0,00");
    }

    fn commit(input: &str) -> (PropertyStore, Vec<NotifyEvent>) {
        let (mut store, mut behavior) = setup(InputNumber::definition());
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(input))
        });
        (store, events)
    }

    #[test]
    fn valid_commit_parses_and_notifies_a_number() {
        let (store, events) = commit("1.234,5");
        assert_eq!(store.get("value"), &Value::Num(1234.5));
        assert_eq!(store.get("error"), &Value::Bool(false));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
        assert_eq!(events[0].value, Value::Num(1234.5));
    }

    #[test]
    fn empty_commit_is_zero_by_default_and_null_with_allow_null() {
        let (store, _) = commit("");
        assert_eq!(store.get("value"), &Value::Num(0.0));

        let (mut store, mut behavior) = setup(InputNumber::definition());
        store.set("allow_null", true);
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(""))
        });
        assert_eq!(store.get("value"), &Value::Null);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, Value::Null);
    }

    #[test]
    fn invalid_commit_sets_the_error_state() {
        let (store, events) = commit("12x");
        assert_eq!(store.get("value"), &Value::Num(0.0), "value untouched");
        assert_eq!(
            store.get("error_message"),
            &Value::Str("Invalid number: 12x".into())
        );
        assert_eq!(store.get("error"), &Value::Bool(true));
        assert!(events.is_empty());
    }

    #[test]
    fn display_value_follows_decimals_and_optionality() {
        let (mut store, behavior) = setup(InputNumber::definition());
        assert_eq!(
            behavior.compute(&store, "display_value"),
            Value::Str("0,00".into())
        );
        store.set("value", 1234.5);
        assert_eq!(
            behavior.compute(&store, "display_value"),
            Value::Str("1234,50".into())
        );
        store.set("decimals", 0.0);
        assert_eq!(
            behavior.compute(&store, "display_value"),
            Value::Str("1235".into()),
            "rounded, no decimals"
        );
        store.set("value", Value::Null);
        assert_eq!(
            behavior.compute(&store, "display_value"),
            Value::Str("".into())
        );
    }

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = InputNumber::definition();
        assert_eq!(def.tag_name, "input-number");
        let value = def.property("value").expect("value");
        assert!(value.optional);
        assert_eq!(value.default, uic_core::DefaultValue::Num(0.0));
        let allow_null = def.property("allow_null").expect("allow_null");
        assert_eq!(allow_null.attribute, Some("allow-null"));
        assert_eq!(allow_null.default, uic_core::DefaultValue::Bool(false));
        assert_eq!(def.computed, &["display_value", "input_mode"]);
    }
}
