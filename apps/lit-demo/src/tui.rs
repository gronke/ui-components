//! The terminal plumbing: setup/restore, the frame layout around the app
//! (status line, the docked QR pane), input polling and the event loop that
//! drives the mounted component — plus the `<pair-panel>` driver mirroring
//! a pairing session's view onto the DOM and its commands back out.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use uic_dom::NodeId;
use uic_js::JsHost;
use uic_sync::session::{Command, PanelState};
use uic_tui::crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use uic_tui::{crossterm, ratatui, KeyStroke};

use crate::live::{apply_state, publish, LiveBridge};

pub(crate) type Terminal = ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>;

/// The status line, shared: the p2p mode's pairing thread narrates into it
/// while the terminal loop repaints on change.
pub(crate) type StatusLine = Arc<Mutex<String>>;

/// The terminal loop's handle on the mounted panel: mirror its state in,
/// forward its commands out. The deck's QR element rides along — the panel
/// state's link is its data (ADR 0029).
pub(crate) struct PanelDriver<'a> {
    pub node: NodeId,
    pub qr: NodeId,
    pub state: &'a Arc<Mutex<PanelState>>,
    pub commands: &'a mpsc::UnboundedSender<Command>,
}

/// Mirrors a session view onto the mounted panel — the terminal's half of
/// the `<pair-panel>` property contract (ADR 0029). serde_json spells the
/// Option as true/false/null, exactly the tri-state `connected` expects.
fn apply_panel(
    state: &PanelState,
    host: &mut JsHost,
    node: NodeId,
) -> Result<(), Box<dyn std::error::Error>> {
    let props = [
        ("mode", serde_json::to_string(state.mode.as_str())?),
        ("link", serde_json::to_string(&state.link)?),
        ("status", serde_json::to_string(&state.status)?),
        ("connected", serde_json::to_string(&state.connected)?),
        ("resetLabel", serde_json::to_string(&state.reset_label)?),
    ];
    for (name, json) in &props {
        host.set_prop(node, name, json)?;
    }
    Ok(())
}

/// A string property off a mounted node — `prop_json` speaks JSON, so a
/// missing value reads as `null` and everything else decodes.
fn prop_string(
    host: &mut JsHost,
    node: NodeId,
    name: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let json = host.prop_json(node, name)?;
    if json == "null" {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&json)?))
}

/// The app's key policy over the shared vocabulary (`uic_tui::keys`):
/// printable characters and named keys flow through to the keydown handler,
/// CONTROL/ALT chords stay with the terminal, and F5/F6 alias the shifted
/// arrows so the component only knows the DOM contract.
fn app_key(stroke: KeyStroke) -> Option<KeyStroke> {
    if stroke.ctrl || stroke.alt {
        return None;
    }
    Some(match stroke.key.as_str() {
        "F5" => KeyStroke::shifted("ArrowUp"),
        "F6" => KeyStroke::shifted("ArrowDown"),
        _ => stroke,
    })
}

/// Brackets the run with terminal setup and restore.
pub(crate) fn with_terminal(
    run: impl FnOnce(&mut Terminal) -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::try_init()?;
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
    let result = run(&mut terminal);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::try_restore()?;
    result
}

/// The join URL as a scannable half-block code, painted black on white like
/// the shared widget — a camera wants dark modules on a light ground
/// whatever the terminal theme (ADR 0029).
pub(crate) struct QrPane {
    text: String,
    width: u16,
    height: u16,
    url: String,
    title: &'static str,
}

impl QrPane {
    /// The code plus two border columns and one padding column per side —
    /// `app_area` and `draw` must agree on the same answer.
    fn pane_width(&self) -> u16 {
        self.width + 4
    }

    /// The code plus the URL line and the two border rows.
    fn pane_height(&self) -> u16 {
        self.height + 3
    }
}

pub(crate) fn qr_pane(url: &str, title: &'static str) -> Option<QrPane> {
    let (text, width, height) = uic_tui::qr::render_qr(url)?;
    Some(QrPane {
        text,
        width,
        height,
        url: url.to_string(),
        title,
    })
}

/// The app never squeezes below this to make room for the join pane.
const MIN_APP_WIDTH: u16 = 40;

/// The app's rectangle after the status line and, when it fits, the join
/// pane — draw and mouse hit-testing share the same answer.
fn app_area(frame_area: ratatui::layout::Rect, qr: Option<&QrPane>) -> ratatui::layout::Rect {
    let mut area = frame_area;
    if area.height > 1 {
        area.height -= 1;
    }
    if let Some(qr) = qr {
        if area.width >= qr.pane_width() + MIN_APP_WIDTH && area.height >= qr.pane_height() {
            area.width -= qr.pane_width();
        }
    }
    area
}

fn draw(
    host: &JsHost,
    terminal: &mut Terminal,
    status: &StatusLine,
    qr: Option<&QrPane>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = host.state.clone();
    let status = status.lock().expect("status line").clone();
    terminal.draw(|frame| {
        let mut s = state.borrow_mut();
        s.dirty = false;
        let focused = s.focused;
        let full = frame.area();
        if full.height > 1 {
            let status_area = ratatui::layout::Rect {
                y: full.y + full.height - 1,
                height: 1,
                ..full
            };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(status.as_str())
                    .style(ratatui::style::Style::new().dim()),
                status_area,
            );
        }
        let area = app_area(full, qr);
        if area.width < full.width {
            let qr = qr.expect("a narrower app area implies the join pane");
            let pane = ratatui::layout::Rect {
                x: area.x + area.width,
                y: area.y,
                width: full.width - area.width,
                height: qr.pane_height().min(area.height),
            };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(format!("{}\n{}", qr.text, qr.url))
                    .style(uic_tui::qr::qr_card_style())
                    .block(
                        ratatui::widgets::Block::bordered()
                            .title(qr.title)
                            .padding(ratatui::widgets::Padding::horizontal(1)),
                    ),
                pane,
            );
        }
        uic_tui::dom::paint_document(frame, area, &mut s.doc, focused);
    })?;
    Ok(())
}

enum Input {
    Terminal(Event),
    Web(String),
    Idle,
}

/// Without a bridge the loop blocks on the terminal; with one it polls so
/// browser edits interleave with local keys.
fn next_input(bridge: Option<&mut LiveBridge>) -> Result<Input, Box<dyn std::error::Error>> {
    match bridge {
        Some(bridge) => {
            if let Ok(state) = bridge.inbound.try_recv() {
                return Ok(Input::Web(state));
            }
            if crossterm::event::poll(Duration::from_millis(50))? {
                return Ok(Input::Terminal(crossterm::event::read()?));
            }
            Ok(Input::Idle)
        }
        None => Ok(Input::Terminal(crossterm::event::read()?)),
    }
}

/// Two quick clicks on one node synthesize a dblclick, the browser's own
/// click, click, dblclick order.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

pub(crate) fn run(
    host: &mut JsHost,
    node: NodeId,
    terminal: &mut Terminal,
    status: &StatusLine,
    qr: Option<&QrPane>,
    mut bridge: Option<&mut LiveBridge>,
    panel: Option<PanelDriver>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Double clicks detect by cell, not node: a click's re-render swaps
    // the subtree, so node identities never survive between the two.
    let mut last_click: Option<(u16, u16, std::time::Instant)> = None;
    let mut last_status = status.lock().expect("status line").clone();
    let mut last_panel = PanelState::default();
    draw(host, terminal, status, qr)?;
    loop {
        let changed = match next_input(bridge.as_deref_mut())? {
            Input::Idle => false,
            Input::Web(state) => {
                apply_state(host, node, &state)?;
                true
            }
            Input::Terminal(Event::Key(key)) => match KeyStroke::from_crossterm(&key) {
                Some(stroke) if stroke.is_quit() => return Ok(()),
                Some(stroke) => match app_key(stroke) {
                    Some(stroke) => {
                        host.dispatch(&stroke)?;
                        true
                    }
                    None => false,
                },
                None => false,
            },
            Input::Terminal(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                ..
            })) => {
                let target = {
                    let state = host.state.borrow();
                    let area = app_area(terminal.get_frame().area(), qr);
                    uic_tui::dom::hit_test(&state.doc, area, column, row)
                };
                if let Some(target) = target {
                    host.click_at(target, column, row)?;
                    let doubled = last_click.is_some_and(|(col, row_at, at)| {
                        col == column && row_at == row && at.elapsed() < DOUBLE_CLICK
                    });
                    if doubled {
                        // The click's re-render may have swapped the node;
                        // resolve the cell fresh, like the click itself did.
                        let fresh = {
                            let state = host.state.borrow();
                            let area = app_area(terminal.get_frame().area(), qr);
                            uic_tui::dom::hit_test(&state.doc, area, column, row)
                        };
                        if let Some(fresh) = fresh {
                            host.dblclick(fresh)?;
                        }
                        last_click = None;
                    } else {
                        last_click = Some((column, row, std::time::Instant::now()));
                    }
                }
                target.is_some()
            }
            Input::Terminal(Event::Resize(..)) => true,
            Input::Terminal(_) => false,
        };
        if changed {
            if let Some(bridge) = bridge.as_deref_mut() {
                publish(host, node, bridge)?;
            }
            // A click may have set the panel's command property; forward the
            // intent to the pairing thread and clear it (Boa has no events).
            if let Some(panel) = panel.as_ref() {
                if let Some(name) = prop_string(host, panel.node, "command")? {
                    host.set_prop(panel.node, "command", "null")?;
                    match name.as_str() {
                        "invite" | "reset" => {
                            let _ = panel.commands.send(Command::Renew);
                        }
                        "connect" => {
                            let peer = prop_string(host, panel.node, "peer")?.unwrap_or_default();
                            let _ = panel.commands.send(Command::Connect(peer));
                        }
                        // copy-* / scan have no terminal effect (the link is
                        // selectable text; no clipboard, no camera).
                        _ => {}
                    }
                }
            }
        }
        // The p2p pairing thread narrates into the shared status line —
        // its changes repaint too, not only the app's own.
        let status_changed = {
            let now = status.lock().expect("status line");
            if *now != last_status {
                last_status = now.clone();
                true
            } else {
                false
            }
        };
        // The pairing thread also writes the panel's state; mirror it onto
        // the mounted component when it moves.
        let panel_changed = if let Some(panel) = panel.as_ref() {
            let now = panel.state.lock().expect("panel state").clone();
            if now != last_panel {
                apply_panel(&now, host, panel.node)?;
                // The deck's QR shows the same invite; an unchanged link is
                // a no-op re-set (the property dirty check absorbs it).
                host.set_prop(panel.qr, "data", &serde_json::to_string(&now.link)?)?;
                last_panel = now;
                true
            } else {
                false
            }
        } else {
            false
        };
        if changed || status_changed || panel_changed {
            draw(host, terminal, status, qr)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use uic_sync::session::PanelMode;

    fn pane() -> QrPane {
        QrPane {
            text: String::new(),
            width: 60,
            height: 31,
            url: "http://example/".into(),
            title: "join",
        }
    }

    #[test]
    fn the_app_area_docks_the_pane_only_when_it_fits() {
        // Wide enough: the pane's 64 columns leave the app 116 of 180.
        let area = app_area(Rect::new(0, 0, 180, 45), Some(&pane()));
        assert_eq!((area.width, area.height), (116, 44));
        // The status line always takes the last row.
        let plain = app_area(Rect::new(0, 0, 180, 45), None);
        assert_eq!((plain.width, plain.height), (180, 44));
        // Too narrow for pane + MIN_APP_WIDTH: the app keeps every column.
        let narrow = app_area(Rect::new(0, 0, 100, 45), Some(&pane()));
        assert_eq!(narrow.width, 100);
        // Too short for the pane: same.
        let short = app_area(Rect::new(0, 0, 180, 20), Some(&pane()));
        assert_eq!(short.width, 180);
    }

    #[test]
    fn the_session_view_mirrors_onto_the_mounted_panel() {
        let mut host = JsHost::new().unwrap();
        host.load_package(
            std::path::Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")),
            crate::PACKAGE,
        )
        .unwrap();
        for module in ["theme.js", "qr-code.js", "pair-panel.js"] {
            let src = std::fs::read_to_string(
                std::path::Path::new(env!("UIC_LIT_DEMO_NPM_ROOT"))
                    .join(crate::PACKAGE)
                    .join(module),
            )
            .unwrap();
            host.load_module(&format!("{}/{module}", crate::PACKAGE), &src)
                .unwrap();
        }
        let panel = host.mount("pair-panel", &[]).unwrap();

        let view = PanelState {
            mode: PanelMode::Invite,
            link: "https://host/p2p/#abc".into(),
            status: "share the invite".into(),
            connected: None,
            reset_label: "start over".into(),
        };
        apply_panel(&view, &mut host, panel).unwrap();
        let html = host.state.borrow().doc.inner_html(panel);
        assert!(html.contains("share the invite"), "status mirrors: {html}");
        assert!(html.contains("start over"), "reset label mirrors: {html}");
        assert!(
            html.contains("https://host/p2p/#abc"),
            "the link mirrors: {html}"
        );
        assert_eq!(host.prop_json(panel, "connected").unwrap(), "null");
    }
}
