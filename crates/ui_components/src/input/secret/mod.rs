//! `<input-secret>`: a masked field for a secret (a token, a key).
//!
//! Shows bullets by default; a reveal control toggles visibility and, in the
//! browser, a copy button writes the value to the clipboard. Display-only by
//! default — the host sets `value`, the user reads and copies it — it becomes
//! editable with `editable`: revealing reads as before, and a commit sets a
//! new value. When the host never discloses the stored secret (`value` is
//! null) an editable field is write-only: nothing to reveal but the new input.
//! The terminal twin lives in `ui_components_tui` (`data-tui="secret-input"`),
//! the browser behaviour in `secret.impl.ts`.

use uic_core::{input_shared, Ctx, CustomElement, UiEvent, Value};

#[input_shared]
#[derive(CustomElement, Default)]
#[custom_element(
    tag = "input-secret",
    template_file = "secret.html",
    web_impl_file = "secret.impl.ts"
)]
pub struct InputSecret {
    /// The secret. Rendered masked until revealed; `null` means the client was
    /// never given it (a write-only field then sets a fresh value).
    #[property(notify, default = "")]
    pub value: Option<String>,
    /// Opt into editing. Absent (the default) is display-only, preserving the
    /// read-and-copy use; set, the field commits a new value.
    #[property(reflect)]
    pub editable: bool,
}

impl InputSecretLogic for InputSecret {
    /// Commit a new secret. Mirrored for the browser in `secret.impl.ts`; keep
    /// both in sync. The value is taken exactly as entered — a secret must not
    /// be trimmed — and an empty entry clears to null (the "not set" state).
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        let raw = event.target_value.clone().unwrap_or_default();
        if raw.is_empty() {
            ctx.set("value", Value::Null);
        } else {
            ctx.set("value", raw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_reflects_the_catalog_shape() {
        let def = InputSecret::definition();
        assert_eq!(def.tag_name, "input-secret");
        // Inherits the shared input chrome + Bootstrap styling.
        assert_eq!(def.shared_style_id, Some("input-default"));
        assert!(def.shared_scss.is_some());
        assert!(def.wraps_src.is_some());

        let value = def.property("value").expect("value property");
        assert_eq!(value.js_name, "value");
        // Editable: the value commits, so it notifies.
        assert_eq!(value.notify_event_name().as_deref(), Some("value-changed"));

        let editable = def.property("editable").expect("editable property");
        assert_eq!(editable.attribute, Some("editable"));

        // The chrome supplies label/hint/error; no computed properties needed.
        assert!(def.computed.is_empty());
    }

    #[test]
    fn commit_sets_the_value_verbatim_and_empty_clears_to_null() {
        use uic_core::testing::{cycle, setup};

        let (mut store, mut behavior) = setup(InputSecret::definition());
        // No trim: a secret commits exactly as entered.
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change("  ghs_x y "))
        });
        assert_eq!(store.get("value"), &Value::Str("  ghs_x y ".into()));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "value-changed");

        // An empty entry clears to null — the write-only "not set" state.
        let events = cycle(&mut store, &mut behavior, |b, ctx| {
            b.handle(ctx, "on_change", &UiEvent::change(""))
        });
        assert_eq!(store.get("value"), &Value::Null);
        assert_eq!(events.len(), 1);
    }
}
