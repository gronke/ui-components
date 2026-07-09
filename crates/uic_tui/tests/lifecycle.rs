//! The ReactiveElement flow on the TUI runtime: `will_update` before the
//! commit, notify in between, `updated` after it, and follow-up cycles from
//! `updated` writes. One test owns the shared log — the order is the point.

use std::sync::Mutex;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uic_core::{Changed, Ctx, CustomElement, UiEvent};
use uic_tui::App;

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
    ui_components::link();
    let terminal = Terminal::new(TestBackend::new(40, 6)).expect("test terminal");
    let mut app: App<TestBackend> = App::from_terminal(terminal);
    let el = app.mount("lifecycle-probe").expect("mount");
    el.on("value-changed", |event| {
        log(format!(
            "notify:{} {:?} was {:?}",
            event.event_name, event.value, event.old_value
        ));
    });
    LOG.lock().expect("log lock").clear();

    app.root_mut().expect("mounted").set_attr("value", "x");

    // One external write: will_update sees the batch with the OLD value,
    // the notify listener fires before the commit, updated runs after it —
    // and its `echoed` write drives exactly one converging follow-up cycle.
    let entries = LOG.lock().expect("log lock").clone();
    assert_eq!(
        entries,
        [
            "will_update[value] old=Some(Str(\"\"))",
            "notify:value-changed Str(\"x\") was Str(\"\")",
            "updated[value]",
            "will_update[echoed] old=None",
            "updated[echoed]",
        ],
    );
}
