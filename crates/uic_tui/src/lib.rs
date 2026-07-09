//! Terminal runtime: interprets the template IR with ratatui, computes layout
//! with taffy over terminal cells, and hosts rat-widget input primitives.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! ui_components::link();
//! let mut app = uic_tui::App::new()?;
//! let el = app.mount("input-date")?;
//! el.set_attr("label", "Date of purchase");
//! el.on("value-changed", |ev| eprintln!("{:?}", ev.value));
//! app.run()?;
//! # Ok(())
//! # }
//! ```

mod expand;
mod instance;
mod layout;
mod render;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Terminal;

pub use instance::ElementInstance;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown custom element <{0}>")]
    UnknownTag(String),
    #[error("unsupported data-tui widget '{0}'")]
    UnknownWidget(String),
    #[error("date pattern: {0}")]
    Pattern(String),
    #[error("terminal: {0}")]
    Terminal(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Outcome of [`App::handle_event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Continue,
    Quit,
}

/// Hosts mounted component trees on a terminal. Roots stack vertically like
/// block elements in a document; Tab cycles focus across root boundaries.
pub struct App<B: Backend> {
    terminal: Terminal<B>,
    roots: Vec<ElementInstance>,
    active: usize,
    /// Focus parked outside every element (a click into nothing): no ring,
    /// no caret, until the next key or widget click.
    blurred: bool,
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
            roots: Vec::new(),
            active: 0,
            blurred: false,
            status: None,
        }
    }

    /// Mounts a registered custom element as a root — the
    /// `document.createElement` + append moment, firing `connected`.
    /// Every mount appends a root; focus starts on the first one.
    pub fn mount(&mut self, tag: &str) -> Result<&mut ElementInstance, Error> {
        self.roots.push(ElementInstance::mount(tag)?);
        Ok(self.roots.last_mut().expect("just mounted"))
    }

    /// The root focus currently lives in.
    pub fn root_mut(&mut self) -> Option<&mut ElementInstance> {
        self.roots.get_mut(self.active)
    }

    /// Number of mounted roots.
    pub fn root_len(&self) -> usize {
        self.roots.len()
    }

    /// A mounted root by index, in mount order.
    pub fn root_at_mut(&mut self, index: usize) -> Option<&mut ElementInstance> {
        self.roots.get_mut(index)
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
            roots,
            active,
            blurred,
            status,
        } = self;
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
                // Roots stack with one blank row between them; the active
                // root's popup paints after all content so the overlay wins
                // over the roots below its anchor.
                let mut y = area.y;
                for (index, root) in roots.iter_mut().enumerate() {
                    if y >= area.bottom() {
                        break;
                    }
                    let band = Rect {
                        y,
                        height: area.bottom() - y,
                        ..area
                    };
                    let focus_here = index == *active && !*blurred;
                    let used = render::render_instance(frame, band, root, focus_here);
                    y = y.saturating_add(used).saturating_add(1);
                }
                if let Some(root) = roots.get_mut(*active) {
                    render::paint_popup(frame, area, root);
                }
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
        if let Some(root) = self.roots.get_mut(self.active) {
            if root.popup_open() {
                root.close_popup();
            }
            root.commit_focused();
        }
        self.blurred = true;
    }

    /// Routes one terminal event: an open calendar first (it is modal), then
    /// quit and focus/commit keys, everything else to the focused widget.
    /// A click focuses the widget under the pointer, committing the one it
    /// leaves; a click outside every element blurs.
    pub fn handle_event(&mut self, event: &Event) -> Control {
        if self.roots.is_empty() {
            return Control::Continue;
        }
        if let Event::Mouse(mouse) = event {
            return self.handle_mouse(event, mouse.kind, mouse.column, mouse.row);
        }
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                self.blurred = false;
                let root = &mut self.roots[self.active];
                // The calendar sees the key first: Esc closes it instead of
                // quitting; Tab closes it and falls through to the commit.
                if root.popup_open() && root.handle_popup_event(event) {
                    return Control::Continue;
                }
                match key.code {
                    KeyCode::Esc => return Control::Quit,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Control::Quit;
                    }
                    KeyCode::Tab => {
                        root.commit_focused();
                        // A wrap hands focus to the next root, like Tab
                        // crossing element boundaries in a document.
                        if root.focus_next() {
                            self.active = (self.active + 1) % self.roots.len();
                            self.roots[self.active].focus_first();
                        }
                        return Control::Continue;
                    }
                    KeyCode::Enter => {
                        // A textarea takes the newline; it commits on focus
                        // leave (Tab), like `@change` on blur in the browser.
                        if root.focused_multiline() {
                            root.handle_focused(event);
                        } else {
                            root.commit_focused();
                        }
                        return Control::Continue;
                    }
                    KeyCode::F(4) | KeyCode::Down
                        if root.focused_date_enabled() || root.focused_select_enabled() =>
                    {
                        root.open_popup();
                        return Control::Continue;
                    }
                    _ => {}
                }
            }
        }
        self.roots[self.active].handle_focused(event);
        Control::Continue
    }

    fn handle_mouse(
        &mut self,
        event: &Event,
        kind: MouseEventKind,
        column: u16,
        row: u16,
    ) -> Control {
        // The open overlay sees the pointer first (it is modal); a press
        // outside it closes the overlay and falls through, so the same
        // click still focuses whatever it landed on.
        if !self.blurred {
            let root = &mut self.roots[self.active];
            if root.popup_open() && root.handle_popup_event(event) {
                return Control::Continue;
            }
        }
        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let hit =
                    self.roots.iter().enumerate().find_map(|(index, root)| {
                        root.hit_test(column, row).map(|flat| (index, flat))
                    });
                match hit {
                    Some((index, flat)) => {
                        // Leaving a widget commits it, like the browser's
                        // change-on-blur; a blurred click already committed.
                        let leaving =
                            index != self.active || flat != self.roots[self.active].focused;
                        if leaving && !self.blurred {
                            self.roots[self.active].commit_focused();
                        }
                        self.blurred = false;
                        self.active = index;
                        self.roots[index].focused = flat;
                        // The same press places the caret under the pointer;
                        // a select opens its list.
                        self.roots[index].place_cursor(column, row, false);
                    }
                    None => self.blur(),
                }
                Control::Continue
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // A drag extends the focused widget's selection toward the
                // pointer.
                if !self.blurred {
                    self.roots[self.active].place_cursor(column, row, true);
                }
                Control::Continue
            }
            _ => Control::Continue,
        }
    }
}
