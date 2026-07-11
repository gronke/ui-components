//! `<app-root>` — the demo form as one component around a `state` object.
//!
//! State trickles down: each member reaches its input child through a
//! computed with the child's own default as the missing-member fallback, so
//! a sparse state pushes no changes. Child commits trickle back up: the
//! `@value-changed` handlers fold the event detail into a fresh map, and
//! `state-changed` hands the snapshot to whatever transport hosts the
//! component (ADR 0013).

use uic_core::{Ctx, CustomElement, ObjectMap, PropertyStore, SelectOption, UiEvent, Value};

// The demo composition serves the dev pages and the runtimes, not the
// published package (ADR 0013).
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "app-root",
    template_file = "app_root.mhtml",
    web_impl_file = "app_root.impl.ts",
    dist = false
)]
pub struct AppRoot {
    /// The application state: one member per form field, scalars only.
    #[property(notify)]
    pub state: ObjectMap,
}

/// `state[key]`, or the child's own default when the member is absent.
fn member(store: &PropertyStore, key: &str, missing: Value) -> Value {
    match store.get("state").as_object().and_then(|s| s.get(key)) {
        Some(value) => value.clone(),
        None => missing,
    }
}

/// Clone-on-write member update; an equal value leaves `state` untouched —
/// the echo brake the browser twin mirrors in `setMember`.
fn set_member(ctx: &mut Ctx, key: &str, value: Value) {
    let Value::Object(state) = ctx.get("state") else {
        return;
    };
    if state.get(key) == Some(&value) {
        return;
    }
    let mut next = state.clone();
    next.insert(key, value);
    ctx.set("state", next);
}

/// The notify payload of a child's `*-changed` event.
fn detail(event: &UiEvent) -> Value {
    event.detail.clone().unwrap_or(Value::Null)
}

impl AppRootLogic for AppRoot {
    fn date(&self, store: &PropertyStore) -> Value {
        member(store, "date", "".into())
    }

    fn start(&self, store: &PropertyStore) -> Value {
        member(store, "start", "".into())
    }

    fn end(&self, store: &PropertyStore) -> Value {
        member(store, "end", "".into())
    }

    fn note(&self, store: &PropertyStore) -> Value {
        member(store, "note", "".into())
    }

    /// The number child's default is 0 (`number.rs`), not empty.
    fn amount(&self, store: &PropertyStore) -> Value {
        member(store, "amount", 0.0.into())
    }

    fn pick(&self, store: &PropertyStore) -> Value {
        member(store, "pick", "".into())
    }

    fn essay(&self, store: &PropertyStore) -> Value {
        member(store, "essay", "".into())
    }

    fn zone(&self, store: &PropertyStore) -> Value {
        member(store, "zone", "".into())
    }

    /// Keep in sync with `pickOptions` in `app_root.impl.ts`.
    fn pick_options(&self, _store: &PropertyStore) -> Value {
        Value::Options(vec![
            SelectOption::new("Europe/Amsterdam").with_short("Amsterdam"),
            SelectOption::new("Europe/Berlin").with_short("Berlin"),
            SelectOption::new("America/New_York").with_short("New_York"),
            SelectOption::new("Pacific/Auckland").with_short("Auckland"),
        ])
    }

    /// One line of `key: value` pairs in key order — byte-identical to the
    /// browser twin's `stateLine`, the cross-target assertion hook.
    fn state_line(&self, store: &PropertyStore) -> Value {
        let Value::Object(state) = store.get("state") else {
            return "".into();
        };
        state
            .iter()
            .map(|(key, value)| format!("{key}: {}", value.display_text()))
            .collect::<Vec<_>>()
            .join(" · ")
            .into()
    }

    fn on_date(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "date", detail(event));
    }

    fn on_start(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "start", detail(event));
    }

    fn on_end(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "end", detail(event));
    }

    fn on_note(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "note", detail(event));
    }

    fn on_amount(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "amount", detail(event));
    }

    fn on_pick(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "pick", detail(event));
    }

    fn on_essay(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "essay", detail(event));
    }

    fn on_zone(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        set_member(ctx, "zone", detail(event));
    }
}
