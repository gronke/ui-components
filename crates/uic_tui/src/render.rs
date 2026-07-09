//! Painting: walks the laid-out render tree onto the ratatui frame, mapping
//! the small set of Bootstrap text classes to terminal styles and hosting the
//! rat-widget input leaves. Widget leaves of nested children resolve their
//! owning instance along the `SlotRef` path.

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
use uic_template::AttrPart;

use crate::expand::{expand, resolve_expr, RNode, SlotRef};
use crate::instance::{resolve_options, ElementInstance, WidgetState};
use crate::layout;

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

/// Paints one root into `area` and reports the content height it used, so a
/// host can stack several roots. An inactive root (focus parked on a sibling
/// root) paints no widget as focused; its popup is painted separately via
/// [`paint_popup`] so overlays win over content below them.
pub(crate) fn render_instance(
    frame: &mut Frame,
    area: Rect,
    instance: &mut ElementInstance,
    active: bool,
) -> u16 {
    let template = instance.def.template();
    let rnodes = expand(template, instance);
    let laid = layout::compute(&rnodes, area);
    // The flat focus index resolves once per frame to its (path, slot) so
    // widget leaves can compare against their own SlotRef.
    let focus = if active {
        instance.locate_path(instance.focused)
    } else {
        None
    };
    for node in &laid {
        paint(frame, node, instance, &focus, Hints::default());
    }
    content_height(&laid, area)
}

fn content_height(laid: &[layout::Laid], area: Rect) -> u16 {
    fn bottom(laid: &layout::Laid) -> u16 {
        laid.children
            .iter()
            .map(bottom)
            .max()
            .unwrap_or(0)
            .max(laid.rect.bottom())
    }
    laid.iter()
        .map(bottom)
        .max()
        .unwrap_or(area.y)
        .saturating_sub(area.y)
}

fn paint(
    frame: &mut Frame,
    laid: &layout::Laid,
    instance: &mut ElementInstance,
    focus: &Option<(Vec<usize>, usize)>,
    hints: Hints,
) {
    if laid.rect.width == 0 || laid.rect.height == 0 {
        return;
    }
    match laid.rnode {
        RNode::Text(text) => {
            let paragraph = Paragraph::new(Line::from(text.as_str())).style(hints.style());
            let paragraph = if hints.center {
                paragraph.alignment(Alignment::Center)
            } else {
                paragraph
            };
            frame.render_widget(paragraph, laid.rect);
        }
        RNode::Element { classes, slot, .. } => {
            let hints = hints.merge(classes);
            if let Some(slot) = slot {
                // Widget text alignment comes from the same classes the
                // browser styles.
                let align = if classes.iter().any(|c| c == "text-end") {
                    Some(Alignment::Right)
                } else if classes.iter().any(|c| c == "text-center") {
                    Some(Alignment::Center)
                } else {
                    None
                };
                paint_widget(frame, laid.rect, instance, slot, focus, align);
                return;
            }
            if classes.iter().any(|c| c == "input-group") {
                // Error wins over focus, like the browser keeping the red
                // outline on a focused invalid input.
                let border = if classes.iter().any(|c| c == "is-invalid") {
                    Style::new().fg(ERROR_BORDER)
                } else if contains_focus(laid, focus) {
                    Style::new().fg(FOCUS_RING)
                } else {
                    Style::new().dark_gray()
                };
                frame.render_widget(Block::bordered().border_style(border), laid.rect);
            }
            for child in &laid.children {
                paint(frame, child, instance, focus, hints);
            }
        }
    }
}

/// True when the focused widget's slot lies inside this laid subtree.
fn contains_focus(laid: &layout::Laid, focus: &Option<(Vec<usize>, usize)>) -> bool {
    fn walk(laid: &layout::Laid, path: &[usize], slot: usize) -> bool {
        if let RNode::Element {
            slot: Some(slot_ref),
            ..
        } = laid.rnode
        {
            if slot_ref.path == path && slot_ref.slot == slot {
                return true;
            }
        }
        laid.children.iter().any(|child| walk(child, path, slot))
    }
    match focus {
        Some((path, slot)) => walk(laid, path, *slot),
        None => false,
    }
}

fn paint_widget(
    frame: &mut Frame,
    rect: Rect,
    root: &mut ElementInstance,
    slot_ref: &SlotRef,
    focus: &Option<(Vec<usize>, usize)>,
    align: Option<Alignment>,
) {
    let focused = focus
        .as_ref()
        .is_some_and(|(path, slot)| path == &slot_ref.path && *slot == slot_ref.slot);
    let owner = root.descend_mut(&slot_ref.path);
    // Everything the paint needs from the immutable side resolves before the
    // mutable widget borrow: the disabled flag, a select's option list, and
    // the placeholder text.
    let (disabled, options, placeholder) = match owner.slots.get(slot_ref.slot) {
        Some(slot) => (
            slot.is_disabled(&owner.store, owner.behavior.as_ref()),
            slot.options_prop
                .as_ref()
                .map(|prop| resolve_options(&owner.store, owner.behavior.as_ref(), prop)),
            slot.placeholder.as_ref().map(|parts| {
                parts
                    .iter()
                    .map(|part| match part {
                        AttrPart::Static(text) => text.clone(),
                        AttrPart::Expr(expr) => {
                            resolve_expr(expr, &owner.store, owner.behavior.as_ref()).display_text()
                        }
                    })
                    .collect::<String>()
            }),
        ),
        None => return,
    };
    let Some(slot) = owner.slots.get_mut(slot_ref.slot) else {
        return;
    };
    slot.state.set_focus(focused && !disabled);
    let dim = disabled.then(|| Style::new().dim());
    match &mut slot.state {
        WidgetState::Date { input, popup } => {
            popup.anchor = rect;
            let mut widget = DateInput::new();
            if let Some(style) = dim {
                widget = widget.style(style);
            }
            frame.render_stateful_widget(widget, rect, input);
        }
        WidgetState::Text(state) | WidgetState::Number(state) => {
            let mut widget = TextInput::new();
            if let Some(style) = dim {
                widget = widget.style(style);
            }
            frame.render_stateful_widget(widget, rect, state);
        }
        WidgetState::TextArea(state) => {
            let mut widget = TextArea::new();
            if let Some(style) = dim {
                widget = widget.style(style);
            }
            frame.render_stateful_widget(widget, rect, state.as_mut());
        }
        WidgetState::Select(state) => {
            let options = options.unwrap_or_default();
            let mut widget = choice_widget(&options);
            if let Some(style) = dim {
                widget = widget.style(style);
            }
            let (widget, _) = widget.into_widgets();
            frame.render_stateful_widget(widget, rect, state.as_mut());
            // The closed line shows the compact label (`short || label ||
            // value` of the selected option) while the popup lists full
            // labels; rat renders the same line in both places, so the item
            // render is skipped above and the closed text painted here. The
            // default-styled Line patches nothing over the focus styling.
            let value = state.value();
            let closed = options
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
    if !matches!(slot.state, WidgetState::Select(_)) {
        let text = slot.state.committed_text();
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
        let cursor = match &slot.state {
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

/// Paints the focused slot's open overlay (calendar or option list) after
/// the normal pass; ratatui buffers are last-write-wins per cell, so it
/// overlays the flow.
pub(crate) fn paint_popup(frame: &mut Frame, area: Rect, instance: &mut ElementInstance) {
    let Some((path, slot)) = instance.locate_path(instance.focused) else {
        return;
    };
    let owner = instance.descend_mut(&path);
    let options = match owner.slots.get(slot) {
        Some(slot) => slot
            .options_prop
            .as_ref()
            .map(|prop| resolve_options(&owner.store, owner.behavior.as_ref(), prop)),
        None => return,
    };
    let Some(slot) = owner.slots.get_mut(slot) else {
        return;
    };
    match &mut slot.state {
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
        WidgetState::Select(state) => {
            if !state.is_popup_active() {
                return;
            }
            // The main pass already rendered the closed half (setting the
            // anchor `state.area` and syncing the values); the popup half
            // positions itself off it.
            let options = options.unwrap_or_default();
            let (_, popup) = choice_widget(&options).popup_boundary(area).into_widgets();
            frame.render_stateful_widget(popup, Rect::default(), state.as_mut());
        }
        _ => {}
    }
}
