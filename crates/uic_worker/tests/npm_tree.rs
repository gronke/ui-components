//! The npm tree emits compiled ESM and a coherent package manifest.

use std::fs;
use std::path::Path;

#[test]
fn the_npm_tree_is_publish_ready() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("npm-worker");
    let modules = uic_worker::npm_tree(&out, "0.0.0-test").expect("emit the npm tree");
    assert_eq!(modules, ["client.js", "tui-worker.js"]);

    // Type-stripped, ESM, and free of TypeScript syntax.
    let client = fs::read_to_string(out.join("client.js")).expect("read client.js");
    assert!(client.contains("export function connectWorkerSession"));
    assert!(!client.contains(": WorkerSession"));

    // The manifest names every module and enters through the client.
    let package = fs::read_to_string(out.join("package.json")).expect("read package.json");
    let manifest: serde_json::Value = serde_json::from_str(&package).expect("manifest is JSON");
    assert_eq!(manifest["name"], "@schuhkarton/uic-worker");
    assert_eq!(manifest["version"], "0.0.0-test");
    assert_eq!(manifest["exports"]["."], "./client.js");
    assert!(manifest.get("dependencies").is_none());
    assert_eq!(
        manifest["files"],
        serde_json::json!(["client.js", "tui-worker.js"])
    );
}
