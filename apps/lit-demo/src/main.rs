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
//! - `cargo run -p uic_lit_demo -- p2p [link-or-code]` → a serverless peer:
//!   the terminal pairs with a browser over WebRTC through the shared
//!   `<pair-panel>`, no server between them (ADR 0028).
//! - `UIC_LIT_DEMO_ADDR=host:port` moves the listener (default
//!   `127.0.0.1:8090`; live defaults to `0.0.0.0:8090`).
//!
//! The modules split by concern: `tui` is the terminal plumbing and event
//! loop, `live` the state bridge and web server, `pair` the WebRTC swap
//! and the driver of `uic_sync::session`'s pairing machine.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use uic_dom::NodeId;
use uic_js::JsHost;

mod live;
mod pair;
mod tui;

use live::{lan_ip, listen_addr, live_bridge, serve_live};
use tui::{qr_pane, run, with_terminal, PanelDriver, StatusLine};

const PACKAGE: &str = "@schuhkarton/lit-todo";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The QR widget registration lives in uic_tui's qr feature — the
    // anchor keeps its object (and the inventory constructor) linked.
    uic_tui::qr::link();
    match std::env::args().nth(1).as_deref() {
        None => terminal_app(),
        Some("serve") => serve_web(),
        Some("live") => live(),
        Some("p2p") => p2p(std::env::args().nth(2)),
        Some(other) => Err(format!(
            "unknown mode {other:?}: no arguments runs the terminal app, `serve` the browser host, `live` both on one state, `p2p [link-or-token]` a serverless peer"
        )
        .into()),
    }
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

/// The first element of a tag below the document root — how the p2p mode
/// finds the components its deck composed (the deck renders once and is
/// never re-committed, so the nodes stay put).
fn node_by_tag(host: &JsHost, tag: &str) -> Option<NodeId> {
    let state = host.state.borrow();
    state.doc.descendant_by_tag(state.doc.root(), tag)
}

fn terminal_app() -> Result<(), Box<dyn std::error::Error>> {
    let (mut host, node) = mounted_host()?;
    let status: StatusLine = Arc::new(Mutex::new(
        "lit-todo via Boa · typing lands in the input · Enter adds/edits · Space toggles · F5/F6 reorder · Del removes · Esc quits".into(),
    ));
    with_terminal(|terminal| run(&mut host, node, terminal, &status, None, None, None))
}

fn serve_web() -> Result<(), Box<dyn std::error::Error>> {
    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    let addr = listen_addr(SocketAddr::from(([127, 0, 0, 1], 8090)))?;
    tokio::runtime::Runtime::new()?.block_on(web_modules::serve(live::frontend(&web), addr))?;
    Ok(())
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
    let (mut bridge, (inbound_tx, outbound_tx, latest)) = live_bridge(&mut host, node)?;

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
    // panel (ADR 0029) stack in a column, the QR (ADR 0029) docks beside
    // them — responsive flexbox from the deck's own styles, no rect math.
    // The extra modules are baked in the package but off the todo-app entry
    // graph, so they load explicitly, import order inside-out; then the
    // mount upgrades the whole composition.
    let mut host = JsHost::new()?;
    host.load_package(Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")), PACKAGE)?;
    for module in ["theme.js", "qr-code.js", "pair-panel.js", "p2p-deck.js"] {
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
                "not a pairing invite: pass the link or the pairing code ({invite:?})"
            )
            .into());
        }
    }

    let runtime = tokio::runtime::Runtime::new()?;
    let page = std::env::var("UIC_LIT_DEMO_P2P_PAGE").unwrap_or_else(|_| P2P_PAGE.into());

    let (mut bridge, (inbound_tx, outbound_tx, latest)) = live_bridge(&mut host, node)?;
    let status: StatusLine = Arc::new(Mutex::new("lit-todo p2p · Esc quits".to_string()));
    let panel_state = Arc::new(Mutex::new(uic_sync::session::PanelState::default()));
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    // The pairing machine (uic_sync::session) runs beside the terminal
    // loop — live()'s threading pattern — and drives the panel through
    // `panel_state`.
    let thread_state = panel_state.clone();
    std::thread::spawn(move || {
        runtime.block_on(pair::drive_session(
            page,
            opener,
            pair::Wiring {
                panel_state: thread_state,
                commands: command_rx,
                inbound: inbound_tx,
                outbound: outbound_tx,
                latest,
            },
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
