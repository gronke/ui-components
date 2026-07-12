//! Dist-build test: the publish-ready npm tree (feature `dist`).

use std::fs;
use std::path::PathBuf;

use uic_codegen_web::DistBuild;

fn dist() -> (Vec<&'static str>, PathBuf) {
    ui_components::link();
    let out = std::env::temp_dir().join(format!("uic-dist-{}", std::process::id()));
    let root = DistBuild::new(&out, "@schuhkarton/ui-components", "0.1.0")
        .repository("https://github.com/schuhkarton/ui-components")
        .extra_module("uic-connectors.ts", ui_components::connect::WEB_TS)
        .run()
        .expect("dist build succeeds");
    // The publish view: catalog components ship, the demo composition stays
    // out (dist = false on app-root, ADR 0013). The exact catalog vector is
    // generate.rs territory — this suite asserts the npm-tree shape.
    assert!(root.components.contains(&"input-date"));
    assert!(
        !root.components.contains(&"app-root"),
        "app-root is dist = false"
    );
    (root.components.clone(), root.root)
}

#[test]
fn emits_the_npm_tree() {
    let (components, root) = dist();
    // Per component: the ESM module, its declarations, and the compiled
    // impl twin exactly when the definition carries one.
    for &tag in &components {
        for file in [
            format!("components/{tag}.js"),
            format!("components/{tag}.d.ts"),
        ] {
            assert!(root.join(&file).exists(), "missing {file}");
        }
        let def = uic_core::CustomElementRegistry::get(tag).expect("registered component");
        assert_eq!(
            root.join(format!("components/{tag}.impl.js")).exists(),
            def.web_impl.is_some(),
            "the impl twin of {tag} follows its definition"
        );
    }
    for file in [
        "components/uic-runtime.js",
        "components/uic-runtime.d.ts",
        "components/uic-impl-helpers.js",
        "components/uic-impl-helpers.d.ts",
        "components/uic-connectors.js",
        "components/uic-connectors.d.ts",
        "elements.css",
        "custom-elements.json",
        "package.json",
        "README.md",
    ] {
        assert!(root.join(file).exists(), "missing {file}");
    }
    // The withheld demo composition leaves no trace in the tree.
    assert!(!root.join("components/app-root.js").exists());
    assert!(!root.join("components/app-root.impl.js").exists());

    // Plain ESM: bare lit import survives, no TypeScript syntax remains.
    let js = fs::read_to_string(root.join("components/input-text.js")).unwrap();
    assert!(js.contains("from \"lit\"") || js.contains("from 'lit'"));
    assert!(!js.contains(": string"));
    assert!(js.contains("customElements.define"));

    // The Temporal import is type-only: erased from the generated class,
    // real only in the hand-written impl module (with its peer dependency).
    let date_js = fs::read_to_string(root.join("components/input-date.js")).unwrap();
    assert!(!date_js.contains("temporal-polyfill"));
    let date_impl = fs::read_to_string(root.join("components/input-date.impl.js")).unwrap();
    assert!(date_impl.contains("temporal-polyfill"));
    let date_dts = fs::read_to_string(root.join("components/input-date.d.ts")).unwrap();
    assert!(date_dts.contains("Temporal.ZonedDateTime | null"));

    // Declarations describe the public surface.
    let dts = fs::read_to_string(root.join("components/input-text.d.ts")).unwrap();
    assert!(dts.contains("export declare class InputText"));
    assert!(dts.contains("value: string | null"));

    // The SelectOption type survives into the declarations and the runtime
    // module; the generated select class stays free of the option markup.
    let select_dts = fs::read_to_string(root.join("components/input-select.d.ts")).unwrap();
    assert!(select_dts.contains("options: SelectOption[]"));
    let runtime_dts = fs::read_to_string(root.join("components/uic-runtime.d.ts")).unwrap();
    assert!(runtime_dts.contains("export type SelectOption"));
    let select_js = fs::read_to_string(root.join("components/input-select.js")).unwrap();
    assert!(select_js.contains("this.selectOptions.map"));

    let css = fs::read_to_string(root.join("elements.css")).unwrap();
    assert!(css.contains(".el-input-default"));
    // The catalog-parity state selectors reach the published stylesheet.
    assert!(css.contains(".el-input-default[error]"));
    assert!(css.contains(".el-input-default[seamless]"));
    assert!(css.contains(".el-input-number"));
    assert!(css.contains(".el-input-date-range .input-group-text"));
    assert!(css.contains(".el-input-select"));
    assert!(css.contains(".el-input-select .input-back"));
    assert!(css.contains(".el-input-suggestion .dropdown-menu"));
    assert!(css.contains(".el-input-textarea"));
    assert!(css.contains(".el-nav-tabs"));

    let package: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("package.json")).unwrap()).unwrap();
    assert_eq!(package["name"], "@schuhkarton/ui-components");
    assert_eq!(package["type"], "module");
    assert_eq!(package["peerDependencies"]["lit"], "^3");
    assert_eq!(package["peerDependencies"]["temporal-polyfill"], "^0.3");
    assert_eq!(
        package["exports"]["./input-date.js"]["default"],
        "./components/input-date.js"
    );
    // The connectors ship as their own public export (ADR 0014).
    assert_eq!(
        package["exports"]["./uic-connectors.js"]["default"],
        "./components/uic-connectors.js"
    );
    let connectors_dts = fs::read_to_string(root.join("components/uic-connectors.d.ts")).unwrap();
    assert!(connectors_dts.contains("QuerySource"));
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
