//! The p2p composition under the scripted (terminal) host, loaded from the
//! real baked package so the shipped components are what runs, not trimmed
//! copies: the shared `<qr-code>` renders its `data-tui="qr"` widget marker
//! (ADR 0029), `<pair-panel>` signals intent through the polled `command`
//! property (ADR 0029), and `<p2p-deck>` composes the panes with the QR
//! beside them.

use std::path::Path;

use uic_dom::NodeId;
use uic_js::JsHost;

const PACKAGE: &str = "@gronke/lit-todo";
const SYNC: &str = "@gronke/uic-sync";

fn npm_root() -> &'static Path {
    Path::new(env!("UIC_LIT_DEMO_NPM_ROOT"))
}

fn module(host: &mut JsHost, package: &str, file: &str) {
    let src = std::fs::read_to_string(npm_root().join(package).join(file))
        .unwrap_or_else(|err| panic!("read {file}: {err}"));
    host.load_module(&format!("{package}/{file}"), &src)
        .unwrap_or_else(|err| panic!("load {file}: {err}"));
}

#[test]
fn qr_code_renders_the_terminal_widget_marker() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, SYNC, "qr-code.js");

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
    module(&mut host, SYNC, "theme.js");
    module(&mut host, SYNC, "qr-code.js");
    module(&mut host, SYNC, "pair-panel.js");
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
    module(&mut host, SYNC, "theme.js");
    module(&mut host, SYNC, "qr-code.js");
    module(&mut host, SYNC, "pair-panel.js");
    module(&mut host, SYNC, "status-navbar.js");
    module(&mut host, PACKAGE, "p2p-deck.js");
    let deck = host.mount("p2p-deck", &[]).unwrap();

    // The deck composes the stack and the QR beside it (the structure the
    // terminal's responsive flex layout works on); pairing-first: the todo
    // and the navbar ship inside hidden wrappers, the pairing pane shows.
    let html = host.state.borrow().doc.inner_html(deck);
    assert!(html.contains("<todo-app"), "the todo card mounts: {html}");
    assert!(html.contains("<pair-panel"), "the panel mounts: {html}");
    assert!(html.contains("<qr-code"), "the deck QR mounts: {html}");
    assert!(html.contains("<status-navbar"), "the navbar mounts: {html}");
    assert!(
        html.contains(r#"<div class="todo-pane" hidden"#),
        "the todo boots hidden: {html}"
    );
    assert!(
        !html.contains(r#"<div class="pairing-pane" hidden"#),
        "the pairing pane boots visible: {html}"
    );

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
    module(&mut host, SYNC, "theme.js");
    module(&mut host, SYNC, "qr-code.js");
    module(&mut host, SYNC, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    let link = "https://example/lit-demo/p2p/#PAYLOAD64";
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "link", &format!("{link:?}")).unwrap();

    let html = host.state.borrow().doc.inner_html(panel);
    // Step 1 is active by default; its body composes the shared component,
    // fed the link as an attribute, the cross-host-safe binding.
    assert!(
        html.contains(&format!("data=\"{link}\"")),
        "the panel embeds <qr-code> carrying the invite link: {html}"
    );
}

#[test]
fn the_wizard_mutes_the_steps_it_has_not_reached() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, SYNC, "theme.js");
    module(&mut host, SYNC, "qr-code.js");
    module(&mut host, SYNC, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    let reply = "https://example/lit-demo/p2p/#OWN.1a2b3c4d";
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "link", &format!("{reply:?}")).unwrap();
    host.set_prop(panel, "step", "2").unwrap();

    // The three step cards, in order: step 2 active, 1 done, 3 to come.
    // Each header's own class carries the mute; its text carries the ✓.
    let headers: Vec<(String, String)> = {
        let state = host.state.borrow();
        let root = state.doc.root();
        state
            .doc
            .descendants(root)
            .filter(|&node| {
                state
                    .doc
                    .element(node)
                    .and_then(|el| el.attr("class"))
                    .is_some_and(|c| c.split_whitespace().any(|w| w == "card-header"))
            })
            .map(|node| {
                let class = state
                    .doc
                    .element(node)
                    .and_then(|el| el.attr("class"))
                    .unwrap_or_default()
                    .to_string();
                (class, state.doc.inner_html(node))
            })
            .collect()
    };
    assert_eq!(headers.len(), 3, "three step cards: {headers:?}");
    assert!(
        headers[0].1.contains('✓'),
        "step 1 is done: {}",
        headers[0].1
    );
    assert!(
        headers[0].0.contains("text-muted") && headers[2].0.contains("text-muted"),
        "the reached-past and not-yet steps mute: {headers:?}"
    );
    assert!(
        !headers[1].0.contains("text-muted"),
        "the active step stays lit: {}",
        headers[1].0
    );

    // The active step 2 holds the reply link; the muted cards carry no
    // controls (nothing to click into; the terminal has no pointer-events).
    let html = host.state.borrow().doc.inner_html(panel);
    assert!(html.contains(reply), "the active step shows the reply link");
    let muted_bodies_have_no_controls = {
        let state = host.state.borrow();
        let root = state.doc.root();
        let muted: Vec<NodeId> = state
            .doc
            .descendants(root)
            .filter(|&node| {
                state
                    .doc
                    .element(node)
                    .and_then(|el| el.attr("class"))
                    .is_some_and(|c| {
                        let classes: Vec<&str> = c.split_whitespace().collect();
                        classes.contains(&"card-body") && classes.contains(&"small")
                    })
            })
            .collect();
        muted.into_iter().all(|node| {
            let body = state.doc.inner_html(node);
            !body.contains("<button") && !body.contains("<textarea")
        })
    };
    assert!(
        muted_bodies_have_no_controls,
        "muted step bodies render a summary, no controls"
    );
}

#[test]
fn the_active_connect_step_carries_the_live_status() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, SYNC, "theme.js");
    module(&mut host, SYNC, "qr-code.js");
    module(&mut host, SYNC, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    // Step 3 connects on its own (no button), so its body IS the live
    // status; a blank card here reads as "nothing is happening" even while a
    // connect is genuinely in flight.
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "step", "3").unwrap();
    host.set_prop(panel, "status", "\"Connecting…\"").unwrap();

    // The three step bodies in order; step 3 is the active one and must not
    // be the empty box it used to be.
    let bodies: Vec<(String, String)> = {
        let state = host.state.borrow();
        let root = state.doc.root();
        state
            .doc
            .descendants(root)
            .filter(|&node| {
                state
                    .doc
                    .element(node)
                    .and_then(|el| el.attr("class"))
                    .is_some_and(|c| c.split_whitespace().any(|w| w == "card-body"))
            })
            .map(|node| {
                let class = state
                    .doc
                    .element(node)
                    .and_then(|el| el.attr("class"))
                    .unwrap_or_default()
                    .to_string();
                (class, state.doc.inner_html(node))
            })
            .collect()
    };
    assert_eq!(bodies.len(), 3, "three step bodies: {bodies:?}");
    assert!(
        !bodies[2].0.contains("text-muted"),
        "the connect step is active: {}",
        bodies[2].0
    );
    assert!(
        bodies[2].1.contains("Connecting…"),
        "the active connect step shows the live status, not a blank card: {}",
        bodies[2].1
    );
}

#[test]
fn the_acknowledge_step_shows_the_live_connecting_status() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, SYNC, "theme.js");
    module(&mut host, SYNC, "qr-code.js");
    module(&mut host, SYNC, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    // The opener owes the reply link AND is connecting; step 2 must show both,
    // with the live seconds counter; the bug was the reply prompt alone, no
    // sign a connect was in flight.
    let reply = "https://example/lit-demo/p2p/#OWN.1a2b3c4d";
    host.set_prop(panel, "mode", "\"invite\"").unwrap();
    host.set_prop(panel, "step", "2").unwrap();
    host.set_prop(panel, "link", &format!("{reply:?}")).unwrap();
    host.set_prop(
        panel,
        "status",
        "\"connecting 12s: send this reply back; you pair the moment they open it\"",
    )
    .unwrap();

    let bodies: Vec<(String, String)> = {
        let state = host.state.borrow();
        let root = state.doc.root();
        state
            .doc
            .descendants(root)
            .filter(|&node| {
                state
                    .doc
                    .element(node)
                    .and_then(|el| el.attr("class"))
                    .is_some_and(|c| c.split_whitespace().any(|w| w == "card-body"))
            })
            .map(|node| {
                let class = state
                    .doc
                    .element(node)
                    .and_then(|el| el.attr("class"))
                    .unwrap_or_default()
                    .to_string();
                (class, state.doc.inner_html(node))
            })
            .collect()
    };
    assert_eq!(bodies.len(), 3, "three step bodies: {bodies:?}");
    assert!(
        !bodies[1].0.contains("text-muted"),
        "the acknowledge step is active: {}",
        bodies[1].0
    );
    assert!(
        bodies[1].1.contains("connecting"),
        "the active step shows the live connecting status: {}",
        bodies[1].1
    );
    assert!(
        bodies[1].1.contains(reply),
        "the active step still shows the reply link: {}",
        bodies[1].1
    );
}

#[test]
fn a_failure_renders_as_a_red_alert() {
    let mut host = JsHost::new().unwrap();
    host.load_package(npm_root(), PACKAGE).unwrap();
    module(&mut host, SYNC, "theme.js");
    module(&mut host, SYNC, "qr-code.js");
    module(&mut host, SYNC, "pair-panel.js");
    let panel = host.mount("pair-panel", &[]).unwrap();

    host.set_prop(panel, "mode", "\"failed\"").unwrap();
    host.set_prop(panel, "status", "\"pairing failed: unreachable\"")
        .unwrap();

    let html = host.state.borrow().doc.inner_html(panel);
    // A Bootstrap danger alert in the browser; `text-danger` maps to ansi
    // light-red in the terminal (tui-overrides.css), so it reads red in both.
    assert!(
        html.contains("alert-danger") && html.contains("text-danger"),
        "the failure is a red alert: {html}"
    );
    assert!(
        html.contains("pairing failed: unreachable"),
        "the alert carries the status: {html}"
    );
}
