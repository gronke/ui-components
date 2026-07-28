//! The p2p composition under the scripted (terminal) host, loaded from the
//! real baked package so the shipped components are what runs — not trimmed
//! copies: the shared `<qr-code>` renders its `data-tui="qr"` widget marker
//! (ADR 0029), `<pair-panel>` signals intent through the polled `command`
//! property (ADR 0029), and `<p2p-deck>` composes the panes with the QR
//! beside them.

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
    let node = host.mount("qr-code", &[("data", "PAYLOAD64")]).unwrap();
    let html = host.state.borrow().doc.inner_html(node);

    assert!(
        html.contains("data-tui=\"qr\""),
        "the native QR widget marker renders: {html}"
    );
    assert!(
        html.contains("PAYLOAD64"),
        "the payload rides the value channel the widget reads: {html}"
    );
}

#[test]
fn the_panel_action_button_signals_through_the_command() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, "theme.js");
    module(&mut host, "qr-code.js");
    module(&mut host, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    // The generic secondary action (ADR 0029's seam, used by the takeover):
    // a host sets the label, the button renders, a click writes the command.
    host.set_prop(panel, "actionLabel", "\"take the session over\"")
        .unwrap();
    let html = host.state.borrow().doc.inner_html(panel);
    assert!(
        html.contains("take the session over"),
        "the action button renders: {html}"
    );

    let button = {
        let state = host.state.borrow();
        let root = state.doc.root();
        state
            .doc
            .find_element(root, |el| {
                el.attr("class").is_some_and(|c| c.contains("action"))
            })
            .expect("the action button")
    };
    host.click(button).unwrap();
    assert_eq!(host.prop_json(panel, "command").unwrap(), "\"action\"");
}

#[test]
fn the_deck_composes_the_panes_and_the_qr() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, "theme.js");
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
        state
            .doc
            .descendant_by_tag(root, "qr-code")
            .expect("the deck's qr-code")
    };
    host.set_prop(qr, "data", "\"DECKCODE\"").unwrap();
    let html = host.state.borrow().doc.inner_html(qr);
    assert!(
        html.contains("data-tui=\"qr\"") && html.contains("DECKCODE"),
        "the driven QR renders its widget marker with the data: {html}"
    );
}

#[test]
fn the_panel_invite_embeds_the_qr_with_the_link() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    // pair-panel, its theme fragment and the <qr-code> it embeds are off the
    // todo-app entry graph (as in main.rs); load the panel's imports first so
    // its scoped specifiers resolve.
    module(&mut host, "theme.js");
    module(&mut host, "qr-code.js");
    module(&mut host, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    let link = "https://example/lit-demo/p2p/#PAYLOAD64";
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
