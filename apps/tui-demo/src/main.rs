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
    match tag.as_str() {
        "input-date" => {
            app.set_attr(element, "label", "Date of purchase");
            app.set_attr(element, "hint", "Format: YYYY-MM-DD");
            app.set_attr(element, "hide-time", "");
            app.set_attr(element, "value", "2026-07-07");
            app.set_attr(element, "min", "2020-01-01");
            app.set_attr(element, "max", "2030-12-31");
            app.set_attr(element, "default-timezone", "Europe/Berlin");
            app.set_attr(element, "show-timezone", "");
            *status.borrow_mut() =
                "Enter commits · F4/Down opens the calendar or zone list · Esc quits".into();
        }
        "input-date-range" => {
            app.set_attr(element, "label", "Stay");
            app.set_attr(element, "hint", "The end never precedes the start");
            app.set_attr(element, "start", "2026-07-07");
            app.set_attr(element, "end", "2026-07-11");
            *status.borrow_mut() =
                "Enter commits an end · the other follows if the range inverts · Esc quits".into();
        }
        "input-text" => {
            app.set_attr(element, "label", "Note");
            app.set_attr(element, "hint", "Trimmed on commit; empty becomes null");
            app.set_attr(element, "allow-null", "");
        }
        "input-number" => {
            app.set_attr(element, "label", "Amount");
            app.set_attr(
                element,
                "hint",
                "Comma or dot decimals; dots group thousands",
            );
            app.set_attr(element, "unit", "EUR");
            app.set_attr(element, "allow-null", "");
        }
        "input-textarea" => {
            app.set_attr(element, "label", "Comment");
            app.set_attr(element, "hint", "Grows with its content up to max-lines");
            *status.borrow_mut() = "Enter adds a line, Tab commits · Esc quits".into();
        }
        "input-select" => {
            app.set_attr(element, "label", "Time zone");
            app.set_attr(element, "hint", "Empty commits null once a default is set");
            app.set_attr(element, "default", "Pick a zone");
            app.set_attr(element, "value", "Europe/Berlin");
            app.set_prop(
                element,
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
            app.set_attr(element, "label", "Time zone");
            app.set_attr(element, "hint", "The platform zone list, UTC first");
            app.set_attr(element, "default", "Pick a zone");
            *status.borrow_mut() =
                "F4/Down/Space opens the list · Enter picks, Esc reverts · Esc quits".into();
        }
        "input-suggestion" => {
            app.set_attr(element, "label", "Word");
            app.set_attr(element, "hint", "Typeahead: a host answers query-changed");
            app.set_attr(element, "placeholder", "start typing");
            app.set_attr(element, "allow-null", "");
            // A standalone mount has no host answering queries (listeners
            // must not re-enter the app), so fixed rows demonstrate the
            // popup; `app-root` wires the live pool in-component.
            app.set_prop(
                element,
                "suggestions",
                vec![
                    uic_core::SelectOption::new("apple"),
                    uic_core::SelectOption::new("apricot"),
                    uic_core::SelectOption::new("avocado"),
                ],
            );
            *status.borrow_mut() =
                "F4/Down opens the fixed rows · app-root wires the pool · Esc quits".into();
        }
        "nav-tabs" => {
            app.set_attr(element, "value", "form");
            app.set_prop(
                element,
                "options",
                vec![
                    uic_core::SelectOption::new("form").with_short("Form"),
                    uic_core::SelectOption::new("about").with_short("About"),
                    uic_core::SelectOption::new("settings").with_short("Settings"),
                ],
            );
            *status.borrow_mut() =
                "Left/Right or a click switches the tab · panes are the host's job · Esc quits"
                    .into();
        }
        "uic-tree" => {
            let node = |id: &str, label: &str, children: Vec<uic_core::Value>| {
                let mut node = uic_core::ObjectMap::new();
                node.insert("id", id);
                node.insert("label", label);
                if !children.is_empty() {
                    node.insert("children", uic_core::Value::Array(children));
                }
                uic_core::Value::Object(node)
            };
            app.set_prop(
                element,
                "nodes",
                uic_core::Value::Array(vec![
                    node(
                        "documents",
                        "Documents",
                        vec![
                            node(
                                "reports",
                                "Reports",
                                vec![
                                    node("q3", "Q3 Report", vec![]),
                                    node("q4", "Q4 Report", vec![]),
                                ],
                            ),
                            node("notes", "Notes", vec![]),
                        ],
                    ),
                    node(
                        "pictures",
                        "Pictures",
                        vec![node("logo", "logo.svg", vec![])],
                    ),
                    node("readme", "README.md", vec![]),
                ]),
            );
            *status.borrow_mut() = "a click toggles a branch or selects a leaf · Esc quits".into();
        }
        _ => {}
    }
    let notify = status.clone();
    app.on(element, "value-changed", move |event| {
        *notify.borrow_mut() = format!(
            "value-changed: {:?} (was {:?})",
            event.value, event.old_value
        );
    });
    let notify = status.clone();
    app.on(element, "timezone-changed", move |event| {
        *notify.borrow_mut() = format!(
            "timezone-changed: {:?} (was {:?})",
            event.value, event.old_value
        );
    });
    let notify = status.clone();
    app.on(element, "query-changed", move |event| {
        *notify.borrow_mut() = format!("query-changed: {:?}", event.value);
    });
    let notify = status.clone();
    app.on(element, "selected-changed", move |event| {
        *notify.borrow_mut() = format!("selected-changed: {:?}", event.value);
    });

    let line = status.clone();
    app.status_bar(move || line.borrow().clone());
    app.run()?;
    Ok(())
}
