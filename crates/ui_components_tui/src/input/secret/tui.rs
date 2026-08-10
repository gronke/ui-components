//! The terminal twin of `<input-secret>` (ADR 0002/0034): a masked field that
//! reads and, when editable, sets a secret.
//!
//! Bullets stand in for the value; a `[x] reveal` checkbox (Space at rest, or a
//! click) toggles visibility and survives an edit. When the element is not
//! `readonly`, Enter opens edit mode — which reveals the text so it can be read
//! while typing — Enter commits and Esc reverts to the value at edit-start;
//! Tabbing away commits too. A field the host never disclosed (`value` null) is
//! write-only: nothing to reveal but the new input. There is no clipboard in
//! the terminal, so copy stays browser-only in `secret.impl.ts`. Registered for
//! `data-tui="secret-input"`.

use uic_core::Value;
use uic_tui::crossterm::event::{Event, KeyCode};
use uic_tui::rat_widget::text::HasScreenCursor;
use uic_tui::rat_widget::text_input::{handle_events, TextInput, TextInputState};
use uic_tui::ratatui::layout::{Alignment, Position, Rect};
use uic_tui::ratatui::style::Style;
use uic_tui::ratatui::text::{Line, Span};
use uic_tui::ratatui::widgets::Clear;
use uic_tui::ratatui::Frame;
use uic_tui::{OverlayOutcome, WidgetAdapter, WidgetRegistration};

uic_core::inventory::submit! {
    WidgetRegistration {
        kind: "secret-input",
        build: SecretAdapter::build,
    }
}

struct SecretAdapter {
    /// The editable text; rat holds it whether or not an edit is in progress.
    input: TextInputState,
    /// The element's `readonly` state, pushed each frame; a readonly field is
    /// display-only and never enters edit mode.
    readonly: bool,
    focused: bool,
    /// The reveal checkbox: visibility at rest, persistent across an edit.
    revealed: bool,
    /// The modal edit state (Enter in, Enter/Esc out).
    editing: bool,
    /// The text at edit-start, restored on an Esc revert.
    original: String,
    /// The last paint's cells, and the checkbox within them, for hit-testing.
    area: Rect,
    checkbox: Rect,
}

impl SecretAdapter {
    fn new() -> SecretAdapter {
        SecretAdapter {
            input: TextInputState::new(),
            readonly: false,
            focused: false,
            revealed: false,
            editing: false,
            original: String::new(),
            area: Rect::default(),
            checkbox: Rect::default(),
        }
    }

    fn build() -> Box<dyn WidgetAdapter> {
        Box::new(SecretAdapter::new())
    }

    /// One bullet per character: the length shows, the value does not.
    fn masked(&self) -> String {
        "•".repeat(self.input.text().chars().count())
    }
}

impl WidgetAdapter for SecretAdapter {
    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        self.input.focus.set(focused);
        if !focused {
            // Leaving the field ends the edit; the text is kept (Tab commits).
            self.editing = false;
        }
    }

    fn set_readonly(&mut self, readonly: bool) {
        self.readonly = readonly;
        if readonly {
            self.editing = false;
        }
    }

    fn area(&self) -> Rect {
        self.area
    }

    fn committed_text(&self) -> String {
        self.input.text().to_string()
    }

    fn sync(&mut self, value: &Value) {
        match value {
            Value::Str(text) => self.input.set_text(text.clone()),
            // Null (or non-string) is the write-only "not set" state.
            _ => {
                self.input.clear();
            }
        }
    }

    fn caret_to_end(&mut self) {
        let len = self.input.len();
        self.input.set_cursor(len, false);
    }

    fn handle(&mut self, _focused: bool, event: &Event) -> bool {
        // At rest, Space toggles the reveal checkbox; editing lives in the
        // overlay (opened on Enter or F4), so nothing here commits.
        if let Event::Key(key) = event {
            if key.code == KeyCode::Char(' ') {
                self.revealed = !self.revealed;
            }
        }
        false
    }

    // Edit mode is an overlay so its keys are captured modally: Enter commits,
    // Esc reverts (and does not quit the app), typing edits. Only an editable
    // field opens it — on Enter (enter_opens_overlay) or the catalog's F4/Down.

    fn opens_overlay(&self) -> bool {
        !self.readonly
    }

    fn enter_opens_overlay(&self) -> bool {
        !self.readonly
    }

    fn overlay_open(&self) -> bool {
        self.editing
    }

    fn open_overlay(&mut self) {
        if self.readonly {
            return;
        }
        // Editing reveals the text to type; remember the value to revert to.
        self.original = self.input.text().to_string();
        self.editing = true;
    }

    fn close_overlay(&mut self) {
        self.editing = false;
    }

    fn overlay_key(&mut self, event: &Event) -> OverlayOutcome {
        let Event::Key(key) = event else {
            return OverlayOutcome::Consumed;
        };
        match key.code {
            // Save and leave edit mode; the host commits committed_text.
            KeyCode::Enter => {
                self.editing = false;
                OverlayOutcome::Commit
            }
            // Revert to the value at edit-start and leave.
            KeyCode::Esc => {
                self.input.set_text(self.original.clone());
                self.editing = false;
                OverlayOutcome::Consumed
            }
            // Tab commits the current text and steps (the global handling).
            KeyCode::Tab => {
                self.editing = false;
                OverlayOutcome::Pass
            }
            // Everything else edits.
            _ => {
                let _ = handle_events(&mut self.input, true, event);
                OverlayOutcome::Consumed
            }
        }
    }

    fn place_cursor(&mut self, column: u16, row: u16, extend: bool) {
        // A click on the reveal checkbox toggles it; elsewhere, while editing,
        // it positions the caret.
        if self.checkbox.contains(Position::new(column, row)) {
            self.revealed = !self.revealed;
            return;
        }
        if self.editing {
            let x = column as i16 - self.input.area.x as i16;
            self.input.set_screen_cursor(x, extend);
        }
    }

    fn paints_value(&self) -> bool {
        true
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        self.area = rect;
        self.checkbox = Rect::default();
        let base = dim.unwrap_or_default();
        frame.render_widget(Clear, rect);

        if self.editing {
            // Editing reveals the text so it reads while typing; a hint stands
            // where the checkbox otherwise sits.
            let hint = " Enter: save · Esc: cancel ";
            let hint_w = (hint.chars().count() as u16).min(rect.width);
            let field_w = rect.width.saturating_sub(hint_w);
            let field_area = Rect {
                width: field_w,
                ..rect
            };
            let hint_area = Rect {
                x: rect.x + field_w,
                width: hint_w,
                ..rect
            };
            let mut text = TextInput::new();
            if let Some(style) = dim {
                text = text.style(style);
            }
            frame.render_stateful_widget(text, field_area, &mut self.input);
            frame.render_widget(
                Line::from(Span::styled(hint, base.dim())).alignment(Alignment::Right),
                hint_area,
            );
            return;
        }

        // At rest: the masked or revealed value on the left, the reveal checkbox
        // on the right (brighter when focused). A readonly field shows the same
        // checkbox — revealing is how it is read.
        let shown = if self.revealed {
            self.input.text().to_string()
        } else {
            self.masked()
        };
        let label = if self.revealed {
            "[x] reveal "
        } else {
            "[ ] reveal "
        };
        let label_w = (label.chars().count() as u16).min(rect.width);
        let value_w = rect.width.saturating_sub(label_w);
        let value_area = Rect {
            width: value_w,
            ..rect
        };
        let box_area = Rect {
            x: rect.x + value_w,
            width: label_w,
            ..rect
        };
        self.checkbox = box_area;
        let label_style = if self.focused { base } else { base.dim() };
        frame.render_widget(Line::from(Span::styled(shown, base)), value_area);
        frame.render_widget(
            Line::from(Span::styled(label, label_style)).alignment(Alignment::Right),
            box_area,
        );
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        if self.editing {
            self.input.screen_cursor()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uic_tui::crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::empty()))
    }

    #[test]
    fn space_toggles_reveal_at_rest_and_masks_by_default() {
        let mut a = SecretAdapter::new();
        a.set_focus(true);
        a.sync(&Value::Str("ghs_secret".into()));

        assert!(!a.revealed);
        assert_eq!(a.masked().chars().count(), "ghs_secret".chars().count());
        assert!(a.masked().chars().all(|c| c == '•'));

        assert!(!a.handle(true, &key(KeyCode::Char(' '))));
        assert!(a.revealed);
        assert!(!a.handle(true, &key(KeyCode::Char(' '))));
        assert!(!a.revealed);
    }

    #[test]
    fn the_edit_overlay_commits_on_enter_and_reverts_on_esc() {
        let mut a = SecretAdapter::new();
        a.set_focus(true); // editable: readonly is false by default
        a.sync(&Value::Str("old".into()));

        // An editable field opens the edit overlay (on Enter or F4).
        assert!(a.opens_overlay());
        assert!(a.enter_opens_overlay());
        assert!(!a.overlay_open());

        // Open, edit, and Enter commits: the outcome asks the host to commit.
        a.open_overlay();
        assert!(a.overlay_open());
        a.input.set_text("new".to_string());
        assert!(matches!(
            a.overlay_key(&key(KeyCode::Enter)),
            OverlayOutcome::Commit
        ));
        assert!(!a.overlay_open());
        assert_eq!(a.committed_text(), "new");

        // Re-open, scribble, then Esc reverts to the value at edit-start.
        a.open_overlay();
        a.input.set_text("scratch".to_string());
        assert!(matches!(
            a.overlay_key(&key(KeyCode::Esc)),
            OverlayOutcome::Consumed
        ));
        assert!(!a.overlay_open());
        assert_eq!(a.committed_text(), "new");
    }

    #[test]
    fn a_readonly_field_opens_no_overlay_but_still_reveals() {
        let mut a = SecretAdapter::new();
        a.set_readonly(true);
        a.set_focus(true);
        a.sync(&Value::Str("token".into()));

        // A display-only field neither opens on Enter nor on F4.
        assert!(!a.opens_overlay());
        assert!(!a.enter_opens_overlay());
        a.open_overlay();
        assert!(!a.overlay_open());

        // But it can still be revealed to read.
        assert!(!a.handle(true, &key(KeyCode::Char(' '))));
        assert!(a.revealed);
    }

    #[test]
    fn a_null_value_is_the_write_only_empty_state() {
        let mut a = SecretAdapter::new();
        a.sync(&Value::Null);
        assert_eq!(a.committed_text(), "");
        assert_eq!(a.masked().chars().count(), 0);
    }
}
