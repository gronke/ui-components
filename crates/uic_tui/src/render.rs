//! Painting: walks the laid-out render tree onto the ratatui frame, mapping
//! the small set of Bootstrap text classes to terminal styles and hosting the
//! rat-widget input leaves.

use rat_widget::date_input::DateInput;
use rat_widget::text_input::TextInput;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::expand::{expand, RNode};
use crate::instance::{ElementInstance, WidgetState};
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
    let rnodes = expand(template, &instance.store, instance.behavior.as_ref());
    let laid = layout::compute(&rnodes, area);
    for node in &laid {
        paint(frame, node, instance, Hints::default());
    }
}

fn paint(frame: &mut Frame, laid: &layout::Laid, instance: &mut ElementInstance, hints: Hints) {
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
            if let Some(index) = *slot {
                paint_widget(frame, laid.rect, instance, index);
                return;
            }
            if classes.iter().any(|c| c == "input-group") {
                frame.render_widget(
                    Block::bordered().border_style(Style::new().dark_gray()),
                    laid.rect,
                );
            }
            for child in &laid.children {
                paint(frame, child, instance, hints);
            }
        }
    }
}

fn paint_widget(frame: &mut Frame, rect: Rect, instance: &mut ElementInstance, index: usize) {
    let focused = instance.focused == index;
    let disabled = match instance.slots.get(index) {
        Some(slot) => slot.is_disabled(&instance.store, instance.behavior.as_ref()),
        None => return,
    };
    let Some(slot) = instance.slots.get_mut(index) else {
        return;
    };
    slot.state.set_focus(focused && !disabled);
    let dim = disabled.then(|| Style::new().dim());
    match &mut slot.state {
        WidgetState::Date(state) => {
            let mut widget = DateInput::new();
            if let Some(style) = dim {
                widget = widget.style(style);
            }
            frame.render_stateful_widget(widget, rect, state);
        }
        WidgetState::Text(state) => {
            let mut widget = TextInput::new();
            if let Some(style) = dim {
                widget = widget.style(style);
            }
            frame.render_stateful_widget(widget, rect, state);
        }
    }
}
