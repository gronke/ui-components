//! A component's `static styles` reach the terminal: json-viewer's own
//! stylesheet — custom properties, `var()`, `calc()`, scoped element rules —
//! drives colors and indentation instead of hardcoded entries.

use std::path::Path;

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::style::Color;
use uic_tui::ratatui::Terminal;

fn data() -> String {
    serde_json::json!({
        "active": true,
        "name": "Schuhkarton",
        "tags": { "first": "a", "second": "b" },
    })
    .to_string()
}

fn paint(host: &JsHost, terminal: &mut Terminal<TestBackend>) {
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let mut s = state.borrow_mut();
            let focused = s.focused;
            uic_tui::dom::paint_document(frame, frame.area(), &mut s.doc, focused);
        })
        .unwrap();
}

/// The buffer position of a substring's first cell, as (x, y).
fn locate(terminal: &Terminal<TestBackend>, needle: &str) -> (u16, u16) {
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        let row: String = (area.left()..area.right())
            .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
            .collect();
        if let Some(pos) = row.find(needle) {
            return (pos as u16, y);
        }
    }
    panic!("{needle:?} not on screen");
}

fn foreground(terminal: &Terminal<TestBackend>, x: u16, y: u16) -> Option<Color> {
    terminal.backend().buffer()[(x, y)].style().fg
}

#[test]
fn the_component_stylesheet_styles_the_terminal() {
    let mut host = JsHost::new().unwrap();
    host.load_dist_dir(Path::new(env!("UIC_JS_VENDOR_DIST")), "json-viewer.js")
        .unwrap();
    let root = host.mount("json-viewer", &[("data", &data())]).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 16)).unwrap();
    paint(&host, &mut terminal);

    // The component's `ul { padding: 0 }` beats the ua indent inside its
    // scope: top-level keys sit at the left edge.
    let (tags_x, tags_y) = locate(&terminal, "tags:");
    assert_eq!(tags_x, 0, "component ul reset applies within its scope");

    // Its palette arrives through custom properties and var(): the key
    // color is --property-color (#6fb3d2), the string --string-color
    // (#a3eea0) — exact 24-bit values, not the retired ANSI entries.
    assert_eq!(
        foreground(&terminal, tags_x, tags_y),
        Some(Color::Rgb(0x6f, 0xb3, 0xd2)),
        "key color from the component palette"
    );
    let (value_x, value_y) = locate(&terminal, "\"Schuhkarton\"");
    assert_eq!(
        foreground(&terminal, value_x, value_y),
        Some(Color::Rgb(0xa3, 0xee, 0xa0)),
        "string color from the component palette"
    );

    // Expanding `tags` (the component's own keyboard handling) indents the
    // members by its calc(var(--indent-size) + var(--line-height)) margin —
    // 1.7rem, two cells.
    host.focus(root).unwrap();
    host.dispatch_key("ArrowDown").unwrap();
    host.dispatch_key("ArrowDown").unwrap();
    host.dispatch_key("ArrowRight").unwrap();
    paint(&host, &mut terminal);
    let (first_x, _) = locate(&terminal, "first:");
    assert_eq!(first_x, 2, "nested rows indent by the component's calc()");
}
