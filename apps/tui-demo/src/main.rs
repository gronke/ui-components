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
            element.set_attr("default-timezone", "Europe/Berlin");
            element.set_attr("show-timezone", "");
            *status.borrow_mut() =
                "Enter commits · F4/Down opens the calendar or zone list · Esc quits".into();
        }
        "input-text" => {
            element.set_attr("label", "Note");
            element.set_attr("hint", "Trimmed on commit; empty becomes null");
            element.set_attr("allow-null", "");
        }
        "input-number" => {
            element.set_attr("label", "Amount");
            element.set_attr("hint", "Comma or dot decimals; dots group thousands");
            element.set_attr("unit", "EUR");
            element.set_attr("allow-null", "");
        }
        "input-textarea" => {
            element.set_attr("label", "Comment");
            element.set_attr("hint", "Grows with its content up to max-lines");
            *status.borrow_mut() = "Enter adds a line, Tab commits · Esc quits".into();
        }
        "input-select" => {
            element.set_attr("label", "Time zone");
            element.set_attr("hint", "Empty commits null once a default is set");
            element.set_attr("default", "Pick a zone");
            element.set_attr("value", "Europe/Berlin");
            element.set_prop(
                "options",
                vec![
                    uic_core::SelectOption::new("Europe/Amsterdam").with_short("Amsterdam"),
                    uic_core::SelectOption::new("Europe/Berlin").with_short("Berlin"),
                    uic_core::SelectOption::new("America/New_York").with_short("New_York"),
                    uic_core::SelectOption::new("Pacific/Auckland").with_short("Auckland"),
                ],
            );
            *status.borrow_mut() =
                "F4/Down/Space opens the list · Enter picks, Esc reverts · Esc quits".into();
        }
        "input-timezone" => {
            element.set_attr("label", "Time zone");
            element.set_attr("hint", "The platform zone list, UTC first");
            element.set_attr("default", "Pick a zone");
            *status.borrow_mut() =
                "F4/Down/Space opens the list · Enter picks, Esc reverts · Esc quits".into();
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
    let notify = status.clone();
    element.on("timezone-changed", move |event| {
        *notify.borrow_mut() = format!(
            "timezone-changed: {:?} (was {:?})",
            event.value, event.old_value
        );
    });

    let line = status.clone();
    app.status_bar(move || line.borrow().clone());
    app.run()?;
    Ok(())
}
