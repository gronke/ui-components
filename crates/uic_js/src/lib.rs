//! A Boa-embedded JS engine hosting real LitElement components on the
//! terminal runtime.
//!
//! The interception point is the `lit` module boundary: components import
//! the mocked runtime (TypeScript under js/src/, compiled per module by the
//! build script) whose `LitElement` renders through flat `__uic_*` natives
//! into the retained `uic_tui::dom::DomDocument`; the existing taffy layout
//! and ratatui paint consume that document unchanged.

#[cfg(feature = "dialogs")]
mod dialogs;
mod error;
mod host;
mod loader;
mod natives;
mod state;
#[cfg(feature = "storage")]
mod storage;

#[cfg(feature = "dialogs")]
pub use dialogs::{DialogKind, DialogRequest};
pub use error::Error;
pub use host::JsHost;
pub use state::HostState;
#[cfg(feature = "sqlite")]
pub use storage::SqliteBackend;
#[cfg(feature = "storage")]
pub use storage::{MemoryBackend, StorageBackend, StorageError};
