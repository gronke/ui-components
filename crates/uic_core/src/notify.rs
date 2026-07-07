//! The notify pass: turns a [`Changed`] batch into property-change events,
//! following the catalog's `LitNotify` mixin (events fire after update, one
//! per changed property with `notify` set, detail `{property, value, oldValue}`).

use crate::behavior::NotifyEvent;
use crate::meta::ComponentDef;
use crate::value::{Changed, PropertyStore};

/// Computes the notify events for one settled update cycle, in change order.
pub fn notify_events(
    def: &'static ComponentDef,
    changed: &Changed,
    store: &PropertyStore,
) -> Vec<NotifyEvent> {
    let mut events = Vec::new();
    for (rust_name, old) in changed.iter() {
        let Some(meta) = def.property(rust_name) else {
            continue;
        };
        let Some(event_name) = meta.notify_event_name() else {
            continue;
        };
        events.push(NotifyEvent {
            property: meta.js_name.to_string(),
            event_name: event_name.into_owned(),
            value: store.get(rust_name).clone(),
            old_value: old.clone(),
        });
    }
    events
}
