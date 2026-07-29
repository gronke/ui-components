//! Terminal runtime: components mount as element nodes on a retained DOM
//! (`uic_dom`), taffy computes layout over terminal cells from the document,
//! paint hosts rat-widget input primitives living in the node payloads, and
//! keys and the pointer travel the tree.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! ui_components::link();
//! let mut app = uic_tui::App::new()?;
//! let el = app.mount("input-date")?;
//! app.set_attr(el, "label", "Date of purchase");
//! app.on(el, "value-changed", |ev| eprintln!("{:?}", ev.value));
//! app.run()?;
//! # Ok(())
//! # }
//! ```

pub mod dialog;
pub mod dom;
pub mod keys;
pub mod lint;

#[cfg(feature = "qr")]
pub use dom::qr;
pub use dom::{App, OverlayOutcome, WidgetAdapter, WidgetRegistration};
pub use keys::KeyStroke;

// The widget-implementation surface for co-located component twins
// (ADR 0002): adapters build against the runtime's own stack, so versions
// can never drift.
pub use {crossterm, rat_widget, ratatui, unicode_width};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown custom element <{0}>")]
    UnknownTag(String),
    #[error("unsupported data-tui widget '{0}'")]
    UnknownWidget(String),
    #[error("date pattern: {0}")]
    Pattern(String),
    #[error("terminal: {0}")]
    Terminal(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Outcome of [`App::handle_event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Continue,
    Quit,
}
