//! Static component metadata, produced by `#[derive(CustomElement)]`.

use std::borrow::Cow;
use std::sync::OnceLock;

use uic_template::Template;

use crate::behavior::Behavior;
use crate::value::Value;

/// The JavaScript-facing type of a reactive property, mirroring the `type`
/// option in a Lit `static properties` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsType {
    String,
    Number,
    Boolean,
    /// `Temporal.ZonedDateTime | null` — object-valued, property-only
    /// (no attribute, no reflection); Rust side is `Option<Zoned>`.
    Zoned,
    /// `SelectOption[]` — object-valued, property-only; Rust side is
    /// `Vec<SelectOption>` and starts empty (ADR 0006).
    Options,
}

/// Notify behavior of a property, mirroring the catalog's `LitNotify` option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notify {
    /// No change event.
    No,
    /// Fires `<attribute-or-js-name>-changed`.
    Auto,
    /// Fires the given event name.
    Named(&'static str),
}

/// Compile-time default of a property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Undefined,
    Str(&'static str),
    Num(f64),
    Bool(bool),
    /// Option lists always start empty (`[]`), never undefined.
    EmptyOptions,
}

impl DefaultValue {
    pub fn value(&self) -> Value {
        match *self {
            DefaultValue::Undefined => Value::Undefined,
            DefaultValue::Str(s) => Value::Str(s.to_string()),
            DefaultValue::Num(n) => Value::Num(n),
            DefaultValue::Bool(b) => Value::Bool(b),
            DefaultValue::EmptyOptions => Value::Options(Vec::new()),
        }
    }
}

/// One reactive property of a component.
#[derive(Debug)]
pub struct PropertyMeta {
    /// Rust field name; templates reference this.
    pub rust_name: &'static str,
    /// JavaScript property name (camelCase of `rust_name`).
    pub js_name: &'static str,
    /// Observed attribute name, `None` for property-only.
    pub attribute: Option<&'static str>,
    pub js_type: JsType,
    /// Declared as `Option<…>` in Rust: the JS field admits null/undefined.
    pub optional: bool,
    /// Reflect property changes back to the attribute.
    pub reflect: bool,
    pub notify: Notify,
    pub default: DefaultValue,
    pub doc: &'static str,
}

impl PropertyMeta {
    /// The change-event name, following the catalog's `LitNotify` rules:
    /// a `notify` string wins, otherwise `<attribute || js_name>-changed`.
    pub fn notify_event_name(&self) -> Option<Cow<'static, str>> {
        match self.notify {
            Notify::No => None,
            Notify::Named(name) => Some(Cow::Borrowed(name)),
            Notify::Auto => Some(Cow::Owned(format!(
                "{}-changed",
                self.attribute.unwrap_or(self.js_name)
            ))),
        }
    }
}

/// How a named behavior hook is provided per target.
///
/// `PerTarget` means a Rust `Logic` implementation drives TUI/native and a
/// co-located `.impl.ts` partial drives the browser.
/// The enum is the seam where a future shared-WASM variant plugs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    PerTarget,
}

/// An event handler referenced by the template via `@event=${name}`.
#[derive(Debug)]
pub struct HandlerMeta {
    pub name: &'static str,
    pub kind: HandlerKind,
}

/// The full definition of one custom element — everything the render targets
/// and generators need, as `&'static` data registered with `inventory`.
#[derive(Debug)]
pub struct ComponentDef {
    /// Custom-element tag, e.g. `input-date`.
    pub tag_name: &'static str,
    /// Generated class name, e.g. `InputDate`.
    pub class_name: &'static str,
    /// `ExternalStyles` identifier; the host element gets class `el-<style_id>`.
    pub style_id: &'static str,
    pub properties: &'static [PropertyMeta],
    /// Handlers referenced from the template.
    pub handlers: &'static [HandlerMeta],
    /// Computed property names referenced from the template (idents that are
    /// not declared properties).
    pub computed: &'static [&'static str],
    /// Template source, embedded via `include_str!` for file templates.
    pub template_src: &'static str,
    /// Chrome template that wraps this component's template at its `<slot/>`.
    pub wraps_src: Option<&'static str>,
    /// Additional shared `ExternalStyles` identifier (host also gets class
    /// `el-<shared_style_id>`), e.g. `input-default` for the input contract.
    pub shared_style_id: Option<&'static str>,
    /// Stylesheet backing `shared_style_id`, emitted once per identifier.
    pub shared_scss: Option<&'static str>,
    /// Co-located component stylesheet (`include_str!` of the `.scss`).
    pub scss: Option<&'static str>,
    /// Co-located web behavior partial (`include_str!` of the `.impl.ts`).
    pub web_impl: Option<&'static str>,
    /// Rust module that defines the component, for diagnostics.
    pub module_path: &'static str,
    /// Instantiates the Rust behavior (TUI/native targets).
    pub new_behavior: fn() -> Box<dyn Behavior>,
    #[doc(hidden)]
    pub template_cache: OnceLock<Template>,
}

impl ComponentDef {
    /// The parsed template, spliced into its chrome when `wraps_src` is set.
    ///
    /// Parsing is lazy and infallible here: the derive macro already parsed,
    /// spliced and validated the identical sources at compile time.
    pub fn template(&'static self) -> &'static Template {
        self.template_cache.get_or_init(|| {
            let inner = uic_template::parse(self.template_src).unwrap_or_else(|err| {
                panic!(
                    "template of <{}> ({}) no longer parses: {err}",
                    self.tag_name, self.module_path
                )
            });
            let Some(wraps_src) = self.wraps_src else {
                return inner;
            };
            let chrome = uic_template::parse(wraps_src).unwrap_or_else(|err| {
                panic!(
                    "chrome template of <{}> ({}) no longer parses: {err}",
                    self.tag_name, self.module_path
                )
            });
            uic_template::splice(&chrome, &inner).unwrap_or_else(|err| {
                panic!(
                    "chrome template of <{}> ({}) no longer splices: {err}",
                    self.tag_name, self.module_path
                )
            })
        })
    }

    pub fn property(&self, rust_name: &str) -> Option<&'static PropertyMeta> {
        self.properties.iter().find(|p| p.rust_name == rust_name)
    }

    pub fn property_by_attribute(&self, attribute: &str) -> Option<&'static PropertyMeta> {
        self.properties
            .iter()
            .find(|p| p.attribute == Some(attribute))
    }

    /// Lookup by the JavaScript property name — template bindings on nested
    /// custom elements (`.value=${…}`) are JS-facing.
    pub fn property_by_js_name(&self, js_name: &str) -> Option<&'static PropertyMeta> {
        self.properties.iter().find(|p| p.js_name == js_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(
        rust_name: &'static str,
        js_name: &'static str,
        attribute: Option<&'static str>,
        notify: Notify,
    ) -> PropertyMeta {
        PropertyMeta {
            rust_name,
            js_name,
            attribute,
            js_type: JsType::String,
            optional: false,
            reflect: false,
            notify,
            default: DefaultValue::Undefined,
            doc: "",
        }
    }

    /// The rules table of the catalog's `eventNameForProperty`.
    #[test]
    fn notify_event_names_follow_lit_notify_rules() {
        // notify: false → no event
        assert_eq!(
            meta("value", "value", Some("value"), Notify::No).notify_event_name(),
            None
        );
        // notify: true → "<attribute>-changed" when an attribute is set
        assert_eq!(
            meta("value", "value", Some("value"), Notify::Auto)
                .notify_event_name()
                .as_deref(),
            Some("value-changed")
        );
        assert_eq!(
            meta(
                "error_message",
                "errorMessage",
                Some("error-message"),
                Notify::Auto
            )
            .notify_event_name()
            .as_deref(),
            Some("error-message-changed")
        );
        // notify: true without attribute → "<js name>-changed"
        assert_eq!(
            meta("date", "date", None, Notify::Auto)
                .notify_event_name()
                .as_deref(),
            Some("date-changed")
        );
        // notify: "custom-name" → that exact event
        assert_eq!(
            meta("value", "value", Some("value"), Notify::Named("picked"))
                .notify_event_name()
                .as_deref(),
            Some("picked")
        );
    }
}
