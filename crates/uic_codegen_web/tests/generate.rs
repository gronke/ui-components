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
    assert_eq!(root.components, vec!["input-date"]);
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
        "components/uic-runtime.ts",
        "elements.scss",
        "custom-elements.json",
    ] {
        assert!(root.join(file).exists(), "missing {file}");
    }

    let elements = fs::read_to_string(root.join("elements.scss")).unwrap();
    assert!(elements.contains("@use \"components/input-date\";"));

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
fn generated_typescript_transpiles_with_oxc() {
    let root = generate("oxc");
    for file in ["components/input-date.ts", "components/uic-runtime.ts"] {
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
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["name"], "value-changed");
}
