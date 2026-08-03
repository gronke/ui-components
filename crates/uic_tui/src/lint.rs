//! TUI-compatibility lint over the registered component templates
//! (ADR 0026).
//!
//! The macros validate grammar and placement at compile time, but two facts
//! only exist once a binary links: which `data-tui` kinds the widget
//! registry serves (`inventory` submissions from any crate) and which notify
//! events a referenced child actually declares. This walk closes that gap;
//! a linked test is the earliest point the full registry exists:
//!
//! ```no_run
//! ui_components_tui::link();
//! uic_tui::lint::assert_tui_compatible();
//! ```
//!
//! Errors are bindings the terminal can never serve; warnings mark web-only
//! markup that is legal but inert here (the browser legitimately has richer
//! interaction).

use std::fmt;

use uic_core::{ComponentDef, CustomElementRegistry};
use uic_template::{Attribute, Element, Node, Template};

use crate::dom::widget::WidgetBox;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The binding cannot work in the terminal; the lint fails on these.
    Error,
    /// Legal but inert in the terminal (web-only markup); reported only.
    Warning,
}

/// One lint finding, anchored by component tag and element breadcrumb.
#[derive(Debug)]
pub struct Finding {
    pub component: String,
    pub path: String,
    pub message: String,
    pub severity: Severity,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{severity}: <{}>", self.component)?;
        if !self.path.is_empty() {
            write!(f, " {}", self.path)?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Lints every registered component; the one-call entry for a test binary.
/// Registry-level defects (nothing linked, duplicate tags, unresolved
/// custom tags) short-circuit; the per-template walk assumes a sane
/// registry.
pub fn check_registry() -> Vec<Finding> {
    if let Err(err) = CustomElementRegistry::assert_valid() {
        return vec![Finding {
            component: "registry".into(),
            path: String::new(),
            message: err.to_string(),
            severity: Severity::Error,
        }];
    }
    CustomElementRegistry::iter().flat_map(check_def).collect()
}

/// Lints one registered component against the live registry.
pub fn check_def(def: &'static ComponentDef) -> Vec<Finding> {
    check_template(def.tag_name, def.template(), &CustomElementRegistry::get)
}

/// Lints one parsed template; `lookup` resolves child custom elements,
/// injectable so tests need no global registrations.
pub fn check_template(
    component: &str,
    template: &Template,
    lookup: &dyn Fn(&str) -> Option<&'static ComponentDef>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut crumbs = Vec::new();
    walk(
        component,
        &template.roots,
        &mut crumbs,
        lookup,
        &mut findings,
    );
    findings
}

/// Prints warnings and panics with every error: the test-suite entry.
pub fn assert_tui_compatible() {
    let findings = check_registry();
    let mut errors = Vec::new();
    for finding in &findings {
        match finding.severity {
            Severity::Warning => eprintln!("{finding}"),
            Severity::Error => errors.push(finding.to_string()),
        }
    }
    assert!(
        errors.is_empty(),
        "the terminal cannot serve {} template binding(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

fn walk(
    component: &str,
    nodes: &[Node],
    crumbs: &mut Vec<String>,
    lookup: &dyn Fn(&str) -> Option<&'static ComponentDef>,
    findings: &mut Vec<Finding>,
) {
    for node in nodes {
        match node {
            Node::Element(el) => {
                crumbs.push(label(el));
                check_element(component, el, crumbs, lookup, findings);
                walk(component, &el.children, crumbs, lookup, findings);
                crumbs.pop();
            }
            Node::If { then, .. } => walk(component, then, crumbs, lookup, findings),
            Node::For { body, .. } => walk(component, body, crumbs, lookup, findings),
            Node::Text(_) | Node::TextHole(_) => {}
        }
    }
}

/// The breadcrumb label: the tag, plus the widget kind where one is named.
fn label(el: &Element) -> String {
    match data_tui(el) {
        Some(Kind::Static(kind)) => format!("{}[data-tui={kind}]", el.tag),
        _ => el.tag.clone(),
    }
}

enum Kind<'t> {
    Static(&'t str),
    Bound,
    /// A plain form element the mount detects by element type (ADR 0026).
    Detected,
    /// A plain `<input>` whose `type` is a hole: the committed value
    /// decides at runtime, the lint cannot see through it.
    BoundType,
}

fn data_tui(el: &Element) -> Option<Kind<'_>> {
    el.attrs
        .iter()
        .find(|attr| attr.name() == "data-tui")
        .map(|attr| match attr {
            Attribute::Static { value, .. } => Kind::Static(value),
            _ => Kind::Bound,
        })
}

fn static_attr<'t>(el: &'t Element, wanted: &str) -> Option<&'t str> {
    el.attrs
        .iter()
        .find(|attr| attr.name() == wanted)
        .and_then(|attr| match attr {
            Attribute::Static { value, .. } => Some(value.as_str()),
            _ => None,
        })
}

/// How the element resolves to a terminal widget, statically: an explicit
/// `data-tui`, the element-type detection of ADR 0026 (through the same
/// shared table the mount uses, so the two can never drift), or a bound
/// `type` hole the lint cannot see through.
fn widget_kind<'t>(el: &'t Element) -> Option<Kind<'t>> {
    if let Some(kind) = data_tui(el) {
        return Some(kind);
    }
    if !matches!(el.tag.as_str(), "input" | "textarea" | "select") {
        return None;
    }
    // The presentation-twin opt-out, mirroring the mount's detection.
    if static_attr(el, "tabindex").is_some_and(|value| value.trim().starts_with('-')) {
        return None;
    }
    let type_attr = el.attrs.iter().find(|attr| attr.name() == "type");
    match type_attr {
        None => uic_template::native::native_widget_kind(&el.tag, None).map(|_| Kind::Detected),
        Some(Attribute::Static { value, .. }) => {
            uic_template::native::native_widget_kind(&el.tag, Some(&value.to_ascii_lowercase()))
                .map(|_| Kind::Detected)
        }
        Some(_) => Some(Kind::BoundType),
    }
}

fn check_element(
    component: &str,
    el: &Element,
    crumbs: &[String],
    lookup: &dyn Fn(&str) -> Option<&'static ComponentDef>,
    findings: &mut Vec<Finding>,
) {
    let mut push = |message: String, severity: Severity| {
        findings.push(Finding {
            component: component.to_string(),
            path: crumbs.join(" > "),
            message,
            severity,
        });
    };

    let kind = widget_kind(el);
    match &kind {
        // Resolve through the runtime's own constructor, so the lint and
        // the mount can never drift; the variant flags do not affect
        // whether a kind exists.
        Some(Kind::Static(name)) => {
            if WidgetBox::new(name, false, false).is_err() {
                push(
                    format!(
                        "unknown data-tui kind '{name}': no built-in widget and no \
                         WidgetRegistration serves it (is the component's terminal twin \
                         linked and its cargo feature enabled?)"
                    ),
                    Severity::Error,
                );
            }
        }
        Some(Kind::Bound) => push(
            "a bound data-tui kind is not statically checkable".into(),
            Severity::Warning,
        ),
        Some(Kind::BoundType) => push(
            format!(
                "a bound type on a plain <{}> is not statically checkable — the \
                 terminal mounts widgets by element type (ADR 0026)",
                el.tag
            ),
            Severity::Warning,
        ),
        Some(Kind::Detected) | None => {}
    }

    // A bound type stays a plain element for the event rules: the committed
    // value could be an excluded control type like checkbox.
    let restricted = matches!(kind, Some(Kind::Static(_) | Kind::Bound | Kind::Detected));
    for attr in &el.attrs {
        let Attribute::Event { name, .. } = attr else {
            continue;
        };
        if restricted {
            if name != "change" && name != "input" {
                push(
                    format!(
                        "@{name} never dispatches on a terminal input widget; the \
                         terminal dispatches only @change (the commit) and @input \
                         (live text)"
                    ),
                    Severity::Error,
                );
            }
        } else if matches!(kind, Some(Kind::BoundType)) && (name == "change" || name == "input") {
            // Plausibly served: the committed type may mount a widget that
            // dispatches these; the bound-type warning already flagged it.
        } else if el.is_custom() {
            // An unresolved child tag is the registry check's finding.
            let Some(child) = lookup(&el.tag) else {
                continue;
            };
            let notify: Vec<String> = child
                .properties
                .iter()
                .filter_map(|property| property.notify_event_name())
                .map(|event| event.into_owned())
                .collect();
            if !notify.iter().any(|event| event == name) {
                let served = if notify.is_empty() {
                    "it notifies nothing".to_string()
                } else {
                    format!("it notifies {}", notify.join(", "))
                };
                push(
                    format!("@{name} is not a notify event of <{}>; {served}", el.tag),
                    Severity::Error,
                );
            }
        } else if name != "click" {
            push(
                format!(
                    "@{name} on <{}> never dispatches in the terminal — plain elements \
                     receive only @click (a web-only handler stays legal)",
                    el.tag
                ),
                Severity::Warning,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    use uic_core::{Behavior, DefaultValue, JsType, Notify, PropertyMeta};

    fn check(source: &str) -> Vec<Finding> {
        let template = uic_template::parse(source).expect("test template parses");
        check_template("x-test", &template, &fake_lookup)
    }

    fn errors(findings: &[Finding]) -> Vec<&Finding> {
        findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect()
    }

    // A minimal child definition for the notify-name check: one notifying
    // `value` property, never instantiated.
    fn never_built() -> Box<dyn Behavior> {
        unreachable!("the lint only reads metadata")
    }

    static FAKE_PROPS: [PropertyMeta; 1] = [PropertyMeta {
        rust_name: "value",
        js_name: "value",
        attribute: Some("value"),
        js_type: JsType::String,
        optional: false,
        reflect: false,
        notify: Notify::Auto,
        default: DefaultValue::Str(""),
        doc: "",
    }];

    static FAKE_CHILD: ComponentDef = ComponentDef {
        tag_name: "fake-child",
        class_name: "FakeChild",
        style_id: "fake-child",
        properties: &FAKE_PROPS,
        handlers: &[],
        computed: &[],
        template_src: "<p>x</p>",
        wraps_src: None,
        shared_style_id: None,
        shared_scss: None,
        scss: None,
        web_impl: None,
        dist: false,
        module_path: "lint::tests",
        new_behavior: never_built,
        template_cache: OnceLock::new(),
    };

    fn fake_lookup(tag: &str) -> Option<&'static ComponentDef> {
        (tag == "fake-child").then_some(&FAKE_CHILD)
    }

    #[test]
    fn a_clean_widget_template_passes() {
        let findings = check(
            r#"<div><input data-tui="text-input" @change=${on_change} @input=${on_input} /></div>"#,
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn an_unknown_widget_kind_is_an_error() {
        let findings = check(r#"<span data-tui="no-such-kind"></span>"#);
        let errors = errors(&findings);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("no-such-kind"));
        assert_eq!(errors[0].path, "span[data-tui=no-such-kind]");
    }

    // Registry-backed resolution (an inventory kind like "tab-bar") is the
    // integration gate's territory: inside this lib's cfg(test) build the
    // catalog links against the OTHER copy of uic_tui, so its submissions
    // land in a different inventory; tests/lint.rs holds that coverage.

    #[test]
    fn an_undispatched_event_on_a_widget_is_an_error() {
        let findings = check(r#"<input data-tui="text-input" @click=${on_click} />"#);
        let errors = errors(&findings);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("@click"));
    }

    #[test]
    fn a_click_on_a_plain_element_passes_and_other_events_warn() {
        // @click dispatches natively (the pointer path), so it lints clean.
        let clean = check(r#"<div @click=${on_click}>x</div>"#);
        assert!(clean.is_empty(), "{clean:?}");

        let findings = check(r#"<div @keydown=${on_key}>x</div>"#);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].message.contains("web-only"));
    }

    #[test]
    fn a_bound_widget_kind_warns_only() {
        let findings = check(r#"<div data-tui=${kind}></div>"#);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn a_plain_input_is_a_detected_widget() {
        // Detection applies the widget event contract to plain form
        // elements (ADR 0026): @click never dispatches there.
        let clean = check(r#"<input type="text" @change=${on_change} @input=${on_input} />"#);
        assert!(clean.is_empty(), "{clean:?}");

        let findings = check(r#"<input @click=${on_click} />"#);
        let click_errors = errors(&findings);
        assert_eq!(click_errors.len(), 1);
        assert!(click_errors[0].message.contains("@click"));

        let textarea = check(r#"<textarea @keydown=${on_key}></textarea>"#);
        assert_eq!(errors(&textarea).len(), 1);
    }

    #[test]
    fn excluded_controls_and_twins_stay_plain_elements() {
        // A checkbox is a pointer control, not a text widget; @click is
        // its native path.
        let checkbox = check(r#"<input type="checkbox" @click=${on_click} />"#);
        assert!(checkbox.is_empty(), "{checkbox:?}");

        // A negative tabindex opts a presentation twin out of detection;
        // its non-click handler warns like any plain element.
        let twin = check(r#"<input tabindex="-1" @keydown=${on_key} />"#);
        assert_eq!(twin.len(), 1);
        assert_eq!(twin[0].severity, Severity::Warning);
        assert!(twin[0].message.contains("web-only"));
    }

    #[test]
    fn a_bound_input_type_warns_and_skips_the_event_rule() {
        let findings = check(r#"<input type=${type} @change=${on_change} />"#);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].message.contains("bound type"));
    }

    #[test]
    fn notify_events_of_a_child_pass_and_typos_fail() {
        let good = check(r#"<fake-child @value-changed=${on_value}></fake-child>"#);
        assert!(good.is_empty(), "{good:?}");

        let typo = check(r#"<fake-child @value-change=${on_value}></fake-child>"#);
        let errors = errors(&typo);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("it notifies value-changed"),
            "{}",
            errors[0]
        );
    }

    #[test]
    fn branches_are_walked_and_the_breadcrumb_names_the_spot() {
        let findings = check(
            r#"<div><template if=${open}><section><a @keydown=${on_go}>go</a></section></template></div>"#,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, "div > section > a");
        assert_eq!(findings[0].to_string().split(':').next(), Some("warning"));
    }
}
