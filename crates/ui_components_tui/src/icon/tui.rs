//! The terminal twin of `<uic-icon>` (ADR 0002): rasterizes the named Material
//! SVG to Braille cells via `uic_icons::raster`, painted in the theme
//! foreground. Registered for `data-tui="icon"`.
//!
//! Icons are only legible at a few cells and up — a one-cell icon is a blob, by
//! nature of the medium; hosts give a `<uic-icon>` room (font-size / CSS) when
//! the glyph needs to read.

use uic_core::Value;
use uic_tui::crossterm::event::Event;
use uic_tui::ratatui::layout::Rect;
use uic_tui::ratatui::style::Style;
use uic_tui::ratatui::text::{Line, Span};
use uic_tui::ratatui::widgets::Clear;
use uic_tui::ratatui::Frame;
use uic_tui::{WidgetAdapter, WidgetRegistration};

uic_core::inventory::submit! {
    WidgetRegistration {
        kind: "icon",
        build: IconAdapter::build,
    }
}

#[derive(Default)]
struct IconAdapter {
    /// The icon name pushed in via the `.value` binding.
    name: String,
    /// The cells of the last paint, for the host's hit-testing.
    area: Rect,
}

impl IconAdapter {
    fn build() -> Box<dyn WidgetAdapter> {
        Box::<IconAdapter>::default()
    }
}

impl WidgetAdapter for IconAdapter {
    fn set_focus(&mut self, _focused: bool) {}

    fn area(&self) -> Rect {
        self.area
    }

    /// Display-only: nothing to commit.
    fn committed_text(&self) -> String {
        String::new()
    }

    fn sync(&mut self, value: &Value) {
        self.name = match value {
            Value::Str(name) => name.clone(),
            _ => String::new(),
        };
    }

    fn handle(&mut self, _focused: bool, _event: &Event) -> bool {
        false
    }

    // A legible default box (8×4 cells ≈ 16×16 Braille subpixels, square) when
    // the layout does not size the icon; hosts enlarge it with CSS. Below a few
    // cells any rasterized icon is a blob — that is the medium.
    fn intrinsic_width(&self) -> Option<u16> {
        Some(8)
    }

    fn intrinsic_height(&self, _max_lines: u16) -> u16 {
        4
    }

    fn place_cursor(&mut self, _column: u16, _row: u16, _extend: bool) {}

    fn paints_value(&self) -> bool {
        true
    }

    fn paint(&mut self, frame: &mut Frame, rect: Rect, dim: Option<Style>) {
        self.area = rect;
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        frame.render_widget(Clear, rect);
        // Keep the icon square: a Braille cell is 2×4 subpixels, so the box's
        // subpixel extent is width*2 × height*4; render the largest square that
        // fits, centered.
        let sub = (rect.width as u32 * 2).min(rect.height as u32 * 4).max(4);
        let cols = ((sub / 2) as u16).max(1);
        let rows = ((sub / 4) as u16).max(1);
        let ox = rect.x + (rect.width - cols) / 2;
        let oy = rect.y + (rect.height - rows) / 2;
        let style = dim.unwrap_or_default();
        for (i, line) in uic_icons::raster::braille(&self.name, cols, rows)
            .into_iter()
            .enumerate()
        {
            let row = Rect {
                x: ox,
                y: oy + i as u16,
                width: cols,
                height: 1,
            };
            frame.render_widget(Line::from(Span::styled(line, style)), row);
        }
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_stores_the_name_and_a_null_clears_it() {
        let mut adapter = IconAdapter::default();
        adapter.sync(&Value::Str("visibility".into()));
        assert_eq!(adapter.name, "visibility");
        adapter.sync(&Value::Null);
        assert_eq!(adapter.name, "");
    }
}
