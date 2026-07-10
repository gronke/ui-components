//! Widget state living in the DOM: every `data-tui` element node carries its
//! rat widget in the document payload — node identity replaces the slot-by-
//! template-order bookkeeping of the retired expansion pipeline.

use chrono::NaiveDate;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use rat_widget::calendar::{selection::SingleSelection, MonthState};
use rat_widget::choice::ChoiceState;
use rat_widget::date_input::DateInputState;
use rat_widget::event::ChoiceOutcome;
use rat_widget::popup::PopupCoreState;
use rat_widget::text_input::TextInputState;
use rat_widget::textarea::TextAreaState;
use ratatui::layout::Rect;
use uic_core::{SelectOption, Value};

use crate::Error;

/// The per-node payload of the runtime's document; empty on everything but
/// `data-tui` elements.
#[derive(Default)]
pub struct WidgetPayload {
    pub(crate) widget: Option<WidgetBox>,
}

/// A mounted terminal widget beside its sync bookkeeping.
pub(crate) struct WidgetBox {
    pub state: WidgetState,
    /// The value last pushed into the widget — the lit-style dirty check, so
    /// uncommitted typing survives unrelated updates and computed bindings
    /// re-sync only when their result actually changes.
    last_synced: Option<Value>,
    /// A select's option list, delivered as a `.options` property write.
    pub options: Vec<SelectOption>,
}

impl WidgetBox {
    pub(crate) fn new(kind: &str) -> Result<Self, Error> {
        Ok(WidgetBox {
            state: widget_state_for(kind)?,
            last_synced: None,
            options: Vec::new(),
        })
    }

    /// Pushes a `.value` property write into the widget, dirty-checked.
    pub(crate) fn sync_value(&mut self, value: &Value) {
        if self.last_synced.as_ref() == Some(value) {
            return;
        }
        self.last_synced = Some(value.clone());
        self.state.sync(value);
    }

    /// The bound value as widget text — what Esc reverts a browsing select
    /// to, like the browser's dropdown.
    pub(crate) fn last_synced_text(&self) -> String {
        match &self.last_synced {
            Some(Value::Str(text)) => text.clone(),
            _ => String::new(),
        }
    }
}

/// The calendar overlay attached to a date widget. `core.is_active()` is the
/// open flag; the anchor is the widget rect recorded during the paint pass.
pub(crate) struct DatePopup {
    pub core: PopupCoreState,
    pub month: MonthState<SingleSelection>,
    pub anchor: Rect,
}

impl DatePopup {
    fn new() -> Box<Self> {
        Box::new(DatePopup {
            core: PopupCoreState::new(),
            month: MonthState::new(),
            anchor: Rect::default(),
        })
    }
}

/// The persistent terminal widget behind a `data-tui` leaf.
/// The calendar state is boxed to keep the variants close in size.
pub(crate) enum WidgetState {
    Date {
        input: DateInputState,
        popup: Box<DatePopup>,
    },
    Text(TextInputState),
    /// A plain text widget; parsing and comma-decimal formatting are the
    /// component's job, like the browser's `type="text"` numeric input.
    Number(TextInputState),
    TextArea(Box<TextAreaState>),
    /// A dropdown select; the option list is data resolved from `.options`
    /// property writes (ADR 0006) and `ChoiceState` owns its popup state.
    Select(Box<ChoiceState<String>>),
}

impl WidgetState {
    pub(crate) fn set_focus(&mut self, focused: bool) {
        match self {
            WidgetState::Date { input, .. } => input.widget.focus.set(focused),
            WidgetState::Text(state) | WidgetState::Number(state) => state.focus.set(focused),
            WidgetState::TextArea(state) => state.focus.set(focused),
            WidgetState::Select(state) => state.focus.set(focused),
        }
    }

    /// True when the widget consumes Enter itself (newline instead of
    /// commit); such widgets commit on focus leave, like `@change` on blur.
    pub(crate) fn is_multiline(&self) -> bool {
        matches!(self, WidgetState::TextArea(_))
    }

    /// The screen cells the widget covered in the last paint, for pointer
    /// hit-testing.
    pub(crate) fn area(&self) -> Rect {
        match self {
            WidgetState::Date { input, .. } => input.widget.area,
            WidgetState::Text(state) | WidgetState::Number(state) => state.area,
            WidgetState::TextArea(state) => state.area,
            WidgetState::Select(state) => state.area,
        }
    }

    /// The value a commit hands to the change handler. The masked date input
    /// passes the normalized date (or the digit-bearing raw text); the plain
    /// text widgets pass their raw text — trimming, parsing and validation
    /// are the component's job, like in the browser.
    pub(crate) fn committed_text(&self) -> String {
        match self {
            WidgetState::Date { input, .. } => match input.value() {
                Ok(date) => date.format("%Y-%m-%d").to_string(),
                Err(_) => {
                    // The pristine mask is all zeros: commit it as empty,
                    // like an untouched browser input fires no change.
                    let raw = input.widget.text();
                    if raw.chars().any(|c| c.is_ascii_digit() && c != '0') {
                        raw.split_whitespace().collect::<Vec<_>>().join("")
                    } else {
                        String::new()
                    }
                }
            },
            WidgetState::Text(state) | WidgetState::Number(state) => state.text().to_string(),
            WidgetState::TextArea(state) => state.text(),
            WidgetState::Select(state) => state.value(),
        }
    }

    /// Pushes a property value into the widget.
    pub(crate) fn sync(&mut self, value: &Value) {
        match self {
            WidgetState::Date { input, popup } => match value {
                Value::Str(text) if !text.is_empty() => {
                    match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                        Ok(date) => {
                            input.set_value(date);
                            // An open calendar follows external value writes.
                            if popup.core.is_active() {
                                popup.month.set_start_date(date);
                                popup.month.select_date(date);
                            }
                        }
                        Err(_) => input.widget.set_text(text.clone()),
                    }
                }
                _ => input.clear(),
            },
            WidgetState::Text(state) | WidgetState::Number(state) => match value {
                Value::Str(text) if !text.is_empty() => state.set_text(text.clone()),
                _ => {
                    state.clear();
                }
            },
            WidgetState::TextArea(state) => match value {
                Value::Str(text) if !text.is_empty() => state.set_text(text),
                _ => {
                    state.clear();
                }
            },
            WidgetState::Select(state) => match value {
                // Empty is a legitimate select value (the null/default row).
                Value::Str(text) => {
                    state.set_value(text.clone());
                }
                _ => {
                    state.set_value(String::new());
                }
            },
        }
    }

    /// Forwards a terminal event to the widget's own handling. Returns true
    /// when the widget changed its committed value and wants a commit (a
    /// closed select's type-ahead, like the browser's dropdown navigation).
    pub(crate) fn handle(&mut self, focused: bool, event: &Event) -> bool {
        match self {
            WidgetState::Date { input, .. } => {
                let _ = rat_widget::date_input::handle_events(input, focused, event);
                false
            }
            WidgetState::Text(state) | WidgetState::Number(state) => {
                let _ = rat_widget::text_input::handle_events(state, focused, event);
                false
            }
            WidgetState::TextArea(state) => {
                let _ = rat_widget::textarea::handle_events(state, focused, event);
                false
            }
            WidgetState::Select(state) => {
                // Navigation keys are filtered while closed: opening goes
                // through the global F4/Down gate, and a closed select must
                // not spin its value. Printables (first-char type-ahead),
                // Space (opens) and Backspace/Delete still reach the widget.
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
                rat_widget::choice::handle_events(state, focused, event) == ChoiceOutcome::Value
            }
        }
    }
}

/// Fresh terminal widget state for a `data-tui` kind.
fn widget_state_for(kind: &str) -> Result<WidgetState, Error> {
    Ok(match kind {
        "date-input" => WidgetState::Date {
            input: DateInputState::new()
                .with_pattern("%Y-%m-%d")
                .map_err(|err| Error::Pattern(err.to_string()))?,
            popup: DatePopup::new(),
        },
        "text-input" => WidgetState::Text(TextInputState::new()),
        "number-input" => WidgetState::Number(TextInputState::new()),
        "text-area" => WidgetState::TextArea(Box::new(TextAreaState::new())),
        "select" => WidgetState::Select(Box::new(ChoiceState::new())),
        _ => return Err(Error::UnknownWidget(kind.to_string())),
    })
}
