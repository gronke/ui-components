//! Serves the frontend baked by `build.rs`.
//!
//! - `cargo run -p uic_web_demo` → live-reload dev server: edits to `web/`
//!   recompile on the fly, with vendored modules and generated components
//!   served from the build-time bake.
//! - `cargo run -p uic_web_demo --release` → serves everything embedded in
//!   the binary (no filesystem).
//! - `WEB_MODULES_EMBEDDED=1` forces embedded serving in any build — used for
//!   deterministic end-to-end runs.
//! - `UIC_WEB_DEMO_ADDR=host:port` moves the listener (default
//!   `127.0.0.1:8080`) — e.g. onto the port a workspace proxy forwards.

use std::net::SocketAddr;
use std::path::PathBuf;

use include_dir::{include_dir, Dir};
use web_modules::{serve, Frontend};

static DIST: Dir = include_dir!("$OUT_DIR/dist");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    // The browser TUI (scripts/build-wasm.sh) is served from disk when built;
    // absent, /tui 404s and the page degrades to the DOM demo alone.
    let web_tui = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web-tui");
    let app = if std::env::var_os("WEB_MODULES_EMBEDDED").is_some() {
        Frontend::embedded(&DIST)
            .mount_dir("/tui", web_tui)
            .router()
    } else {
        Frontend::embedded(&DIST)
            .source(web)
            .mount_dir("/tui", web_tui)
            .auto()
    };
    let addr = match std::env::var("UIC_WEB_DEMO_ADDR") {
        Ok(raw) => raw
            .parse::<SocketAddr>()
            .map_err(|err| format!("UIC_WEB_DEMO_ADDR {raw:?}: {err}"))?,
        Err(_) => SocketAddr::from(([127, 0, 0, 1], 8080)),
    };
    serve(app, addr).await?;
    Ok(())
}
