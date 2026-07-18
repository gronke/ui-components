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

use uic_css::{ComputedStyle, StyleTable};

use super::layout::{self, collapse_whitespace, component_attr, effective_classes, LaidNode};
use super::{css, DomDocument};

/// The browser's focus ring as a palette color: the terminal's own scheme
/// decides the hue (the web pane maps it to Bootstrap's primary emphasis).
const FOCUS_RING: Color = Color::LightBlue;

/// The error outline, the browser's `[error]` border: ANSI red, which the
/// web pane maps to Bootstrap's danger color.
const ERROR_BORDER: Color = Color::Red;

/// Lays out and paints the whole document; the focused widget node (if any)
/// wears the ring and caret.
pub(crate) fn render_document(
    frame: &mut Frame,
    area: Rect,
    doc: &mut DomDocument,
    focused: Option<NodeId>,
) {
    let (laid, styles) = layout::compute_styled(doc, area, focused);
    let root = ComputedStyle::default();
    for node in &laid {
        paint(frame, node, doc, focused, &styles, &root);
    }
}

fn paint(
    frame: &mut Frame,
    laid: &LaidNode,
    doc: &mut DomDocument,
    focused: Option<NodeId>,
    styles: &StyleTable,
    inherited: &ComputedStyle,
) {
    if laid.rect.width == 0 || laid.rect.height == 0 {
        return;
    }
    match doc.node(laid.node) {
        Some(NodeData::Text(text)) => {
            let text = collapse_whitespace(text);
            let (style, center) = css::text_style(inherited);
            // Wrapped like the browser flows prose; the layout reserved the
            // rows (`wrapped_lines`). The collapse already trimmed the ends,
            // so an untrimmed wrap only preserves leading non-breaking
            // spaces — ratatui's trim would strip them as whitespace.
            let paragraph = Paragraph::new(Line::from(text))
                .style(style)
                .wrap(Wrap { trim: false });
            let paragraph = if center {
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
            let computed = styles
                .get(&laid.node)
                .cloned()
                .unwrap_or_else(|| inherited.inherited());
            if computed.background != inherited.background {
                // An element's own background fills its box — a component's
                // `:host { background-color }` paints the whole component
                // area, not just its text runs. `Highlight` stays a
                // text-run effect (the mark).
                if let Some(background) = computed.background {
                    if background != uic_css::Color::Highlight {
                        frame
                            .buffer_mut()
                            .set_style(laid.rect, Style::new().bg(css::convert_color(background)));
                    }
                }
            }
            if computed.border > 0.0 {
                // The cascade reserves the cells; the ring colors stay the
                // runtime's: error wins over focus on the input group, like
                // the browser keeping the red outline on a focused invalid
                // input — read off the component's reflected attribute, its
                // `[error]` stylesheet selector. Other bordered blocks (the
                // card) stay static dark gray (ADR 0017).
                let classes = effective_classes(doc, laid.node);
                let border = if classes.iter().any(|c| c == "input-group") {
                    if component_attr(doc, laid.node, "error").is_some() {
                        Style::new().fg(ERROR_BORDER)
                    } else if contains_focus(doc, laid.node, focused) {
                        Style::new().fg(FOCUS_RING)
                    } else {
                        Style::new().dark_gray()
                    }
                } else {
                    Style::new().dark_gray()
                };
                frame.render_widget(Block::bordered().border_style(border), laid.rect);
            }
            for child in &laid.children {
                paint(frame, child, doc, focused, styles, &computed);
            }
            if focused == Some(laid.node) {
                // A focused plain node (a JS component's roving focus)
                // reads as a one-row selection bar over its first line;
                // widgets paint their own ring instead.
                let row = Rect {
                    height: 1,
                    ..laid.rect
                };
                frame.buffer_mut().set_style(row, Style::new().reversed());
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
