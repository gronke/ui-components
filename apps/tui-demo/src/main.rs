//! Terminal demo: any registered component by tag (default `input-date`),
//! rendered by uic_tui. Tab/Enter commit, Esc quits.
//!
//! ```sh
//! cargo run -p uic_tui_demo               # <input-date>
//! cargo run -p uic_tui_demo input-text    # <input-text>
//! ```

use std::cell::RefCell;
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui_components::link();
    uic_core::CustomElementRegistry::assert_valid()?;

    let tag = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "input-date".to_string());

    let status = Rc::new(RefCell::new(String::from(
        "edit the value, Enter commits · Esc quits",
    )));

    let mut app = uic_tui::App::new()?;
    let element = app.mount(&tag)?;
    match element.tag_name() {
        "input-date" => {
            element.set_attr("label", "Date of purchase");
            element.set_attr("hint", "Format: YYYY-MM-DD");
            element.set_attr("value", "2026-07-07");
            element.set_attr("min", "2020-01-01");
            element.set_attr("max", "2030-12-31");
        }
        "input-text" => {
            element.set_attr("label", "Note");
            element.set_attr("hint", "Trimmed on commit; empty becomes null");
            element.set_attr("allow-null", "");
        }
        _ => {}
    }
    let notify = status.clone();
    element.on("value-changed", move |event| {
        *notify.borrow_mut() = format!(
            "value-changed: {:?} (was {:?})",
            event.value, event.old_value
        );
    });

    let line = status.clone();
    app.status_bar(move || line.borrow().clone());
    app.run()?;
    Ok(())
}
