//! The exploration target (#65): the byte-unmodified `@alenaksu/json-viewer`
//! npm dist renders a JSON tree through the mocked lit and the terminal
//! pipeline.

use std::path::Path;

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;

const DATA: &str =
    r#"{"name":"Schuhkarton","active":true,"total":12.5,"tags":{"first":"a","second":"b"}}"#;

fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn json_viewer_renders_a_collapsed_tree() {
    let mut host = JsHost::new().unwrap();
    host.load_dist_dir(Path::new(env!("UIC_JS_VENDOR_DIST")), "json-viewer.js")
        .unwrap();
    let _node = host.mount("json-viewer", &[("data", DATA)]).unwrap();

    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    let state = host.state.clone();
    terminal
        .draw(|frame| {
            let doc = &mut state.borrow_mut().doc;
            uic_tui::dom::paint_document(frame, frame.area(), doc, None);
        })
        .unwrap();

    let screen = screen_text(&terminal);
    println!("{screen}");
    // Top-level keys paint; the nested object shows its collapsed preview.
    assert!(screen.contains("name:"), "keys should render:\n{screen}");
    assert!(
        screen.contains("Schuhkarton"),
        "primitive values should render:\n{screen}"
    );
    assert!(
        screen.contains("tags:"),
        "nested key should render:\n{screen}"
    );
}
