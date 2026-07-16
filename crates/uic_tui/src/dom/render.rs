//! Painting from the retained DOM: walks the laid document onto the ratatui
//! frame, mapping the small set of Bootstrap text classes to terminal styles
//! and hosting the rat widgets living in the node payloads. State the old
//! pipeline resolved from expressions per frame — placeholders, disabled,
//! the error outline — reads straight off attributes here, the way
//! stylesheet selectors read them in the browser.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;
use uic_dom::{NodeData, NodeId};

use super::layout::{self, collapse_whitespace, component_attr, effective_classes, LaidNode};
use super::DomDocument;

/// The browser's focus ring as a palette color: the terminal's own scheme
/// decides the hue (the web pane maps it to Bootstrap's primary emphasis).
const FOCUS_RING: Color = Color::LightBlue;

/// The error outline, the browser's `[error]` border: ANSI red, which the
/// web pane maps to Bootstrap's danger color.
const ERROR_BORDER: Color = Color::Red;

/// Text styling inherited down the element tree.
#[derive(Debug, Clone, Copy, Default)]
struct Hints {
    bold: bool,
    italic: bool,
    dim: bool,
    center: bool,
    fg: Option<Color>,
}

impl Hints {
    /// Merges an element's contribution: the tag first (a `<th>` reads bold,
    /// like the browser default), then the classes.
    fn merge(mut self, tag: &str, classes: &[String]) -> Self {
        if tag == "th" {
            self.bold = true;
        }
        for class in classes {
            match class.as_str() {
                "form-label" | "fw-bold" => self.bold = true,
                "fst-italic" => self.italic = true,
                "small" | "text-small" => self.dim = true,
                "text-center" => self.center = true,
                // The bright variants read on dark terminal themes; the web
                // pane maps the same classes to Bootstrap's text emphasis.
                "text-danger" => self.fg = Some(Color::LightRed),
                "text-success" => self.fg = Some(Color::LightGreen),
                "text-warning" => self.fg = Some(Color::LightYellow),
                "text-info" => self.fg = Some(Color::LightCyan),
                "text-muted" | "text-secondary" | "text-body-secondary" => {
                    self.fg = Some(Color::DarkGray)
                }
                _ => {}
            }
        }
        self
    }

    fn style(&self) -> Style {
        let mut style = Style::default();
        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if self.bold {
            style = style.bold();
        }
        if self.italic {
            style = style.italic();
        }
        if self.dim {
            style = style.dim();
        }
        style
    }
}

/// Lays out and paints the whole document; the focused widget node (if any)
/// wears the ring and caret.
pub(crate) fn render_document(
    frame: &mut Frame,
    area: Rect,
    doc: &mut DomDocument,
    focused: Option<NodeId>,
) {
    let laid = layout::compute(doc, area);
    for node in &laid {
        paint(frame, node, doc, focused, Hints::default());
    }
}

fn paint(
    frame: &mut Frame,
    laid: &LaidNode,
    doc: &mut DomDocument,
    focused: Option<NodeId>,
    hints: Hints,
) {
    if laid.rect.width == 0 || laid.rect.height == 0 {
        return;
    }
    match doc.node(laid.node) {
        Some(NodeData::Text(text)) => {
            let text = collapse_whitespace(text);
            // Wrapped like the browser flows prose; the layout reserved the
            // rows (`wrapped_lines`). The collapse already trimmed the ends,
            // so an untrimmed wrap only preserves leading non-breaking
            // spaces — ratatui's trim would strip them as whitespace.
            let paragraph = Paragraph::new(Line::from(text))
                .style(hints.style())
                .wrap(Wrap { trim: false });
            let paragraph = if hints.center {
                paragraph.alignment(Alignment::Center)
            } else {
                paragraph
            };
            frame.render_widget(paragraph, laid.rect);
        }
        Some(NodeData::Element(el)) => {
            if el.attr("data-tui").is_some() {
                paint_widget(frame, laid.rect, doc, laid.node, focused);
                return;
            }
            let classes = effective_classes(doc, laid.node);
            let hints = hints.merge(el.tag(), &classes);
            if classes.iter().any(|c| c == "card") {
                // The card border stays static dark gray (ADR 0017): the
                // focus ring and error dressing belong to the input group.
                frame.render_widget(
                    Block::bordered().border_style(Style::new().dark_gray()),
                    laid.rect,
                );
            }
            if classes.iter().any(|c| c == "input-group") {
                // Error wins over focus, like the browser keeping the red
                // outline on a focused invalid input; the error state reads
                // off the component's reflected attribute, its `[error]`
                // stylesheet selector.
                let border = if component_attr(doc, laid.node, "error").is_some() {
                    Style::new().fg(ERROR_BORDER)
                } else if contains_focus(doc, laid.node, focused) {
                    Style::new().fg(FOCUS_RING)
                } else {
                    Style::new().dark_gray()
                };
                frame.render_widget(Block::bordered().border_style(border), laid.rect);
            }
            for child in &laid.children {
                paint(frame, child, doc, focused, hints);
            }
        }
        _ => {}
    }
}

/// True when the focused widget node lies inside this subtree.
fn contains_focus(doc: &DomDocument, node: NodeId, focused: Option<NodeId>) -> bool {
    match focused {
        Some(focused) => doc.ancestors(focused).any(|ancestor| ancestor == node),
        None => false,
    }
}

fn paint_widget(
    frame: &mut Frame,
    rect: Rect,
    doc: &mut DomDocument,
    node: NodeId,
    focused_node: Option<NodeId>,
) {
    // Everything the paint needs from the immutable side reads before the
    // mutable widget borrow: attributes carry what the old pipeline
    // resolved from expressions.
    let (disabled, placeholder, classes) = match doc.element(node) {
        Some(el) => (
            el.attr("disabled").is_some(),
            el.attr("placeholder").map(str::to_string),
            effective_classes(doc, node),
        ),
        None => return,
    };
    let align = if classes.iter().any(|c| c == "text-end") {
        Some(Alignment::Right)
    } else if classes.iter().any(|c| c == "text-center") {
        Some(Alignment::Center)
    } else {
        None
    };
    let focused = focused_node == Some(node);
    let Some(widget) = doc.element_mut(node).and_then(|el| el.data.widget.as_mut()) else {
        return;
    };
    widget.adapter.set_focus(focused && !disabled);
    let dim = disabled.then(|| Style::new().dim());
    widget.adapter.paint(frame, rect, dim);
    // rat has no notion of placeholders or text alignment; both are paint
    // features. The alignment applies at rest — editing stays left-aligned,
    // where the caret math lives. Widgets painting their own value text
    // (the select's closed label) skip this pass.
    if !widget.adapter.paints_value() {
        let text = widget.adapter.committed_text();
        if text.is_empty() {
            if let Some(placeholder) = placeholder.filter(|p| !p.is_empty()) {
                frame.render_widget(Clear, rect);
                frame.render_widget(
                    Paragraph::new(placeholder)
                        .alignment(align.unwrap_or(Alignment::Left))
                        .style(Style::new().dark_gray()),
                    rect,
                );
            }
        } else if let (Some(align), false) = (align, focused) {
            frame.render_widget(Clear, rect);
            let paragraph = Paragraph::new(text).alignment(align);
            let paragraph = match dim {
                Some(style) => paragraph.style(style),
                None => paragraph,
            };
            frame.render_widget(paragraph, rect);
        }
    }
    // The focused text-bearing widget places the terminal caret, like the
    // browser caret in a focused input; a select shows none there either.
    if focused && !disabled {
        if let Some(position) = widget.adapter.screen_cursor() {
            frame.set_cursor_position(position);
        }
    }
}

/// Paints the focused widget's open overlay (calendar or option list) after
/// the normal pass; ratatui buffers are last-write-wins per cell, so it
/// overlays the flow.
pub(crate) fn paint_popup(
    frame: &mut Frame,
    area: Rect,
    doc: &mut DomDocument,
    focused: Option<NodeId>,
) {
    let Some(widget) = focused
        .and_then(|node| doc.element_mut(node))
        .and_then(|el| el.data.widget.as_mut())
    else {
        return;
    };
    if !widget.adapter.overlay_open() {
        return;
    }
    widget.adapter.paint_overlay(frame, area);
}
