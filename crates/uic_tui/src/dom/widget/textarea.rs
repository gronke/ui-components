//! Multi-line text editing: Enter inserts a newline and the commit happens
//! on focus leave, like `@change` on blur in the browser. The box grows with
//! its content up to the component's `max-lines`.

use crossterm::event::Event;
use rat_widget::text::HasScreenCursor;
use rat_widget::textarea::{TextArea, TextAreaState};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;
use uic_core::Value;

use super::WidgetAdapter;

pub(super) struct TextAreaAdapter {
    state: Box<TextAreaState>,
}

impl TextAreaAdapter {
    pub(super) fn new() -> Self {
        TextAreaAdapter {
            state: Box::new(TextAreaState::new()),
        }
    }
}

impl WidgetAdapter for TextAreaAdapter {
    fn set_focus(&mut self, focused: bool) {
        self.state.focus.set(focused);
    }

    fn area(&self) -> Rect {
        self.state.area
    }

    fn committed_text(&self) -> String {
        self.state.text()
    }

    fn sync(&mut self, value: &Value) {
        match value {
            Value::Str(text) if !text.is_empty() => self.state.set_text(text),
            _ => {
                self.state.clear();
            }
        }
    }

    fn handle(&mut self, focused: bool, event: &Event) -> bool {
        let _ = rat_widget::textarea::handle_events(&mut self.state, focused, event);
        false
    }

    fn is_multiline(&self) -> bool {
        true
    }

    fn intrinsic_height(&self, max_lines: u16) -> u16 {
        // rat's text is newline-terminated: the count includes an empty
        // tail line that never shows in the browser.
        let lines = (self.state.len_lines() as u16).saturating_sub(1).max(1);
        lines.clamp(1, max_lines.max(1))
    }

    fn place_cursor(&mut self, column: u16, row: u16, extend: bool) {
        let x = column as i16 - self.state.area.x as i16;
        let y = row as i16 - self.state.area.y as i16;
        self.state.set_screen_cursor((x, y), extend);
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        let mut area = TextArea::new();
        if let Some(style) = dim {
            area = area.style(style);
        }
        frame.render_stateful_widget(area, rect, self.state.as_mut());
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        self.state.screen_cursor()
    }
}
