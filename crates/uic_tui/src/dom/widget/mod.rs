//! Widget state living in the DOM: every element that implies a terminal
//! widget — a plain `<input>`/`<textarea>`/`<select>` by element type, or
//! an explicit `data-tui` override — carries its rat widget in the document
//! payload; node identity replaces the slot-by-template-order bookkeeping
//! of the retired expansion pipeline.
//!
//! One [`WidgetAdapter`] implementation per widget owns the rat state AND
//! the widget's data (a select keeps its option rows and bound value), so
//! focus glue, measurement, painting, event translation and the overlay
//! protocol live beside each other instead of as parallel match arms across
//! the runtime.
//!
//! Beyond the built-in kinds, components register their co-located widget
//! twins through [`WidgetRegistration`] (ADR 0015) — the runtime needs no
//! edit for a new `data-tui` kind.

mod date;
mod select;
mod text;
mod textarea;

use crossterm::event::{Event, MouseEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;
use uic_core::{SelectOption, Value};
use uic_dom::NodeId;

use super::DomDocument;
use crate::Error;

/// The per-node payload of the runtime's document; empty on everything but
/// widget-bearing elements.
#[derive(Default)]
pub struct WidgetPayload {
    pub(crate) widget: Option<WidgetBox>,
}

/// A mounted terminal widget beside its sync bookkeeping.
pub(crate) struct WidgetBox {
    pub adapter: Box<dyn WidgetAdapter>,
    /// The kind the widget was built as — a changed detection (a bound
    /// `type` landing after the first mount) recreates the widget.
    pub(crate) kind: &'static str,
    /// The variant flags the widget was built with (the date's mask follows
    /// hide-time/hide-seconds); a flipped flag recreates the widget.
    pub(crate) variant: (bool, bool),
    /// The value last pushed into the widget — the lit-style dirty check, so
    /// uncommitted typing survives unrelated updates and computed bindings
    /// re-sync only when their result actually changes.
    last_synced: Option<Value>,
}

/// Registers a widget adapter for a `data-tui` kind from outside the
/// runtime — the co-located TUI twin of a component (ADR 0015). Collected
/// through `inventory`; `WidgetBox::new` consults the registry after the
/// built-in kinds.
pub struct WidgetRegistration {
    /// The `data-tui` attribute value the registration serves.
    pub kind: &'static str,
    /// Builds a fresh adapter for one mounted element.
    pub build: fn() -> Box<dyn WidgetAdapter>,
}

uic_core::inventory::collect!(WidgetRegistration);

impl WidgetBox {
    pub(crate) fn new(kind: &str, hide_time: bool, hide_seconds: bool) -> Result<Self, Error> {
        let (kind, adapter): (&'static str, Box<dyn WidgetAdapter>) = match kind {
            "date-input" => (
                "date-input",
                Box::new(date::DateAdapter::new(hide_time, hide_seconds)?),
            ),
            // Number is a plain text widget in the terminal: parsing and
            // comma-decimal formatting are the component's job, like the
            // browser's `type="text"` numeric input.
            "text-input" => ("text-input", Box::new(text::TextAdapter::new())),
            "number-input" => ("number-input", Box::new(text::TextAdapter::new())),
            "text-area" => ("text-area", Box::new(textarea::TextAreaAdapter::new())),
            "select" => ("select", Box::new(select::SelectAdapter::new())),
            other => match uic_core::inventory::iter::<WidgetRegistration>()
                .find(|registration| registration.kind == other)
            {
                Some(registration) => (registration.kind, (registration.build)()),
                None => return Err(Error::UnknownWidget(other.to_string())),
            },
        };
        Ok(WidgetBox {
            adapter,
            kind,
            variant: (hide_time, hide_seconds),
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

    /// The scripted hosts' commit-time value sync, echo-skipped: a value
    /// equal to the widget's live text only records the sync — the
    /// component echoing back what the user just typed must not move the
    /// caret — while a genuinely different value syncs like a property
    /// write.
    pub(crate) fn sync_committed(&mut self, value: &Value) {
        if self.last_synced.as_ref() == Some(value) {
            return;
        }
        if let Value::Str(text) = value {
            if self.adapter.committed_text() == *text {
                self.last_synced = Some(value.clone());
                return;
            }
        }
        self.last_synced = Some(value.clone());
        self.adapter.sync(value);
        // The browser parks the caret at the end when a script assigns
        // `value`; rat's set_text parks it at the start — align.
        self.adapter.caret_to_end();
    }
}

/// The widget kind an element mounts, with its variant flags. An explicit
/// `data-tui` wins — the extension point for registered kinds and the
/// discriminator of the framework's own input templates — and mounts
/// anywhere, even on a `<ul>`. Plain form elements resolve by element type
/// (ADR 0027, the shared `uic_template::native` table), except presentation
/// twins opted out with a negative tabindex.
pub(crate) fn detect_kind(
    el: &uic_dom::ElementData<WidgetPayload>,
) -> Option<(String, (bool, bool))> {
    if let Some(kind) = el.attr("data-tui") {
        let variant = (
            el.attr("hide-time").is_some(),
            el.attr("hide-seconds").is_some(),
        );
        return Some((kind.to_string(), variant));
    }
    if el
        .attr("tabindex")
        .is_some_and(|value| value.trim().starts_with('-'))
    {
        return None;
    }
    let input_type = el.attr("type").map(|value| value.to_ascii_lowercase());
    let kind = uic_template::native::native_widget_kind(el.tag().as_ref(), input_type.as_deref())?;
    // Detected date variants are type-implied: the browser's date input is
    // date-only, datetime-local carries minutes. Seconds want the explicit
    // data-tui override.
    let variant = match (kind, input_type.as_deref()) {
        ("date-input", Some("date")) => (true, true),
        ("date-input", _) => (false, true),
        _ => (false, false),
    };
    Some((kind.to_string(), variant))
}

/// Creates the terminal widget for every element below `root` that implies
/// one — idempotent by the payload's kind and variant, so a changed
/// detection (a bound `type` landing after the bind-time mount, a flipped
/// date mask) recreates the widget, resetting typed state on purpose.
/// Shared by the Rust mounts and the scripted hosts' commit.
pub(crate) fn mount_widgets(doc: &mut DomDocument, root: NodeId) {
    let nodes: Vec<(NodeId, String, bool, bool)> = doc
        .descendants(root)
        .skip(1)
        .filter_map(|node| {
            let el = doc.element(node)?;
            let (kind, variant) = detect_kind(el)?;
            if el
                .data
                .widget
                .as_ref()
                .is_some_and(|widget| widget.kind == kind && widget.variant == variant)
            {
                return None;
            }
            Some((node, kind, variant.0, variant.1))
        })
        .collect();
    for (node, kind, hide_time, hide_seconds) in nodes {
        if let Ok(widget) = WidgetBox::new(&kind, hide_time, hide_seconds) {
            if let Some(el) = doc.element_mut(node) {
                el.data.widget = Some(widget);
            }
        }
    }
}

/// What an overlay did with an event, for the host to act on.
pub enum OverlayOutcome {
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
pub trait WidgetAdapter {
    fn set_focus(&mut self, focused: bool);

    /// The screen cells the widget covered in the last paint, for pointer
    /// hit-testing.
    fn area(&self) -> Rect;

    /// The value a commit hands to the change handler — raw text; trimming,
    /// parsing and validation are the component's job, like in the browser.
    fn committed_text(&self) -> String;

    /// Pushes a property value into the widget.
    fn sync(&mut self, value: &Value);

    /// Moves the caret behind the last character — what the browser does
    /// when a script assigns `value`. Only the scripted hosts call it;
    /// widgets without a movable caret ignore it.
    fn caret_to_end(&mut self) {}

    /// Receives a `.options` property write (ADR 0006); only the select
    /// stores rows.
    fn set_options(&mut self, _options: Vec<SelectOption>) {}

    /// Forwards a terminal event to the widget's own handling. Returns true
    /// when the widget changed its committed value and wants a commit (a
    /// closed select's type-ahead).
    fn handle(&mut self, focused: bool, event: &Event) -> bool;

    /// The live text after the widget's own handling changed it, consumed
    /// once — the host routes it into the template's `@input` binding, the
    /// browser's per-keystroke `input` event. Only widgets with live-text
    /// behavior report it.
    fn take_input(&mut self) -> Option<String> {
        None
    }

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

#[cfg(test)]
mod tests {
    use super::detect_kind;
    use crate::dom::DomDocument;

    fn detected(html: &str) -> Option<(String, (bool, bool))> {
        let doc: DomDocument = uic_dom::Document::parse_fragment(html, "body");
        let root = doc.root();
        let node = doc.children(root).next().expect("one element");
        detect_kind(doc.element(node).expect("an element"))
    }

    #[test]
    fn plain_form_elements_resolve_by_element_type() {
        assert_eq!(
            detected("<input>"),
            Some(("text-input".into(), (false, false)))
        );
        assert_eq!(
            detected(r#"<input type="text">"#),
            Some(("text-input".into(), (false, false)))
        );
        assert_eq!(
            detected(r#"<input type="EMAIL">"#),
            Some(("text-input".into(), (false, false)))
        );
        assert_eq!(
            detected(r#"<input type="number">"#),
            Some(("number-input".into(), (false, false)))
        );
        assert_eq!(
            detected("<textarea></textarea>"),
            Some(("text-area".into(), (false, false)))
        );
        assert_eq!(
            detected("<select></select>"),
            Some(("select".into(), (false, false)))
        );
    }

    #[test]
    fn date_variants_follow_the_type() {
        assert_eq!(
            detected(r#"<input type="date">"#),
            Some(("date-input".into(), (true, true)))
        );
        assert_eq!(
            detected(r#"<input type="datetime-local">"#),
            Some(("date-input".into(), (false, true)))
        );
        // The type implies the mask; the attrs only steer explicit data-tui.
        assert_eq!(
            detected(r#"<input type="date" hide-seconds>"#),
            Some(("date-input".into(), (true, true)))
        );
    }

    #[test]
    fn controls_and_presentation_twins_stay_plain() {
        assert_eq!(detected(r#"<input type="checkbox">"#), None);
        assert_eq!(detected(r#"<input type="submit">"#), None);
        assert_eq!(detected(r#"<input tabindex="-1">"#), None);
        assert_eq!(detected(r#"<select tabindex="-1"></select>"#), None);
        assert_eq!(detected("<div></div>"), None);
    }

    #[test]
    fn data_tui_overrides_everything() {
        assert_eq!(
            detected(r#"<span data-tui="text-input"></span>"#),
            Some(("text-input".into(), (false, false)))
        );
        assert_eq!(
            detected(r#"<input data-tui="text-input" tabindex="-1">"#),
            Some(("text-input".into(), (false, false)))
        );
        assert_eq!(
            detected(r#"<input data-tui="date-input" hide-time>"#),
            Some(("date-input".into(), (true, false)))
        );
    }
}
