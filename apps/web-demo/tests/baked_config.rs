//! The baked example configs: the page registers its `on_notify` callbacks
//! from the config's notify list, so what build.rs filters here decides
//! which TUI events reach the page at all.

use std::path::Path;

fn baked(route: &str) -> String {
    let path = Path::new(env!("OUT_DIR"))
        .join("dist")
        .join(route)
        .join("index.html");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the baked page {} reads: {error}", path.display()))
}

#[test]
fn the_form_config_registers_its_whole_state_notify() {
    // The channel route consumes whole-state snapshots: its rich-typed
    // `state` notify must survive the scalar filter, or the TUI→page
    // direction of the form's binding is severed (nothing else registers
    // `on_notify`).
    let html = baked("demo");
    assert!(
        html.contains(r#""notify":[{"event":"state-changed","prop":"state"}]"#),
        "the form page registers the state notify:\n{}",
        html.lines()
            .find(|line| line.contains("\"notify\""))
            .unwrap_or("(no config line)")
    );
}

#[test]
fn a_component_config_keeps_the_scalar_sync_list() {
    // The per-property pane sync carries JSON-faithful scalars only: the
    // rich Zoned `date` notify stays out, its scalar `value` twin carries
    // the same information.
    let html = baked("components/input-date");
    assert!(
        html.contains(
            r#""notify":[{"event":"value-changed","prop":"value"},{"event":"timezone-changed","prop":"timezone"}]"#
        ),
        "the date page syncs its scalar notifies:\n{}",
        html.lines()
            .find(|line| line.contains("\"notify\""))
            .unwrap_or("(no config line)")
    );
    assert!(
        !html.contains("date-changed"),
        "the rich Zoned notify stays filtered on sync routes"
    );
}
