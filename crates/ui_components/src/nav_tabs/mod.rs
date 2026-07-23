//! `<nav-tabs>` — a value-driven Bootstrap tab bar, the catalog's first
//! non-input component. The rows arrive as `.options` data (ADR 0006), the
//! selected value leaves through `value-changed`; panes are the host's job
//! (two `<template if>` branches beside the bar, see the demo).
//!
//! Every asset lives in this directory (ADR 0015): the shared template and
//! logic here, the button rows in `nav_tabs.impl.ts`, the terminal tab row
//! in `tui.rs`.

#[cfg(feature = "tui")]
mod tui;

use uic_core::{Ctx, CustomElement, SelectOption, UiEvent};

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "nav-tabs",
    template_file = "nav_tabs.html",
    scss_file = "nav_tabs.scss",
    web_impl_file = "nav_tabs.impl.ts"
)]
pub struct NavTabs {
    /// The selected tab's value; a pick notifies `value-changed`.
    #[property(notify, default = "")]
    pub value: String,
    /// The tab rows (ADR 0006); captions render `short || label || value`.
    #[property]
    pub options: Vec<SelectOption>,
}

impl NavTabsLogic for NavTabs {
    /// Both targets route a pick through the list's `@input` binding: the
    /// terminal widget reports it as live text, a browser button dispatches
    /// a bubbling `input` event. Mirrored in `nav_tabs.impl.ts`.
    fn on_input(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("value", event.target_value.clone().unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_core::testing::{cycle, setup};
    use uic_core::Value;

    #[test]
    fn a_pick_commits_the_value_and_notifies() {
        let (mut store, mut behavior) = setup(NavTabs::definition());
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_input", &UiEvent::input("about"))
        });
        assert_eq!(store.get("value"), &Value::Str("about".into()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");
        assert_eq!(events[0].value, Value::Str("about".into()));
    }

    #[test]
    fn repicking_the_bound_value_stays_silent() {
        let (mut store, mut behavior) = setup(NavTabs::definition());
        store.set("value", "form");
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_input", &UiEvent::input("form"))
        });
        assert!(events.is_empty());
    }

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = NavTabs::definition();
        assert_eq!(def.tag_name, "nav-tabs");
        assert!(def.scss.is_some());
        assert!(def.dist, "the bar ships in the npm package");

        // The rows are data, property-only (ADR 0006).
        let options = def.property("options").expect("options");
        assert_eq!(options.js_type, uic_core::JsType::Options);
        assert_eq!(options.attribute, None);

        let value = def.property("value").expect("value");
        assert_eq!(value.notify_event_name().as_deref(), Some("value-changed"));

        let template = def.template();
        assert!(template.referenced_idents().contains("options"));
        assert!(template.referenced_handlers().contains("on_input"));
    }
}
