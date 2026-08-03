//! The shared `<pair-panel>` (ADR 0029) under the scripted host: the REAL
//! component, compiled from `@gronke/uic-sync`'s web root. The panel used
//! to live in the demo app, out of `uic_js`'s reach, so this test could only
//! stub it; now it ships in `uic_sync`, and the scripted-host library hosts
//! the shipped pairing UI directly, no demo package in between.
//!
//! The seam it pins is the terminal's contract: properties in, intent out
//! through the polled `command` property (Boa has no `dispatchEvent`).

use std::path::Path;

use uic_js::JsHost;

/// Compiles a `uic_sync` web module (TypeScript) and registers it under its
/// package specifier, so `pair-panel`'s `./theme.js` and `./qr-code.js`
/// resolve within `@gronke/uic-sync`.
fn load(host: &mut JsHost, name: &str) {
    let file = format!("{name}.ts");
    let src = std::fs::read_to_string(uic_sync::web_root().join(&file))
        .unwrap_or_else(|err| panic!("read {file}: {err}"));
    let js = web_modules::typescript::compile_str(&src, Path::new(&file))
        .unwrap_or_else(|err| panic!("compile {file}: {err}"));
    host.load_module(&format!("@gronke/uic-sync/{name}.js"), &js)
        .unwrap_or_else(|err| panic!("load {name}: {err}"));
}

/// A host with the real pair-panel mounted, its imports loaded inside-out.
fn panel_host() -> (JsHost, uic_dom::NodeId) {
    let mut host = JsHost::new().unwrap();
    load(&mut host, "theme");
    load(&mut host, "qr-code");
    load(&mut host, "pair-panel");
    let panel = host.mount("pair-panel", &[]).unwrap();
    (host, panel)
}

fn node_by(host: &JsHost, class: &str) -> uic_dom::NodeId {
    let state = host.state.borrow();
    let root = state.doc.root();
    state
        .doc
        .find_element(root, |el| {
            el.attr("class")
                .is_some_and(|c| c.split_whitespace().any(|w| w == class))
        })
        .unwrap_or_else(|| panic!("a node with class {class}"))
}

#[test]
fn properties_drive_the_render() {
    let (mut host, panel) = panel_host();

    // The invite mode renders the three-step wizard; the active step carries
    // the link and the status the host set.
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "link", "\"https://example/p2p/#abc123\"")
        .unwrap();
    host.set_prop(panel, "status", "\"share the invite\"")
        .unwrap();
    host.set_prop(panel, "resetLabel", "\"start over\"")
        .unwrap();

    let html = host.state.borrow().doc.inner_html(panel);
    assert!(html.contains("abc123"), "the invite link renders: {html}");
    assert!(
        html.contains("share the invite"),
        "the status renders: {html}"
    );
    assert!(
        html.contains("start over"),
        "the reset button renders: {html}"
    );
}

#[test]
fn buttons_signal_intent_through_the_command_property() {
    let (mut host, panel) = panel_host();

    // Idle: "create an invite" writes command="invite", the only channel back
    // to the terminal host (no events under Boa).
    let invite = node_by(&host, "invite");
    host.click(invite).unwrap();
    assert_eq!(host.prop_json(panel, "command").unwrap(), "\"invite\"");

    // The host reads, acts, clears; the next click writes afresh.
    host.set_prop(panel, "command", "null").unwrap();
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "resetLabel", "\"start over\"")
        .unwrap();

    // Step 1's connect button and the reset control both signal.
    let connect = node_by(&host, "connect");
    host.click(connect).unwrap();
    assert_eq!(host.prop_json(panel, "command").unwrap(), "\"connect\"");

    host.set_prop(panel, "command", "null").unwrap();
    let reset = node_by(&host, "reset");
    host.click(reset).unwrap();
    assert_eq!(host.prop_json(panel, "command").unwrap(), "\"reset\"");
}
