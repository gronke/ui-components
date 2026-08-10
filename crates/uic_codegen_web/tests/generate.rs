//! Full-run codegen test against the real <input-date> definition, with a
//! committed snapshot of the emitted TypeScript and an oxc transpile smoke
//! test (the same transform web_modules applies at consumer build time).
//!
//! Refresh the snapshot with `UPDATE_EXPECTED=1 cargo test -p uic_codegen_web`.

use std::fs;
use std::path::{Path, PathBuf};

use uic_codegen_web::WebCodegen;

fn generate(test: &str) -> PathBuf {
    ui_components::link();
    ui_components_demo::link();
    let out = std::env::temp_dir().join(format!("uic-codegen-{test}-{}", std::process::id()));
    let root = WebCodegen::new(&out)
        .manifest(true)
        .extra_module("uic-connectors.ts", ui_components::connect::WEB_TS)
        .extra_module("uic-icons.ts", uic_icons::WEB_TS)
        .run()
        .expect("codegen succeeds");
    assert_eq!(
        root.components,
        vec![
            "app-root",
            "input-date",
            "input-date-range",
            "input-number",
            "input-secret",
            "input-select",
            "input-suggestion",
            "input-text",
            "input-textarea",
            "input-timezone",
            "nav-breadcrumb",
            "nav-tabs",
            "uic-icon",
            "uic-tree"
        ]
    );
    assert_eq!(root.module_path("input-date"), "components/input-date.js");
    root.root
}

fn assert_matches_snapshot(generated: &str, snapshot: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/expected")
        .join(snapshot);
    if std::env::var_os("UPDATE_EXPECTED").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, generated).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing snapshot {}; run with UPDATE_EXPECTED=1",
            path.display()
        )
    });
    assert!(
        generated == expected,
        "generated output differs from {}; run with UPDATE_EXPECTED=1 and review the diff\n\
         ---- generated ----\n{generated}",
        path.display()
    );
}

#[test]
fn emits_the_full_generated_root() {
    let root = generate("root");
    for file in [
        "components/app-root.ts",
        "components/app-root.impl.ts",
        "components/input-date.ts",
        "components/input-date.impl.ts",
        "components/_input-date.scss",
        "components/input-date-range.ts",
        "components/input-date-range.impl.ts",
        "components/_input-date-range.scss",
        "components/input-number.ts",
        "components/input-number.impl.ts",
        "components/_input-number.scss",
        "components/input-select.ts",
        "components/input-select.impl.ts",
        "components/_input-select.scss",
        "components/input-suggestion.ts",
        "components/input-suggestion.impl.ts",
        "components/_input-suggestion.scss",
        "components/input-text.ts",
        "components/input-text.impl.ts",
        "components/input-textarea.ts",
        "components/input-textarea.impl.ts",
        "components/_input-textarea.scss",
        "components/input-timezone.ts",
        "components/input-timezone.impl.ts",
        "components/nav-breadcrumb.ts",
        "components/nav-breadcrumb.impl.ts",
        "components/_nav-breadcrumb.scss",
        "components/nav-tabs.ts",
        "components/nav-tabs.impl.ts",
        "components/_nav-tabs.scss",
        "components/uic-icon.ts",
        "components/uic-icon.impl.ts",
        "components/_uic-icon.scss",
        "components/uic-tree.ts",
        "components/uic-tree.impl.ts",
        "components/_uic-tree.scss",
        "components/_input-default.scss",
        "components/uic-runtime.ts",
        "components/uic-impl-helpers.ts",
        "components/uic-connectors.ts",
        "components/uic-icons.ts",
        "elements.scss",
        "custom-elements.json",
    ] {
        assert!(root.join(file).exists(), "missing {file}");
    }

    let elements = fs::read_to_string(root.join("elements.scss")).unwrap();
    assert!(elements.contains("@use \"components/input-date\";"));
    // Shared stylesheets come first, so component styles can override.
    let shared = elements
        .find("components/input-default")
        .expect("shared @use");
    let component = elements
        .find("components/input-date")
        .expect("component @use");
    assert!(shared < component);

    // The impl partial is copied verbatim from the component module.
    let copied = fs::read_to_string(root.join("components/input-date.impl.ts")).unwrap();
    assert_eq!(
        copied,
        include_str!("../../ui_components/src/input/date.impl.ts")
    );
}

#[test]
fn generated_class_matches_the_snapshot() {
    let root = generate("snapshot");
    let generated = fs::read_to_string(root.join("components/input-date.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-date.ts");
}

#[test]
fn generated_app_root_class_matches_the_snapshot() {
    let root = generate("snapshot-app-root");
    let generated = fs::read_to_string(root.join("components/app-root.ts")).unwrap();
    assert_matches_snapshot(&generated, "app-root.ts");
}

#[test]
fn generated_date_range_class_matches_the_snapshot() {
    let root = generate("snapshot-date-range");
    let generated = fs::read_to_string(root.join("components/input-date-range.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-date-range.ts");
}

#[test]
fn generated_suggestion_class_matches_the_snapshot() {
    let root = generate("snapshot-suggestion");
    let generated = fs::read_to_string(root.join("components/input-suggestion.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-suggestion.ts");
}

#[test]
fn generated_text_class_matches_the_snapshot() {
    let root = generate("snapshot-text");
    let generated = fs::read_to_string(root.join("components/input-text.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-text.ts");
}

#[test]
fn generated_number_class_matches_the_snapshot() {
    let root = generate("snapshot-number");
    let generated = fs::read_to_string(root.join("components/input-number.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-number.ts");
}

#[test]
fn generated_secret_class_matches_the_snapshot() {
    let root = generate("snapshot-secret");
    let generated = fs::read_to_string(root.join("components/input-secret.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-secret.ts");
}

#[test]
fn generated_icon_class_matches_the_snapshot() {
    let root = generate("snapshot-icon");
    let generated = fs::read_to_string(root.join("components/uic-icon.ts")).unwrap();
    assert_matches_snapshot(&generated, "uic-icon.ts");
}

#[test]
fn generated_textarea_class_matches_the_snapshot() {
    let root = generate("snapshot-textarea");
    let generated = fs::read_to_string(root.join("components/input-textarea.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-textarea.ts");
}

#[test]
fn generated_select_class_matches_the_snapshot() {
    let root = generate("snapshot-select");
    let generated = fs::read_to_string(root.join("components/input-select.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-select.ts");
}

#[test]
fn generated_timezone_class_matches_the_snapshot() {
    let root = generate("snapshot-timezone");
    let generated = fs::read_to_string(root.join("components/input-timezone.ts")).unwrap();
    assert_matches_snapshot(&generated, "input-timezone.ts");
}

#[test]
fn generated_nav_tabs_class_matches_the_snapshot() {
    let root = generate("snapshot-nav-tabs");
    let generated = fs::read_to_string(root.join("components/nav-tabs.ts")).unwrap();
    assert_matches_snapshot(&generated, "nav-tabs.ts");
}

#[test]
fn generated_nav_breadcrumb_class_matches_the_snapshot() {
    let root = generate("snapshot-nav-breadcrumb");
    let generated = fs::read_to_string(root.join("components/nav-breadcrumb.ts")).unwrap();
    assert_matches_snapshot(&generated, "nav-breadcrumb.ts");
}

#[test]
fn generated_tree_class_matches_the_snapshot() {
    let root = generate("snapshot-tree");
    let generated = fs::read_to_string(root.join("components/uic-tree.ts")).unwrap();
    assert_matches_snapshot(&generated, "uic-tree.ts");
}

#[test]
fn generated_typescript_transpiles_with_oxc() {
    let root = generate("oxc");
    for file in [
        "components/app-root.ts",
        "components/input-date.ts",
        "components/input-date-range.ts",
        "components/input-number.ts",
        "components/input-select.ts",
        "components/input-suggestion.ts",
        "components/input-text.ts",
        "components/input-textarea.ts",
        "components/input-timezone.ts",
        "components/nav-breadcrumb.ts",
        "components/nav-tabs.ts",
        "components/uic-tree.ts",
        "components/uic-tree.impl.ts",
        "components/uic-runtime.ts",
        "components/uic-impl-helpers.ts",
        "components/uic-connectors.ts",
    ] {
        let source = fs::read_to_string(root.join(file)).unwrap();
        let js = web_modules::typescript::compile_str(&source, Path::new(file))
            .unwrap_or_else(|err| panic!("{file} does not transpile: {err}"));
        assert!(!js.is_empty());
    }
}

/// The manifest module of one component, looked up by path; positional
/// indices shift whenever a component joins the catalog.
fn module_by_path<'a>(manifest: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    manifest["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|module| module["path"] == path)
        .unwrap_or_else(|| panic!("no module {path} in the manifest"))
}

#[test]
fn manifest_describes_the_component() {
    let root = generate("manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("custom-elements.json")).unwrap())
            .unwrap();
    // Modules sort by tag; the order is asserted as a property, the entries
    // are found by path.
    let tags: Vec<&str> = manifest["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|module| module["declarations"][0]["tagName"].as_str().unwrap())
        .collect();
    let mut sorted = tags.clone();
    sorted.sort_unstable();
    assert_eq!(tags, sorted, "modules sort by tag");

    let module = module_by_path(&manifest, "components/input-date.ts");
    let declaration = &module["declarations"][0];
    assert_eq!(declaration["tagName"], "input-date");
    let events = declaration["events"].as_array().unwrap();
    let names: Vec<_> = events.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec!["value-changed", "date-changed", "timezone-changed"]
    );

    // The Zoned property is typed and property-only in the manifest.
    let members = declaration["members"].as_array().unwrap();
    let date = members
        .iter()
        .find(|m| m["name"] == "date")
        .expect("date member");
    assert_eq!(date["type"]["text"], "Temporal.ZonedDateTime | null");
    let attributes = declaration["attributes"].as_array().unwrap();
    assert!(
        attributes.iter().all(|a| a["fieldName"] != "date"),
        "no attribute for the date property"
    );

    // The Options property is typed and property-only as well.
    let select = module_by_path(&manifest, "components/input-select.ts");
    let declaration = &select["declarations"][0];
    assert_eq!(declaration["tagName"], "input-select");
    let members = declaration["members"].as_array().unwrap();
    let options = members
        .iter()
        .find(|m| m["name"] == "options")
        .expect("options member");
    assert_eq!(options["type"]["text"], "SelectOption[]");
    assert_eq!(options["default"], "[]");
    let attributes = declaration["attributes"].as_array().unwrap();
    assert!(
        attributes.iter().all(|a| a["fieldName"] != "options"),
        "no attribute for the options property"
    );

    // The suggestion input notifies both its commit and its live query.
    let suggestion = module_by_path(&manifest, "components/input-suggestion.ts");
    let declaration = &suggestion["declarations"][0];
    assert_eq!(declaration["tagName"], "input-suggestion");
    let events = declaration["events"].as_array().unwrap();
    let names: Vec<_> = events.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["value-changed", "query-changed"]);

    // The tab bar notifies its pick; its rows are property-only options.
    let nav_tabs = module_by_path(&manifest, "components/nav-tabs.ts");
    let declaration = &nav_tabs["declarations"][0];
    assert_eq!(declaration["tagName"], "nav-tabs");
    let events = declaration["events"].as_array().unwrap();
    let names: Vec<_> = events.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["value-changed"]);
    let members = declaration["members"].as_array().unwrap();
    let options = members
        .iter()
        .find(|m| m["name"] == "options")
        .expect("options member");
    assert_eq!(options["type"]["text"], "SelectOption[]");
    let attributes = declaration["attributes"].as_array().unwrap();
    assert!(
        attributes.iter().all(|a| a["fieldName"] != "options"),
        "no attribute for the options property"
    );

    // The breadcrumb trail is static: property-only rows, no events.
    let breadcrumb = module_by_path(&manifest, "components/nav-breadcrumb.ts");
    let declaration = &breadcrumb["declarations"][0];
    assert_eq!(declaration["tagName"], "nav-breadcrumb");
    assert!(declaration["events"]
        .as_array()
        .is_none_or(|events| events.is_empty()));
    let members = declaration["members"].as_array().unwrap();
    let items = members
        .iter()
        .find(|m| m["name"] == "items")
        .expect("items member");
    assert_eq!(items["type"]["text"], "Record<string, unknown>[]");
    assert_eq!(items["default"], "[]");
    let attributes = declaration["attributes"].as_array().unwrap();
    assert!(
        attributes.iter().all(|a| a["fieldName"] != "items"),
        "no attribute for the items property"
    );

    // The Object property (app-root's state) is typed and property-only.
    let app_root = module_by_path(&manifest, "components/app-root.ts");
    let declaration = &app_root["declarations"][0];
    assert_eq!(declaration["tagName"], "app-root");
    let members = declaration["members"].as_array().unwrap();
    let state = members
        .iter()
        .find(|m| m["name"] == "state")
        .expect("state member");
    assert_eq!(state["type"]["text"], "Record<string, unknown>");
    assert_eq!(state["default"], "{}");
    let events = declaration["events"].as_array().unwrap();
    let names: Vec<_> = events.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["state-changed"]);
    let attributes = declaration["attributes"].as_array().unwrap();
    assert!(attributes.is_empty(), "no attribute for the state property");
}
