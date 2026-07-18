//! The demo served to a browser: the unmodified
//! `@alenaksu/json-viewer` runs in a native Boa session, xterm.js shows its
//! ANSI over a WebSocket, and browser keys and clicks feed back in.
//!
//! ```sh
//! cargo run -p uic_js --example json_viewer_web            # sample document
//! cargo run -p uic_js --example json_viewer_web data.json  # your own JSON
//! ```
//!
//! Every connection gets its own session thread (one `JsHost` each — the
//! host state is thread-local by design).

use std::net::SocketAddr;
use std::path::Path;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Html;
use axum::routing::get;
use uic_js::JsHost;
use uic_tui::ratatui::layout::Rect;
use uic_tui::ratatui::Terminal;
use uic_tui_web::{Output, XtermBackend};

const COLS: u16 = 100;
const ROWS: u16 = 32;

const SAMPLE: &str = include_str!("../sample.json");

const PAGE: &str = include_str!("page.html");

enum ClientMsg {
    Key(String),
    Mouse(u16, u16),
}

fn draw(
    host: &JsHost,
    terminal: &mut Terminal<XtermBackend>,
    out: &Output,
) -> Result<String, Box<dyn std::error::Error>> {
    let state = host.state.clone();
    terminal.draw(|frame| {
        let mut s = state.borrow_mut();
        s.dirty = false;
        let focused = s.focused;
        uic_tui::dom::paint_document(frame, frame.area(), &mut s.doc, focused);
    })?;
    Ok(out.take())
}

fn session_thread(
    data: String,
    inbound: std::sync::mpsc::Receiver<ClientMsg>,
    outbound: tokio::sync::mpsc::UnboundedSender<String>,
) {
    let run = move || -> Result<(), Box<dyn std::error::Error>> {
        let mut host = JsHost::new()?;
        host.load_dist_dir(Path::new(env!("UIC_JS_VENDOR_DIST")), "json-viewer.js")?;
        let node = host.mount("json-viewer", &[("data", &data)])?;
        host.focus(node)?;

        let (backend, out) = XtermBackend::new(COLS, ROWS);
        let mut terminal = Terminal::new(backend)?;

        outbound.send(draw(&host, &mut terminal, &out)?)?;
        while let Ok(message) = inbound.recv() {
            match message {
                ClientMsg::Key(key) => {
                    host.dispatch_key(&key)?;
                }
                ClientMsg::Mouse(col, row) => {
                    let target = {
                        let state = host.state.borrow();
                        uic_tui::dom::hit_test(&state.doc, Rect::new(0, 0, COLS, ROWS), col, row)
                    };
                    if let Some(target) = target {
                        host.click(target)?;
                    }
                }
            }
            outbound.send(draw(&host, &mut terminal, &out)?)?;
        }
        Ok(())
    };
    if let Err(err) = run() {
        eprintln!("session ended: {err}");
    }
}

async fn handle_socket(mut socket: WebSocket, data: String) {
    let (inbound_tx, inbound_rx) = std::sync::mpsc::channel::<ClientMsg>();
    let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || session_thread(data, inbound_rx, outbound_tx));

    loop {
        tokio::select! {
            ansi = outbound_rx.recv() => {
                let Some(ansi) = ansi else { break };
                if socket.send(Message::Text(ansi.into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                let Some(Ok(Message::Text(text))) = incoming else { break };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                let message = match value["type"].as_str() {
                    Some("key") => value["key"].as_str().map(|k| ClientMsg::Key(k.to_string())),
                    Some("mouse") => {
                        let col = value["col"].as_u64().unwrap_or(0) as u16;
                        let row = value["row"].as_u64().unwrap_or(0) as u16;
                        Some(ClientMsg::Mouse(col, row))
                    }
                    _ => None,
                };
                if let Some(message) = message {
                    if inbound_tx.send(message).is_err() {
                        break;
                    }
                }
            }
        }
    }
}

fn asset(relative: &str) -> String {
    let path = Path::new(env!("UIC_JS_VENDOR_XTERM")).join(relative);
    std::fs::read_to_string(path).expect("vendored xterm asset")
}

/// Serves the vendored npm trees for the DOM pane. Rejects traversal on its
/// own, per the file-serving contract: every handler checks `..` itself.
async fn vendor_asset(
    axum::extract::Path(rest): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if rest.split(['/', '\\']).any(|part| part == "..") {
        return (axum::http::StatusCode::BAD_REQUEST, "traversal rejected").into_response();
    }
    let path = Path::new(env!("UIC_JS_VENDOR_ROOT")).join(&rest);
    let Ok(body) = std::fs::read(&path) else {
        return (axum::http::StatusCode::NOT_FOUND, "not vendored").into_response();
    };
    let content_type = match path.extension().and_then(|e| e.to_str()) {
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    };
    ([("content-type", content_type)], body).into_response()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(path)?,
        None => SAMPLE.to_string(),
    };

    let page = PAGE
        .replace("__COLS__", &COLS.to_string())
        .replace("__ROWS__", &ROWS.to_string())
        .replace("__DATA__", &data);

    let app = axum::Router::new()
        .route("/", get(move || async move { Html(page.clone()) }))
        .route(
            "/xterm.js",
            get(|| async { ([("content-type", "text/javascript")], asset("lib/xterm.js")) }),
        )
        .route(
            "/xterm.css",
            get(|| async { ([("content-type", "text/css")], asset("css/xterm.css")) }),
        )
        .route("/vendor/{*rest}", get(vendor_asset))
        .route(
            "/session",
            get(move |upgrade: WebSocketUpgrade| async move {
                upgrade.on_upgrade(move |socket| handle_socket(socket, data.clone()))
            }),
        );

    let addr = SocketAddr::from(([127, 0, 0, 1], 8091));
    println!("json-viewer via Boa, browser-served: http://{addr}/");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
