//! The dropdown select: rat's `ChoiceState` plus the option rows delivered
//! as `.options` property writes (ADR 0005) and the bound value Esc reverts
//! to, like the browser's dropdown.

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEvent, MouseEventKind};
use rat_widget::choice::{Choice, ChoiceState};
use rat_widget::event::ChoiceOutcome;
use rat_widget::popup::Placement;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::Frame;
use uic_core::{SelectOption, Value};
use unicode_width::UnicodeWidthStr;

use super::{OverlayOutcome, WidgetAdapter};

pub(super) struct SelectAdapter {
    state: Box<ChoiceState<String>>,
    /// The option list, data resolved from `.options` property writes.
    options: Vec<SelectOption>,
    /// The bound value — what Esc reverts browsing to.
    bound: String,
}

impl SelectAdapter {
    pub(super) fn new() -> Self {
        SelectAdapter {
            state: Box::new(ChoiceState::new()),
            options: Vec::new(),
            bound: String::new(),
        }
    }

    /// Restores the widget value from its bound property — rat's browsing
    /// mutates the value continuously, so Esc reverts like the browser's
    /// dropdown.
    fn revert(&mut self) {
        self.state.set_value(self.bound.clone());
    }

    /// The compact closed-line label of the current value.
    fn closed_label(&self) -> String {
        let value = self.state.value();
        self.options
            .iter()
            .find(|option| option.value == value)
            .map(|option| option.short_label().to_string())
            .unwrap_or_default()
    }
}

/// The transient rat widget: full labels as items (the popup's rows and
/// rat's first-char type-ahead both use them), closed item render skipped
/// in favor of the compact label painted by `paint`. A free function over
/// the option slice, so the state stays borrowable beside it.
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

impl WidgetAdapter for SelectAdapter {
    fn set_focus(&mut self, focused: bool) {
        self.state.focus.set(focused);
    }

    fn area(&self) -> Rect {
        self.state.area
    }

    fn committed_text(&self) -> String {
        self.state.value()
    }

    fn sync(&mut self, value: &Value) {
        match value {
            // Empty is a legitimate select value (the null/default row).
            Value::Str(text) => {
                self.bound = text.clone();
                self.state.set_value(text.clone());
            }
            _ => {
                self.bound = String::new();
                self.state.set_value(String::new());
            }
        }
    }

    fn set_options(&mut self, options: Vec<SelectOption>) {
        self.options = options;
    }

    fn handle(&mut self, focused: bool, event: &Event) -> bool {
        // Navigation keys are filtered while closed: opening goes through
        // the global F4/Down gate, and a closed select must not spin its
        // value. Printables (first-char type-ahead), Space (opens) and
        // Backspace/Delete still reach the widget.
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press
                && matches!(
                    key.code,
                    KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::Home
                        | KeyCode::End
                        | KeyCode::PageUp
                        | KeyCode::PageDown
                )
            {
                return false;
            }
        }
        rat_widget::choice::handle_events(&mut self.state, focused, event) == ChoiceOutcome::Value
    }

    fn intrinsic_width(&self) -> Option<u16> {
        // The select hugs its closed label plus rat's three marker cells —
        // the catalog's fit-content.
        Some((self.closed_label().width() as u16).saturating_add(3))
    }

    fn place_cursor(&mut self, _column: u16, _row: u16, extend: bool) {
        if !extend && !self.state.is_popup_active() {
            self.state.set_popup_active(true);
            self.state.scroll_to_selected();
        }
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        let mut choice = choice_widget(&self.options);
        if let Some(style) = dim {
            choice = choice.style(style);
        }
        let (closed_widget, _) = choice.into_widgets();
        frame.render_stateful_widget(closed_widget, rect, self.state.as_mut());
        // The closed line shows the compact label (`short || label || value`
        // of the selected option) while the popup lists full labels; rat
        // renders the same line in both places, so the item render is
        // skipped above and the closed text painted here.
        frame.render_widget(Line::from(self.closed_label()), self.state.item_area);
    }

    fn paints_value(&self) -> bool {
        true
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        None
    }

    fn opens_overlay(&self) -> bool {
        true
    }

    fn overlay_open(&self) -> bool {
        self.state.is_popup_active()
    }

    fn open_overlay(&mut self) {
        self.state.set_popup_active(true);
        self.state.scroll_to_selected();
    }

    fn close_overlay(&mut self) {
        self.state.set_popup_active(false);
        self.state.popup.clear_areas();
    }

    /// Browsing (arrows, paging, type-ahead) mutates the widget value
    /// silently; Enter commits, Esc reverts to the bound value, Tab closes
    /// and passes so the global handling commits the browsed value and
    /// advances focus.
    fn overlay_key(&mut self, event: &Event) -> OverlayOutcome {
        let Event::Key(key) = event else {
            return OverlayOutcome::Consumed;
        };
        match key.code {
            KeyCode::Esc => {
                self.revert();
                self.close_overlay();
                OverlayOutcome::Consumed
            }
            KeyCode::Tab => {
                self.close_overlay();
                OverlayOutcome::Pass
            }
            KeyCode::Enter => {
                self.close_overlay();
                OverlayOutcome::Commit
            }
            _ => {
                let _ = rat_widget::choice::handle_events(&mut self.state, true, event);
                OverlayOutcome::Consumed
            }
        }
    }

    fn overlay_mouse(&mut self, mouse: MouseEvent) -> OverlayOutcome {
        let position = Position::new(mouse.column, mouse.row);
        if !self.state.popup.area.contains(position) {
            if matches!(mouse.kind, MouseEventKind::Down(_)) {
                // The outside press dismisses (reverting the browsed value)
                // and passes, so the same click still focuses its target.
                self.revert();
                self.close_overlay();
                return OverlayOutcome::Pass;
            }
            return OverlayOutcome::Consumed;
        }
        match mouse.kind {
            MouseEventKind::Down(_) => {
                // Picks resolve against the published row geometry instead
                // of rat's mouse handling — see `place_cursor`.
                if let Some(row) = self
                    .state
                    .item_areas
                    .iter()
                    .position(|item| item.contains(position))
                {
                    let _ = self.state.select(self.state.offset() + row);
                    self.close_overlay();
                    return OverlayOutcome::Commit;
                }
                OverlayOutcome::Consumed
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                // The wheel scrolls the list window without moving the
                // selection, like the browser's open dropdown.
                let offset = self.state.offset();
                let target = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                    offset.saturating_sub(1)
                } else {
                    offset.saturating_add(1)
                };
                let _ = self.state.set_offset(target);
                OverlayOutcome::Consumed
            }
            _ => OverlayOutcome::Consumed,
        }
    }

    fn paint_overlay(&mut self, frame: &mut Frame, boundary: Rect) {
        // The main pass already rendered the closed half (setting the
        // anchor `state.area` and syncing the values); the popup half
        // positions itself off it.
        let (_, popup) = choice_widget(&self.options)
            .popup_boundary(boundary)
            .into_widgets();
        frame.render_stateful_widget(popup, Rect::default(), self.state.as_mut());
    }
}
