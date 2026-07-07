//! Terminal demo: the same `<input-date>` definition the web demo serves,
//! rendered by uic_tui. Tab/Enter commit, Esc quits.

use std::cell::RefCell;
use std::rc::Rc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui_components::link();
    uic_core::CustomElementRegistry::assert_valid()?;

    let status = Rc::new(RefCell::new(String::from(
        "edit the date, Enter commits · Esc quits",
    )));

    let mut app = uic_tui::App::new()?;
    let element = app.mount("input-date")?;
    element.set_attr("label", "Date of purchase");
    element.set_attr("hint", "Format: YYYY-MM-DD");
    element.set_attr("value", "2026-07-07");
    element.set_attr("min", "2020-01-01");
    element.set_attr("max", "2030-12-31");
    let notify = status.clone();
    element.on("value-changed", move |event| {
        *notify.borrow_mut() = format!(
            "value-changed: {} (was {})",
            event.value.display_text(),
            event.old_value.display_text()
        );
    });

    let line = status.clone();
    app.status_bar(move || line.borrow().clone());
    app.run()?;
    Ok(())
}
