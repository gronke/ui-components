//! Single-line text editing — serves `text-input` AND `number-input`: the
//! number's parsing and comma-decimal formatting are the component's job,
//! so the terminal side is one adapter.

use crossterm::event::Event;
use rat_widget::text::HasScreenCursor;
use rat_widget::text_input::{TextInput, TextInputState};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;
use uic_core::Value;

use super::WidgetAdapter;

pub(super) struct TextAdapter {
    state: TextInputState,
}

impl TextAdapter {
    pub(super) fn new() -> Self {
        TextAdapter {
            state: TextInputState::new(),
        }
    }
}

impl WidgetAdapter for TextAdapter {
    fn set_focus(&mut self, focused: bool) {
        self.state.focus.set(focused);
    }

    fn area(&self) -> Rect {
        self.state.area
    }

    fn committed_text(&self) -> String {
        self.state.text().to_string()
    }

    fn sync(&mut self, value: &Value) {
        match value {
            Value::Str(text) if !text.is_empty() => self.state.set_text(text.clone()),
            _ => {
                self.state.clear();
            }
        }
    }

    fn handle(&mut self, focused: bool, event: &Event) -> bool {
        let _ = rat_widget::text_input::handle_events(&mut self.state, focused, event);
        false
    }

    fn place_cursor(&mut self, column: u16, _row: u16, extend: bool) {
        let x = column as i16 - self.state.area.x as i16;
        self.state.set_screen_cursor(x, extend);
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        let mut text = TextInput::new();
        if let Some(style) = dim {
            text = text.style(style);
        }
        frame.render_stateful_widget(text, rect, &mut self.state);
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        self.state.screen_cursor()
    }
}
