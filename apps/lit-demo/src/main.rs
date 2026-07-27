//! One Lit todo app, two hosts.
//!
//! - `cargo run -p uic_lit_demo` → the app in this terminal: the baked npm
//!   tree loads into the Boa host and ratatui paints it (type + Enter adds,
//!   Space toggles, Enter edits the selected row, arrows select, a click
//!   toggles, Esc quits).
//! - `cargo run -p uic_lit_demo -- serve` → the same sources on real lit:
//!   web_modules serves the baked dist, dev builds recompile the page
//!   sources live (`WEB_MODULES_EMBEDDED=1` forces the embedded bake).
//! - `cargo run -p uic_lit_demo -- live` → both at once, one state: the
//!   terminal app also serves the browser build, and every client shares
//!   the terminal's state over a WebSocket — edits anywhere land
//!   everywhere. The terminal shows the join URL as a scannable QR pane
//!   and listens on every interface so phones on the network can join.
//! - `UIC_LIT_DEMO_ADDR=host:port` moves the listener (default
//!   `127.0.0.1:8090`; live defaults to `0.0.0.0:8090`).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use include_dir::{include_dir, Dir};
use tokio::sync::{broadcast, mpsc};
use uic_dom::NodeId;
use uic_js::JsHost;
use uic_tui::crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use uic_tui::{crossterm, ratatui, KeyStroke};
use web_modules::{serve, Frontend};

mod pair;
mod qr_widget;

static DIST: Dir = include_dir!("$OUT_DIR/dist");

const PACKAGE: &str = "@schuhkarton/lit-todo";

/// The reactive properties the live bridge mirrors — one shared state
/// (ADR 0013's one-object story, spelled per property).
const STATE_FIELDS: [&str; 4] = ["draft", "editing", "items", "selected"];

type Terminal = ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>;

/// The status line, shared: the p2p mode's pairing thread narrates into it
/// while the terminal loop repaints on change.
type StatusLine = Arc<Mutex<String>>;

/// The `<pair-panel>` view state (ADR 0029): the pairing thread writes it,
/// the terminal loop mirrors it onto the mounted panel with `set_prop` —
/// the same component the browser renders, driven by properties.
#[derive(Clone, PartialEq, Default)]
struct PanelState {
    mode: String,
    link: String,
    token: String,
    status: String,
    connected: Option<bool>,
    reset_label: String,
}

impl PanelState {
    fn apply(&self, host: &mut JsHost, node: NodeId) -> Result<(), Box<dyn std::error::Error>> {
        host.set_prop(node, "mode", &serde_json::to_string(&self.mode)?)?;
        host.set_prop(node, "link", &serde_json::to_string(&self.link)?)?;
        host.set_prop(node, "token", &serde_json::to_string(&self.token)?)?;
        host.set_prop(node, "status", &serde_json::to_string(&self.status)?)?;
        let connected = match self.connected {
            Some(value) => value.to_string(),
            None => "null".into(),
        };
        host.set_prop(node, "connected", &connected)?;
        host.set_prop(
            node,
            "resetLabel",
            &serde_json::to_string(&self.reset_label)?,
        )?;
        Ok(())
    }
}

/// A button intent read off the mounted panel and forwarded to the pairing
/// thread (there is no event-out seam under Boa — ADR 0029).
enum Command {
    /// Start a fresh invite (the reset / "invite somebody else" button).
    Renew,
    /// Connect to a pasted invite (the "connect" button).
    Connect(String),
}

/// The terminal loop's handle on the mounted panel: mirror its state in,
/// forward its commands out. The deck's QR element rides along — the panel
/// state's link is its data (ADR 0030).
struct PanelDriver<'a> {
    node: NodeId,
    qr: NodeId,
    state: &'a Arc<Mutex<PanelState>>,
    commands: &'a mpsc::UnboundedSender<Command>,
}

/// The first element of a tag below the document root — how the p2p mode
/// finds the components its deck composed (the deck renders once and is
/// never re-committed, so the nodes stay put).
fn node_by_tag(host: &JsHost, tag: &str) -> Option<NodeId> {
    let state = host.state.borrow();
    let root = state.doc.root();
    let found = state.doc.descendants(root).find(|&node| {
        state
            .doc
            .element(node)
            .is_some_and(|el| el.tag().as_ref() == tag)
    });
    found
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None => tui(),
        Some("serve") => serve_web(),
        Some("live") => live(),
        Some("p2p") => p2p(std::env::args().nth(2)),
        Some(other) => Err(format!(
            "unknown mode {other:?}: no arguments runs the terminal app, `serve` the browser host, `live` both on one state, `p2p [link-or-token]` a serverless peer"
        )
        .into()),
    }
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

fn mounted_host() -> Result<(JsHost, NodeId), Box<dyn std::error::Error>> {
    let mut host = JsHost::new()?;
    host.load_package(Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")), PACKAGE)?;
    // The terminal is a dark surface: the mounted root opts into Bootstrap's
    // dark theme, and the mapped sheet's variables flip with it (the browser
    // page sets the same attribute from the OS preference instead).
    // No host-level focus: the app autofocuses its draft input, and a
    // focus here would steal the keyboard right back from it.
    let node = host.mount("todo-app", &[("data-bs-theme", "dark")])?;
    Ok((host, node))
}

fn tui() -> Result<(), Box<dyn std::error::Error>> {
    let (mut host, node) = mounted_host()?;
    let status: StatusLine = Arc::new(Mutex::new(
        "lit-todo via Boa · typing lands in the input · Enter adds/edits · Space toggles · F5/F6 reorder · Del removes · Esc quits".into(),
    ));
    with_terminal(|terminal| run(&mut host, node, terminal, &status, None, None, None))
}

/// Brackets the run with terminal setup and restore.
fn with_terminal(
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
/// whatever the terminal theme (ADR 0030).
struct QrPane {
    text: String,
    width: u16,
    height: u16,
    url: String,
    title: &'static str,
}

fn qr_pane(url: &str, title: &'static str) -> Option<QrPane> {
    let (text, width, height) = qr_widget::render_qr(url)?;
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
        // Two border columns plus one padding column per side.
        let pane_width = qr.width + 4;
        if area.width >= pane_width + MIN_APP_WIDTH && area.height >= qr.height + 3 {
            area.width -= pane_width;
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
                height: (qr.height + 3).min(area.height),
            };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(format!("{}\n{}", qr.text, qr.url))
                    .style(qr_widget::qr_card_style())
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

fn run(
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
                let command = host.prop_json(panel.node, "command")?;
                if command != "null" {
                    host.set_prop(panel.node, "command", "null")?;
                    let name: String = serde_json::from_str(&command)?;
                    match name.as_str() {
                        "invite" | "reset" => {
                            let _ = panel.commands.send(Command::Renew);
                        }
                        "connect" => {
                            let peer: String =
                                serde_json::from_str(&host.prop_json(panel.node, "peer")?)?;
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
                now.apply(host, panel.node)?;
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

// ---- the live bridge: one state between the terminal and every client ----

struct LiveBridge {
    inbound: mpsc::UnboundedReceiver<String>,
    outbound: broadcast::Sender<String>,
    latest: Arc<Mutex<String>>,
}

/// The canonical snapshot of the app's shared state, serialized through the
/// component's own accessors. Keys sort at every depth — npm-utils turns on
/// serde_json's preserve_order in this binary, and the sync tooling's codec
/// dedupes by byte equality (ADR 0024).
fn state_snapshot(host: &mut JsHost, node: NodeId) -> Result<String, Box<dyn std::error::Error>> {
    let mut state = serde_json::Map::new();
    for name in STATE_FIELDS {
        let json = host.prop_json(node, name)?;
        state.insert(name.to_string(), serde_json::from_str(&json)?);
    }
    let mut value = serde_json::Value::Object(state);
    value.sort_all_objects();
    Ok(value.to_string())
}

/// A client snapshot lands property by property; unknown or missing fields
/// stay untouched.
fn apply_state(
    host: &mut JsHost,
    node: NodeId,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let Ok(state) = serde_json::from_str::<serde_json::Value>(text) else {
        return Ok(());
    };
    for name in STATE_FIELDS {
        if let Some(value) = state.get(name) {
            host.set_prop(node, name, &value.to_string())?;
        }
    }
    Ok(())
}

/// Broadcasts the canonical state when it moved; clients suppress their own
/// echo, so convergence needs no origin tracking.
fn publish(
    host: &mut JsHost,
    node: NodeId,
    bridge: &LiveBridge,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = state_snapshot(host, node)?;
    let mut latest = bridge.latest.lock().expect("latest snapshot lock");
    if *latest != state {
        *latest = state.clone();
        let _ = bridge.outbound.send(state);
    }
    Ok(())
}

/// The address the LAN sees: a UDP socket "connected" to a public address
/// reveals the route's local IP without sending a packet.
fn lan_ip() -> std::net::IpAddr {
    std::net::UdpSocket::bind(("0.0.0.0", 0))
        .and_then(|socket| {
            socket.connect(("8.8.8.8", 80))?;
            socket.local_addr()
        })
        .map(|addr| addr.ip())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
}

fn live() -> Result<(), Box<dyn std::error::Error>> {
    let (mut host, node) = mounted_host()?;

    // Live listens on every interface so the QR reaches phones on the
    // network; UIC_LIT_DEMO_ADDR narrows it back down.
    let addr = listen_addr(SocketAddr::from(([0, 0, 0, 0], 8090)))?;
    // Fail before taking over the screen: a taken port must not degrade the
    // session into a terminal-only run with a dead URL in the status line.
    // (The probe closes before the server binds — a benign race.)
    drop(std::net::TcpListener::bind(addr).map_err(|err| format!("bind {addr}: {err}"))?);
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let (outbound_tx, _) = broadcast::channel(64);
    let latest = Arc::new(Mutex::new(state_snapshot(&mut host, node)?));
    let mut bridge = LiveBridge {
        inbound: inbound_rx,
        outbound: outbound_tx.clone(),
        latest: latest.clone(),
    };

    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    std::thread::spawn(move || {
        if let Err(err) = serve_live(web, addr, inbound_tx, outbound_tx, latest) {
            eprintln!("live server ended: {err}");
        }
    });

    let join_host = if addr.ip().is_unspecified() {
        lan_ip()
    } else {
        addr.ip()
    };
    let join_url = format!("http://{join_host}:{}/", addr.port());
    let qr = qr_pane(&join_url, "join");
    let status: StatusLine = Arc::new(Mutex::new(format!(
        "lit-todo live · everyone edits one state · {join_url} · Esc quits"
    )));
    with_terminal(|terminal| {
        run(
            &mut host,
            node,
            terminal,
            &status,
            qr.as_ref(),
            Some(&mut bridge),
            None,
        )
    })
}

// ---- the p2p peer: the terminal joins the serverless pairing (ADR 0028) --

/// The published pairing page an invite link opens;
/// `UIC_LIT_DEMO_P2P_PAGE` points it elsewhere (the dev server).
const P2P_PAGE: &str = "https://schuhkarton.github.io/ui-components/lit-demo/p2p/";

fn p2p(invite: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    // One mounted root, the p2p deck: the todo card and the shared pairing
    // panel (ADR 0029) stack in a column, the QR (ADR 0030) docks beside
    // them — responsive flexbox from the deck's own styles, no rect math.
    // The extra modules are baked in the package but off the todo-app entry
    // graph, so they load explicitly, import order inside-out; then the
    // mount upgrades the whole composition.
    let mut host = JsHost::new()?;
    host.load_package(Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")), PACKAGE)?;
    for module in ["qr-code.js", "pair-panel.js", "p2p-deck.js"] {
        let src = std::fs::read_to_string(
            Path::new(env!("UIC_LIT_DEMO_NPM_ROOT"))
                .join(PACKAGE)
                .join(module),
        )?;
        host.load_module(&format!("@schuhkarton/lit-todo/{module}"), &src)?;
    }
    host.mount("p2p-deck", &[])?;
    let node = node_by_tag(&host, "todo-app").ok_or("the deck mounts a todo-app")?;
    let panel = node_by_tag(&host, "pair-panel").ok_or("the deck mounts a pair-panel")?;
    // The deck's own QR — captured now, while the panel is still idle and
    // has not rendered its (terminal-hidden) inline copy; the deck never
    // re-commits, so the node stays put.
    let qr = node_by_tag(&host, "qr-code").ok_or("the deck mounts a qr-code")?;

    // Fail on garbage before taking over the screen.
    let opener = invite.as_deref().map(uic_sync::pair::link_payload);
    if let Some(payload) = &opener {
        if uic_sync::pair::payload_role(payload) != Some(uic_sync::pair::Role::Offer) {
            return Err(format!(
                "not a pairing invite: pass the link or the uics1. token ({invite:?})"
            )
            .into());
        }
    }

    let runtime = tokio::runtime::Runtime::new()?;
    let page = std::env::var("UIC_LIT_DEMO_P2P_PAGE").unwrap_or_else(|_| P2P_PAGE.into());

    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let (outbound_tx, _) = broadcast::channel(64);
    let latest = Arc::new(Mutex::new(state_snapshot(&mut host, node)?));
    let mut bridge = LiveBridge {
        inbound: inbound_rx,
        outbound: outbound_tx.clone(),
        latest: latest.clone(),
    };
    let status: StatusLine = Arc::new(Mutex::new("lit-todo p2p · Esc quits".to_string()));
    let panel_state = Arc::new(Mutex::new(PanelState::default()));
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    // The pairing state machine runs beside the terminal loop — live()'s
    // threading pattern — and drives the panel through `panel_state`.
    let thread_state = panel_state.clone();
    std::thread::spawn(move || {
        runtime.block_on(pairing_loop(
            opener,
            page,
            thread_state,
            command_rx,
            inbound_tx,
            outbound_tx,
            latest,
        ));
    });

    with_terminal(|terminal| {
        run(
            &mut host,
            node,
            terminal,
            &status,
            None,
            Some(&mut bridge),
            Some(PanelDriver {
                node: panel,
                qr,
                state: &panel_state,
                commands: &command_tx,
            }),
        )
    })
}

/// Builds an invite link the pairing page opens: the payload as a single
/// URL-safe fragment `#uics1.…`, so a chat app linkifies the whole URL.
fn invite_link(page: &str, payload: &str) -> String {
    format!("{page}#{payload}")
}

/// The terminal's pairing state machine: create an invite, wait for the peer
/// (its pasted token — pairing is a mutual exchange, ADR 0031), connect, and
/// stand ready to renew. Everything it learns lands in `panel_state`, which
/// the terminal loop mirrors onto the mounted `<pair-panel>`.
#[allow(clippy::too_many_arguments)]
async fn pairing_loop(
    mut opener: Option<String>,
    page: String,
    panel_state: Arc<Mutex<PanelState>>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    inbound: mpsc::UnboundedSender<String>,
    outbound: broadcast::Sender<String>,
    latest: Arc<Mutex<String>>,
) {
    let set = |state: PanelState| *panel_state.lock().expect("panel state") = state;
    let set_status =
        |text: &str| panel_state.lock().expect("panel state").status = text.to_string();

    loop {
        let swap = match pair::Swap::new().await {
            Ok(swap) => swap,
            Err(err) => {
                set(PanelState {
                    mode: "failed".into(),
                    status: format!("pairing setup failed: {err}"),
                    reset_label: "try again".into(),
                    ..PanelState::default()
                });
                match commands.recv().await {
                    Some(_) => continue,
                    None => return,
                }
            }
        };

        // Pairing is a mutual exchange (ADR 0031): each side sends its invite
        // and opens the other's. An opener already holds the peer's payload
        // from the link it opened; an inviter waits for the peer's token.
        let answering = opener.is_some();
        let link = invite_link(&page, &swap.payload);
        eprintln!("invite link:  {link}");
        set(PanelState {
            mode: "invite".into(),
            link,
            token: swap.payload.clone(),
            status: if answering {
                "opened their invite — send your token back so they connect too".into()
            } else {
                "send your invite, then paste their token to connect".into()
            },
            connected: None,
            reset_label: "start over".into(),
        });

        // Find the peer: an opener holds it already; an inviter waits for a
        // pasted token (the panel's connect button) or a renew.
        let peer_payload: Option<String> = if let Some(peer) = opener.take() {
            Some(peer)
        } else {
            match commands.recv().await {
                Some(Command::Connect(peer)) => Some(uic_sync::pair::link_payload(&peer)),
                Some(Command::Renew) => {
                    swap.close().await;
                    None
                }
                None => return,
            }
        };
        let Some(peer_payload) = peer_payload else {
            continue;
        };

        set_status("connecting…");
        let closed: Arc<dyn Fn(String) + Send + Sync> = {
            let panel_state = panel_state.clone();
            Arc::new(move |text: String| {
                let mut state = panel_state.lock().expect("panel state");
                state.mode = "dropped".into();
                state.connected = Some(false);
                state.status = text;
                state.reset_label = "invite somebody else".into();
            })
        };
        match swap
            .connect(
                &peer_payload,
                inbound.clone(),
                outbound.clone(),
                latest.clone(),
                closed,
            )
            .await
        {
            Ok(()) => set(PanelState {
                mode: "connected".into(),
                status: "paired — one list, two ends".into(),
                connected: Some(true),
                reset_label: "invite somebody else".into(),
                ..PanelState::default()
            }),
            Err(err) => set(PanelState {
                mode: "failed".into(),
                status: format!("pairing failed: {err}"),
                reset_label: "start a new pairing".into(),
                ..PanelState::default()
            }),
        }

        // Stand ready: the reset button renews, a pasted token connects anew.
        match commands.recv().await {
            Some(Command::Renew) => {
                swap.close().await;
            }
            Some(Command::Connect(peer)) => {
                swap.close().await;
                opener = Some(uic_sync::pair::link_payload(&peer));
            }
            None => return,
        }
    }
}

/// The frontend router plus the live endpoints: `/live` answers the page
/// glue's probe, `/ws` carries state snapshots both ways.
fn serve_live(
    web: PathBuf,
    addr: SocketAddr,
    inbound: mpsc::UnboundedSender<String>,
    outbound: broadcast::Sender<String>,
    latest: Arc<Mutex<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = frontend(&web)
        .route("/live", axum::routing::get(|| async { "live" }))
        .route(
            "/ws",
            axum::routing::get(move |upgrade: WebSocketUpgrade| {
                let inbound = inbound.clone();
                let updates = outbound.subscribe();
                let latest = latest.clone();
                async move {
                    upgrade
                        .on_upgrade(move |socket| client_session(socket, inbound, updates, latest))
                }
            }),
        );
    tokio::runtime::Runtime::new()?.block_on(serve(app, addr))?;
    Ok(())
}

/// One connected browser: the canonical state greets it, then updates flow
/// out and client snapshots flow in.
async fn client_session(
    mut socket: WebSocket,
    inbound: mpsc::UnboundedSender<String>,
    mut updates: broadcast::Receiver<String>,
    latest: Arc<Mutex<String>>,
) {
    let hello = latest.lock().expect("latest snapshot lock").clone();
    if socket.send(Message::Text(hello.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            update = updates.recv() => match update {
                Ok(state) => {
                    if socket.send(Message::Text(state.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => {
                let Some(Ok(Message::Text(text))) = incoming else { break };
                if inbound.send(text.to_string()).is_err() {
                    break;
                }
            }
        }
    }
}

fn frontend(web: &Path) -> axum::Router {
    if std::env::var_os("WEB_MODULES_EMBEDDED").is_some() {
        Frontend::embedded(&DIST).router()
    } else {
        Frontend::embedded(&DIST).source(web.join("pages")).auto()
    }
}

fn listen_addr(default: SocketAddr) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    match std::env::var("UIC_LIT_DEMO_ADDR") {
        Ok(raw) => Ok(raw
            .parse::<SocketAddr>()
            .map_err(|err| format!("UIC_LIT_DEMO_ADDR {raw:?}: {err}"))?),
        Err(_) => Ok(default),
    }
}

fn serve_web() -> Result<(), Box<dyn std::error::Error>> {
    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    let addr = listen_addr(SocketAddr::from(([127, 0, 0, 1], 8090)))?;
    tokio::runtime::Runtime::new()?.block_on(serve(frontend(&web), addr))?;
    Ok(())
}
