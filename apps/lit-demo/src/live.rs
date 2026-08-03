//! The live bridge — one state between the terminal and every client — and
//! the web server around it: the Rust twin of `@gronke/uic-sync`'s
//! `wire.ts` + `sync.ts` glue, per property over `STATE_FIELDS`, canonical
//! snapshots deduped by byte equality (ADR 0013). It stays in the app
//! because it drives the mounted component through `uic_js::JsHost`.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use include_dir::{include_dir, Dir};
use tokio::sync::{broadcast, mpsc};
use uic_dom::NodeId;
use uic_js::JsHost;
use web_modules::{serve, Frontend};

static DIST: Dir = include_dir!("$OUT_DIR/dist");

/// The reactive properties the live bridge mirrors — one shared state
/// (ADR 0013's one-object story, spelled per property). TS twin:
/// `STATE_FIELDS` in `web/src/todo-app.ts`, which the browser pages import.
pub(crate) const STATE_FIELDS: [&str; 4] = ["draft", "editing", "items", "selected"];

pub(crate) struct LiveBridge {
    pub inbound: mpsc::UnboundedReceiver<String>,
    pub outbound: broadcast::Sender<String>,
    pub latest: Arc<Mutex<String>>,
}

/// The bridge's feeding ends: the inbound sender, the outbound broadcaster
/// and the latest-snapshot slot.
pub(crate) type BridgeEnds = (
    mpsc::UnboundedSender<String>,
    broadcast::Sender<String>,
    Arc<Mutex<String>>,
);

/// The bridge plus its feeding ends, seeded with the current snapshot —
/// `live` hands the ends to the web server, `p2p` to the pairing thread.
pub(crate) fn live_bridge(
    host: &mut JsHost,
    node: NodeId,
) -> Result<(LiveBridge, BridgeEnds), Box<dyn std::error::Error>> {
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let (outbound_tx, _) = broadcast::channel(64);
    let latest = Arc::new(Mutex::new(state_snapshot(host, node)?));
    let bridge = LiveBridge {
        inbound: inbound_rx,
        outbound: outbound_tx.clone(),
        latest: latest.clone(),
    };
    Ok((bridge, (inbound_tx, outbound_tx, latest)))
}

/// The canonical snapshot of the app's shared state, serialized through the
/// component's own accessors. Keys sort at every depth — npm-utils turns on
/// serde_json's preserve_order in this binary, and the sync tooling's codec
/// dedupes by byte equality (ADR 0013).
pub(crate) fn state_snapshot(
    host: &mut JsHost,
    node: NodeId,
) -> Result<String, Box<dyn std::error::Error>> {
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
pub(crate) fn apply_state(
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
pub(crate) fn publish(
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
pub(crate) fn lan_ip() -> std::net::IpAddr {
    std::net::UdpSocket::bind(("0.0.0.0", 0))
        .and_then(|socket| {
            socket.connect(("8.8.8.8", 80))?;
            socket.local_addr()
        })
        .map(|addr| addr.ip())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
}

/// The frontend router plus the live endpoints: `/live` answers the page
/// glue's probe, `/ws` carries state snapshots both ways.
pub(crate) fn serve_live(
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

pub(crate) fn frontend(web: &Path) -> axum::Router {
    if std::env::var_os("WEB_MODULES_EMBEDDED").is_some() {
        Frontend::embedded(&DIST).router()
    } else {
        Frontend::embedded(&DIST).source(web.join("pages")).auto()
    }
}

pub(crate) fn listen_addr(default: SocketAddr) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    match std::env::var("UIC_LIT_DEMO_ADDR") {
        Ok(raw) => Ok(raw
            .parse::<SocketAddr>()
            .map_err(|err| format!("UIC_LIT_DEMO_ADDR {raw:?}: {err}"))?),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_are_canonical_and_unknown_fields_stay_out() {
        let (mut host, node) = crate::mounted_host(&crate::BackendArg::Memory).unwrap();

        // The snapshot sorts keys at every depth: draft < editing < items
        // < selected, and the row objects spell done < id < text.
        let snapshot = state_snapshot(&mut host, node).unwrap();
        assert!(snapshot.starts_with(r#"{"draft":"#), "sorted: {snapshot}");
        let items_at = snapshot.find(r#""items":"#).expect("items key");
        let row = &snapshot[items_at..];
        assert!(
            row.contains(r#"{"done":true,"id":1,"text":"#),
            "nested keys sort too: {snapshot}"
        );

        // Applying tolerates unknown fields and garbage without touching
        // the known state.
        apply_state(&mut host, node, r#"{"draft":"typed","stranger":42}"#).unwrap();
        apply_state(&mut host, node, "not json at all").unwrap();
        let applied = state_snapshot(&mut host, node).unwrap();
        assert!(
            applied.contains(r#""draft":"typed""#),
            "the known field landed: {applied}"
        );
        assert!(
            !applied.contains("stranger"),
            "the unknown field stayed out: {applied}"
        );
    }
}
