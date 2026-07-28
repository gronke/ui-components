//! `<nav-breadcrumb>` — a static breadcrumb trail. The crumbs arrive as
//! `.items` data rows (`{label, href?}`, ADR 0005 spirit); a computed
//! decorates them with the divider so both targets paint the same text
//! separators, because CSS `::before` dividers cannot render in a terminal
//! (ADR 0017). The trail is static content: no widget twin, no events.

use uic_core::{CustomElement, ObjectMap, PropertyStore, Value};

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "nav-breadcrumb",
    template_file = "nav_breadcrumb.html",
    scss_file = "nav_breadcrumb.scss",
    web_impl_file = "nav_breadcrumb.impl.ts"
)]
pub struct NavBreadcrumb {
    /// The trail rows (`{label, href?}`); a crumb without an href renders
    /// as plain text instead of a link.
    #[property]
    pub items: Vec<Value>,
    /// The separator text between crumbs.
    #[property(default = "›")]
    pub divider: String,
}

impl NavBreadcrumbLogic for NavBreadcrumb {
    /// The display rows `{label, href, sep, plain}`: `sep` is empty on the
    /// first crumb and the divider afterwards; `plain` complements `href`
    /// (loop members cannot be negated, ADR 0001). Mirrored for the browser
    /// in `nav_breadcrumb.impl.ts` — keep both in sync.
    fn crumbs(&self, store: &PropertyStore) -> Value {
        let divider = match store.get("divider") {
            Value::Str(divider) => divider.clone(),
            _ => String::new(),
        };
        let items = store.get("items").as_array().unwrap_or_default();
        let crumbs = items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let row = item.as_object();
                let text = |key: &str| {
                    row.and_then(|row| row.get(key))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                let href = text("href");
                let mut crumb = ObjectMap::new();
                crumb.insert("label", text("label"));
                crumb.insert("sep", if index == 0 { "" } else { divider.as_str() });
                crumb.insert("plain", href.is_empty());
                crumb.insert("href", href);
                Value::Object(crumb)
            })
            .collect();
        Value::Array(crumbs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, href: Option<&str>) -> Value {
        let mut row = ObjectMap::new();
        row.insert("label", label);
        if let Some(href) = href {
            row.insert("href", href);
        }
        Value::Object(row)
    }

    fn crumbs_of(items: Vec<Value>, divider: Option<&str>) -> Vec<Value> {
        let def = NavBreadcrumb::definition();
        let mut store = PropertyStore::new(def.properties);
        store.set("items", Value::Array(items));
        if let Some(divider) = divider {
            store.set("divider", divider);
        }
        let Value::Array(crumbs) = NavBreadcrumb::default().crumbs(&store) else {
            panic!("expected an array of crumbs");
        };
        crumbs
    }

    fn member(crumb: &Value, key: &str) -> Value {
        crumb
            .as_object()
            .and_then(|row| row.get(key))
            .expect("crumb member")
            .clone()
    }

    #[test]
    fn the_divider_separates_every_crumb_after_the_first() {
        let crumbs = crumbs_of(
            vec![
                item("Documents", Some("/documents")),
                item("Reports", Some("/documents/reports")),
                item("Q3", None),
            ],
            None,
        );
        assert_eq!(crumbs.len(), 3);
        let seps: Vec<Value> = crumbs.iter().map(|c| member(c, "sep")).collect();
        assert_eq!(seps, vec!["".into(), "›".into(), "›".into()]);
        assert_eq!(member(&crumbs[0], "label"), "Documents".into());
        assert_eq!(member(&crumbs[2], "label"), "Q3".into());
    }

    #[test]
    fn a_custom_divider_replaces_the_default() {
        let crumbs = crumbs_of(
            vec![item("Home", Some("/")), item("Files", None)],
            Some("/"),
        );
        assert_eq!(member(&crumbs[1], "sep"), "/".into());
    }

    #[test]
    fn a_missing_or_empty_href_renders_plain() {
        let crumbs = crumbs_of(
            vec![
                item("Linked", Some("/linked")),
                item("Blank", Some("")),
                item("Bare", None),
            ],
            None,
        );
        assert_eq!(member(&crumbs[0], "plain"), Value::Bool(false));
        assert_eq!(member(&crumbs[0], "href"), "/linked".into());
        assert_eq!(member(&crumbs[1], "plain"), Value::Bool(true));
        assert_eq!(member(&crumbs[2], "plain"), Value::Bool(true));
        assert_eq!(member(&crumbs[2], "href"), "".into());
    }

    #[test]
    fn empty_items_produce_no_crumbs() {
        assert!(crumbs_of(Vec::new(), None).is_empty());
    }

    #[test]
    fn non_object_items_become_plain_empty_crumbs() {
        let crumbs = crumbs_of(vec![Value::Null, item("End", None)], None);
        assert_eq!(crumbs.len(), 2);
        assert_eq!(member(&crumbs[0], "label"), "".into());
        assert_eq!(member(&crumbs[0], "plain"), Value::Bool(true));
        assert_eq!(member(&crumbs[1], "label"), "End".into());
    }

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = NavBreadcrumb::definition();
        assert_eq!(def.tag_name, "nav-breadcrumb");
        assert!(def.scss.is_some());
        assert!(def.dist, "the trail ships in the npm package");

        // The rows are data, property-only (ADR 0005).
        let items = def.property("items").expect("items");
        assert_eq!(items.js_type, uic_core::JsType::Array);
        assert_eq!(items.attribute, None);
        assert_eq!(items.default, uic_core::DefaultValue::EmptyArray);

        let divider = def.property("divider").expect("divider");
        assert_eq!(divider.attribute, Some("divider"));
        assert_eq!(divider.default, uic_core::DefaultValue::Str("›"));

        assert_eq!(def.computed, ["crumbs"]);
        let template = def.template();
        assert!(template.referenced_idents().contains("crumbs"));
        assert!(template.referenced_handlers().is_empty());
    }
}
