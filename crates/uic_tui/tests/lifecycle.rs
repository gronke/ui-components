//! The ReactiveElement flow on the TUI runtime: `will_update` before the
//! commit, the notify dispatch in between, `updated` after it, and follow-up
//! cycles from `updated` writes. One test owns the shared log — the order is
//! the point. The mid-cycle observer is a DOM event listener: notify events
//! dispatch as bubbling events during the cycle, exactly the browser's
//! timing.

use std::sync::Mutex;

use uic_core::{Changed, Ctx, CustomElement, UiEvent};
use uic_dom::ListenerOptions;
use uic_tui::dom::DomHost;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn log(entry: String) {
    LOG.lock().expect("log lock").push(entry);
}

fn names(changed: &Changed) -> String {
    changed
        .iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(CustomElement, Default)]
#[custom_element(
    tag = "lifecycle-probe",
    template = "<div><input type=\"text\" data-tui=\"text-input\" .value=${value} @change=${on_change} /></div>"
)]
struct LifecycleProbe {
    #[property(notify)]
    value: String,
    /// Set by `updated` — the write that requests a follow-up cycle.
    #[property(reflect)]
    echoed: bool,
}

impl LifecycleProbeLogic for LifecycleProbe {
    fn on_change(&mut self, ctx: &mut Ctx, event: &UiEvent) {
        ctx.set("value", event.target_value.clone().unwrap_or_default());
    }

    fn will_update(&mut self, _ctx: &mut Ctx, changed: &Changed) {
        log(format!(
            "will_update[{}] old={:?}",
            names(changed),
            changed.old("value").cloned()
        ));
    }

    fn updated(&mut self, ctx: &mut Ctx, changed: &Changed) {
        log(format!("updated[{}]", names(changed)));
        if changed.has("value") {
            ctx.set("echoed", true);
        }
    }
}

#[test]
fn the_cycle_orders_will_update_notify_commit_updated_and_follows_up() {
    ui_components_tui::link();
    let mut host = DomHost::mount("lifecycle-probe").expect("mount");
    let root = host.doc().root();
    host.doc_mut().add_event_listener(
        root,
        "value-changed",
        ListenerOptions::default(),
        |_doc, event| {
            log(format!(
                "notify:{} {:?}",
                event.event_type(),
                event.detail.display_text()
            ));
        },
    );
    LOG.lock().expect("log lock").clear();

    host.set_attr("value", "x");

    // One external write: will_update sees the batch with the OLD value,
    // the notify event dispatches before the commit, updated runs after it —
    // and its `echoed` write drives exactly one converging follow-up cycle.
    let entries = LOG.lock().expect("log lock").clone();
    assert_eq!(
        entries,
        [
            "will_update[value] old=Some(Str(\"\"))",
            "notify:value-changed \"x\"",
            "updated[value]",
            "will_update[echoed] old=None",
            "updated[echoed]",
        ],
    );
}
