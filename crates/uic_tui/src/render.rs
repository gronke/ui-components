//! Painting: walks the laid-out render tree onto the ratatui frame, mapping
//! the small set of Bootstrap text classes to terminal styles and hosting the
//! rat-widget input leaves. Widget leaves of nested children resolve their
//! owning instance along the `SlotRef` path.

use rat_widget::calendar::Month;
use rat_widget::choice::Choice;
use rat_widget::date_input::DateInput;
use rat_widget::popup::{Placement, PopupCore};
use rat_widget::text_input::TextInput;
use rat_widget::textarea::TextArea;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use uic_core::SelectOption;

use crate::expand::{expand, RNode, SlotRef};
use crate::instance::{resolve_options, ElementInstance, WidgetState};
use crate::layout;

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
                "text-danger" => self.fg = Some(Color::Red),
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

pub(crate) fn render_instance(frame: &mut Frame, area: Rect, instance: &mut ElementInstance) {
    let template = instance.def.template();
    let rnodes = expand(template, instance);
    let laid = layout::compute(&rnodes, area);
    // The flat focus index resolves once per frame to its (path, slot) so
    // widget leaves can compare against their own SlotRef.
    let focus = instance.locate_path(instance.focused);
    for node in &laid {
        paint(frame, node, instance, &focus, Hints::default());
    }
    paint_popup(frame, area, instance);
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
                paint_widget(frame, laid.rect, instance, slot, focus);
                return;
            }
            if classes.iter().any(|c| c == "input-group") {
                frame.render_widget(
                    Block::bordered().border_style(Style::new().dark_gray()),
                    laid.rect,
                );
            }
            for child in &laid.children {
                paint(frame, child, instance, focus, hints);
            }
        }
    }
}

fn paint_widget(
    frame: &mut Frame,
    rect: Rect,
    root: &mut ElementInstance,
    slot_ref: &SlotRef,
    focus: &Option<(Vec<usize>, usize)>,
) {
    let focused = focus
        .as_ref()
        .is_some_and(|(path, slot)| path == &slot_ref.path && *slot == slot_ref.slot);
    let owner = root.descend_mut(&slot_ref.path);
    // Everything the paint needs from the immutable side resolves before the
    // mutable widget borrow: the disabled flag and a select's option list.
    let (disabled, options) = match owner.slots.get(slot_ref.slot) {
        Some(slot) => (
            slot.is_disabled(&owner.store, owner.behavior.as_ref()),
            slot.options_prop
                .as_ref()
                .map(|prop| resolve_options(&owner.store, owner.behavior.as_ref(), prop)),
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
fn paint_popup(frame: &mut Frame, area: Rect, instance: &mut ElementInstance) {
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
