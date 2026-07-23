//! The npm tree emits compiled ESM and a coherent package manifest.

use std::fs;
use std::path::Path;

#[test]
fn the_npm_tree_is_publish_ready() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("npm-sync");
    let modules = uic_sync::npm_tree(&out, "0.0.0-test").expect("emit the npm tree");
    assert_eq!(modules, ["codec.js", "pair.js", "sync.js", "wire.js"]);

    // Type-stripped, ESM, and free of TypeScript syntax.
    let sync = fs::read_to_string(out.join("sync.js")).expect("read sync.js");
    assert!(sync.contains("export function attach"));
    assert!(!sync.contains(": Wire"));
    let pair = fs::read_to_string(out.join("pair.js")).expect("read pair.js");
    assert!(pair.contains("export async function createHost"));
    assert!(pair.contains("export async function join"));

    // Intra-package imports stay relative, resolvable in the compiled tree.
    assert!(sync.contains("from \"./codec.js\"") || sync.contains("from './codec.js'"));

    // The manifest names every module and enters through sync.
    let package = fs::read_to_string(out.join("package.json")).expect("read package.json");
    let manifest: serde_json::Value = serde_json::from_str(&package).expect("manifest is JSON");
    assert_eq!(manifest["name"], "@schuhkarton/uic-sync");
    assert_eq!(manifest["version"], "0.0.0-test");
    assert_eq!(manifest["exports"]["."], "./sync.js");
    assert!(manifest.get("dependencies").is_none());
    assert_eq!(
        manifest["files"],
        serde_json::json!(["codec.js", "pair.js", "sync.js", "wire.js"])
    );
}
