//! Widget state living in the DOM: every `data-tui` element node carries its
//! rat widget in the document payload — node identity replaces the slot-by-
//! template-order bookkeeping of the retired expansion pipeline.
//!
//! One [`WidgetAdapter`] implementation per widget owns the rat state AND
//! the widget's data (a select keeps its option rows and bound value), so
//! focus glue, measurement, painting, event translation and the overlay
//! protocol live beside each other instead of as parallel match arms across
//! the runtime.

mod date;
mod select;
mod text;
mod textarea;

use crossterm::event::{Event, MouseEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;
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
    pub adapter: Box<dyn WidgetAdapter>,
    /// The value last pushed into the widget — the lit-style dirty check, so
    /// uncommitted typing survives unrelated updates and computed bindings
    /// re-sync only when their result actually changes.
    last_synced: Option<Value>,
}

impl WidgetBox {
    pub(crate) fn new(kind: &str) -> Result<Self, Error> {
        let adapter: Box<dyn WidgetAdapter> = match kind {
            "date-input" => Box::new(date::DateAdapter::new()?),
            // Number is a plain text widget in the terminal: parsing and
            // comma-decimal formatting are the component's job, like the
            // browser's `type="text"` numeric input.
            "text-input" | "number-input" => Box::new(text::TextAdapter::new()),
            "text-area" => Box::new(textarea::TextAreaAdapter::new()),
            "select" => Box::new(select::SelectAdapter::new()),
            _ => return Err(Error::UnknownWidget(kind.to_string())),
        };
        Ok(WidgetBox {
            adapter,
            last_synced: None,
        })
    }

    /// Pushes a `.value` property write into the widget, dirty-checked.
    pub(crate) fn sync_value(&mut self, value: &Value) {
        if self.last_synced.as_ref() == Some(value) {
            return;
        }
        self.last_synced = Some(value.clone());
        self.adapter.sync(value);
    }
}

/// What an overlay did with an event, for the host to act on.
pub(crate) enum OverlayOutcome {
    /// The overlay consumed the event.
    Consumed,
    /// Not consumed — the global handling continues (Tab closes and falls
    /// through to commit-and-step, an outside press falls through to the
    /// hit test).
    Pass,
    /// The overlay closed over a pick: the host commits the widget.
    Commit,
}

/// The terminal widget behind a `data-tui` leaf: rat state, its data, and
/// every per-widget behavior the runtime dispatches.
pub(crate) trait WidgetAdapter {
    fn set_focus(&mut self, focused: bool);

    /// The screen cells the widget covered in the last paint, for pointer
    /// hit-testing.
    fn area(&self) -> Rect;

    /// The value a commit hands to the change handler — raw text; trimming,
    /// parsing and validation are the component's job, like in the browser.
    fn committed_text(&self) -> String;

    /// Pushes a property value into the widget.
    fn sync(&mut self, value: &Value);

    /// Receives a `.options` property write (ADR 0006); only the select
    /// stores rows.
    fn set_options(&mut self, _options: Vec<SelectOption>) {}

    /// Forwards a terminal event to the widget's own handling. Returns true
    /// when the widget changed its committed value and wants a commit (a
    /// closed select's type-ahead).
    fn handle(&mut self, focused: bool, event: &Event) -> bool;

    /// True when the widget consumes Enter itself (newline instead of
    /// commit); such widgets commit on focus leave, like `@change` on blur.
    fn is_multiline(&self) -> bool {
        false
    }

    /// Layout height in lines; growing widgets read their content.
    fn intrinsic_height(&self, _max_lines: u16) -> u16 {
        1
    }

    /// Content width for widgets that size to content the way the browser
    /// does; `None` keeps the flex default.
    fn intrinsic_width(&self) -> Option<u16> {
        None
    }

    /// Places the caret under the pointer (a drag extends the selection),
    /// or opens a select's list — the click semantics of the browser. rat's
    /// own mouse path stays unused everywhere: its click arming reads the
    /// system clock, which wasm32 does not have.
    fn place_cursor(&mut self, column: u16, row: u16, extend: bool);

    /// Renders the widget into its rect; `dim` styles a disabled widget.
    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>);

    /// True when `paint` already draws the widget's value text (the
    /// select's closed label): the generic placeholder and at-rest
    /// alignment pass skips such widgets.
    fn paints_value(&self) -> bool {
        false
    }

    /// The caret cell of a focused text-bearing widget, from the last paint.
    fn screen_cursor(&self) -> Option<(u16, u16)>;

    /// True when F4/Down may open an overlay (calendar, option list).
    fn opens_overlay(&self) -> bool {
        false
    }

    fn overlay_open(&self) -> bool {
        false
    }

    fn open_overlay(&mut self) {}

    fn close_overlay(&mut self) {}

    /// Routes a key press while the overlay is open (overlays are modal).
    fn overlay_key(&mut self, _event: &Event) -> OverlayOutcome {
        OverlayOutcome::Consumed
    }

    /// Routes the pointer while the overlay is open: picks commit, wheel
    /// and drags browse, an outside press dismisses and passes.
    fn overlay_mouse(&mut self, _mouse: MouseEvent) -> OverlayOutcome {
        OverlayOutcome::Consumed
    }

    /// Paints the open overlay after the normal pass; ratatui buffers are
    /// last-write-wins per cell, so it overlays the flow.
    fn paint_overlay(&mut self, _frame: &mut Frame, _boundary: Rect) {}
}
