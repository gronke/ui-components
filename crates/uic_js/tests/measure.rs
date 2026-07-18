//! Magnitude measurements for the exploration record (#65) — run manually:
//!
//! ```sh
//! cargo test -p uic_js --release --test measure -- --ignored --nocapture
//! ```

use std::path::Path;
use std::time::Instant;

use uic_js::JsHost;
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::Terminal;

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

#[test]
#[ignore = "measurement, run with --ignored --nocapture"]
fn magnitudes() {
    let t = Instant::now();
    let mut host = JsHost::new().unwrap();
    println!(
        "engine boot (context + bootstrap):        {:?}",
        t.elapsed()
    );

    let t = Instant::now();
    host.load_dist_dir(Path::new(env!("UIC_JS_VENDOR_DIST")), "json-viewer.js")
        .unwrap();
    println!(
        "json-viewer modules load+link+evaluate:   {:?}",
        t.elapsed()
    );

    let small = r#"{"name":"Schuhkarton","tags":{"first":"a","second":"b"},"active":true}"#;
    let t = Instant::now();
    let _node = host.mount("json-viewer", &[("data", small)]).unwrap();
    println!(
        "mount + first render (small document):    {:?}",
        t.elapsed()
    );

    let mut terminal = Terminal::new(TestBackend::new(80, 40)).unwrap();
    let t = Instant::now();
    paint(&host, &mut terminal);
    println!(
        "layout + paint 80×40:                     {:?}",
        t.elapsed()
    );

    // A wide document: 500 keys at the top level.
    let wide: String = {
        let members: Vec<String> = (0..500)
            .map(|i| format!("\"key{i:03}\": {{\"value\": {i}, \"label\": \"row {i}\"}}"))
            .collect();
        format!("{{{}}}", members.join(","))
    };
    let mut host = JsHost::new().unwrap();
    host.load_dist_dir(Path::new(env!("UIC_JS_VENDOR_DIST")), "json-viewer.js")
        .unwrap();
    let node = host.mount("json-viewer", &[("data", &wide)]).unwrap();
    let t = Instant::now();
    host.focus(node).unwrap();
    println!(
        "focus entry over 500 rows:                {:?}",
        t.elapsed()
    );

    let t = Instant::now();
    host.dispatch_key("ArrowDown").unwrap();
    println!(
        "one ArrowDown (dispatch + re-render):     {:?}",
        t.elapsed()
    );

    let t = Instant::now();
    host.dispatch_key("ArrowRight").unwrap();
    println!(
        "one toggle over 500 rows (subtree swap):  {:?}",
        t.elapsed()
    );

    let t = Instant::now();
    paint(&host, &mut terminal);
    println!(
        "layout + paint 500 rows at 80×40:         {:?}",
        t.elapsed()
    );
}
