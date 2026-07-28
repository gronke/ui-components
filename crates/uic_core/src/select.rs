//! The closed option-list entry behind `.options` bindings (ADR 0005).
//!
//! Select options are data, not template structure: components expose a
//! `Vec<SelectOption>` (stored or computed) and both render targets consume
//! it — the generated Lit class maps it to `<option>` children, the TUI
//! feeds it to its dropdown widget.

/// One entry of a select option list.
///
/// `value` is the string a selection commits; `short` is the compact text
/// shown while the select is closed; `label` the full text shown in the
/// open list. Fallbacks follow the catalog's falsy `||` chains: closed
/// shows `short || label || value`, open shows `label || value`.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectOption {
    pub value: String,
    pub short: Option<String>,
    pub label: Option<String>,
}

impl SelectOption {
    /// A plain option whose labels fall back to the value.
    pub fn new(value: impl Into<String>) -> Self {
        SelectOption {
            value: value.into(),
            short: None,
            label: None,
        }
    }

    pub fn with_short(mut self, short: impl Into<String>) -> Self {
        self.short = Some(short.into());
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The text of the open list: `label || value` (falsy fallback).
    pub fn full_label(&self) -> &str {
        match self.label.as_deref() {
            Some(label) if !label.is_empty() => label,
            _ => &self.value,
        }
    }

    /// The text of the closed line: `short || label || value`.
    pub fn short_label(&self) -> &str {
        match self.short.as_deref() {
            Some(short) if !short.is_empty() => short,
            _ => self.full_label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_fall_back_along_the_catalog_chain() {
        let plain = SelectOption::new("Europe/Berlin");
        assert_eq!(plain.full_label(), "Europe/Berlin");
        assert_eq!(plain.short_label(), "Europe/Berlin");

        let shorted = SelectOption::new("Europe/Berlin").with_short("Berlin");
        assert_eq!(shorted.full_label(), "Europe/Berlin");
        assert_eq!(shorted.short_label(), "Berlin");

        let labeled = SelectOption::new("").with_label("Pick a zone");
        assert_eq!(labeled.full_label(), "Pick a zone");
        assert_eq!(labeled.short_label(), "Pick a zone");

        // Falsy chain: empty strings fall through like in JavaScript.
        let empty = SelectOption::new("x").with_short("").with_label("");
        assert_eq!(empty.full_label(), "x");
        assert_eq!(empty.short_label(), "x");
    }

    #[test]
    fn equality_is_structural() {
        assert_eq!(SelectOption::new("a"), SelectOption::new("a"));
        assert_ne!(
            SelectOption::new("a"),
            SelectOption::new("a").with_short("A")
        );
    }
}
