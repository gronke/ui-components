//! Which plain HTML elements imply a terminal widget — the one table the
//! runtime mount, the template lint and the macro checks all consult, so
//! the three can never drift (ADR 0026). `data-tui` stays the explicit
//! override beside it: the extension point for registered kinds and the
//! discriminator inside the framework's own input templates, which render
//! `<input type="text">` for four different kinds on purpose.

/// The `<input type>` values that are controls rather than text editors —
/// they never mount a widget without an explicit `data-tui`.
pub const NON_WIDGET_INPUT_TYPES: &[&str] = &[
    "button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit",
];

/// The widget kind a plain element implies, by tag and (lowercased) input
/// type. `None` for everything that stays a plain element. Callers apply
/// the surrounding rules themselves: an explicit `data-tui` wins before
/// this table, and a negative `tabindex` opts a presentation twin out.
pub fn native_widget_kind(tag: &str, input_type: Option<&str>) -> Option<&'static str> {
    match tag {
        "textarea" => Some("text-area"),
        "select" => Some("select"),
        "input" => match input_type {
            Some("number") => Some("number-input"),
            Some("date") | Some("datetime-local") => Some("date-input"),
            Some(other) if NON_WIDGET_INPUT_TYPES.contains(&other) => None,
            // Absent, the textual types, and HTML's rule that an unknown
            // type value falls back to the text state.
            _ => Some("text-input"),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::native_widget_kind;

    #[test]
    fn the_table_maps_tags_and_types() {
        assert_eq!(native_widget_kind("textarea", None), Some("text-area"));
        assert_eq!(native_widget_kind("select", None), Some("select"));
        assert_eq!(native_widget_kind("input", None), Some("text-input"));
        assert_eq!(
            native_widget_kind("input", Some("text")),
            Some("text-input")
        );
        assert_eq!(
            native_widget_kind("input", Some("email")),
            Some("text-input")
        );
        assert_eq!(
            native_widget_kind("input", Some("number")),
            Some("number-input")
        );
        assert_eq!(
            native_widget_kind("input", Some("date")),
            Some("date-input")
        );
        assert_eq!(
            native_widget_kind("input", Some("datetime-local")),
            Some("date-input")
        );
        // Unknown types fall back to the text state, HTML's own rule.
        assert_eq!(
            native_widget_kind("input", Some("month")),
            Some("text-input")
        );
        // Controls stay plain.
        assert_eq!(native_widget_kind("input", Some("checkbox")), None);
        assert_eq!(native_widget_kind("input", Some("submit")), None);
        assert_eq!(native_widget_kind("div", None), None);
        assert_eq!(native_widget_kind("ul", None), None);
    }
}
