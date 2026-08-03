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
/// state's link is its data (ADR 0029) — and so do the navbar and the
/// deck's wrapper divs, the pairing-first screen gates.
pub(crate) struct PanelDriver<'a> {
    pub node: NodeId,
    pub qr: NodeId,
    pub navbar: NodeId,
    pub bar: NodeId,
    pub todo_pane: NodeId,
    pub pairing_pane: NodeId,
    pub state: &'a Arc<Mutex<PanelState>>,
    pub commands: &'a mpsc::UnboundedSender<Command>,
    /// The live wire's nominated route, written by the pairing thread —
    /// shown while the todo screen stands; the LAN address otherwise.
    pub endpoints: &'a Arc<Mutex<Option<String>>>,
    pub lan: String,
    /// The clipboard read throttle — p2p rides beside the panel it drives.
    pub clipboard: crate::clipboard::ClipboardWatch,
}

/// The pairing-first screen rule: the todo (with the navbar) shows while a
/// wire stands or just dropped — the badge goes red on a blip and the
/// disconnect control offers the way back — and the pairing card owns every
/// other mode.
fn todo_screen(mode: uic_sync::session::PanelMode) -> bool {
    use uic_sync::session::PanelMode;
    matches!(mode, PanelMode::Connected | PanelMode::Dropped)
}

fn set_hidden(host: &JsHost, node: NodeId, hidden: bool) {
    let mut state = host.state.borrow_mut();
    let handle = state.handle(node);
    if hidden {
        state.set_attribute(handle, "hidden", "");
    } else {
        state.remove_attribute(handle, "hidden");
    }
}

/// The first descendant matching a selector, resolved on the live document
/// — how the screen swap finds the input to hand focus to.
fn descendant_matching(host: &JsHost, root: NodeId, selector: &str) -> Option<NodeId> {
    let mut state = host.state.borrow_mut();
    let handle = state.handle(root);
    let first = state.query(handle, selector).ok()?.first().copied()?;
    state.node(first)
}

/// Swaps the deck between its two screens by toggling `hidden` on the plain
/// wrapper divs (attribute writes only — nothing re-commits, the todo's
/// live state stays put) and hands focus to the screen's input: keys go to
/// the focused node whether or not it is visible, so the handoff is what
/// keeps typing meaningful. Runs on every mode change — the pairing card
/// swaps its body per mode, so the focus target has to re-resolve anyway.
fn apply_screen(
    host: &mut JsHost,
    panel: &PanelDriver,
    todo: NodeId,
    mode: uic_sync::session::PanelMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let todo_screen = todo_screen(mode);
    set_hidden(host, panel.bar, !todo_screen);
    set_hidden(host, panel.todo_pane, !todo_screen);
    set_hidden(host, panel.pairing_pane, todo_screen);
    let target = if todo_screen {
        descendant_matching(host, todo, "input.draft")
    } else {
        // Invite renders the reply textarea; other pairing bodies have no
        // input — focus stays where it is until one appears.
        descendant_matching(host, panel.node, "textarea")
    };
    if let Some(target) = target {
        host.focus(target)?;
    }
    Ok(())
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
        ("step", state.step.as_u8().to_string()),
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

/// Brackets the run with terminal setup and restore. Bracketed paste makes
/// a paste arrive as one `Event::Paste` instead of a key hail — one bulk
/// insert, one render.
pub(crate) fn with_terminal(
    run: impl FnOnce(&mut Terminal) -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::try_init()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableBracketedPaste
    )?;
    let result = run(&mut terminal);
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableBracketedPaste,
        crossterm::event::DisableMouseCapture
    );
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

/// A dialog the loop shows modally and who asked for it: a JS component's
/// alert/confirm/prompt (answered back through the runtime by id) or the
/// host's own question, whose verdict drives a [`HostIntent`].
struct ActiveDialog {
    dialog: uic_tui::dialog::Dialog,
    source: DialogSource,
    /// The step the host question belongs to; when the session leaves it,
    /// the question is moot and the dialog auto-dismisses.
    step: u8,
}

enum DialogSource {
    /// A runtime request; the id routes the answer to its parked promise.
    Js(u32),
    /// The host's own question and what an accept does.
    Host(HostIntent),
}

/// What a host dialog's "accept" carries out — today, connecting to a
/// pairing credential that arrived mid-pairing (the conflict prompt).
enum HostIntent {
    AcceptPeer(String),
}

/// One clipboard read, routed. While the pairing screen shows, a peer
/// credential that is not the one we already dial continues the step: step
/// 1 (or a reply naming our current invite) dials it straight, anything
/// else mid-pairing raises the conflict prompt. Returns whether the screen
/// needs a repaint (a dialog opened). Contents are never logged.
fn clipboard_tick(
    host: &JsHost,
    panel: &mut PanelDriver,
    last_panel: &PanelState,
    last_peer: &mut Option<String>,
    dialog: &mut Option<ActiveDialog>,
) -> bool {
    if !matches!(last_panel.mode, uic_sync::session::PanelMode::Invite) {
        return false;
    }
    // The read goes through the mocked DOM's clipboard backend — the same
    // one navigator.clipboard exposes to JS. `clipboard` and `commands` are
    // disjoint fields, so the throttle borrows mutably while the send stays.
    let Some(text) = panel
        .clipboard
        .poll(std::time::Instant::now(), || host.clipboard_read())
    else {
        return false;
    };
    let own = uic_sync::pair::link_payload(&last_panel.link);
    let Some(find) = crate::clipboard::classify(&text, &own) else {
        return false;
    };
    if last_peer.as_deref() == Some(find.payload.as_str()) {
        return false; // already dialing this one
    }
    let step = last_panel.step.as_u8();
    if step <= 1 || find.reply_to_us {
        *last_peer = Some(find.payload);
        let _ = panel.commands.send(Command::Connect(text));
        return false;
    }
    if dialog.is_some() {
        return false; // one question at a time
    }
    let mut prompt = uic_tui::dialog::Dialog::confirm(
        "a different pairing link arrived — accept the new pairing?",
    );
    prompt.ok_label = "accept".into();
    prompt.cancel_label = "keep waiting".into();
    *dialog = Some(ActiveDialog {
        dialog: prompt,
        source: DialogSource::Host(HostIntent::AcceptPeer(text)),
        step,
    });
    true
}

/// The JSON a JS dialog answers with — the browser's own return shapes.
fn dialog_answer(dialog: &uic_tui::dialog::Dialog, ok: bool) -> String {
    use uic_tui::dialog::DialogKind;
    match dialog.kind {
        DialogKind::Alert => "null".to_string(),
        DialogKind::Confirm => ok.to_string(),
        DialogKind::Prompt if ok => {
            serde_json::to_string(&dialog.input).unwrap_or_else(|_| "null".into())
        }
        DialogKind::Prompt => "null".to_string(),
    }
}

fn draw(
    host: &JsHost,
    terminal: &mut Terminal,
    status: &StatusLine,
    qr: Option<&QrPane>,
    dialog: Option<&uic_tui::dialog::Dialog>,
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
        // Painted last so it overlays the document, the QR pane and the
        // status line — the buffer's last write wins (the popup rule).
        if let Some(dialog) = dialog {
            uic_tui::dialog::paint_dialog(frame, full, dialog);
        }
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
    mut panel: Option<PanelDriver>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Double clicks detect by cell, not node: a click's re-render swaps
    // the subtree, so node identities never survive between the two.
    let mut last_click: Option<(u16, u16, std::time::Instant)> = None;
    let mut last_status = status.lock().expect("status line").clone();
    let mut last_panel = PanelState::default();
    // The last peer the loop dialed — a clipboard find or paste matching it
    // is not a conflict, just the credential we already expect.
    let mut last_peer: Option<String> = None;
    // A modal dialog — a component's alert/confirm/prompt, or a host
    // question. While one shows, every key is its own and clicks are
    // swallowed; the session keeps mirroring beneath it.
    let mut dialog: Option<ActiveDialog> = None;
    draw(host, terminal, status, qr, None)?;
    loop {
        // Coalesce a buffered burst — an unbracketed paste's key hail,
        // held-key autorepeat — into one publish/command/draw tail: handle
        // everything already queued, then run the tail once.
        let mut input = next_input(bridge.as_deref_mut())?;
        let mut changed = false;
        loop {
            changed |= match input {
                // The idle tick is where the clipboard watch reads: a matching
                // credential continues the step, a different one mid-pairing
                // asks first. Only while the pairing screen shows.
                Input::Idle => match panel.as_mut() {
                    Some(driver) => {
                        clipboard_tick(host, driver, &last_panel, &mut last_peer, &mut dialog)
                    }
                    None => false,
                },
                Input::Web(state) => {
                    apply_state(host, node, &state)?;
                    true
                }
                // A dialog owns the keyboard first — before is_quit, or Escape
                // would quit the app instead of closing the box. Ctrl+C still
                // hard-quits; ^D and the page never see these keys.
                Input::Terminal(Event::Key(key)) if dialog.is_some() => {
                    match KeyStroke::from_crossterm(&key) {
                        Some(stroke) if stroke.ctrl && stroke.key == "c" => return Ok(()),
                        Some(stroke) => {
                            use uic_tui::dialog::DialogOutcome;
                            let active = dialog.as_mut().expect("a shown dialog");
                            match active.dialog.key(&stroke) {
                                DialogOutcome::Open => {}
                                outcome => {
                                    let ok = outcome == DialogOutcome::Ok;
                                    let ActiveDialog {
                                        dialog: box_,
                                        source,
                                        ..
                                    } = dialog.take().expect("a shown dialog");
                                    match source {
                                        DialogSource::Js(id) => {
                                            host.answer_dialog(id, &dialog_answer(&box_, ok))?;
                                        }
                                        // The conflict prompt: accepting dials the
                                        // credential that arrived mid-pairing.
                                        DialogSource::Host(HostIntent::AcceptPeer(text)) => {
                                            if ok {
                                                if let Some(panel) = panel.as_ref() {
                                                    let _ = panel
                                                        .commands
                                                        .send(Command::Connect(text.clone()));
                                                }
                                                last_peer =
                                                    Some(uic_sync::pair::link_payload(&text));
                                            }
                                        }
                                    }
                                }
                            }
                            true
                        }
                        None => false,
                    }
                }
                Input::Terminal(Event::Key(key)) => match KeyStroke::from_crossterm(&key) {
                    Some(stroke) if stroke.is_quit() => return Ok(()),
                    // ^D disconnects app-globally: control chords never reach
                    // the focused widget, so typing cannot collide with it. The
                    // session answers with a close and a fresh invite — the
                    // pairing screen comes back on the mode mirror below.
                    Some(stroke) if stroke.ctrl && stroke.key == "d" && panel.is_some() => {
                        if let Some(panel) = panel.as_ref() {
                            let _ = panel.commands.send(Command::Renew);
                        }
                        false
                    }
                    Some(stroke) => match app_key(stroke) {
                        Some(stroke) => {
                            host.dispatch(&stroke)?;
                            true
                        }
                        None => false,
                    },
                    None => false,
                },
                // A dialog swallows clicks — it is keyboard-driven; nothing
                // beneath it takes the pointer.
                Input::Terminal(Event::Mouse(_)) if dialog.is_some() => false,
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
                // A paste while a prompt shows belongs to its input line.
                Input::Terminal(Event::Paste(text)) if dialog.is_some() => {
                    dialog.as_mut().expect("a shown dialog").dialog.paste(&text);
                    true
                }
                // One bulk insert into the focused widget and one `input`
                // event — a pasted pairing token lands whole, in one render.
                Input::Terminal(Event::Paste(text)) => host.paste(&text)?,
                Input::Terminal(_) => false,
            };
            if !crossterm::event::poll(Duration::ZERO)? {
                break;
            }
            input = Input::Terminal(crossterm::event::read()?);
        }
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
                            // The connect control lives only on step 1, so a
                            // paste is always the peer we mean to dial.
                            last_peer = Some(uic_sync::pair::link_payload(&peer));
                            let _ = panel.commands.send(Command::Connect(peer));
                        }
                        // copy-* / scan have no terminal effect (the link is
                        // selectable text; no clipboard, no camera).
                        _ => {}
                    }
                }
                // The navbar speaks the same polled-command seam; its
                // disconnect is ^D's pointer twin.
                if let Some(name) = prop_string(host, panel.navbar, "command")? {
                    host.set_prop(panel.navbar, "command", "null")?;
                    if name == "disconnect" {
                        let _ = panel.commands.send(Command::Renew);
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
                // The navbar wears the same view: the badge off `connected`,
                // the narration off `status`, and the wire's real route
                // while one stands.
                host.set_prop(
                    panel.navbar,
                    "connected",
                    &serde_json::to_string(&now.connected)?,
                )?;
                host.set_prop(panel.navbar, "status", &serde_json::to_string(&now.status)?)?;
                let address = panel
                    .endpoints
                    .lock()
                    .expect("endpoints slot")
                    .clone()
                    .filter(|_| todo_screen(now.mode))
                    .unwrap_or_else(|| panel.lan.clone());
                host.set_prop(panel.navbar, "address", &serde_json::to_string(&address)?)?;
                // A mode move swaps the screen (and the focus with it) —
                // after apply_panel, so the new body exists to focus into.
                if now.mode != last_panel.mode {
                    apply_screen(host, panel, node, now.mode)?;
                }
                // A conflict prompt outlives its moment when the step it
                // asked about moves on — the question is moot, drop it.
                if let Some(active) = &dialog {
                    if matches!(active.source, DialogSource::Host(_))
                        && active.step != now.step.as_u8()
                    {
                        dialog = None;
                    }
                }
                last_panel = now;
                true
            } else {
                false
            }
        } else {
            false
        };
        // A component may have asked a question this iteration; show the
        // oldest waiting one when the box is free (one at a time — the
        // queue holds the rest).
        let dialog_opened = if dialog.is_none() {
            match host.take_dialog_request() {
                Some(request) => {
                    dialog = Some(ActiveDialog {
                        dialog: dialog_from(&request),
                        source: DialogSource::Js(request.id),
                        step: last_panel.step.as_u8(),
                    });
                    true
                }
                None => false,
            }
        } else {
            false
        };
        if changed || status_changed || panel_changed || dialog_opened {
            let shown = dialog.as_ref().map(|active| &active.dialog);
            draw(host, terminal, status, qr, shown)?;
        }
    }
}

/// The dialog box for a runtime request — the browser's own default
/// button focus (confirm and prompt land on ok).
fn dialog_from(request: &uic_js::DialogRequest) -> uic_tui::dialog::Dialog {
    use uic_js::DialogKind;
    match request.kind {
        DialogKind::Alert => uic_tui::dialog::Dialog::alert(&request.message),
        DialogKind::Confirm => uic_tui::dialog::Dialog::confirm(&request.message),
        DialogKind::Prompt => uic_tui::dialog::Dialog::prompt(
            &request.message,
            request.default.as_deref().unwrap_or(""),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use uic_sync::session::{PanelMode, Step};
    use uic_tui::dialog::Dialog;

    #[test]
    fn dialog_answers_carry_the_browser_return_shapes() {
        assert_eq!(dialog_answer(&Dialog::alert("done"), true), "null");
        assert_eq!(dialog_answer(&Dialog::confirm("sure?"), true), "true");
        assert_eq!(dialog_answer(&Dialog::confirm("sure?"), false), "false");
        let mut prompt = Dialog::prompt("who?", "");
        prompt.input = "world".into();
        assert_eq!(dialog_answer(&prompt, true), "\"world\"");
        assert_eq!(dialog_answer(&prompt, false), "null");
    }

    // A component's confirm rides the queue, the loop's dialog box answers
    // it, and the awaiting component continues — the whole JS-dialog path
    // minus the terminal I/O.
    #[test]
    fn a_component_confirm_answers_through_the_dialog_box() {
        let mut host = JsHost::new().unwrap();
        host.load_module(
            "test:asks",
            r#"
            import { html, LitElement } from 'lit';
            class AsksDialog extends LitElement {
                static properties = { verdict: {} };
                constructor() {
                    super();
                    this.verdict = 'undecided';
                    void confirm('accept?').then((v) => { this.verdict = v ? 'yes' : 'no'; });
                }
                render() { return html`<span>${this.verdict}</span>`; }
            }
            customElements.define('asks-dialog', AsksDialog);
            "#,
        )
        .unwrap();
        let node = host.mount("asks-dialog", &[]).unwrap();

        let request = host.take_dialog_request().expect("the component asked");
        let dialog = dialog_from(&request);
        assert_eq!(dialog.kind, uic_tui::dialog::DialogKind::Confirm);
        host.answer_dialog(request.id, &dialog_answer(&dialog, true))
            .unwrap();
        assert_eq!(host.prop_json(node, "verdict").unwrap(), "\"yes\"");
    }

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

    // Load a baked module under its package specifier — the moved pairing UI
    // resolves from @gronke/uic-sync (ADR 0029), the deck from the app.
    fn load_module_from(host: &mut JsHost, package: &str, file: &str) {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("UIC_LIT_DEMO_NPM_ROOT"))
                .join(package)
                .join(file),
        )
        .unwrap();
        host.load_module(&format!("{package}/{file}"), &src)
            .unwrap();
    }

    fn deck_host() -> (JsHost, PanelDriverNodes) {
        let mut host = JsHost::new().unwrap();
        host.load_package(
            std::path::Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")),
            crate::PACKAGE,
        )
        .unwrap();
        for module in [
            "theme.js",
            "qr-code.js",
            "pair-panel.js",
            "status-navbar.js",
        ] {
            load_module_from(&mut host, "@gronke/uic-sync", module);
        }
        load_module_from(&mut host, crate::PACKAGE, "p2p-deck.js");
        host.mount("p2p-deck", &[]).unwrap();
        let nodes = PanelDriverNodes {
            todo: crate::node_by_tag(&host, "todo-app").unwrap(),
            panel: crate::node_by_tag(&host, "pair-panel").unwrap(),
            qr: crate::node_by_tag(&host, "qr-code").unwrap(),
            navbar: crate::node_by_tag(&host, "status-navbar").unwrap(),
            bar: crate::node_by_class(&host, "bar").unwrap(),
            todo_pane: crate::node_by_class(&host, "todo-pane").unwrap(),
            pairing_pane: crate::node_by_class(&host, "pairing-pane").unwrap(),
        };
        (host, nodes)
    }

    struct PanelDriverNodes {
        todo: NodeId,
        panel: NodeId,
        qr: NodeId,
        navbar: NodeId,
        bar: NodeId,
        todo_pane: NodeId,
        pairing_pane: NodeId,
    }

    fn hidden(host: &JsHost, node: NodeId) -> bool {
        let mut state = host.state.borrow_mut();
        let handle = state.handle(node);
        state.has_attribute(handle, "hidden")
    }

    #[test]
    fn the_screens_swap_on_mode_and_focus_follows() {
        let (mut host, nodes) = deck_host();
        let (tx, _rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(PanelState::default()));
        let endpoints = Arc::new(Mutex::new(None));
        let driver = PanelDriver {
            node: nodes.panel,
            qr: nodes.qr,
            navbar: nodes.navbar,
            bar: nodes.bar,
            todo_pane: nodes.todo_pane,
            pairing_pane: nodes.pairing_pane,
            state: &state,
            commands: &tx,
            endpoints: &endpoints,
            lan: "192.0.2.1".into(),
            clipboard: crate::clipboard::ClipboardWatch::default(),
        };

        // Boot: pairing-first — the deck ships the todo and the bar hidden.
        assert!(hidden(&host, nodes.todo_pane), "todo hidden at boot");
        assert!(hidden(&host, nodes.bar), "bar hidden at boot");
        assert!(
            !hidden(&host, nodes.pairing_pane),
            "pairing visible at boot"
        );

        // The invite arrives: the pairing screen stays and the focus lands
        // in the reply textarea (after apply_panel rendered the body).
        let invite = PanelState {
            mode: PanelMode::Invite,
            link: "https://host/p2p/#abc".into(),
            status: "share the invite".into(),
            connected: None,
            reset_label: "start over".into(),
            step: Step::Init,
        };
        apply_panel(&invite, &mut host, driver.node).unwrap();
        apply_screen(&mut host, &driver, nodes.todo, invite.mode).unwrap();
        let textarea = descendant_matching(&host, driver.node, "textarea").unwrap();
        assert_eq!(host.state.borrow().focused, Some(textarea));

        // Connected: the screens flip, the navbar mirrors, the draft takes
        // the keyboard.
        let connected = PanelState {
            mode: PanelMode::Connected,
            link: String::new(),
            status: "paired — one list, two ends".into(),
            connected: Some(true),
            reset_label: "invite somebody else".into(),
            step: Step::Connect,
        };
        apply_panel(&connected, &mut host, driver.node).unwrap();
        host.set_prop(
            driver.navbar,
            "connected",
            &serde_json::to_string(&connected.connected).unwrap(),
        )
        .unwrap();
        apply_screen(&mut host, &driver, nodes.todo, connected.mode).unwrap();
        assert!(!hidden(&host, nodes.todo_pane), "todo shows connected");
        assert!(!hidden(&host, nodes.bar), "bar shows connected");
        assert!(hidden(&host, nodes.pairing_pane), "pairing hides connected");
        let draft = descendant_matching(&host, nodes.todo, "input.draft").unwrap();
        assert_eq!(host.state.borrow().focused, Some(draft));
        let navbar_html = host.state.borrow().doc.inner_html(driver.navbar);
        assert!(
            navbar_html.contains("text-bg-success"),
            "the badge turns green: {navbar_html}"
        );

        // Back to pairing (a disconnect renewed): flip again, textarea back.
        apply_panel(&invite, &mut host, driver.node).unwrap();
        apply_screen(&mut host, &driver, nodes.todo, invite.mode).unwrap();
        assert!(hidden(&host, nodes.todo_pane));
        assert!(!hidden(&host, nodes.pairing_pane));
        let textarea = descendant_matching(&host, driver.node, "textarea").unwrap();
        assert_eq!(host.state.borrow().focused, Some(textarea));
    }

    #[test]
    fn typing_after_the_focus_handoff_lands_in_the_reply_box() {
        let (mut host, nodes) = deck_host();
        let (tx, _rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(PanelState::default()));
        let endpoints = Arc::new(Mutex::new(None));
        let driver = PanelDriver {
            node: nodes.panel,
            qr: nodes.qr,
            navbar: nodes.navbar,
            bar: nodes.bar,
            todo_pane: nodes.todo_pane,
            pairing_pane: nodes.pairing_pane,
            state: &state,
            commands: &tx,
            endpoints: &endpoints,
            lan: "192.0.2.1".into(),
            clipboard: crate::clipboard::ClipboardWatch::default(),
        };
        let invite = PanelState {
            mode: PanelMode::Invite,
            link: "https://host/p2p/#abc".into(),
            status: "share the invite".into(),
            connected: None,
            reset_label: "start over".into(),
            step: Step::Init,
        };
        apply_panel(&invite, &mut host, driver.node).unwrap();
        apply_screen(&mut host, &driver, nodes.todo, invite.mode).unwrap();

        for key in ["a", "b", "c"] {
            host.dispatch_key(key).unwrap();
        }
        assert_eq!(host.prop_json(driver.node, "peer").unwrap(), "\"abc\"");
    }

    #[test]
    fn a_pasted_credential_lands_whole_in_the_reply_box() {
        let (mut host, nodes) = deck_host();
        let (tx, _rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(PanelState::default()));
        let endpoints = Arc::new(Mutex::new(None));
        let driver = PanelDriver {
            node: nodes.panel,
            qr: nodes.qr,
            navbar: nodes.navbar,
            bar: nodes.bar,
            todo_pane: nodes.todo_pane,
            pairing_pane: nodes.pairing_pane,
            state: &state,
            commands: &tx,
            endpoints: &endpoints,
            lan: "192.0.2.1".into(),
            clipboard: crate::clipboard::ClipboardWatch::default(),
        };
        let invite = PanelState {
            mode: PanelMode::Invite,
            link: "https://host/p2p/#abc".into(),
            status: "share the invite".into(),
            connected: None,
            reset_label: "start over".into(),
            step: Step::Init,
        };
        apply_panel(&invite, &mut host, driver.node).unwrap();
        apply_screen(&mut host, &driver, nodes.todo, invite.mode).unwrap();

        // One paste, one input event — the whole credential in one render,
        // not a keystroke hail.
        let link = "https://host/p2p/#dWljMVBhc3RlZFRva2Vu";
        assert!(host.paste(link).unwrap());
        assert_eq!(
            host.prop_json(driver.node, "peer").unwrap(),
            format!("{link:?}")
        );
    }

    #[test]
    fn the_navbar_disconnect_writes_the_polled_command() {
        let (mut host, nodes) = deck_host();
        let button = {
            let state = host.state.borrow();
            state
                .doc
                .find_element(nodes.navbar, |el| {
                    el.attr("class").is_some_and(|c| c.contains("disconnect"))
                })
                .expect("the disconnect button")
        };
        host.click(button).unwrap();
        assert_eq!(
            host.prop_json(nodes.navbar, "command").unwrap(),
            "\"disconnect\""
        );
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
            load_module_from(&mut host, "@gronke/uic-sync", module);
        }
        let panel = host.mount("pair-panel", &[]).unwrap();

        let view = PanelState {
            mode: PanelMode::Invite,
            link: "https://host/p2p/#abc".into(),
            status: "share the invite".into(),
            connected: None,
            reset_label: "start over".into(),
            step: Step::Acknowledge,
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
        assert_eq!(
            host.prop_json(panel, "step").unwrap(),
            "2",
            "the step mirrors"
        );
    }
}
