//! A Boa-embedded JS engine hosting real LitElement components on the
//! terminal runtime.
//!
//! The interception point is the `lit` module boundary: components import
//! the mocked runtime (TypeScript under js/src/, compiled per module by the
//! build script) whose `LitElement` renders through flat `__uic_*` natives
//! into the retained `uic_tui::dom::DomDocument`; the existing taffy layout
//! and ratatui paint consume that document unchanged.

use std::path::{Path, PathBuf};

#[cfg(feature = "clipboard")]
mod clipboard;
#[cfg(feature = "dialogs")]
mod dialogs;
mod error;
mod host;
mod loader;
mod natives;
mod state;
#[cfg(feature = "storage")]
mod storage;

#[cfg(feature = "clipboard")]
pub use clipboard::ClipboardBackend;
#[cfg(feature = "dialogs")]
pub use dialogs::{DialogKind, DialogRequest};
pub use error::Error;
pub use host::JsHost;
pub use state::HostState;
#[cfg(feature = "sqlite")]
pub use storage::SqliteBackend;
#[cfg(feature = "storage")]
pub use storage::{MemoryBackend, StorageBackend, StorageError};

/// The mocked-runtime TypeScript sources (`js/src`): the mocked `lit` module
/// family and the `__uic_*` runtime, compiled per module by a consumer's
/// build. The browser worker host (`uic_worker::worker_runtime_tree`) compiles
/// this tree so the browser's own engine runs the same runtime the Boa host
/// bakes, sourcing it here instead of reaching across the workspace by path.
pub fn js_src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("js/src")
}
