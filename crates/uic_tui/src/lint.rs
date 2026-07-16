//! TUI-compatibility lint over the registered component templates
//! (ADR 0016).
//!
//! The macros validate grammar and placement at compile time, but two facts
//! only exist once a binary links: which `data-tui` kinds the widget
//! registry serves (`inventory` submissions from any crate) and which notify
//! events a referenced child actually declares. This walk closes that gap —
//! a linked test is the earliest point the full registry exists:
//!
//! ```no_run
//! ui_components::link();
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
    /// The binding cannot work in the terminal — the lint fails on these.
    Error,
    /// Legal but inert in the terminal (web-only markup) — reported only.
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
/// custom tags) short-circuit — the per-template walk assumes a sane
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

/// Lints one parsed template; `lookup` resolves child custom elements —
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

/// Prints warnings and panics with every error — the test-suite entry.
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

    let kind = data_tui(el);
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
        None => {}
    }

    for attr in &el.attrs {
        let Attribute::Event { name, .. } = attr else {
            continue;
        };
        if kind.is_some() {
            if name != "change" && name != "input" {
                push(
                    format!(
                        "@{name} never dispatches on a data-tui widget; the terminal \
                         dispatches only @change (the commit) and @input (live text)"
                    ),
                    Severity::Error,
                );
            }
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
        } else {
            push(
                format!(
                    "@{name} on <{}> never dispatches in the terminal — only data-tui \
                     widgets receive events (a web-only handler stays legal)",
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
    // land in a different inventory — tests/lint.rs holds that coverage.

    #[test]
    fn an_undispatched_event_on_a_widget_is_an_error() {
        let findings = check(r#"<input data-tui="text-input" @click=${on_click} />"#);
        let errors = errors(&findings);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("@click"));
    }

    #[test]
    fn an_event_on_a_plain_element_warns_only() {
        let findings = check(r#"<div @click=${on_click}>x</div>"#);
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
            r#"<div><template if=${open}><section><a @click=${on_go}>go</a></section></template></div>"#,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, "div > section > a");
        assert_eq!(findings[0].to_string().split(':').next(), Some("warning"));
    }
}
