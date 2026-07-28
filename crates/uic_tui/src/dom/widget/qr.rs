//! The terminal twin of `<qr-code>` (ADR 0029): a native QR widget mounted
//! through the `data-tui="qr"` registry, plus the half-block renderer hosts
//! reuse for standalone panes (the lit-demo's live-mode join pane). The
//! browser half of the same element draws an SVG instead.

use crossterm::event::Event;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use uic_core::Value;

use super::{WidgetAdapter, WidgetRegistration};

/// Anchors this feature's object code so the `inventory` registration
/// survives the linker in consuming binaries — the `ui_components::link()`
/// discipline: without a genuine symbol reference into the object, lazy
/// archive extraction drops the registration constructor and `data-tui="qr"`
/// degrades to a generic container.
#[inline(never)]
pub fn link() {}

/// The QR's own card, explicit rather than the theme's colors: black
/// modules on a white ground scan on any terminal.
pub fn qr_card_style() -> Style {
    Style::new()
        .fg(Color::Rgb(0, 0, 0))
        .bg(Color::Rgb(255, 255, 255))
}

uic_core::inventory::submit! {
    WidgetRegistration {
        kind: "qr",
        build: QrAdapter::build,
    }
}

/// Renders `data` as a QR code in half-block characters (Dense1x2),
/// standard polarity: dark modules are the block glyphs, light modules the
/// background, quiet zone included. Callers paint it black on white — a
/// camera wants dark modules on a light ground whatever the terminal theme,
/// the same reason the browser's SVG sits on its own white card. `None` for
/// empty data or a payload the encoder rejects (too long for any version).
pub fn render_qr(data: &str) -> Option<(String, u16, u16)> {
    use qrcode::render::unicode::Dense1x2;
    if data.is_empty() {
        return None;
    }
    let code = qrcode::QrCode::new(data).ok()?;
    let text = code.render::<Dense1x2>().build();
    let width = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let height = text.lines().count() as u16;
    Some((text, width, height))
}

/// The mounted QR widget: non-interactive, it paints the cached half-block
/// render at its natural size and reports that size to the layout.
struct QrAdapter {
    /// The rendered QR and its cell dimensions, rebuilt when the value moves.
    render: Option<(String, u16, u16)>,
    /// The screen cells the last paint covered, for pointer hit-testing.
    area: Rect,
}

impl QrAdapter {
    fn build() -> Box<dyn WidgetAdapter> {
        Box::new(QrAdapter {
            render: None,
            area: Rect::default(),
        })
    }
}

impl WidgetAdapter for QrAdapter {
    fn set_focus(&mut self, _focused: bool) {}

    fn area(&self) -> Rect {
        self.area
    }

    /// The QR carries no editable value; an empty string keeps the scripted
    /// host's echo-skip from ever short-circuiting a genuine data change.
    fn committed_text(&self) -> String {
        String::new()
    }

    fn sync(&mut self, value: &Value) {
        self.render = match value {
            Value::Str(data) => render_qr(data),
            _ => None,
        };
    }

    fn handle(&mut self, _focused: bool, _event: &Event) -> bool {
        false
    }

    fn intrinsic_width(&self) -> Option<u16> {
        self.render.as_ref().map(|(_, width, _)| *width)
    }

    fn intrinsic_height(&self, _max_lines: u16) -> u16 {
        // The QR needs its full height; it does not clamp to max-lines.
        self.render
            .as_ref()
            .map(|(_, _, height)| *height)
            .unwrap_or(0)
    }

    fn place_cursor(&mut self, _column: u16, _row: u16, _extend: bool) {}

    fn paint(&mut self, frame: &mut Frame, rect: Rect, _dim: Option<Style>) {
        self.area = rect;
        if let Some((text, _, _)) = &self.render {
            frame.render_widget(Paragraph::new(text.clone()).style(qr_card_style()), rect);
        }
    }

    fn paints_value(&self) -> bool {
        true
    }

    fn screen_cursor(&self) -> Option<(u16, u16)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_scales_with_the_payload() {
        // A short payload fits a low QR version; a long one needs a bigger
        // grid — the module count (and so the cell width) grows with it.
        let (_, short_width, short_height) = render_qr("hi").expect("a short QR");
        let long = "A".repeat(300);
        let (_, long_width, long_height) = render_qr(&long).expect("a long QR");
        assert!(short_width >= 21, "a QR is at least version 1 (21 modules)");
        assert!(long_width > short_width, "more data widens the grid");
        assert!(long_height > short_height, "more data heightens the grid");
        // Dense1x2 packs two vertical modules per row, so height trails width.
        assert!(short_height <= short_width);
    }

    #[test]
    fn empty_data_renders_nothing() {
        assert!(render_qr("").is_none());
    }

    #[test]
    fn the_adapter_sizes_to_its_synced_value() {
        let mut adapter = QrAdapter {
            render: None,
            area: Rect::default(),
        };
        assert_eq!(adapter.intrinsic_width(), None);
        adapter.sync(&Value::Str("somePairingCode".into()));
        let width = adapter.intrinsic_width().expect("a sized QR after sync");
        assert!(width >= 21);
        assert_eq!(adapter.intrinsic_height(10), adapter.intrinsic_height(1));
        // A non-string value clears the render.
        adapter.sync(&Value::Null);
        assert_eq!(adapter.intrinsic_width(), None);
    }
}
