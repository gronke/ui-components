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

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
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

/// Hosts one mounted component tree on a terminal.
pub struct App<B: Backend> {
    terminal: Terminal<B>,
    root: Option<ElementInstance>,
    status: Option<Box<dyn Fn() -> String>>,
}

impl App<ratatui::backend::CrosstermBackend<std::io::Stdout>> {
    /// Takes over the terminal (alternate screen, raw mode).
    pub fn new() -> Result<Self, Error> {
        Ok(Self::from_terminal(ratatui::try_init()?))
    }

    /// Runs the event loop until Esc/Ctrl-C, then restores the terminal.
    /// Tab commits and cycles focus, Enter commits.
    pub fn run(mut self) -> Result<(), Error> {
        let result = self.event_loop();
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
            root: None,
            status: None,
        }
    }

    /// Mounts a registered custom element as the root — the
    /// `document.createElement` + append moment, firing `connected`.
    pub fn mount(&mut self, tag: &str) -> Result<&mut ElementInstance, Error> {
        self.root = Some(ElementInstance::mount(tag)?);
        Ok(self.root.as_mut().expect("just mounted"))
    }

    pub fn root_mut(&mut self) -> Option<&mut ElementInstance> {
        self.root.as_mut()
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
            root,
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
                if let Some(root) = root {
                    render::render_instance(frame, area, root);
                }
            })
            .map_err(|err| Error::Terminal(err.to_string()))?;
        Ok(())
    }

    /// Routes one terminal event: quit keys, focus/commit keys, everything
    /// else to the focused widget.
    pub fn handle_event(&mut self, event: &Event) -> Control {
        let Some(root) = &mut self.root else {
            return Control::Continue;
        };
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Esc => return Control::Quit,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Control::Quit;
                    }
                    KeyCode::Tab => {
                        root.commit_focused();
                        root.focus_next();
                        return Control::Continue;
                    }
                    KeyCode::Enter => {
                        root.commit_focused();
                        return Control::Continue;
                    }
                    _ => {}
                }
            }
        }
        let focused = root.focused;
        let enabled = root
            .slots
            .get(focused)
            .is_some_and(|slot| !slot.is_disabled(&root.store, root.behavior.as_ref()));
        if enabled {
            if let Some(slot) = root.slots.get_mut(focused) {
                slot.state.handle(true, event);
            }
        }
        Control::Continue
    }
}
