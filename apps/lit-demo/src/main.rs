//! One Lit todo app, two hosts.
//!
//! - `cargo run -p uic_lit_demo` → the app in this terminal: the baked npm
//!   tree loads into the Boa host and ratatui paints it (type + Enter adds,
//!   Space toggles, Enter edits the selected row, arrows select, a click
//!   toggles, Esc quits).
//! - `cargo run -p uic_lit_demo -- serve` → the same sources on real lit:
//!   web_modules serves the baked dist, dev builds recompile the page
//!   sources live (`WEB_MODULES_EMBEDDED=1` forces the embedded bake).
//! - `UIC_LIT_DEMO_ADDR=host:port` moves the listener (default
//!   `127.0.0.1:8090`).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use include_dir::{include_dir, Dir};
use uic_js::JsHost;
use uic_tui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use uic_tui::{crossterm, ratatui};
use web_modules::{serve, Frontend};

static DIST: Dir = include_dir!("$OUT_DIR/dist");

const PACKAGE: &str = "@schuhkarton/lit-todo";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None => tui(),
        Some("serve") => serve_web(),
        Some(other) => Err(format!(
            "unknown mode {other:?}: no arguments runs the terminal app, `serve` the browser host"
        )
        .into()),
    }
}

/// The DOM key name for a terminal key event — printable characters flow
/// through to the app's keydown handler; CONTROL/ALT chords stay with the
/// terminal.
fn dom_key(key: &KeyEvent) -> Option<String> {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    Some(match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Up => "ArrowUp".into(),
        KeyCode::Down => "ArrowDown".into(),
        KeyCode::Left => "ArrowLeft".into(),
        KeyCode::Right => "ArrowRight".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        _ => return None,
    })
}

fn tui() -> Result<(), Box<dyn std::error::Error>> {
    let mut host = JsHost::new()?;
    host.load_package(Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")), PACKAGE)?;
    let node = host.mount("todo-app", &[])?;
    host.focus(node)?;

    let mut terminal = ratatui::try_init()?;
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    let result = run(&mut host, &mut terminal);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::try_restore()?;
    result
}

fn run(
    host: &mut JsHost,
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = "lit-todo via Boa · type + Enter adds · Space toggles · Enter edits · Esc quits";
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
                    ratatui::widgets::Paragraph::new(status)
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
                    host.dispatch_key(&name)?;
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

fn serve_web() -> Result<(), Box<dyn std::error::Error>> {
    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    let app = if std::env::var_os("WEB_MODULES_EMBEDDED").is_some() {
        Frontend::embedded(&DIST).router()
    } else {
        Frontend::embedded(&DIST).source(web.join("pages")).auto()
    };
    let addr = match std::env::var("UIC_LIT_DEMO_ADDR") {
        Ok(raw) => raw
            .parse::<SocketAddr>()
            .map_err(|err| format!("UIC_LIT_DEMO_ADDR {raw:?}: {err}"))?,
        Err(_) => SocketAddr::from(([127, 0, 0, 1], 8090)),
    };
    tokio::runtime::Runtime::new()?.block_on(serve(app, addr))?;
    Ok(())
}
