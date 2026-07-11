//! The masked date input and its calendar overlay: rat's `DateInputState`
//! plus a `Month` in a popup, seeded from the current value and paged with
//! browser date-picker semantics.

use chrono::{Datelike, Days, Months, NaiveDate};
use crossterm::event::{Event, KeyCode, MouseEvent, MouseEventKind};
use rat_widget::calendar::{selection::SingleSelection, Month, MonthState};
use rat_widget::date_input::{DateInput, DateInputState};
use rat_widget::event::{CalOutcome, HandleEvent, Regular};
use rat_widget::popup::{Placement, PopupCore, PopupCoreState};
use rat_widget::text::HasScreenCursor;
use ratatui::layout::{Alignment, Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;
use uic_core::Value;

use super::{OverlayOutcome, WidgetAdapter};
use crate::Error;

pub(super) struct DateAdapter {
    input: DateInputState,
    /// `core.is_active()` is the open flag; the anchor is the widget rect
    /// recorded during the paint pass.
    core: PopupCoreState,
    month: MonthState<SingleSelection>,
    anchor: Rect,
}

impl DateAdapter {
    pub(super) fn new() -> Result<Self, Error> {
        Ok(DateAdapter {
            input: DateInputState::new()
                .with_pattern("%Y-%m-%d")
                .map_err(|err| Error::Pattern(err.to_string()))?,
            core: PopupCoreState::new(),
            month: MonthState::new(),
            anchor: Rect::default(),
        })
    }

    /// Pages the open calendar by whole months, keeping the selected
    /// day-of-month (clamped to the target month's length).
    fn shift_month(&mut self, months: i32) {
        let base = self
            .month
            .selected_date()
            .unwrap_or_else(|| self.month.start_date());
        let target = if months < 0 {
            base.checked_sub_months(Months::new(months.unsigned_abs()))
        } else {
            base.checked_add_months(Months::new(months as u32))
        };
        if let Some(target) = target {
            self.month.set_start_date(target);
            self.month.select_date(target);
        }
    }
}

impl WidgetAdapter for DateAdapter {
    fn set_focus(&mut self, focused: bool) {
        self.input.widget.focus.set(focused);
    }

    fn area(&self) -> Rect {
        self.input.widget.area
    }

    /// The normalized date, or the digit-bearing raw text — the pristine
    /// mask is all zeros and commits as empty, like an untouched browser
    /// input fires no change.
    fn committed_text(&self) -> String {
        match self.input.value() {
            Ok(date) => date.format("%Y-%m-%d").to_string(),
            Err(_) => {
                let raw = self.input.widget.text();
                if raw.chars().any(|c| c.is_ascii_digit() && c != '0') {
                    raw.split_whitespace().collect::<Vec<_>>().join("")
                } else {
                    String::new()
                }
            }
        }
    }

    fn sync(&mut self, value: &Value) {
        match value {
            Value::Str(text) if !text.is_empty() => {
                match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                    Ok(date) => {
                        self.input.set_value(date);
                        // An open calendar follows external value writes.
                        if self.core.is_active() {
                            self.month.set_start_date(date);
                            self.month.select_date(date);
                        }
                    }
                    Err(_) => self.input.widget.set_text(text.clone()),
                }
            }
            _ => self.input.clear(),
        }
    }

    fn handle(&mut self, focused: bool, event: &Event) -> bool {
        let _ = rat_widget::date_input::handle_events(&mut self.input, focused, event);
        false
    }

    fn place_cursor(&mut self, column: u16, _row: u16, extend: bool) {
        let x = column as i16 - self.input.widget.area.x as i16;
        self.input.widget.set_screen_cursor(x, extend);
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        self.anchor = rect;
        let mut date = DateInput::new();
        if let Some(style) = dim {
            date = date.style(style);
        }
        frame.render_stateful_widget(date, rect, &mut self.input);
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        self.input.widget.screen_cursor()
    }

    fn opens_overlay(&self) -> bool {
        true
    }

    fn overlay_open(&self) -> bool {
        self.core.is_active()
    }

    /// Opens the calendar seeded from the widget's current date, falling
    /// back to today (the system clock — the wasm host shims it).
    fn open_overlay(&mut self) {
        let seed = self
            .input
            .value()
            .ok()
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        self.month.set_start_date(seed);
        self.month.select_date(seed);
        self.month.focus.set(true);
        self.core.set_active(true);
    }

    fn close_overlay(&mut self) {
        self.core.set_active(false);
        self.month.focus.set(false);
        self.core.clear_areas();
    }

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
                let picked = self.month.selected_date();
                if let Some(date) = picked {
                    self.input.set_value(date);
                }
                self.close_overlay();
                if picked.is_some() {
                    OverlayOutcome::Commit
                } else {
                    OverlayOutcome::Consumed
                }
            }
            KeyCode::PageUp => {
                self.shift_month(-1);
                OverlayOutcome::Consumed
            }
            KeyCode::PageDown => {
                self.shift_month(1);
                OverlayOutcome::Consumed
            }
            code @ (KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down) => {
                if self.month.handle(event, Regular) == CalOutcome::Continue {
                    // The month widget stops at its edges; roll over into the
                    // neighboring month like the browser's date picker.
                    if let Some(selected) = self.month.selected_date() {
                        let target = match code {
                            KeyCode::Left => selected.checked_sub_days(Days::new(1)),
                            KeyCode::Right => selected.checked_add_days(Days::new(1)),
                            KeyCode::Up => selected.checked_sub_days(Days::new(7)),
                            KeyCode::Down => selected.checked_add_days(Days::new(7)),
                            _ => None,
                        };
                        if let Some(target) = target {
                            self.month.set_start_date(target);
                            self.month.select_date(target);
                        }
                    }
                }
                OverlayOutcome::Consumed
            }
            _ => {
                let _ = self.month.handle(event, Regular);
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
                // Picks resolve against the published day rects instead of
                // rat's mouse handling — see `place_cursor`.
                let start = self.month.start_date();
                let date = self
                    .month
                    .area_days
                    .iter()
                    .position(|day| day.contains(position))
                    .and_then(|index| start.with_day(index as u32 + 1));
                if let Some(date) = date {
                    self.input.set_value(date);
                    self.close_overlay();
                    return OverlayOutcome::Commit;
                }
                OverlayOutcome::Consumed
            }
            MouseEventKind::ScrollUp => {
                self.shift_month(-1);
                OverlayOutcome::Consumed
            }
            MouseEventKind::ScrollDown => {
                self.shift_month(1);
                OverlayOutcome::Consumed
            }
            _ => OverlayOutcome::Consumed,
        }
    }

    fn paint_overlay(&mut self, frame: &mut Frame, boundary: Rect) {
        // The month view controls its own start date (paging); the widget
        // only styles it. Selection shows reversed via the default focus
        // style.
        let month = Month::new().block(Block::bordered().border_style(Style::new().dark_gray()));
        let size = Rect::new(0, 0, month.width(), month.height(&self.month));
        frame.render_stateful_widget(
            PopupCore::new()
                .constraint(Placement::BelowOrAbove.into_constraint(Alignment::Left, self.anchor))
                .boundary(boundary),
            size,
            &mut self.core,
        );
        let popup_area = self.core.area;
        frame.render_stateful_widget(month, popup_area, &mut self.month);
    }
}
