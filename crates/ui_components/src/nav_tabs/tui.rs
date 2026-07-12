//! The terminal twin of `<nav-tabs>` (ADR 0002/0015): rat's `Tabbed` in its
//! glued form — one row of captions, no content block — while the browser
//! half in `nav_tabs.impl.ts` builds Bootstrap button rows. Registered for
//! `data-tui="tab-bar"` through the widget registry, so the runtime needs
//! no edit.
//!
//! The bound value is the single source of truth: the highlighted index
//! derives from it at paint (falling back to the first tab), and a pick
//! leaves through `take_input` into the `@input` route — the same binding
//! the browser buttons dispatch through.

use uic_core::{SelectOption, Value};
use uic_tui::crossterm::event::{Event, KeyCode, KeyEventKind};
use uic_tui::rat_widget::tabbed::{TabPlacement, TabType, Tabbed, TabbedState};
use uic_tui::ratatui::layout::{Position, Rect};
use uic_tui::ratatui::style::{Color, Style};
use uic_tui::ratatui::Frame;
use uic_tui::{WidgetAdapter, WidgetRegistration};

uic_core::inventory::submit! {
    WidgetRegistration {
        kind: "tab-bar",
        build: TabBarAdapter::build,
    }
}

struct TabBarAdapter {
    state: TabbedState,
    /// The option rows delivered as `.options` property writes (ADR 0006).
    options: Vec<SelectOption>,
    /// The bound value; the highlighted index derives from it at paint.
    bound: String,
    /// The pick, drained by `take_input` into the `@input` route.
    pending: Option<String>,
}

impl TabBarAdapter {
    fn build() -> Box<dyn WidgetAdapter> {
        Box::new(TabBarAdapter {
            state: TabbedState::new(),
            options: Vec::new(),
            bound: String::new(),
            pending: None,
        })
    }

    /// The highlighted index: the bound value's row, or the first tab when
    /// nothing matches — mirroring the browser rows' fallback.
    fn selected_index(&self) -> Option<usize> {
        if self.options.is_empty() {
            return None;
        }
        Some(
            self.options
                .iter()
                .position(|option| option.value == self.bound)
                .unwrap_or(0),
        )
    }

    /// Adopts a row: an actual change records the pick for `take_input`.
    fn choose(&mut self, index: usize) {
        let Some(option) = self.options.get(index) else {
            return;
        };
        if option.value != self.bound {
            self.bound = option.value.clone();
            self.pending = Some(self.bound.clone());
        }
    }

    fn step(&mut self, delta: isize) {
        let Some(selected) = self.selected_index() else {
            return;
        };
        let last = self.options.len() - 1;
        self.choose(selected.saturating_add_signed(delta).min(last));
    }
}

impl WidgetAdapter for TabBarAdapter {
    fn set_focus(&mut self, focused: bool) {
        self.state.focus.set(focused);
    }

    fn area(&self) -> Rect {
        self.state.area
    }

    fn committed_text(&self) -> String {
        self.bound.clone()
    }

    fn sync(&mut self, value: &Value) {
        match value {
            Value::Str(text) => self.bound = text.clone(),
            _ => self.bound.clear(),
        }
    }

    fn set_options(&mut self, options: Vec<SelectOption>) {
        self.options = options;
    }

    /// Arrows switch the tab, rat's own four-key binding — Down is free
    /// because the bar opens no overlay. The pick dispatches in the same
    /// `handle_event` call through the `@input` flush; no commit request.
    fn handle(&mut self, focused: bool, event: &Event) -> bool {
        let Event::Key(key) = event else {
            return false;
        };
        if !focused || key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Left | KeyCode::Up => self.step(-1),
            KeyCode::Right | KeyCode::Down => self.step(1),
            _ => {}
        }
        false
    }

    fn take_input(&mut self) -> Option<String> {
        self.pending.take()
    }

    /// The click path: the runtime places the "caret" right after a press
    /// focuses the bar — here that adopts the caption under the pointer
    /// (rat's own mouse handling stays unused, it reads the system clock).
    fn place_cursor(&mut self, column: u16, row: u16, extend: bool) {
        if extend {
            return;
        }
        let position = Position::new(column, row);
        if let Some(index) = self
            .state
            .tab_title_areas
            .iter()
            .position(|tab| tab.contains(position))
        {
            self.choose(index);
        }
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        self.state.select(self.selected_index());
        let mut tabbed = Tabbed::new()
            .tab_type(TabType::Glued)
            .placement(TabPlacement::Top)
            .tabs(
                self.options
                    .iter()
                    .map(|option| option.short_label().to_string()),
            )
            .select_style(Style::new().reversed())
            .focus_style(Style::new().fg(Color::Black).bg(Color::LightBlue));
        if let Some(style) = dim {
            tabbed = tabbed.style(style);
        }
        frame.render_stateful_widget(tabbed, rect, &mut self.state);
    }

    fn paints_value(&self) -> bool {
        true
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        None
    }
}
