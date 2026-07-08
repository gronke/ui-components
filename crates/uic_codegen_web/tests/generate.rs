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
    let out = std::env::temp_dir().join(format!("uic-codegen-{test}-{}", std::process::id()));
    let root = WebCodegen::new(&out)
        .manifest(true)
        .run()
        .expect("codegen succeeds");
    assert_eq!(
        root.components,
        vec![
            "input-date",
            "input-number",
            "input-select",
            "input-text",
            "input-textarea",
            "input-timezone"
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
        "components/input-date.ts",
        "components/input-date.impl.ts",
        "components/_input-date.scss",
        "components/input-number.ts",
        "components/input-number.impl.ts",
        "components/_input-number.scss",
        "components/input-select.ts",
        "components/input-select.impl.ts",
        "components/_input-select.scss",
        "components/input-text.ts",
        "components/input-text.impl.ts",
        "components/input-textarea.ts",
        "components/input-textarea.impl.ts",
        "components/_input-textarea.scss",
        "components/input-timezone.ts",
        "components/input-timezone.impl.ts",
        "components/_input-default.scss",
        "components/uic-runtime.ts",
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
fn generated_typescript_transpiles_with_oxc() {
    let root = generate("oxc");
    for file in [
        "components/input-date.ts",
        "components/input-number.ts",
        "components/input-select.ts",
        "components/input-text.ts",
        "components/input-textarea.ts",
        "components/input-timezone.ts",
        "components/uic-runtime.ts",
    ] {
        let source = fs::read_to_string(root.join(file)).unwrap();
        let js = web_modules::typescript::compile_str(&source, Path::new(file))
            .unwrap_or_else(|err| panic!("{file} does not transpile: {err}"));
        assert!(!js.is_empty());
    }
}

#[test]
fn manifest_describes_the_component() {
    let root = generate("manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("custom-elements.json")).unwrap())
            .unwrap();
    let module = &manifest["modules"][0];
    assert_eq!(module["path"], "components/input-date.ts");
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
    let select = &manifest["modules"][2];
    assert_eq!(select["path"], "components/input-select.ts");
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
}
