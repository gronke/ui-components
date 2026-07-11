//! Cross-target parity: the same state through the Rust computeds and the
//! compiled TypeScript twin must agree. This test generates the fixtures
//! FROM the Rust behavior (write with `UPDATE_EXPECTED=1`, asserted
//! otherwise) and stages the compiled twin under `tests/parity/build/`;
//! `scripts/parity-check.mjs` replays the fixtures against it in node.

use std::fs;
use std::path::Path;

use serde_json::json;

/// One computed of a fresh app-root holding `state`, as JSON.
fn compute(state: &serde_json::Value, name: &str) -> serde_json::Value {
    let def = uic_core::CustomElementRegistry::get("app-root").expect("app-root registered");
    let mut store = uic_core::PropertyStore::new(def.properties);
    let behavior = (def.new_behavior)();
    store.set(
        "state",
        uic_core::json::value_from_json(state).expect("state converts"),
    );
    uic_core::json::value_to_json(&behavior.compute(&store, name))
}

#[test]
fn rust_fixtures_and_the_compiled_twin_stage_for_the_node_replay() {
    ui_components::link();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity");

    // The compiled twin and its helpers, for scripts/parity-check.mjs.
    let build = dir.join("build");
    fs::create_dir_all(&build).unwrap();
    let def = uic_core::CustomElementRegistry::get("app-root").unwrap();
    let impl_ts = def.web_impl.expect("app-root has a web impl");
    let js = web_modules::typescript::compile_str(impl_ts, Path::new("app-root.impl.ts")).unwrap();
    fs::write(build.join("app-root.impl.js"), js).unwrap();
    let helpers = include_str!("../src/uic-impl-helpers.ts");
    let js =
        web_modules::typescript::compile_str(helpers, Path::new("uic-impl-helpers.ts")).unwrap();
    fs::write(build.join("uic-impl-helpers.js"), js).unwrap();

    // The fixture states: sparse, full and null-bearing — the shapes the
    // transport delivers (ADR 0013).
    let states = [
        json!({}),
        json!({ "date": "2026-07-07", "note": "hi", "amount": 12.5 }),
        json!({ "note": null, "pick": "Europe/Berlin", "zone": "UTC" }),
    ];
    let cases: Vec<serde_json::Value> = states
        .iter()
        .map(|state| {
            json!({
                "state": state,
                "expect": {
                    "stateLine": compute(state, "state_line"),
                    "amount": compute(state, "amount"),
                    "date": compute(state, "date"),
                },
            })
        })
        .collect();
    let fixtures = json!({ "cases": cases });

    let path = dir.join("fixtures.json");
    if std::env::var_os("UPDATE_EXPECTED").is_some() {
        fs::write(
            &path,
            serde_json::to_string_pretty(&fixtures).unwrap() + "\n",
        )
        .unwrap();
        return;
    }
    let expected: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("missing {}; run with UPDATE_EXPECTED=1", path.display())),
    )
    .unwrap();
    // Value equality, not string: the map flavor (preserve_order) must not
    // matter for the committed file.
    assert_eq!(
        fixtures, expected,
        "the fixtures are the Rust behavior; refresh with UPDATE_EXPECTED=1"
    );
}
