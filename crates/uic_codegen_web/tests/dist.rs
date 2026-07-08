//! Dist-build test: the publish-ready npm tree (feature `dist`).

use std::fs;
use std::path::PathBuf;

use uic_codegen_web::DistBuild;

fn dist() -> PathBuf {
    ui_components::link();
    let out = std::env::temp_dir().join(format!("uic-dist-{}", std::process::id()));
    let root = DistBuild::new(&out, "@schuhkarton/ui-components", "0.1.0")
        .repository("https://github.com/schuhkarton/ui-components")
        .run()
        .expect("dist build succeeds");
    assert_eq!(root.components, vec!["input-date", "input-text"]);
    root.root
}

#[test]
fn emits_the_npm_tree() {
    let root = dist();
    for file in [
        "components/input-date.js",
        "components/input-date.d.ts",
        "components/input-date.impl.js",
        "components/input-text.js",
        "components/input-text.d.ts",
        "components/uic-runtime.js",
        "components/uic-runtime.d.ts",
        "elements.css",
        "custom-elements.json",
        "package.json",
        "README.md",
    ] {
        assert!(root.join(file).exists(), "missing {file}");
    }

    // Plain ESM: bare lit import survives, no TypeScript syntax remains.
    let js = fs::read_to_string(root.join("components/input-text.js")).unwrap();
    assert!(js.contains("from \"lit\"") || js.contains("from 'lit'"));
    assert!(!js.contains(": string"));
    assert!(js.contains("customElements.define"));

    // Declarations describe the public surface.
    let dts = fs::read_to_string(root.join("components/input-text.d.ts")).unwrap();
    assert!(dts.contains("export declare class InputText"));
    assert!(dts.contains("value: string | null"));

    let css = fs::read_to_string(root.join("elements.css")).unwrap();
    assert!(css.contains(".el-input-default"));

    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("package.json")).unwrap()).unwrap();
    assert_eq!(package["name"], "@schuhkarton/ui-components");
    assert_eq!(package["type"], "module");
    assert_eq!(package["peerDependencies"]["lit"], "^3");
    assert_eq!(
        package["exports"]["./input-date.js"]["default"],
        "./components/input-date.js"
    );
    assert_eq!(package["customElements"], "custom-elements.json");

    // Publish metadata (the release workflow rehearses `npm publish`).
    assert_eq!(package["publishConfig"]["access"], "public");
    assert_eq!(
        package["repository"]["url"],
        "git+https://github.com/schuhkarton/ui-components.git"
    );
    assert_eq!(
        package["bugs"],
        "https://github.com/schuhkarton/ui-components/issues"
    );

    let readme = fs::read_to_string(root.join("README.md")).unwrap();
    assert!(readme.contains("# @schuhkarton/ui-components"));
    assert!(readme.contains("import '@schuhkarton/ui-components/input-date.js';"));
}
