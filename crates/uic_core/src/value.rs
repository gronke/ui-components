//! Runtime property values and the per-instance property store.
//!
//! Values follow JavaScript semantics (truthiness, attribute conversion) so
//! the TUI/native runtime observes the same behavior as the generated Lit
//! class in the browser.

use crate::meta::{JsType, PropertyMeta};
use crate::select::SelectOption;
use crate::zoned::Zoned;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Str(String),
    Num(f64),
    Bool(bool),
    /// An object-valued zoned timestamp (`Temporal.ZonedDateTime` analog).
    Zoned(Zoned),
    /// An object-valued select option list (`SelectOption[]` analog).
    Options(Vec<SelectOption>),
}

impl Value {
    /// JavaScript truthiness: `""`, `0`, `NaN`, `false`, `null` and
    /// `undefined` are false; objects are always true.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Str(s) => !s.is_empty(),
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Bool(b) => *b,
            Value::Zoned(_) => true,
            // Arrays are objects: truthy even when empty.
            Value::Options(_) => true,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_zoned(&self) -> Option<&Zoned> {
        match self {
            Value::Zoned(z) => Some(z),
            _ => None,
        }
    }

    pub fn as_options(&self) -> Option<&[SelectOption]> {
        match self {
            Value::Options(options) => Some(options),
            _ => None,
        }
    }

    /// Text rendering for text and attribute positions.
    /// `undefined`/`null` render empty, like lit-html child parts.
    /// A zoned timestamp renders Temporal-style ISO.
    pub fn display_text(&self) -> String {
        match self {
            Value::Undefined | Value::Null => String::new(),
            Value::Str(s) => s.clone(),
            Value::Num(n) => format_number(*n),
            Value::Bool(b) => b.to_string(),
            Value::Zoned(z) => z.iso(),
            // Option lists never legitimately reach text position (the
            // derive restricts `.options` bindings); render nothing.
            Value::Options(_) => String::new(),
        }
    }
}

/// JS-style number formatting: integral values print without a decimal point.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Num(n)
    }
}

impl From<Zoned> for Value {
    fn from(z: Zoned) -> Self {
        Value::Zoned(z)
    }
}

impl From<Vec<SelectOption>> for Value {
    fn from(options: Vec<SelectOption>) -> Self {
        Value::Options(options)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Num(n as f64)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    /// `None` maps to `undefined`, matching uninitialized optional JS fields.
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => Value::Undefined,
        }
    }
}

/// Converts an observed attribute to a property value, mirroring Lit's
/// default converter: `null` (attribute removed) clears, Boolean is presence,
/// Number goes through JS `Number()` (invalid → NaN).
pub fn attribute_to_value(js_type: JsType, raw: Option<&str>) -> Value {
    match (js_type, raw) {
        (JsType::Boolean, raw) => Value::Bool(raw.is_some()),
        (_, None) => Value::Null,
        (JsType::String, Some(raw)) => Value::Str(raw.to_string()),
        (JsType::Number, Some(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Value::Num(0.0)
            } else {
                Value::Num(trimmed.parse().unwrap_or(f64::NAN))
            }
        }
        // Zoned and Options properties are property-only (the derive rejects
        // attribute options), so no attribute ever maps to them.
        (JsType::Zoned, Some(_)) | (JsType::Options, Some(_)) => Value::Null,
    }
}

/// Per-instance property state for the TUI/native runtime.
/// Browser instances keep their state in the generated Lit class instead.
#[derive(Debug)]
pub struct PropertyStore {
    entries: Vec<(&'static str, Value)>,
}

impl PropertyStore {
    /// Seeds every declared property with its default.
    pub fn new(properties: &'static [PropertyMeta]) -> Self {
        PropertyStore {
            entries: properties
                .iter()
                .map(|p| (p.rust_name, p.default.value()))
                .collect(),
        }
    }

    pub fn get(&self, rust_name: &str) -> &Value {
        self.entries
            .iter()
            .find(|(name, _)| *name == rust_name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("unknown property '{rust_name}'"))
    }

    pub fn has(&self, rust_name: &str) -> bool {
        self.entries.iter().any(|(name, _)| *name == rust_name)
    }

    /// Writes a value, returning the previous one when it actually changed.
    pub fn set(&mut self, rust_name: &str, value: impl Into<Value>) -> Option<Value> {
        let value = value.into();
        let entry = self
            .entries
            .iter_mut()
            .find(|(name, _)| *name == rust_name)
            .unwrap_or_else(|| panic!("unknown property '{rust_name}'"));
        if entry.1 == value {
            return None;
        }
        Some(std::mem::replace(&mut entry.1, value))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &Value)> {
        self.entries.iter().map(|(name, value)| (*name, value))
    }
}

/// The batch of properties changed during one update cycle, with the value
/// each had before its first change (mirrors Lit's `changedProperties`).
#[derive(Debug, Default, Clone)]
pub struct Changed {
    entries: Vec<(&'static str, Value)>,
}

impl Changed {
    /// Records the pre-change value; only the first change per property wins.
    pub fn record(&mut self, rust_name: &'static str, old: Value) {
        if !self.has(rust_name) {
            self.entries.push((rust_name, old));
        }
    }

    pub fn has(&self, rust_name: &str) -> bool {
        self.entries.iter().any(|(name, _)| *name == rust_name)
    }

    pub fn old(&self, rust_name: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(name, _)| *name == rust_name)
            .map(|(_, old)| old)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &Value)> {
        self.entries.iter().map(|(name, old)| (*name, old))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness_follows_javascript() {
        assert!(!Value::Undefined.truthy());
        assert!(!Value::Null.truthy());
        assert!(!Value::Str(String::new()).truthy());
        assert!(Value::Str("x".into()).truthy());
        assert!(!Value::Num(0.0).truthy());
        assert!(!Value::Num(f64::NAN).truthy());
        assert!(Value::Num(2.0).truthy());
        assert!(!Value::Bool(false).truthy());
        assert!(Value::Bool(true).truthy());
    }

    #[test]
    fn display_text_renders_like_lit_child_parts() {
        assert_eq!(Value::Undefined.display_text(), "");
        assert_eq!(Value::Null.display_text(), "");
        assert_eq!(Value::Str("a".into()).display_text(), "a");
        assert_eq!(Value::Num(3.0).display_text(), "3");
        assert_eq!(Value::Num(3.5).display_text(), "3.5");
        assert_eq!(Value::Bool(true).display_text(), "true");
    }

    #[test]
    fn attribute_conversion_mirrors_lit_defaults() {
        assert_eq!(
            attribute_to_value(JsType::String, Some("x")),
            Value::Str("x".into())
        );
        assert_eq!(attribute_to_value(JsType::String, None), Value::Null);
        assert_eq!(
            attribute_to_value(JsType::Boolean, Some("")),
            Value::Bool(true)
        );
        assert_eq!(
            attribute_to_value(JsType::Boolean, None),
            Value::Bool(false)
        );
        assert_eq!(
            attribute_to_value(JsType::Number, Some("42")),
            Value::Num(42.0)
        );
        let nan = attribute_to_value(JsType::Number, Some("x"));
        assert!(matches!(nan, Value::Num(n) if n.is_nan()));
    }

    #[test]
    fn store_set_reports_only_real_changes() {
        static PROPS: &[PropertyMeta] = &[PropertyMeta {
            rust_name: "value",
            js_name: "value",
            attribute: Some("value"),
            js_type: JsType::String,
            optional: false,
            reflect: false,
            notify: crate::meta::Notify::Auto,
            default: crate::meta::DefaultValue::Str(""),
            doc: "",
        }];
        let mut store = PropertyStore::new(PROPS);
        assert_eq!(store.get("value"), &Value::Str(String::new()));
        assert_eq!(store.set("value", "a"), Some(Value::Str(String::new())));
        assert_eq!(store.set("value", "a"), None);
        assert_eq!(store.set("value", "b"), Some(Value::Str("a".into())));
    }
}
