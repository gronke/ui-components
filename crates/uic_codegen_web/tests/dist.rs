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
    // Every registered component ships, the demo composition included
    // (ADR 0013; a dist skip-list would be a separate change).
    assert_eq!(
        root.components,
        vec![
            "app-root",
            "input-date",
            "input-date-range",
            "input-number",
            "input-select",
            "input-text",
            "input-textarea",
            "input-timezone"
        ]
    );
    root.root
}

#[test]
fn emits_the_npm_tree() {
    let root = dist();
    for file in [
        "components/app-root.js",
        "components/app-root.d.ts",
        "components/app-root.impl.js",
        "components/input-date.js",
        "components/input-date.d.ts",
        "components/input-date.impl.js",
        "components/input-date-range.js",
        "components/input-date-range.d.ts",
        "components/input-date-range.impl.js",
        "components/input-number.js",
        "components/input-number.d.ts",
        "components/input-number.impl.js",
        "components/input-select.js",
        "components/input-select.d.ts",
        "components/input-select.impl.js",
        "components/input-text.js",
        "components/input-text.d.ts",
        "components/input-textarea.js",
        "components/input-textarea.d.ts",
        "components/input-textarea.impl.js",
        "components/input-timezone.js",
        "components/input-timezone.d.ts",
        "components/input-timezone.impl.js",
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
    assert!(css.contains(".el-input-select"));
    assert!(css.contains(".el-input-select .input-back"));
    assert!(css.contains(".el-input-textarea"));

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
