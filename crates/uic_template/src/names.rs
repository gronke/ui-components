//! The component model's JS naming rules, shared by the derive macro and
//! the web codegen: Rust field and handler names become JS members and
//! dash-case attribute names. One implementation keeps the emitted class,
//! the manifest and the macro's metadata in agreement.

/// `error_message` → `errorMessage`.
pub fn camel_case(rust_name: &str) -> String {
    let mut out = String::with_capacity(rust_name.len());
    let mut upper_next = false;
    for ch in rust_name.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// `error_message` → `error-message`.
pub fn dash_case(rust_name: &str) -> String {
    rust_name.replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_names_map_to_js_members_and_attributes() {
        assert_eq!(camel_case("value"), "value");
        assert_eq!(camel_case("error_message"), "errorMessage");
        assert_eq!(camel_case("on_change"), "onChange");
        assert_eq!(dash_case("value"), "value");
        assert_eq!(dash_case("error_message"), "error-message");
    }
}
