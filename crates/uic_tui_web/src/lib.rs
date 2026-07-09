//! Browser host for the terminal runtime: compiles the `uic_tui` stack to
//! WebAssembly, renders frames as ANSI for xterm.js, and feeds DOM keyboard
//! and pointer events back in as terminal events.

mod backend;
mod keymap;
mod session;

pub use backend::{Output, XtermBackend};
pub use keymap::{translate_key, translate_mouse};
pub use session::TuiSession;
