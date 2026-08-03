//! JSON ⇄ [`Value`] conversion (feature `json`): the wire format of
//! property values crossing a process or transport boundary (ADR 0013).

use crate::object::ObjectMap;
use crate::value::Value;

/// Renders a value as JSON, matching the browser notify detail: `undefined`
/// and `NaN` become `null`, a zoned timestamp its ISO string (the
/// `Temporal.ZonedDateTime.toJSON` output), option lists their data rows,
/// object maps sorted-key objects, and arrays their elements.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Undefined | Value::Null => serde_json::Value::Null,
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::Num(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Zoned(_) => serde_json::Value::String(value.display_text()),
        Value::Options(options) => serde_json::Value::Array(
            options
                .iter()
                .map(|option| {
                    serde_json::json!({
                        "value": option.value,
                        "short": option.short,
                        "label": option.label,
                    })
                })
                .collect(),
        ),
        Value::Object(object) => serde_json::Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.to_string(), value_to_json(value)))
                .collect(),
        ),
        Value::Array(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
    }
}

/// Renders a value as its canonical JSON string: compact, keys sorted at
/// every level. [`value_to_json`] keeps object maps sorted on its own (the
/// [`ObjectMap`] iterates sorted), but serde_json's map flavor is a build
/// property; feature unification anywhere in the graph can flip it to
/// insertion order (`preserve_order`), which reorders the option rows and
/// any hand-built `json!` literal. Snapshot identities and test expectations
/// compare this string instead of `to_string`, so byte equality never
/// depends on the build. Numbers render as serde_json does (`1.0` keeps its
/// point), canonical across builds, not across languages.
pub fn canonical_json(value: &Value) -> String {
    canonical_string(&value_to_json(value))
}

fn canonical_string(json: &serde_json::Value) -> String {
    match json {
        serde_json::Value::Array(items) => {
            let items: Vec<String> = items.iter().map(canonical_string).collect();
            format!("[{}]", items.join(","))
        }
        serde_json::Value::Object(members) => {
            let mut members: Vec<(&String, &serde_json::Value)> = members.iter().collect();
            members.sort_by_key(|(key, _)| *key);
            let members: Vec<String> = members
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::Value::String((*key).clone()),
                        canonical_string(value)
                    )
                })
                .collect();
            format!("{{{}}}", members.join(","))
        }
        scalar => scalar.to_string(),
    }
}

/// Reads a value from JSON: `null`, booleans, numbers and strings map 1:1,
/// objects recurse into [`ObjectMap`], arrays into [`Value::Array`]. The
/// conversion is deliberately lossy-asymmetric: strings stay strings (no
/// `Zoned` re-hydration) and a JSON array of option rows reads back as a
/// plain array of objects, not `Value::Options` (options are one-way data).
pub fn value_from_json(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Num(n.as_f64().unwrap_or(f64::NAN)),
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Object(members) => {
            let mut object = ObjectMap::new();
            for (key, member) in members {
                object.insert(key.clone(), value_from_json(member));
            }
            Value::Object(object)
        }
        serde_json::Value::Array(items) => {
            Value::Array(items.iter().map(value_from_json).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select::SelectOption;

    #[test]
    fn scalars_round_trip() {
        for value in [
            Value::Null,
            Value::Bool(true),
            Value::Num(12.5),
            Value::Str("x".into()),
        ] {
            assert_eq!(value_from_json(&value_to_json(&value)), value);
        }
    }

    #[test]
    fn objects_round_trip_with_sorted_keys() {
        let state: ObjectMap = [("zone", Value::from("UTC")), ("date", "2026-07-07".into())]
            .into_iter()
            .collect();
        let value = Value::Object(state.clone());
        assert_eq!(
            canonical_json(&value),
            r#"{"date":"2026-07-07","zone":"UTC"}"#
        );
        assert_eq!(
            value_from_json(&value_to_json(&value)),
            Value::Object(state)
        );
    }

    #[test]
    fn canonical_json_sorts_keys_at_every_level() {
        // The option rows are built in value/short/label order; the
        // canonical form sorts them regardless of the map flavor.
        let options = vec![SelectOption::new("UTC")];
        assert_eq!(
            canonical_json(&Value::Options(options)),
            r#"[{"label":null,"short":null,"value":"UTC"}]"#
        );
        let nested: ObjectMap = [("b", Value::from(1.5)), ("a", "x".into())]
            .into_iter()
            .collect();
        assert_eq!(
            canonical_json(&Value::Object(nested)),
            r#"{"a":"x","b":1.5}"#
        );
    }

    #[test]
    fn undefined_and_nan_flatten_to_null() {
        assert_eq!(value_to_json(&Value::Undefined), serde_json::Value::Null);
        assert_eq!(
            value_to_json(&Value::Num(f64::NAN)),
            serde_json::Value::Null
        );
    }

    /// One-way by design: the ISO string comes back as a string.
    #[test]
    fn zoned_flattens_to_its_iso_string() {
        use chrono::TimeZone;
        let zoned = crate::zoned::Zoned::new(
            chrono_tz::Europe::Berlin
                .with_ymd_and_hms(2026, 7, 7, 0, 0, 0)
                .unwrap(),
        );
        let json = value_to_json(&Value::Zoned(zoned));
        assert_eq!(
            json,
            serde_json::Value::String("2026-07-07T00:00:00+02:00[Europe/Berlin]".into())
        );
        assert!(matches!(value_from_json(&json), Value::Str(_)));
    }

    /// Value comparison, not string: the row key order follows serde_json's
    /// map flavor, which feature unification can flip to preserve_order.
    #[test]
    fn options_flatten_to_rows_and_read_back_as_a_plain_array() {
        let options = vec![SelectOption::new("Europe/Berlin").with_short("Berlin")];
        let json = value_to_json(&Value::Options(options));
        assert_eq!(
            json,
            serde_json::json!([
                { "value": "Europe/Berlin", "short": "Berlin", "label": null }
            ])
        );
        // Options are one-way: the rows come back as a plain array of objects.
        let back = value_from_json(&json);
        assert!(matches!(&back, Value::Array(items) if items.len() == 1));
    }

    #[test]
    fn arrays_round_trip() {
        let value = Value::Array(vec![
            Value::Str("a".into()),
            Value::Num(2.0),
            Value::Object([("k", Value::from("v"))].into_iter().collect()),
        ]);
        assert_eq!(value_from_json(&value_to_json(&value)), value);
        assert_eq!(canonical_json(&value), r#"["a",2.0,{"k":"v"}]"#);
    }
}
