//! The exploration demo (#65): the unmodified `@alenaksu/json-viewer` npm
//! component, interactive in a real terminal.
//!
//! ```sh
//! cargo run -p uic_js --example json_viewer            # sample document
//! cargo run -p uic_js --example json_viewer data.json  # your own JSON
//! ```
//!
//! Arrows/Home/End navigate, ArrowRight/Left expand and collapse, a click
//! toggles — all handled by the component's own LitElement code. Esc quits.

use std::path::Path;
use std::time::Instant;

use uic_js::JsHost;
use uic_tui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use uic_tui::{crossterm, ratatui};

const SAMPLE: &str = r#"{
    "project": "schuhkarton/ui-components",
    "issue": 65,
    "claims": {
        "component": "@alenaksu/json-viewer, byte-unmodified npm dist",
        "engine": "Boa 0.21, embedded",
        "lit": "mocked at the module boundary",
        "pipeline": {"layout": "taffy", "paint": "ratatui"}
    },
    "interaction": ["ArrowUp", "ArrowDown", "ArrowRight", "ArrowLeft", "Home", "End", "click"],
    "active": true,
    "coverage": null
}"#;

fn dom_key(key: &KeyEvent) -> Option<&'static str> {
    Some(match key.code {
        KeyCode::Up => "ArrowUp",
        KeyCode::Down => "ArrowDown",
        KeyCode::Left => "ArrowLeft",
        KeyCode::Right => "ArrowRight",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        _ => return None,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(path)?,
        None => SAMPLE.to_string(),
    };

    let started = Instant::now();
    let mut host = JsHost::new()?;
    host.load_dist_dir(Path::new(env!("UIC_JS_VENDOR_DIST")), "json-viewer.js")?;
    let node = host.mount("json-viewer", &[("data", &data)])?;
    host.focus(node)?;
    let startup = started.elapsed();

    let mut terminal = ratatui::try_init()?;
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    let result = run(&mut host, &mut terminal, startup);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::try_restore()?;
    result
}

fn run(
    host: &mut JsHost,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    startup: std::time::Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = format!(
        "json-viewer via Boa · engine+mount {}ms · arrows navigate, click toggles, Esc quits",
        startup.as_millis()
    );
    loop {
        let state = host.state.clone();
        terminal.draw(|frame| {
            let mut s = state.borrow_mut();
            s.dirty = false;
            let focused = s.focused;
            let mut area = frame.area();
            if area.height > 1 {
                let status_area = ratatui::layout::Rect {
                    y: area.y + area.height - 1,
                    height: 1,
                    ..area
                };
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(status.as_str())
                        .style(ratatui::style::Style::new().dim()),
                    status_area,
                );
                area.height -= 1;
            }
            uic_tui::dom::paint_document(frame, area, &mut s.doc, focused);
        })?;

        match crossterm::event::read()? {
            Event::Key(key) => {
                if key.code == KeyCode::Esc
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                {
                    return Ok(());
                }
                if let Some(name) = dom_key(&key) {
                    host.dispatch_key(name)?;
                }
            }
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            }) => {
                let target = {
                    let state = host.state.borrow();
                    let mut area = terminal.get_frame().area();
                    area.height = area.height.saturating_sub(1);
                    uic_tui::dom::hit_test(&state.doc, area, column, row)
                };
                if let Some(target) = target {
                    host.click(target)?;
                }
            }
            _ => {}
        }
    }
}
