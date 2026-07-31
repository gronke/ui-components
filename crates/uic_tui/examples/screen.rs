//! Renders one <input-date> frame into an in-memory terminal and prints it —
//! the quickest way to see what the TUI backend produces.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui_components_tui::link();
    let mut app = uic_tui::App::from_terminal(Terminal::new(TestBackend::new(44, 8))?);
    let element = app.mount("input-date")?;
    app.set_attr(element, "label", "Date of purchase");
    app.set_attr(element, "hint", "Format: YYYY-MM-DD");
    app.set_attr(element, "hide-time", "");
    app.set_attr(element, "value", "2026-07-07");
    app.draw()?;

    let buffer = app.terminal().backend().buffer();
    for y in 0..buffer.area.height {
        let row: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        println!("|{}|", row.trim_end());
    }
    Ok(())
}
