//! Bakes the one Lit app into both artifacts: the npm-shaped package tree
//! (`$OUT_DIR/npm`, which the Boa host loads like any installed package)
//! and the browser dist (`$OUT_DIR/dist`: vendored lit family, the same
//! compiled tree, the Tera-rendered page), which `main.rs` embeds.

use std::path::{Path, PathBuf};

use serde_json::json;
use web_modules::build::{build, BuildOptions};
use web_modules::vendor::specs_from_package_json;

const PACKAGE: &str = "@schuhkarton/lit-todo";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=web");
    println!("cargo:rerun-if-changed={}", uic_sync::web_root().display());

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");

    let npm = out.join("npm");
    npm_tree(&web.join("src"), &npm.join(PACKAGE));
    // The sync tooling's tree lands beside the app's: the pages import both
    // under their package names from the shared npm root.
    uic_sync::npm_tree(
        &npm.join("@schuhkarton/uic-sync"),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("bake the sync tooling tree");
    println!("cargo:rustc-env=UIC_LIT_DEMO_NPM_ROOT={}", npm.display());

    // The browser dist: import-map entries derive from each vendored
    // package.json, the compiled app tree rides along as a root, and the
    // page's `{{ importmap }}` hole fills at render.
    let specs = specs_from_package_json(&web.join("package.json"))
        .expect("read browser dependencies from web/package.json");
    build(&BuildOptions {
        specs: &specs,
        roots: &[web.join("pages"), npm],
        out: &out.join("dist"),
        mount: "./web_modules",
        html: "",
        template: None,
        processors: Default::default(),
        output: Default::default(),
    })
    .expect("build the lit-todo frontend");
}

/// The compiled `@schuhkarton/lit-todo` tree: each `web/src/*.ts` compiles
/// beside a generated manifest, and both hosts consume the result —
/// `JsHost::load_package` natively, the browser build as a source root.
fn npm_tree(src: &Path, out: &Path) {
    uic_npm::emit_tree(
        &uic_npm::TreeSpec {
            web_root: src,
            name: PACKAGE,
            version: env!("CARGO_PKG_VERSION"),
            description: "One hand-written Lit todo app, rendered by the browser and the ui-components terminal runtime alike",
            exports: json!({
                ".": "./todo-app.js",
                "./todo-app.js": "./todo-app.js",
                "./todo-item.js": "./todo-item.js",
                "./p2p-deck.js": "./p2p-deck.js",
                "./theme.js": "./theme.js"
            }),
            peer_dependencies: Some(json!({ "lit": "^3" })),
        },
        out,
    )
    .expect("bake the lit-todo tree");
}
