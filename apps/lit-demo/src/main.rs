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
//!   `<pair-panel>`, no server between them (ADR 0028). Pairing first: the
//!   todo appears once connected, under a navbar whose disconnect (`^D` or
//!   a click) returns to a fresh invite. `--serve` hosts the pairing page
//!   from this process and points its invites at this machine, so one
//!   command stands the whole demo up on a LAN; `--clipboard` opts into
//!   watching the system clipboard to auto-continue a step.
//! - `--backend memory://` (default) keeps the terminal's localStorage in
//!   memory; an SQLite location (`sqlite://todos.db` or a bare path)
//!   persists it between runs. `serve` ignores it — the browser has the
//!   real thing.
//! - `UIC_LIT_DEMO_ADDR=host:port` moves the listener (default
//!   `127.0.0.1:8090`; live defaults to `0.0.0.0:8090`).
//!
//! The modules split by concern: `tui` is the terminal plumbing and event
//! loop, `live` the state bridge and web server, `pair` the WebRTC swap
//! and the driver of `uic_sync::session`'s pairing machine.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clap::{Parser, Subcommand};
use tokio::sync::mpsc;
use uic_dom::NodeId;
use uic_js::JsHost;

mod clipboard;
mod live;
mod pair;
mod tui;

use live::{lan_ip, listen_addr, live_bridge, serve_live};
use tui::{qr_pane, run, with_terminal, PanelDriver, StatusLine};

const PACKAGE: &str = "@gronke/lit-todo";

/// One Lit todo app, two hosts — no mode runs it in this terminal.
#[derive(Parser)]
struct Cli {
    /// Where the terminal's localStorage lives: memory:// or an SQLite
    /// location (sqlite://<path> or a bare path). serve ignores it — the
    /// browser has the real thing.
    #[arg(long, global = true, default_value = "memory://")]
    backend: BackendArg,
    /// Watch the system clipboard in p2p to auto-continue a pairing step,
    /// and expose it to the page as navigator.clipboard. Off unless asked —
    /// a pasted or scanned link always pairs regardless.
    #[arg(long, global = true)]
    clipboard: bool,
    #[command(subcommand)]
    mode: Option<Mode>,
}

#[derive(Subcommand)]
enum Mode {
    /// The same sources on real lit, served to the browser
    Serve,
    /// Terminal and browsers editing one state over a WebSocket
    Live,
    /// A serverless WebRTC peer pairing with a browser
    P2p {
        /// The invite link or pairing code from the other side
        link: Option<String>,
        /// Host the pairing page from this process and point invites at
        /// this machine — one command instead of a separate `serve`.
        #[arg(long)]
        serve: bool,
    },
}

/// The storage backend behind the runtime's localStorage.
#[derive(Clone, Debug, PartialEq)]
enum BackendArg {
    Memory,
    Sqlite(PathBuf),
}

impl std::str::FromStr for BackendArg {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw == "memory://" || raw == "memory" {
            return Ok(BackendArg::Memory);
        }
        if let Some(path) = raw.strip_prefix("sqlite://") {
            if path.is_empty() {
                return Err("sqlite:// needs a path".into());
            }
            return Ok(BackendArg::Sqlite(PathBuf::from(path)));
        }
        if raw.is_empty() || raw.contains("://") {
            return Err(format!(
                "unknown backend {raw:?}: memory:// or sqlite://<path> (a bare path is sqlite)"
            ));
        }
        Ok(BackendArg::Sqlite(PathBuf::from(raw)))
    }
}

/// The backend the flag selected — sqlite opens (and creates) its file
/// here, before any mode takes over the screen.
fn storage_backend(
    arg: &BackendArg,
) -> Result<Box<dyn uic_js::StorageBackend>, Box<dyn std::error::Error>> {
    Ok(match arg {
        BackendArg::Memory => Box::new(uic_js::MemoryBackend::default()),
        BackendArg::Sqlite(path) => Box::new(uic_js::SqliteBackend::open(path)?),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The QR widget registration lives in uic_tui's qr feature — the
    // anchor keeps its object (and the inventory constructor) linked.
    uic_tui::qr::link();
    let cli = Cli::parse();
    match cli.mode {
        None => terminal_app(&cli.backend),
        Some(Mode::Serve) => serve_web(),
        Some(Mode::Live) => live(&cli.backend),
        Some(Mode::P2p { link, serve }) => p2p(link, serve, &cli.backend, cli.clipboard),
    }
}

fn mounted_host(backend: &BackendArg) -> Result<(JsHost, NodeId), Box<dyn std::error::Error>> {
    let mut host = JsHost::with_storage(storage_backend(backend)?)?;
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

/// The first element carrying a class below the document root — the deck's
/// plain wrapper divs, the pairing-first screen gates, resolve this way.
fn node_by_class(host: &JsHost, class: &str) -> Option<NodeId> {
    let state = host.state.borrow();
    let root = state.doc.root();
    state.doc.find_element(root, |el| {
        el.attr("class")
            .is_some_and(|value| value.split_whitespace().any(|c| c == class))
    })
}

fn terminal_app(backend: &BackendArg) -> Result<(), Box<dyn std::error::Error>> {
    let (mut host, node) = mounted_host(backend)?;
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

fn live(backend: &BackendArg) -> Result<(), Box<dyn std::error::Error>> {
    let (mut host, node) = mounted_host(backend)?;

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
const P2P_PAGE: &str = "https://gronke.github.io/ui-components/lit-demo/p2p/";

/// Where a p2p invite links, resolved before the screen is taken: an
/// explicit `UIC_LIT_DEMO_P2P_PAGE` wins; `--serve` hosts the page here and
/// points invites at this machine (binding the server on a thread, failing
/// fast on a taken port); otherwise the published page.
fn p2p_page(serve: bool) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(url) = std::env::var("UIC_LIT_DEMO_P2P_PAGE") {
        return Ok(url);
    }
    if !serve {
        return Ok(P2P_PAGE.into());
    }
    let addr = listen_addr(SocketAddr::from(([0, 0, 0, 0], 8090)))?;
    // Fail before the alt screen: a taken port must not leave a dead URL in
    // the invite. (The probe closes before the server binds — a benign race.)
    drop(std::net::TcpListener::bind(addr).map_err(|err| format!("bind {addr}: {err}"))?);
    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    std::thread::spawn(move || match tokio::runtime::Runtime::new() {
        Ok(runtime) => {
            if let Err(err) = runtime.block_on(web_modules::serve(live::frontend(&web), addr)) {
                eprintln!("p2p page server ended: {err}");
            }
        }
        Err(err) => eprintln!("p2p page server runtime: {err}"),
    });
    let host = if addr.ip().is_unspecified() {
        lan_ip()
    } else {
        addr.ip()
    };
    Ok(format!("http://{host}:{}/p2p/", addr.port()))
}

fn p2p(
    invite: Option<String>,
    serve: bool,
    backend: &BackendArg,
    watch_clipboard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // One mounted root, the p2p deck: the todo card and the shared pairing
    // panel (ADR 0029) stack in a column, the QR (ADR 0029) docks beside
    // them — responsive flexbox from the deck's own styles, no rect math.
    // The extra modules are off the todo-app entry graph, so they load
    // explicitly, inside-out: the shared pairing UI from @gronke/uic-sync
    // (ADR 0029), then the todo-specific deck from the app package; the mount
    // upgrades the whole composition.
    let mut host = JsHost::with_storage(storage_backend(backend)?)?;
    host.load_package(Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")), PACKAGE)?;
    for module in [
        "theme.js",
        "qr-code.js",
        "pair-panel.js",
        "status-navbar.js",
    ] {
        let src = std::fs::read_to_string(
            Path::new(env!("UIC_LIT_DEMO_NPM_ROOT"))
                .join("@gronke/uic-sync")
                .join(module),
        )?;
        host.load_module(&format!("@gronke/uic-sync/{module}"), &src)?;
    }
    let deck = std::fs::read_to_string(
        Path::new(env!("UIC_LIT_DEMO_NPM_ROOT"))
            .join(PACKAGE)
            .join("p2p-deck.js"),
    )?;
    host.load_module(&format!("{PACKAGE}/p2p-deck.js"), &deck)?;
    host.mount("p2p-deck", &[])?;
    let node = node_by_tag(&host, "todo-app").ok_or("the deck mounts a todo-app")?;
    let panel = node_by_tag(&host, "pair-panel").ok_or("the deck mounts a pair-panel")?;
    // The deck's own QR — captured now, while the panel is still idle and
    // has not rendered its (terminal-hidden) inline copy; the deck never
    // re-commits, so the node stays put.
    let qr = node_by_tag(&host, "qr-code").ok_or("the deck mounts a qr-code")?;
    // The connected screen's chrome and the wrapper divs the screens gate
    // through (pairing-first: the deck boots with the todo hidden).
    let navbar = node_by_tag(&host, "status-navbar").ok_or("the deck mounts a status-navbar")?;
    let bar = node_by_class(&host, "bar").ok_or("the deck wraps the navbar in .bar")?;
    let todo_pane = node_by_class(&host, "todo-pane").ok_or("the deck wraps the todo")?;
    let pairing_pane = node_by_class(&host, "pairing-pane").ok_or("the deck wraps the panel")?;
    // The navbar's static face: where this peer can be reached, and the
    // keyboard way back to the pairing screen.
    host.set_prop(
        navbar,
        "address",
        &serde_json::to_string(&lan_ip().to_string())?,
    )?;
    host.set_prop(navbar, "hint", "\"^D disconnect\"")?;

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

    // Resolve (and, under --serve, start hosting) the invite page before
    // the screen is taken.
    let page = p2p_page(serve)?;
    let runtime = tokio::runtime::Runtime::new()?;

    let (mut bridge, (inbound_tx, outbound_tx, latest)) = live_bridge(&mut host, node)?;
    let status: StatusLine = Arc::new(Mutex::new(
        "lit-todo p2p · ^D disconnects · Esc quits".to_string(),
    ));
    let panel_state = Arc::new(Mutex::new(uic_sync::session::PanelState::default()));
    let endpoints = Arc::new(Mutex::new(None));
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    // The pairing machine (uic_sync::session) runs beside the terminal
    // loop — live()'s threading pattern — and drives the panel through
    // `panel_state`.
    let thread_state = panel_state.clone();
    let thread_endpoints = endpoints.clone();
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
                endpoints: thread_endpoints,
            },
        ));
    });

    // The mocked DOM's navigator.clipboard and the loop's auto-continue
    // share this backend; inert when disabled or headless.
    host.install_clipboard(std::rc::Rc::new(clipboard::SystemClipboard::new(
        watch_clipboard,
    )));
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
                navbar,
                bar,
                todo_pane,
                pairing_pane,
                state: &panel_state,
                commands: &command_tx,
                endpoints: &endpoints,
                lan: lan_ip().to_string(),
                clipboard: clipboard::ClipboardWatch::default(),
            }),
        )
    })
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn backends_parse_by_scheme() {
        assert_eq!("memory://".parse(), Ok(BackendArg::Memory));
        assert_eq!("memory".parse(), Ok(BackendArg::Memory));
        assert_eq!(
            "sqlite://todos.db".parse(),
            Ok(BackendArg::Sqlite(PathBuf::from("todos.db")))
        );
        assert_eq!(
            "todos.db".parse(),
            Ok(BackendArg::Sqlite(PathBuf::from("todos.db")))
        );
        assert!("sqlite://".parse::<BackendArg>().is_err());
        assert!("redis://x".parse::<BackendArg>().is_err());
        assert!("".parse::<BackendArg>().is_err());
    }

    #[test]
    fn the_flag_rides_any_mode() {
        let cli = Cli::try_parse_from(["demo"]).unwrap();
        assert_eq!(cli.backend, BackendArg::Memory);
        assert!(cli.mode.is_none());

        let cli = Cli::try_parse_from(["demo", "--backend", "todos.db"]).unwrap();
        assert_eq!(cli.backend, BackendArg::Sqlite(PathBuf::from("todos.db")));

        // Global: the flag parses after a subcommand too.
        let cli = Cli::try_parse_from(["demo", "p2p", "--backend", "sqlite://x.db"]).unwrap();
        assert_eq!(cli.backend, BackendArg::Sqlite(PathBuf::from("x.db")));
        assert!(matches!(
            cli.mode,
            Some(Mode::P2p {
                link: None,
                serve: false
            })
        ));

        let cli = Cli::try_parse_from(["demo", "p2p", "https://x/#code"]).unwrap();
        assert!(matches!(cli.mode, Some(Mode::P2p { link: Some(_), .. })));

        assert!(Cli::try_parse_from(["demo", "--backend", "redis://x"]).is_err());
    }

    #[test]
    fn p2p_serve_is_an_opt_in_flag() {
        let cli = Cli::try_parse_from(["demo", "p2p"]).unwrap();
        assert!(matches!(cli.mode, Some(Mode::P2p { serve: false, .. })));

        let cli = Cli::try_parse_from(["demo", "p2p", "--serve"]).unwrap();
        assert!(matches!(cli.mode, Some(Mode::P2p { serve: true, .. })));

        // --serve rides beside an opened link and the global backend.
        let cli = Cli::try_parse_from([
            "demo",
            "p2p",
            "https://x/#code",
            "--serve",
            "--backend",
            "m.db",
        ])
        .unwrap();
        assert!(matches!(
            cli.mode,
            Some(Mode::P2p {
                link: Some(_),
                serve: true
            })
        ));
        assert_eq!(cli.backend, BackendArg::Sqlite(PathBuf::from("m.db")));
    }

    #[test]
    fn the_clipboard_is_opt_in() {
        // Off unless asked — reading a user's clipboard is not a default.
        assert!(!Cli::try_parse_from(["demo", "p2p"]).unwrap().clipboard);
        assert!(
            Cli::try_parse_from(["demo", "p2p", "--clipboard"])
                .unwrap()
                .clipboard
        );
        // Global, like --backend: it parses before a subcommand too.
        assert!(
            Cli::try_parse_from(["demo", "--clipboard", "p2p"])
                .unwrap()
                .clipboard
        );
        // The old opt-out spelling is gone now that off is the default.
        assert!(Cli::try_parse_from(["demo", "p2p", "--no-clipboard"]).is_err());
    }
}
