//! The state-sync tooling as a reusable artifact (ADR 0013): a tagged
//! structured-clone codec, one string-payload wire seam (WebSocket,
//! RTCDataChannel), root-component attachment, and the compact pairing
//! that connects two browsers through mutually scanned QR codes, no
//! signaling server.
//!
//! The shared pairing UI ships here too (ADR 0029): `pair-panel.ts`,
//! `qr-code.ts` and `status-navbar.ts` are the one component set both hosts
//! render, loaded from `@gronke/uic-sync` beside the wire they drive.
//!
//! Consumers integrate one of two ways: hand [`web_root`] to a
//! `web_modules` build as an extra source root, or emit the compiled npm
//! tree with [`npm_tree`] and install it like any package.
//!
//! [`pair`] carries the compact payload codec in Rust too (one byte
//! contract, two languages), so a native peer (ADR 0028) exchanges the
//! same pairing codes as the browser. [`session`] is that peer's pairing
//! lifecycle as a pure state machine (`web/session.ts` is the browser
//! sibling, whose cross-tab job stays TS-only).

pub mod pair;
pub mod session;

use std::path::{Path, PathBuf};

use serde_json::json;

/// The TypeScript sources (`codec.ts`, `wire.ts`, `sync.ts`, `pair.ts`,
/// `session.ts`), an extra root for a consumer's `web_modules` build.
pub fn web_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
}

/// Emits the publish-ready `@gronke/uic-sync` npm tree: the compiled
/// modules plus a `package.json`. Returns the emitted module names.
pub fn npm_tree(out: &Path, version: &str) -> Result<Vec<String>, String> {
    let root = web_root();
    uic_npm::emit_tree(
        &uic_npm::TreeSpec {
            web_root: &root,
            name: "@gronke/uic-sync",
            version,
            description: "One wire for component state: structured-clone snapshots over WebSocket or WebRTC, plus serverless QR pairing",
            exports: json!({
                ".": "./sync.js",
                "./codec.js": "./codec.js",
                "./pair.js": "./pair.js",
                "./pair-panel.js": "./pair-panel.js",
                "./qr-code.js": "./qr-code.js",
                "./session.js": "./session.js",
                "./status-navbar.js": "./status-navbar.js",
                "./sync.js": "./sync.js",
                "./theme.js": "./theme.js",
                "./wire.js": "./wire.js"
            }),
            // The pairing components import `lit` (ADR 0029); the codec-only
            // modules do not, but the tree ships them together.
            peer_dependencies: Some(json!({ "lit": "^3" })),
        },
        out,
    )
}
