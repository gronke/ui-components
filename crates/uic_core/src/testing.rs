//! Unit-test helpers for component logic: the runtime's update cycle in
//! miniature (feature `testing`).
//!
//! [`cycle`] replicates what the hosts do per trigger — the trigger's writes
//! join a [`Changed`] batch, `will_update` runs once over that batch's
//! snapshot (its own writes join the same batch), and the notify pass
//! reports the result. Reflect, commit and `updated` stay host territory;
//! so does cascade re-entry (the runtime loops until the batch settles,
//! component tests pin single-step logic).

use crate::behavior::{Behavior, NotifyEvent};
use crate::meta::ComponentDef;
use crate::notify::notify_events;
use crate::value::{Changed, PropertyStore};
use crate::Ctx;

/// A fresh property store and behavior for one component definition.
pub fn setup(def: &'static ComponentDef) -> (PropertyStore, Box<dyn Behavior>) {
    (PropertyStore::new(def.properties), (def.new_behavior)())
}

/// Runs one update cycle and returns the notify events it produced.
pub fn cycle(
    store: &mut PropertyStore,
    behavior: &mut Box<dyn Behavior>,
    trigger: impl FnOnce(&mut dyn Behavior, &mut Ctx),
) -> Vec<NotifyEvent> {
    let def = behavior.def();
    let mut changed = Changed::default();
    {
        let mut ctx = Ctx::new(store, &mut changed);
        trigger(behavior.as_mut(), &mut ctx);
    }
    let snapshot = changed.clone();
    {
        let mut ctx = Ctx::new(store, &mut changed);
        behavior.will_update(&mut ctx, &snapshot);
    }
    notify_events(def, &changed, store)
}
