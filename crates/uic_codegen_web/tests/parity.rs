//! Cross-target parity: the same state through the Rust computeds and the
//! compiled TypeScript twin must agree. This test generates the fixtures
//! FROM the Rust behavior (write with `UPDATE_EXPECTED=1`, asserted
//! otherwise) and stages the compiled twin under `tests/parity/build/`;
//! `scripts/parity-check.mjs` replays the fixtures against it in node.

use std::fs;
use std::path::Path;

use serde_json::json;
use ui_components::connect::QuerySource;

/// One computed of a fresh app-root holding `state`, as JSON.
fn compute(state: &serde_json::Value, name: &str) -> serde_json::Value {
    let def = uic_core::CustomElementRegistry::get("app-root").expect("app-root registered");
    let mut store = uic_core::PropertyStore::new(def.properties);
    let behavior = (def.new_behavior)();
    store.set("state", uic_core::json::value_from_json(state));
    uic_core::json::value_to_json(&behavior.compute(&store, name))
}

/// The `crumbs` computed of a fresh nav-breadcrumb holding `items` and
/// `divider`, as JSON.
fn breadcrumb_crumbs(items: &serde_json::Value, divider: &str) -> serde_json::Value {
    let def =
        uic_core::CustomElementRegistry::get("nav-breadcrumb").expect("nav-breadcrumb registered");
    let mut store = uic_core::PropertyStore::new(def.properties);
    let behavior = (def.new_behavior)();
    store.set("items", uic_core::json::value_from_json(items));
    store.set("divider", divider);
    uic_core::json::value_to_json(&behavior.compute(&store, "crumbs"))
}

#[test]
fn rust_fixtures_and_the_compiled_twin_stage_for_the_node_replay() {
    ui_components::link();
    ui_components_demo::link();
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
    // The twin imports the connectors module (the word pool, ADR 0014).
    let js = web_modules::typescript::compile_str(
        ui_components::connect::WEB_TS,
        Path::new("uic-connectors.ts"),
    )
    .unwrap();
    fs::write(build.join("uic-connectors.js"), js).unwrap();
    // The breadcrumb twin (type-only imports, so it stages standalone).
    let breadcrumb_def = uic_core::CustomElementRegistry::get("nav-breadcrumb").unwrap();
    let impl_ts = breadcrumb_def
        .web_impl
        .expect("nav-breadcrumb has a web impl");
    let js =
        web_modules::typescript::compile_str(impl_ts, Path::new("nav-breadcrumb.impl.ts")).unwrap();
    fs::write(build.join("nav-breadcrumb.impl.js"), js).unwrap();

    // The fixture states: sparse, full, null-bearing and tabbed, the
    // shapes the transport delivers (ADR 0013).
    let states = [
        json!({}),
        json!({ "date": "2026-07-07", "note": "hi", "amount": 12.5 }),
        json!({ "note": null, "pick": "Europe/Berlin", "zone": "UTC" }),
        json!({ "tab": "about", "note": "hi" }),
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
                    "tab": compute(state, "tab"),
                    "showForm": compute(state, "show_form"),
                    "showAbout": compute(state, "show_about"),
                },
            })
        })
        .collect();
    // The suggest fixtures: the Rust pool's answers for replayed queries;
    // the node side must resolve the same rows through the TS pool.
    let queries = ["", "a", "AP", "apple", "zzz"];
    let suggest: Vec<serde_json::Value> = queries
        .iter()
        .map(|query| {
            let mut values: Vec<String> = Vec::new();
            ui_components_demo::app_root::WORD_POOL.query(
                query,
                Box::new(|rows| values = rows.into_iter().map(|row| row.value).collect()),
            );
            json!({ "query": query, "expect": values })
        })
        .collect();

    // The breadcrumb fixtures: items and divider through the trail's crumbs
    // computed; the node side must decorate identically.
    let trails = [
        (json!([]), "›"),
        (
            json!([
                { "label": "Documents", "href": "/documents" },
                { "label": "Reports", "href": "/documents/reports" },
                { "label": "Q3" }
            ]),
            "›",
        ),
        (json!([null, { "label": "End", "href": "" }]), "/"),
    ];
    let breadcrumb: Vec<serde_json::Value> = trails
        .iter()
        .map(|(items, divider)| {
            json!({
                "items": items,
                "divider": divider,
                "expect": breadcrumb_crumbs(items, divider),
            })
        })
        .collect();

    let fixtures = json!({ "cases": cases, "suggest": suggest, "breadcrumb": breadcrumb });

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
