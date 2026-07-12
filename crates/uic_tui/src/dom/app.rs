//! The application host: mounted components live as element nodes in one
//! document, layout and paint read the tree, and keys and the pointer travel
//! it — focus is a node, not an index into template order, and unrendered
//! conditional branches are unfocusable because their nodes do not exist.

use crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::backend::Backend;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Terminal;
use uic_core::{NotifyEvent, Value};
use uic_dom::{Event as DomEvent, NodeId};

use super::host::{deliver, Listener, Mount};
use super::render;
use super::widget::{OverlayOutcome, WidgetBox};
use super::DomDocument;
use crate::{Control, Error};

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
    listeners: Vec<((usize, String), Listener)>,
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
    ///
    /// Listeners run synchronously inside the update, under the `&mut self`
    /// borrow — a callback must not call back into the app (no draw, no
    /// set_prop). The wasm session's borrow guard enforces this in the
    /// browser; native embedders queue follow-up work instead (the
    /// BroadcastChannel pattern: deliver, return, apply asynchronously).
    pub fn on(&mut self, index: usize, event: &str, listener: impl FnMut(&NotifyEvent) + 'static) {
        self.listeners
            .push(((index, event.to_string()), Box::new(listener)));
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
        let control = self.route_event(event);
        // Live text the widget's handling produced routes into the
        // template's `@input` binding — with the popup open or closed.
        self.flush_widget_input();
        control
    }

    fn route_event(&mut self, event: &Event) -> Control {
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
                .is_some_and(|widget| widget.adapter.area().contains(position))
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
            .is_some_and(|widget| widget.adapter.is_multiline())
    }

    /// True when the focused widget owns an overlay F4/Down may open (the
    /// date's calendar, the select's option list). Disabled widgets never
    /// hold focus, so no separate guard is needed.
    fn focused_opens_overlay(&self) -> bool {
        self.focused_widget()
            .is_some_and(|widget| widget.adapter.opens_overlay())
    }

    /// Forwards the event to the focused widget's own handling; true when
    /// the widget requests a commit.
    fn forward_to_focused(&mut self, event: &Event) -> bool {
        self.focused_widget_mut()
            .is_some_and(|widget| widget.adapter.handle(true, event))
    }

    /// Places the caret under the pointer, extends the selection on drag,
    /// or opens a select's list — the click semantics of the browser,
    /// dispatched to the focused widget's adapter.
    fn place_cursor(&mut self, column: u16, row: u16, extend: bool) {
        if let Some(widget) = self.focused_widget_mut() {
            widget.adapter.place_cursor(column, row, extend);
        }
    }

    /// True when the focused widget's overlay (calendar or option list) is
    /// open.
    fn popup_open(&self) -> bool {
        self.focused_widget()
            .is_some_and(|widget| widget.adapter.overlay_open())
    }

    fn open_popup(&mut self) {
        if let Some(widget) = self.focused_widget_mut() {
            widget.adapter.open_overlay();
        }
    }

    fn close_popup(&mut self) {
        if let Some(widget) = self.focused_widget_mut() {
            widget.adapter.close_overlay();
        }
    }

    /// Routes a key press while an overlay is open (overlays are modal).
    /// Returns whether the event was consumed; the adapter's outcome
    /// decides — a pick commits, Tab reports unconsumed so the global
    /// commit-and-focus handling still runs.
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
        let Some(widget) = self.focused_widget_mut() else {
            return true;
        };
        match widget.adapter.overlay_key(event) {
            OverlayOutcome::Consumed => true,
            OverlayOutcome::Pass => false,
            OverlayOutcome::Commit => {
                self.commit_focused();
                true
            }
        }
    }

    /// Routes the pointer while an overlay is open: picks commit like
    /// Enter, and a press outside dismisses the overlay and reports
    /// unconsumed so the click still focuses whatever it landed on.
    fn handle_popup_mouse(&mut self, mouse: MouseEvent) -> bool {
        let Some(widget) = self.focused_widget_mut() else {
            return false;
        };
        match widget.adapter.overlay_mouse(mouse) {
            OverlayOutcome::Consumed => true,
            OverlayOutcome::Pass => false,
            OverlayOutcome::Commit => {
                self.commit_focused();
                true
            }
        }
    }

    /// Routes the focused widget's pending live text into the `@input`
    /// binding its template declares, and bubbles an `input` event through
    /// the document — the browser's per-keystroke event beside
    /// `commit_focused`'s change.
    fn flush_widget_input(&mut self) {
        let Some(node) = self.focused else {
            return;
        };
        let Some(text) = self
            .doc
            .element_mut(node)
            .and_then(|el| el.data.widget.as_mut())
            .and_then(|widget| widget.adapter.take_input())
        else {
            return;
        };
        let Some(index) = self.root_index_of(node) else {
            return;
        };
        let events = self.mounts[index].dispatch_widget_input(&mut self.doc, node, &text);
        let mut input = DomEvent::input().with_detail(Value::Str(text));
        self.doc.dispatch_event(node, &mut input);
        self.publish(index, events);
        self.ensure_focus();
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
            .map(|widget| widget.adapter.committed_text())
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
        deliver(&mut self.listeners, &events, |(root, name), event| {
            *root == index && name == &event.event_name
        });
    }
}
