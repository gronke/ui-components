//! Todo persistence over the storage feature, on the real baked package:
//! rows seeded into the runtime's localStorage load in place of the seed
//! rows, item changes write back, and garbage falls back cleanly. The
//! browser runs the same component code over its native localStorage.

use std::path::Path;

use uic_js::JsHost;

const PACKAGE: &str = "@schuhkarton/lit-todo";

fn mounted_with(seed: Option<&str>) -> (JsHost, uic_dom::NodeId) {
    let mut host = JsHost::new().unwrap();
    host.load_package(Path::new(env!("UIC_LIT_DEMO_NPM_ROOT")), PACKAGE)
        .unwrap();
    if let Some(raw) = seed {
        host.eval(&format!("localStorage.setItem('uic-todos', {raw:?})"))
            .unwrap();
    }
    let node = host
        .mount("todo-app", &[("data-bs-theme", "dark")])
        .unwrap();
    (host, node)
}

/// The stored snapshot, read off the engine value in Rust.
fn stored(host: &mut JsHost) -> Option<String> {
    let value = host.eval("localStorage.getItem('uic-todos')").unwrap();
    if value.is_null() {
        return None;
    }
    Some(
        value
            .as_string()
            .expect("a string snapshot")
            .to_std_string_escaped(),
    )
}

#[test]
fn stored_rows_load_and_changes_write_back() {
    let seed = r#"[{"id":7,"text":"persisted row","done":false}]"#;
    let (mut host, node) = mounted_with(Some(seed));
    assert_eq!(host.prop_json(node, "items").unwrap(), seed);

    // A state change persists: the accessor write runs lit's update, and
    // updated() snapshots the items back into storage.
    let toggled = r#"[{"id":7,"text":"persisted row","done":true}]"#;
    host.set_prop(node, "items", toggled).unwrap();
    assert_eq!(stored(&mut host).as_deref(), Some(toggled));
}

#[test]
fn a_stored_empty_list_is_honored() {
    let (mut host, node) = mounted_with(Some("[]"));
    assert_eq!(host.prop_json(node, "items").unwrap(), "[]");
}

#[test]
fn absent_or_garbage_storage_falls_back_to_the_seed_rows() {
    for seed in [None, Some("not json at all"), Some(r#"{"an":"object"}"#)] {
        let (mut host, node) = mounted_with(seed);
        let items = host.prop_json(node, "items").unwrap();
        assert!(
            items.contains("render a web app in the terminal"),
            "seed rows for {seed:?}: {items}"
        );
    }
}
