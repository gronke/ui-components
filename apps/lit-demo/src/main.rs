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

static DIST: Dir = include_dir!("$OUT_DIR/dist");

const PACKAGE: &str = "@schuhkarton/lit-todo";

/// The reactive properties the live bridge mirrors — one shared state
/// (ADR 0013's one-object story, spelled per property).
const STATE_FIELDS: [&str; 4] = ["draft", "editing", "items", "selected"];

type Terminal = ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None => tui(),
        Some("serve") => serve_web(),
        Some("live") => live(),
        Some(other) => Err(format!(
            "unknown mode {other:?}: no arguments runs the terminal app, `serve` the browser host, `live` both on one state"
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
    let node = host.mount("todo-app", &[("data-bs-theme", "dark")])?;
    host.focus(node)?;
    Ok((host, node))
}

fn tui() -> Result<(), Box<dyn std::error::Error>> {
    let (mut host, node) = mounted_host()?;
    let status = "lit-todo via Boa · type + Enter adds · Space toggles · Enter edits · F5/F6 reorder · Del removes · Esc quits";
    with_terminal(|terminal| run(&mut host, node, terminal, status, None, None))
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

/// The join URL as a scannable half-block code. Dark modules stay unpainted
/// (the terminal background), light ones paint in the foreground — standard
/// polarity on dark terminals.
struct QrPane {
    text: String,
    width: u16,
    height: u16,
    url: String,
}

fn qr_pane(url: &str) -> Option<QrPane> {
    use qrcode::render::unicode::Dense1x2;
    let code = qrcode::QrCode::new(url).ok()?;
    let text = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .build();
    let width = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let height = text.lines().count() as u16;
    Some(QrPane {
        text,
        width,
        height,
        url: url.to_string(),
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
    status: &str,
    qr: Option<&QrPane>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = host.state.clone();
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
                ratatui::widgets::Paragraph::new(status).style(ratatui::style::Style::new().dim()),
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
                ratatui::widgets::Paragraph::new(format!("{}\n{}", qr.text, qr.url)).block(
                    ratatui::widgets::Block::bordered()
                        .title("join")
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
    status: &str,
    qr: Option<&QrPane>,
    mut bridge: Option<&mut LiveBridge>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Double clicks detect by cell, not node: a click's re-render swaps
    // the subtree, so node identities never survive between the two.
    let mut last_click: Option<(u16, u16, std::time::Instant)> = None;
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
                    host.click(target)?;
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
    let qr = qr_pane(&join_url);
    let status = format!("lit-todo live · everyone edits one state · {join_url} · Esc quits");
    with_terminal(|terminal| {
        run(
            &mut host,
            node,
            terminal,
            &status,
            qr.as_ref(),
            Some(&mut bridge),
        )
    })
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
