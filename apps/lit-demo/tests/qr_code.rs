//! The shared `<qr-code>` under the scripted (terminal) host (ADR 0030): the
//! same element the browser draws as an SVG renders, on the terminal, a
//! `data-tui="qr"` marker carrying its payload on the `value` attribute — the
//! exact input the native QR widget mounts from (see `src/qr_widget.rs` for
//! the adapter). Loaded from the real baked package, so it exercises the
//! component the app ships, not a trimmed copy.

use std::path::Path;

use uic_js::JsHost;

const PACKAGE: &str = "@schuhkarton/lit-todo";

fn npm_root() -> &'static Path {
    Path::new(env!("UIC_LIT_DEMO_NPM_ROOT"))
}

fn module(host: &mut JsHost, file: &str) {
    let src = std::fs::read_to_string(npm_root().join(PACKAGE).join(file))
        .unwrap_or_else(|err| panic!("read {file}: {err}"));
    host.load_module(&format!("{PACKAGE}/{file}"), &src)
        .unwrap_or_else(|err| panic!("load {file}: {err}"));
}

#[test]
fn qr_code_renders_the_terminal_widget_marker() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, "qr-code.js");

    // The data attribute (composition flows as attributes, not `.prop=`)
    // reaches the element and drives its render.
    let node = host.mount("qr-code", &[("data", "uics1.PAYLOAD")]).unwrap();
    let html = host.state.borrow().doc.inner_html(node);

    assert!(
        html.contains("data-tui=\"qr\""),
        "the native QR widget marker renders: {html}"
    );
    assert!(
        html.contains("uics1.PAYLOAD"),
        "the payload rides the value channel the widget reads: {html}"
    );
}

#[test]
fn the_deck_composes_the_panes_and_the_qr() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, "qr-code.js");
    module(&mut host, "pair-panel.js");
    module(&mut host, "p2p-deck.js");
    let deck = host.mount("p2p-deck", &[]).unwrap();

    // The deck composes the stack and the QR beside it — the structure the
    // terminal's responsive flex layout works on.
    let html = host.state.borrow().doc.inner_html(deck);
    assert!(html.contains("<todo-app"), "the todo card mounts: {html}");
    assert!(html.contains("<pair-panel"), "the panel mounts: {html}");
    assert!(html.contains("<qr-code"), "the deck QR mounts: {html}");

    // The terminal loop drives the deck's QR by node, exactly like main.rs.
    let qr = {
        let state = host.state.borrow();
        let root = state.doc.root();
        let found = state.doc.descendants(root).find(|&node| {
            state
                .doc
                .element(node)
                .is_some_and(|el| el.tag().as_ref() == "qr-code")
        });
        found.expect("the deck's qr-code")
    };
    host.set_prop(qr, "data", "\"uics1.DECK\"").unwrap();
    let html = host.state.borrow().doc.inner_html(qr);
    assert!(
        html.contains("data-tui=\"qr\"") && html.contains("uics1.DECK"),
        "the driven QR renders its widget marker with the data: {html}"
    );
}

#[test]
fn the_panel_invite_embeds_the_qr_with_the_link() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    // pair-panel and the <qr-code> it embeds are off the todo-app entry graph
    // (as in main.rs); load qr-code first so the panel's import resolves.
    module(&mut host, "qr-code.js");
    module(&mut host, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    let link = "https://example/lit-demo/p2p/#uics1.PAYLOAD";
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "link", &format!("{link:?}")).unwrap();

    let html = host.state.borrow().doc.inner_html(panel);
    // The invite body composes the shared component, fed the link as an
    // attribute — the cross-host-safe binding.
    assert!(
        html.contains(&format!("data=\"{link}\"")),
        "the panel embeds <qr-code> carrying the invite link: {html}"
    );
}
