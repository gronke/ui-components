//! Painting from the retained DOM: walks the laid document onto the ratatui
//! frame, mapping the small set of Bootstrap text classes to terminal styles
//! and hosting the rat widgets living in the node payloads. State the old
//! pipeline resolved from expressions per frame — placeholders, disabled,
//! the error outline — reads straight off attributes here, the way
//! stylesheet selectors read them in the browser.

use rat_widget::calendar::Month;
use rat_widget::choice::Choice;
use rat_widget::date_input::DateInput;
use rat_widget::popup::{Placement, PopupCore};
use rat_widget::text::HasScreenCursor;
use rat_widget::text_input::TextInput;
use rat_widget::textarea::TextArea;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;
use uic_core::SelectOption;
use uic_dom::{NodeData, NodeId};

use super::layout::{self, collapse_whitespace, component_attr, effective_classes, LaidNode};
use super::widget::{WidgetBox, WidgetState};
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
    fn merge(mut self, classes: &[String]) -> Self {
        for class in classes {
            match class.as_str() {
                "form-label" => self.bold = true,
                "fst-italic" => self.italic = true,
                "small" | "text-small" => self.dim = true,
                "text-center" => self.center = true,
                // The bright variant reads on dark terminal themes; the web
                // pane maps it to Bootstrap's danger text emphasis.
                "text-danger" => self.fg = Some(Color::LightRed),
                "text-muted" | "text-secondary" => self.fg = Some(Color::DarkGray),
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
            let paragraph = Paragraph::new(Line::from(text)).style(hints.style());
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
            let hints = hints.merge(&classes);
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
    widget.state.set_focus(focused && !disabled);
    let dim = disabled.then(|| Style::new().dim());
    match &mut widget.state {
        WidgetState::Date { input, popup } => {
            popup.anchor = rect;
            let mut date = DateInput::new();
            if let Some(style) = dim {
                date = date.style(style);
            }
            frame.render_stateful_widget(date, rect, input);
        }
        WidgetState::Text(state) | WidgetState::Number(state) => {
            let mut text = TextInput::new();
            if let Some(style) = dim {
                text = text.style(style);
            }
            frame.render_stateful_widget(text, rect, state);
        }
        WidgetState::TextArea(state) => {
            let mut area = TextArea::new();
            if let Some(style) = dim {
                area = area.style(style);
            }
            frame.render_stateful_widget(area, rect, state.as_mut());
        }
        WidgetState::Select(state) => {
            let mut choice = choice_widget(&widget.options);
            if let Some(style) = dim {
                choice = choice.style(style);
            }
            let (closed_widget, _) = choice.into_widgets();
            frame.render_stateful_widget(closed_widget, rect, state.as_mut());
            // The closed line shows the compact label (`short || label ||
            // value` of the selected option) while the popup lists full
            // labels; rat renders the same line in both places, so the item
            // render is skipped above and the closed text painted here.
            let value = state.value();
            let closed = widget
                .options
                .iter()
                .find(|option| option.value == value)
                .map(|option| option.short_label())
                .unwrap_or_default();
            frame.render_widget(Line::from(closed), state.item_area);
        }
    }
    // rat has no notion of placeholders or text alignment; both are paint
    // features like the select's closed label. The alignment applies at
    // rest — editing stays left-aligned, where the caret math lives.
    if !matches!(widget.state, WidgetState::Select(_)) {
        let text = widget.state.committed_text();
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
        let cursor = match &widget.state {
            WidgetState::Date { input, .. } => input.widget.screen_cursor(),
            WidgetState::Text(state) | WidgetState::Number(state) => state.screen_cursor(),
            WidgetState::TextArea(state) => state.screen_cursor(),
            WidgetState::Select(_) => None,
        };
        if let Some(position) = cursor {
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
    let WidgetBox { state, options, .. } = widget;
    match state {
        WidgetState::Date { popup, .. } => {
            if !popup.core.is_active() {
                return;
            }
            // The month view controls its own start date (paging); the widget
            // only styles it. Selection shows reversed via the default focus
            // style.
            let month =
                Month::new().block(Block::bordered().border_style(Style::new().dark_gray()));
            let size = Rect::new(0, 0, month.width(), month.height(&popup.month));
            frame.render_stateful_widget(
                PopupCore::new()
                    .constraint(
                        Placement::BelowOrAbove.into_constraint(Alignment::Left, popup.anchor),
                    )
                    .boundary(area),
                size,
                &mut popup.core,
            );
            let popup_area = popup.core.area;
            frame.render_stateful_widget(month, popup_area, &mut popup.month);
        }
        WidgetState::Select(choice) => {
            if !choice.is_popup_active() {
                return;
            }
            // The main pass already rendered the closed half (setting the
            // anchor `state.area` and syncing the values); the popup half
            // positions itself off it.
            let (_, popup) = choice_widget(options).popup_boundary(area).into_widgets();
            frame.render_stateful_widget(popup, Rect::default(), choice.as_mut());
        }
        _ => {}
    }
}

/// The transient select widget: full labels as items (the popup's rows and
/// rat's first-char type-ahead both use them), closed item render skipped in
/// favor of the compact label painted by the caller.
fn choice_widget(options: &[SelectOption]) -> Choice<'_, String> {
    Choice::new()
        .items(
            options
                .iter()
                .map(|option| (option.value.clone(), option.full_label())),
        )
        .skip_item_render(true)
        .popup_len(10)
        .popup_block(Block::bordered().border_style(Style::new().dark_gray()))
        .popup_placement(Placement::BelowOrAbove)
}
