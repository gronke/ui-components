//! The DOM runtime (ADR 0011/0012): components mount on the retained
//! `uic_dom::Document`, widget state lives in the node payload, layout and
//! paint read the tree, and events travel it.

mod app;
mod composite;
mod host;
mod layout;
mod render;
mod resolve;
pub(crate) mod widget;

pub use app::App;
pub use host::DomHost;
pub use widget::{OverlayOutcome, WidgetAdapter, WidgetPayload, WidgetRegistration};

/// The runtime's document type: every element node can carry a terminal
/// widget.
pub type DomDocument = uic_dom::Document<WidgetPayload>;

/// Paints a document without the [`App`] host: layout, content, focused
/// overlay. External hosts — the uic_js exploration, a widget twin's tests —
/// own their document and focus and call this once per frame.
pub fn paint_document(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    doc: &mut DomDocument,
    focused: Option<uic_dom::NodeId>,
) {
    render::render_document(frame, area, doc, focused);
    render::paint_popup(frame, area, doc, focused);
}

/// Resolves the deepest laid node containing the cell — the pointer entry
/// for external hosts. Recomputes layout for `area`, exactly like the paint.
pub fn hit_test(
    doc: &DomDocument,
    area: ratatui::layout::Rect,
    x: u16,
    y: u16,
) -> Option<uic_dom::NodeId> {
    fn find(nodes: &[layout::LaidNode], x: u16, y: u16) -> Option<uic_dom::NodeId> {
        for laid in nodes {
            if laid.rect.contains(ratatui::layout::Position { x, y }) {
                return find(&laid.children, x, y).or(Some(laid.node));
            }
        }
        None
    }
    find(&layout::compute(doc, area), x, y)
}
