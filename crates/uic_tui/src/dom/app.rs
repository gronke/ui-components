//! The application host: mounted components live as element nodes in one
//! document, layout and paint read the tree, and keys and the pointer travel
//! it — focus is a node, not an index into template order, and unrendered
//! conditional branches are unfocusable because their nodes do not exist.

use chrono::{Datelike, Days, Months};
use crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use rat_widget::event::{CalOutcome, HandleEvent, Regular};
use ratatui::backend::Backend;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Terminal;
use uic_core::{NotifyEvent, Value};
use uic_dom::{Event as DomEvent, NodeId};

use super::host::Mount;
use super::render;
use super::widget::{WidgetBox, WidgetState};
use super::DomDocument;
use crate::{Control, Error};

type Listener = Box<dyn FnMut(&NotifyEvent)>;

/// Hosts mounted component trees on a terminal, rendering from the retained
/// document. Roots stack vertically like block elements in a document; Tab
/// cycles focus across root boundaries.
pub struct App<B: Backend> {
    terminal: Terminal<B>,
    doc: DomDocument,
    mounts: Vec<Mount>,
    focused: Option<NodeId>,
    /// Focus parked outside every element (a click into nothing): no ring,
    /// no caret, until the next key or widget click.
    blurred: bool,
    listeners: Vec<(usize, String, Listener)>,
    status: Option<Box<dyn Fn() -> String>>,
}

// The OS event loop; a browser host drives `from_terminal` + `handle_event`
// with synthesized events instead.
#[cfg(not(target_arch = "wasm32"))]
impl App<ratatui::backend::CrosstermBackend<std::io::Stdout>> {
    /// Takes over the terminal (alternate screen, raw mode, mouse capture).
    pub fn new() -> Result<Self, Error> {
        let terminal = ratatui::try_init()?;
        crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;
        Ok(Self::from_terminal(terminal))
    }

    /// Runs the event loop until Esc/Ctrl-C, then restores the terminal.
    /// Tab commits and cycles focus, Enter commits, clicks focus and pick.
    pub fn run(mut self) -> Result<(), Error> {
        let result = self.event_loop();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
        ratatui::try_restore()?;
        result
    }

    fn event_loop(&mut self) -> Result<(), Error> {
        loop {
            self.draw()?;
            let event = crossterm::event::read()?;
            if self.handle_event(&event) == Control::Quit {
                return Ok(());
            }
        }
    }
}

impl<B: Backend> App<B> {
    /// Hosts the runtime on an existing terminal (e.g. ratatui's
    /// `TestBackend`).
    pub fn from_terminal(terminal: Terminal<B>) -> Self {
        App {
            terminal,
            doc: DomDocument::new(),
            mounts: Vec::new(),
            focused: None,
            blurred: false,
            listeners: Vec::new(),
            status: None,
        }
    }

    /// Mounts a registered custom element as a root — the
    /// `document.createElement` + append moment, firing `connected`.
    /// Returns the root index for attribute and listener access.
    pub fn mount(&mut self, tag: &str) -> Result<usize, Error> {
        let root = self.doc.root();
        let mut mount = Mount::create(&mut self.doc, root, tag)?;
        // Nobody listens at mount time; the events drop like the browser's.
        let _ = mount.update_cycle(&mut self.doc, |behavior, ctx| behavior.connected(ctx));
        self.mounts.push(mount);
        if self.focused.is_none() {
            self.focused = self.focusables().into_iter().next();
        }
        Ok(self.mounts.len() - 1)
    }

    /// Sets an observed attribute on a mounted root.
    pub fn set_attr(&mut self, index: usize, name: &str, value: &str) {
        let Some(mount) = self.mounts.get_mut(index) else {
            return;
        };
        let events = mount.set_attr(&mut self.doc, name, Some(value));
        self.publish(index, events);
        self.ensure_focus();
    }

    /// Sets a property on a mounted root, `el.prop = …`.
    pub fn set_prop(&mut self, index: usize, name: &str, value: impl Into<Value>) {
        let Some(mount) = self.mounts.get_mut(index) else {
            return;
        };
        let events = mount.set_prop(&mut self.doc, name, value.into());
        self.publish(index, events);
        self.ensure_focus();
    }

    /// Subscribes to a mounted root's notify events.
    pub fn on(&mut self, index: usize, event: &str, listener: impl FnMut(&NotifyEvent) + 'static) {
        self.listeners
            .push((index, event.to_string(), Box::new(listener)));
    }

    /// The document, for assertions.
    pub fn doc(&self) -> &DomDocument {
        &self.doc
    }

    /// The underlying terminal, e.g. to inspect a `TestBackend` buffer.
    pub fn terminal(&self) -> &Terminal<B> {
        &self.terminal
    }

    /// A dim one-line status bar rendered at the bottom.
    pub fn status_bar(&mut self, text: impl Fn() -> String + 'static) {
        self.status = Some(Box::new(text));
    }

    pub fn draw(&mut self) -> Result<(), Error> {
        let App {
            terminal,
            doc,
            focused,
            blurred,
            status,
            ..
        } = self;
        let focused = if *blurred { None } else { *focused };
        terminal
            .draw(|frame| {
                let mut area = frame.area();
                if let Some(status) = status {
                    if area.height > 1 {
                        let status_area = Rect {
                            y: area.y + area.height - 1,
                            height: 1,
                            ..area
                        };
                        frame.render_widget(
                            Paragraph::new(status()).style(Style::new().dim()),
                            status_area,
                        );
                        area.height -= 1;
                    }
                }
                render::render_document(frame, area, doc, focused);
                // The focused widget's overlay paints after all content;
                // ratatui buffers are last-write-wins per cell, so it wins
                // over the roots below its anchor.
                render::paint_popup(frame, area, doc, focused);
            })
            .map_err(|err| Error::Terminal(err.to_string()))?;
        Ok(())
    }

    /// Blurs the focus like a click outside every element: the focused
    /// widget commits (`@change` on blur) and neither ring nor caret shows
    /// until the next key or widget click.
    pub fn blur(&mut self) {
        if self.blurred {
            return;
        }
        if self.popup_open() {
            self.close_popup();
        }
        self.commit_focused();
        self.blurred = true;
    }

    /// Routes one terminal event: an open overlay first (it is modal), then
    /// quit and focus/commit keys, everything else to the focused widget.
    /// A click focuses the widget under the pointer, committing the one it
    /// leaves; a click outside every element blurs.
    pub fn handle_event(&mut self, event: &Event) -> Control {
        if let Event::Mouse(mouse) = event {
            return self.handle_mouse(*mouse);
        }
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                self.blurred = false;
                // The overlay sees the key first: Esc closes it instead of
                // quitting; Tab closes it and falls through to the commit.
                if self.popup_open() && self.handle_popup_event(event) {
                    return Control::Continue;
                }
                match key.code {
                    KeyCode::Esc => return Control::Quit,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Control::Quit;
                    }
                    KeyCode::Tab => {
                        self.commit_focused();
                        self.step_focus(1);
                        return Control::Continue;
                    }
                    KeyCode::BackTab => {
                        self.commit_focused();
                        self.step_focus(-1);
                        return Control::Continue;
                    }
                    KeyCode::Enter => {
                        // A textarea takes the newline; it commits on focus
                        // leave (Tab), like `@change` on blur in the browser.
                        if self.focused_multiline() {
                            self.forward_to_focused(event);
                        } else {
                            self.commit_focused();
                        }
                        return Control::Continue;
                    }
                    KeyCode::F(4) | KeyCode::Down if self.focused_opens_overlay() => {
                        self.open_popup();
                        return Control::Continue;
                    }
                    _ => {}
                }
            }
        }
        if self.forward_to_focused(event) {
            // The widget changed its committed value and asked for a commit
            // (a closed select's type-ahead).
            self.commit_focused();
        }
        Control::Continue
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Control {
        // The open overlay sees the pointer first (it is modal); a press
        // outside it closes the overlay and falls through, so the same
        // click still focuses whatever it landed on.
        if !self.blurred && self.popup_open() && self.handle_popup_mouse(mouse) {
            return Control::Continue;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                match self.hit_test(mouse.column, mouse.row) {
                    Some(node) => {
                        // Leaving a widget commits it, like the browser's
                        // change-on-blur; a blurred click already committed.
                        if Some(node) != self.focused && !self.blurred {
                            self.commit_focused();
                        }
                        self.blurred = false;
                        self.focused = Some(node);
                        // The same press places the caret under the pointer;
                        // a select opens its list.
                        self.place_cursor(mouse.column, mouse.row, false);
                    }
                    None => self.blur(),
                }
                Control::Continue
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // A drag extends the focused widget's selection toward the
                // pointer.
                if !self.blurred {
                    self.place_cursor(mouse.column, mouse.row, true);
                }
                Control::Continue
            }
            _ => Control::Continue,
        }
    }

    /// Widgets reachable by focus, in document order — disabled widgets are
    /// skipped, and unrendered branches are absent by construction.
    fn focusables(&self) -> Vec<NodeId> {
        self.doc
            .descendants(self.doc.root())
            .filter(|&node| {
                let Some(el) = self.doc.element(node) else {
                    return false;
                };
                el.attr("data-tui").is_some()
                    && el.data.widget.is_some()
                    && el.attr("disabled").is_none()
            })
            .collect()
    }

    /// A structural change may have removed the focused node or disabled
    /// its widget; fall back to the first focusable.
    fn ensure_focus(&mut self) {
        let focusables = self.focusables();
        match self.focused {
            Some(node) if focusables.contains(&node) => {}
            _ => self.focused = focusables.into_iter().next(),
        }
    }

    fn step_focus(&mut self, direction: isize) {
        let focusables = self.focusables();
        if focusables.is_empty() {
            self.focused = None;
            return;
        }
        let current = self
            .focused
            .and_then(|node| focusables.iter().position(|&n| n == node))
            .unwrap_or(0);
        let count = focusables.len() as isize;
        let next = (current as isize + direction).rem_euclid(count) as usize;
        self.focused = Some(focusables[next]);
    }

    /// The widget node under the given screen cell, resolved against the
    /// areas recorded during the last paint.
    fn hit_test(&self, column: u16, row: u16) -> Option<NodeId> {
        let position = Position::new(column, row);
        self.focusables().into_iter().find(|&node| {
            self.doc
                .element(node)
                .and_then(|el| el.data.widget.as_ref())
                .is_some_and(|widget| widget.state.area().contains(position))
        })
    }

    fn focused_widget(&self) -> Option<&WidgetBox> {
        let node = self.focused?;
        self.doc
            .element(node)
            .and_then(|el| el.data.widget.as_ref())
    }

    fn focused_widget_mut(&mut self) -> Option<&mut WidgetBox> {
        let node = self.focused?;
        self.doc
            .element_mut(node)
            .and_then(|el| el.data.widget.as_mut())
    }

    fn focused_multiline(&self) -> bool {
        self.focused_widget()
            .is_some_and(|widget| widget.state.is_multiline())
    }

    /// True when the focused widget owns an overlay F4/Down may open (the
    /// date's calendar, the select's option list). Disabled widgets never
    /// hold focus, so no separate guard is needed.
    fn focused_opens_overlay(&self) -> bool {
        self.focused_widget().is_some_and(|widget| {
            matches!(
                widget.state,
                WidgetState::Date { .. } | WidgetState::Select(_)
            )
        })
    }

    /// Forwards the event to the focused widget's own handling; true when
    /// the widget requests a commit.
    fn forward_to_focused(&mut self, event: &Event) -> bool {
        self.focused_widget_mut()
            .is_some_and(|widget| widget.state.handle(true, event))
    }

    /// Places the caret under the pointer for the focused text-bearing
    /// widget (a drag extends the selection), or opens a select's list —
    /// the click semantics of the browser. rat's own mouse path stays
    /// unused everywhere: its click arming reads the system clock, which
    /// wasm32 does not have.
    fn place_cursor(&mut self, column: u16, row: u16, extend: bool) {
        let Some(widget) = self.focused_widget_mut() else {
            return;
        };
        match &mut widget.state {
            WidgetState::Date { input, .. } => {
                let x = column as i16 - input.widget.area.x as i16;
                input.widget.set_screen_cursor(x, extend);
            }
            WidgetState::Text(state) | WidgetState::Number(state) => {
                let x = column as i16 - state.area.x as i16;
                state.set_screen_cursor(x, extend);
            }
            WidgetState::TextArea(state) => {
                let x = column as i16 - state.area.x as i16;
                let y = row as i16 - state.area.y as i16;
                state.set_screen_cursor((x, y), extend);
            }
            WidgetState::Select(state) => {
                if !extend && !state.is_popup_active() {
                    state.set_popup_active(true);
                    state.scroll_to_selected();
                }
            }
        }
    }

    /// True when the focused widget's overlay (calendar or option list) is
    /// open.
    fn popup_open(&self) -> bool {
        self.focused_widget()
            .is_some_and(|widget| match &widget.state {
                WidgetState::Date { popup, .. } => popup.core.is_active(),
                WidgetState::Select(state) => state.is_popup_active(),
                _ => false,
            })
    }

    /// Opens the focused widget's overlay: the calendar seeded from the
    /// widget's current date (falling back to today), or the option list
    /// scrolled to the current selection.
    fn open_popup(&mut self) {
        let Some(widget) = self.focused_widget_mut() else {
            return;
        };
        match &mut widget.state {
            WidgetState::Date { input, popup } => {
                let seed = input
                    .value()
                    .ok()
                    .unwrap_or_else(|| chrono::Local::now().date_naive());
                popup.month.set_start_date(seed);
                popup.month.select_date(seed);
                popup.month.focus.set(true);
                popup.core.set_active(true);
            }
            WidgetState::Select(state) => {
                state.set_popup_active(true);
                state.scroll_to_selected();
            }
            _ => {}
        }
    }

    fn close_popup(&mut self) {
        let Some(widget) = self.focused_widget_mut() else {
            return;
        };
        match &mut widget.state {
            WidgetState::Date { popup, .. } => {
                popup.core.set_active(false);
                popup.month.focus.set(false);
                popup.core.clear_areas();
            }
            WidgetState::Select(state) => {
                state.set_popup_active(false);
                state.popup.clear_areas();
            }
            _ => {}
        }
    }

    /// Routes a key press while an overlay is open (overlays are modal).
    /// Returns whether the event was consumed; Tab closes and reports
    /// unconsumed so the global commit-and-focus handling still runs.
    fn handle_popup_event(&mut self, event: &Event) -> bool {
        if let Event::Mouse(mouse) = event {
            return self.handle_popup_mouse(*mouse);
        }
        let Event::Key(key) = event else {
            return false;
        };
        if key.kind != KeyEventKind::Press {
            return true;
        }
        let select_open = matches!(
            self.focused_widget(),
            Some(widget) if matches!(widget.state, WidgetState::Select(_))
        );
        if select_open {
            return self.handle_select_popup_event(event, key.code);
        }
        match key.code {
            KeyCode::Esc => {
                self.close_popup();
                true
            }
            KeyCode::Tab => {
                self.close_popup();
                false
            }
            KeyCode::Enter => {
                let mut picked = false;
                if let Some(widget) = self.focused_widget_mut() {
                    if let WidgetState::Date { input, popup } = &mut widget.state {
                        if let Some(date) = popup.month.selected_date() {
                            input.set_value(date);
                            picked = true;
                        }
                    }
                }
                self.close_popup();
                if picked {
                    self.commit_focused();
                }
                true
            }
            KeyCode::PageUp => {
                self.shift_popup_month(-1);
                true
            }
            KeyCode::PageDown => {
                self.shift_popup_month(1);
                true
            }
            code @ (KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down) => {
                let Some(widget) = self.focused_widget_mut() else {
                    return true;
                };
                let WidgetState::Date { popup, .. } = &mut widget.state else {
                    return true;
                };
                if popup.month.handle(event, Regular) == CalOutcome::Continue {
                    // The month widget stops at its edges; roll over into the
                    // neighboring month like the browser's date picker.
                    if let Some(selected) = popup.month.selected_date() {
                        let target = match code {
                            KeyCode::Left => selected.checked_sub_days(Days::new(1)),
                            KeyCode::Right => selected.checked_add_days(Days::new(1)),
                            KeyCode::Up => selected.checked_sub_days(Days::new(7)),
                            KeyCode::Down => selected.checked_add_days(Days::new(7)),
                            _ => None,
                        };
                        if let Some(target) = target {
                            popup.month.set_start_date(target);
                            popup.month.select_date(target);
                        }
                    }
                }
                true
            }
            _ => {
                if let Some(widget) = self.focused_widget_mut() {
                    if let WidgetState::Date { popup, .. } = &mut widget.state {
                        let _ = popup.month.handle(event, Regular);
                    }
                }
                true
            }
        }
    }

    /// Routes the pointer while an overlay is open: a click picks the day or
    /// option under it (committing like Enter), the wheel and drags browse,
    /// and a press outside dismisses the overlay and reports unconsumed so
    /// the click still focuses whatever it landed on.
    fn handle_popup_mouse(&mut self, mouse: MouseEvent) -> bool {
        enum Overlay {
            Date,
            Select,
        }
        let position = Position::new(mouse.column, mouse.row);
        let (overlay, inside) = match self.focused_widget() {
            Some(widget) => match &widget.state {
                WidgetState::Date { popup, .. } => {
                    (Overlay::Date, popup.core.area.contains(position))
                }
                WidgetState::Select(state) => {
                    (Overlay::Select, state.popup.area.contains(position))
                }
                _ => return false,
            },
            None => return false,
        };
        if !inside {
            if matches!(mouse.kind, MouseEventKind::Down(_)) {
                if matches!(overlay, Overlay::Select) {
                    self.revert_select();
                }
                self.close_popup();
                return false;
            }
            return true;
        }
        // Picks resolve against the overlay's published geometry (day rects,
        // option rows) instead of rat's mouse handling — see [`Self::place_cursor`].
        match overlay {
            Overlay::Date => match mouse.kind {
                MouseEventKind::Down(_) => {
                    let mut picked = false;
                    if let Some(widget) = self.focused_widget_mut() {
                        if let WidgetState::Date { input, popup } = &mut widget.state {
                            let start = popup.month.start_date();
                            let date = popup
                                .month
                                .area_days
                                .iter()
                                .position(|day| day.contains(position))
                                .and_then(|index| start.with_day(index as u32 + 1));
                            if let Some(date) = date {
                                input.set_value(date);
                                picked = true;
                            }
                        }
                    }
                    if picked {
                        self.close_popup();
                        self.commit_focused();
                    }
                    true
                }
                MouseEventKind::ScrollUp => {
                    self.shift_popup_month(-1);
                    true
                }
                MouseEventKind::ScrollDown => {
                    self.shift_popup_month(1);
                    true
                }
                _ => true,
            },
            Overlay::Select => match mouse.kind {
                MouseEventKind::Down(_) => {
                    let mut picked = false;
                    if let Some(widget) = self.focused_widget_mut() {
                        if let WidgetState::Select(state) = &mut widget.state {
                            if let Some(row) = state
                                .item_areas
                                .iter()
                                .position(|item| item.contains(position))
                            {
                                let _ = state.select(state.offset() + row);
                                picked = true;
                            }
                        }
                    }
                    if picked {
                        self.close_popup();
                        self.commit_focused();
                    }
                    true
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    // The wheel scrolls the list window without moving the
                    // selection, like the browser's open dropdown.
                    if let Some(widget) = self.focused_widget_mut() {
                        if let WidgetState::Select(state) = &mut widget.state {
                            let offset = state.offset();
                            let target = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                                offset.saturating_sub(1)
                            } else {
                                offset.saturating_add(1)
                            };
                            let _ = state.set_offset(target);
                        }
                    }
                    true
                }
                _ => true,
            },
        }
    }

    /// Routes a key press while the option list is open. Browsing (arrows,
    /// paging, type-ahead) mutates the widget value silently; Enter commits,
    /// Esc reverts to the bound value, Tab closes and falls through so the
    /// global handling commits the browsed value and advances focus.
    fn handle_select_popup_event(&mut self, event: &Event, code: KeyCode) -> bool {
        match code {
            KeyCode::Esc => {
                self.revert_select();
                self.close_popup();
                true
            }
            KeyCode::Tab => {
                self.close_popup();
                false
            }
            KeyCode::Enter => {
                self.close_popup();
                self.commit_focused();
                true
            }
            _ => {
                if let Some(widget) = self.focused_widget_mut() {
                    if let WidgetState::Select(state) = &mut widget.state {
                        let _ = rat_widget::choice::handle_events(state, true, event);
                    }
                }
                true
            }
        }
    }

    /// Restores the focused select's widget value from its bound property —
    /// rat's browsing mutates the value continuously, so Esc reverts like
    /// the browser's dropdown.
    fn revert_select(&mut self) {
        let Some(widget) = self.focused_widget_mut() else {
            return;
        };
        let bound = widget.last_synced_text();
        if let WidgetState::Select(state) = &mut widget.state {
            state.set_value(bound);
        }
    }

    /// Pages the open calendar by whole months, keeping the selected
    /// day-of-month (clamped to the target month's length).
    fn shift_popup_month(&mut self, months: i32) {
        let Some(widget) = self.focused_widget_mut() else {
            return;
        };
        let WidgetState::Date { popup, .. } = &mut widget.state else {
            return;
        };
        let base = popup
            .month
            .selected_date()
            .unwrap_or_else(|| popup.month.start_date());
        let target = if months < 0 {
            base.checked_sub_months(Months::new(months.unsigned_abs()))
        } else {
            base.checked_add_months(Months::new(months as u32))
        };
        if let Some(target) = target {
            popup.month.set_start_date(target);
            popup.month.select_date(target);
        }
    }

    /// The focused widget commits: its text routes into the `@change`
    /// binding the template declares, and a `change` event bubbles through
    /// the document — both halves of the browser's change-on-commit.
    fn commit_focused(&mut self) {
        let Some(node) = self.focused else {
            return;
        };
        let Some(text) = self
            .doc
            .element(node)
            .and_then(|el| el.data.widget.as_ref())
            .map(|widget| widget.state.committed_text())
        else {
            return;
        };
        let Some(index) = self.root_index_of(node) else {
            return;
        };
        let events = self.mounts[index].dispatch_widget_change(&mut self.doc, node, &text);
        let mut change = DomEvent::change().with_detail(Value::Str(text));
        self.doc.dispatch_event(node, &mut change);
        self.publish(index, events);
        self.ensure_focus();
    }

    fn root_index_of(&self, node: NodeId) -> Option<usize> {
        self.mounts.iter().position(|mount| {
            self.doc
                .ancestors(node)
                .any(|ancestor| ancestor == mount.host)
        })
    }

    fn publish(&mut self, index: usize, events: Vec<NotifyEvent>) {
        for event in &events {
            for (root, name, listener) in self.listeners.iter_mut() {
                if *root == index && name == &event.event_name {
                    listener(event);
                }
            }
        }
    }
}
