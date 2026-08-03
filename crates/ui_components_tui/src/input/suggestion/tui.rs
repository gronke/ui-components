//! The terminal twin of `<input-suggestion>` (ADR 0002/0015): rat's text
//! editing plus a hand-painted suggestion popup; the browser half lives in
//! `suggestion.impl.ts`. Registered for `data-tui="suggestion-input"`
//! through the widget registry, so the runtime needs no edit.

use uic_core::{SelectOption, Value};
use uic_tui::crossterm::event::{Event, KeyCode, MouseEvent, MouseEventKind};
use uic_tui::rat_widget::popup::{Placement, PopupCore, PopupCoreState};
use uic_tui::rat_widget::text::HasScreenCursor;
use uic_tui::rat_widget::text_input::{TextInput, TextInputState};
use uic_tui::ratatui::layout::{Alignment, Position, Rect};
use uic_tui::ratatui::style::Style;
use uic_tui::ratatui::text::Line;
use uic_tui::ratatui::widgets::{Block, Clear};
use uic_tui::ratatui::Frame;
use uic_tui::unicode_width::UnicodeWidthStr;
use uic_tui::{OverlayOutcome, WidgetAdapter, WidgetRegistration};

/// The popup shows at most this many rows, like the select's option list.
const VISIBLE_ROWS: usize = 10;

uic_core::inventory::submit! {
    WidgetRegistration {
        kind: "suggestion-input",
        build: SuggestionAdapter::build,
    }
}

struct SuggestionAdapter {
    input: TextInputState,
    /// The option rows delivered as `.options` property writes (ADR 0014).
    options: Vec<SelectOption>,
    /// The keyboard highlight; `None` means Enter commits the typed text.
    selected: Option<usize>,
    /// First visible row of the scrolled popup window.
    offset: usize,
    /// `core.is_active()` is the open flag; the anchor is the widget rect
    /// recorded during the paint pass.
    core: PopupCoreState,
    anchor: Rect,
    /// The visible rows' screen cells from the last overlay paint, for
    /// pointer picks.
    item_areas: Vec<Rect>,
    /// Live text the widget's own handling produced, drained by
    /// `take_input` into the `@input` route.
    pending_input: Option<String>,
}

impl SuggestionAdapter {
    fn build() -> Box<dyn WidgetAdapter> {
        Box::new(SuggestionAdapter {
            input: TextInputState::new(),
            options: Vec::new(),
            selected: None,
            offset: 0,
            core: PopupCoreState::new(),
            anchor: Rect::default(),
            item_areas: Vec::new(),
            pending_input: None,
        })
    }

    /// Forwards to rat's text editing and records changed text for the
    /// `@input` route; typing resets the highlight: it belonged to the
    /// previous query.
    fn edit(&mut self, focused: bool, event: &Event) {
        let before = self.input.text().to_string();
        let _ = uic_tui::rat_widget::text_input::handle_events(&mut self.input, focused, event);
        if self.input.text() != before {
            self.pending_input = Some(self.input.text().to_string());
            self.selected = None;
            self.offset = 0;
        }
    }

    /// Moves the highlight; stepping above the first row clears it, so
    /// Enter falls back to committing the typed text.
    fn move_selection(&mut self, delta: isize) {
        if self.options.is_empty() {
            return;
        }
        self.selected = match self.selected {
            None if delta > 0 => Some(0),
            None => None,
            Some(0) if delta < 0 => None,
            Some(index) if delta < 0 => Some(index - 1),
            Some(index) => Some((index + 1).min(self.options.len() - 1)),
        };
        if let Some(selected) = self.selected {
            if selected < self.offset {
                self.offset = selected;
            } else if selected >= self.offset + VISIBLE_ROWS {
                self.offset = selected + 1 - VISIBLE_ROWS;
            }
        }
    }

    /// Adopts a row: its value becomes the widget text, like the browser
    /// pick filling the input.
    fn adopt(&mut self, index: usize) {
        if let Some(option) = self.options.get(index) {
            self.input.set_text(option.value.clone());
        }
    }
}

impl WidgetAdapter for SuggestionAdapter {
    fn set_focus(&mut self, focused: bool) {
        self.input.focus.set(focused);
    }

    fn area(&self) -> Rect {
        self.input.area
    }

    fn committed_text(&self) -> String {
        self.input.text().to_string()
    }

    fn sync(&mut self, value: &Value) {
        match value {
            Value::Str(text) if !text.is_empty() => self.input.set_text(text.clone()),
            _ => {
                self.input.clear();
            }
        }
    }

    fn set_options(&mut self, options: Vec<SelectOption>) {
        let changed = options != self.options;
        self.options = options;
        if changed {
            self.selected = None;
            self.offset = 0;
        }
        if self.options.is_empty() {
            self.close_overlay();
        } else if changed && self.input.focus.get() && !self.input.text().is_empty() {
            // Rows arriving while the user types open the popup; in-cycle
            // delivery repaints it in the same frame (ADR 0014).
            self.core.set_active(true);
        }
    }

    fn handle(&mut self, focused: bool, event: &Event) -> bool {
        self.edit(focused, event);
        false
    }

    fn take_input(&mut self) -> Option<String> {
        self.pending_input.take()
    }

    fn place_cursor(&mut self, column: u16, _row: u16, extend: bool) {
        let x = column as i16 - self.input.area.x as i16;
        self.input.set_screen_cursor(x, extend);
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        self.anchor = rect;
        let mut text = TextInput::new();
        if let Some(style) = dim {
            text = text.style(style);
        }
        frame.render_stateful_widget(text, rect, &mut self.input);
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        self.input.screen_cursor()
    }

    fn opens_overlay(&self) -> bool {
        !self.options.is_empty()
    }

    fn overlay_open(&self) -> bool {
        self.core.is_active()
    }

    fn open_overlay(&mut self) {
        self.core.set_active(true);
    }

    fn close_overlay(&mut self) {
        self.core.set_active(false);
        self.core.clear_areas();
        self.item_areas.clear();
    }

    /// Arrows move the highlight, Enter adopts it (or commits the typed
    /// text), Esc only closes: there is nothing to revert, the popup never
    /// touches the text. Everything else keeps editing while open.
    fn overlay_key(&mut self, event: &Event) -> OverlayOutcome {
        let Event::Key(key) = event else {
            return OverlayOutcome::Consumed;
        };
        match key.code {
            KeyCode::Esc => {
                self.close_overlay();
                OverlayOutcome::Consumed
            }
            KeyCode::Tab => {
                self.close_overlay();
                OverlayOutcome::Pass
            }
            KeyCode::Enter => {
                if let Some(selected) = self.selected {
                    self.adopt(selected);
                }
                self.close_overlay();
                OverlayOutcome::Commit
            }
            KeyCode::Down => {
                self.move_selection(1);
                OverlayOutcome::Consumed
            }
            KeyCode::Up => {
                self.move_selection(-1);
                OverlayOutcome::Consumed
            }
            _ => {
                self.edit(true, event);
                OverlayOutcome::Consumed
            }
        }
    }

    fn overlay_mouse(&mut self, mouse: MouseEvent) -> OverlayOutcome {
        let position = Position::new(mouse.column, mouse.row);
        if !self.core.area.contains(position) {
            if matches!(mouse.kind, MouseEventKind::Down(_)) {
                // The outside press dismisses and passes, so the same click
                // still focuses its target.
                self.close_overlay();
                return OverlayOutcome::Pass;
            }
            return OverlayOutcome::Consumed;
        }
        match mouse.kind {
            MouseEventKind::Down(_) => {
                if let Some(row) = self
                    .item_areas
                    .iter()
                    .position(|item| item.contains(position))
                {
                    self.adopt(self.offset + row);
                    self.close_overlay();
                    return OverlayOutcome::Commit;
                }
                OverlayOutcome::Consumed
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                // The wheel scrolls the list window without moving the
                // highlight, like the browser's open dropdown.
                let max_offset = self.options.len().saturating_sub(VISIBLE_ROWS);
                self.offset = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                    self.offset.saturating_sub(1)
                } else {
                    (self.offset + 1).min(max_offset)
                };
                OverlayOutcome::Consumed
            }
            _ => OverlayOutcome::Consumed,
        }
    }

    fn paint_overlay(&mut self, frame: &mut Frame, boundary: Rect) {
        if self.options.is_empty() {
            return;
        }
        let rows = self.options.len().min(VISIBLE_ROWS) as u16;
        let content = self
            .options
            .iter()
            .map(|option| option.full_label().width())
            .max()
            .unwrap_or(0) as u16;
        // At least the anchor's width, like the browser's dropdown; the
        // border adds a cell on each side.
        let width = (content + 2)
            .max(self.anchor.width)
            .min(boundary.width.max(1));
        let size = Rect::new(0, 0, width, rows + 2);
        frame.render_stateful_widget(
            PopupCore::new()
                .constraint(Placement::BelowOrAbove.into_constraint(Alignment::Left, self.anchor))
                .boundary(boundary),
            size,
            &mut self.core,
        );
        let area = self.core.area;
        frame.render_widget(Clear, area);
        let block = Block::bordered().border_style(Style::new().dark_gray());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.item_areas.clear();
        for (row, option) in self
            .options
            .iter()
            .skip(self.offset)
            .take(inner.height as usize)
            .enumerate()
        {
            let line_area = Rect {
                y: inner.y + row as u16,
                height: 1,
                ..inner
            };
            let style = if self.selected == Some(self.offset + row) {
                Style::new().reversed()
            } else {
                Style::new()
            };
            frame.render_widget(Line::from(option.full_label()).style(style), line_area);
            self.item_areas.push(line_area);
        }
    }
}
