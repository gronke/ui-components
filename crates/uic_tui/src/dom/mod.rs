//! The DOM runtime (ADR 0011/0012): components mount on the retained
//! `uic_dom::Document`, widget state lives in the node payload, layout and
//! paint read the tree, and events travel it.

mod app;
mod host;
mod layout;
mod render;
mod widget;

pub use app::App;
pub use host::DomHost;
pub use widget::WidgetPayload;

/// The runtime's document type: every element node can carry a terminal
/// widget.
pub type DomDocument = uic_dom::Document<WidgetPayload>;
