//! Serves the frontend baked by `build.rs`.
//!
//! - `cargo run -p uic_web_demo` → live-reload dev server: edits to `web/`
//!   recompile on the fly, with vendored modules and generated components
//!   served from the build-time bake.
//! - `cargo run -p uic_web_demo --release` → serves everything embedded in
//!   the binary (no filesystem).
//! - `WEB_MODULES_EMBEDDED=1` forces embedded serving in any build — used for
//!   deterministic end-to-end runs.

use std::net::SocketAddr;
use std::path::PathBuf;

use include_dir::{include_dir, Dir};
use web_modules::{serve, Frontend};

static DIST: Dir = include_dir!("$OUT_DIR/dist");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let web = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    let app = if std::env::var_os("WEB_MODULES_EMBEDDED").is_some() {
        Frontend::embedded(&DIST).router()
    } else {
        Frontend::embedded(&DIST).source(web).auto()
    };
    serve(app, SocketAddr::from(([127, 0, 0, 1], 8080))).await?;
    Ok(())
}
