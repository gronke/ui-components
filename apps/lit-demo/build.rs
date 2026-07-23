//! Bakes the one Lit app into both artifacts: the npm-shaped package tree
//! (`$OUT_DIR/npm`, which the Boa host loads like any installed package)
//! and the browser dist (`$OUT_DIR/dist`: vendored lit family, the same
//! compiled tree, the Tera-rendered page), which `main.rs` embeds.

use std::fs;
use std::path::{Path, PathBuf};

use npm_utils::package_json::manifest::{self, remove_field, set_field};
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

/// The compiled package tree, `uic_worker::npm_tree`'s shape: each
/// `web/src/*.ts` compiles beside a generated manifest, and both hosts
/// consume the result — `JsHost::load_package` natively, the browser build
/// as a source root.
fn npm_tree(src: &Path, out: &Path) {
    fs::create_dir_all(out).expect("create the npm tree");
    let mut modules = Vec::new();
    for entry in fs::read_dir(src).expect("read web/src").flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !name.ends_with(".ts") || name.ends_with(".d.ts") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read app module");
        let compiled = web_modules::typescript::compile_str(&source, Path::new(&name))
            .unwrap_or_else(|err| panic!("compile {name}: {err}"));
        let module = name.trim_end_matches(".ts").to_string() + ".js";
        fs::write(out.join(&module), compiled).expect("write compiled module");
        modules.push(module);
    }
    modules.sort();

    let mut doc = manifest::scaffold(PACKAGE, env!("CARGO_PKG_VERSION"));
    // The tree carries no dependencies of its own; lit stays a peer.
    remove_field(&mut doc, "dependencies");
    set_field(
        &mut doc,
        "description",
        json!("One hand-written Lit todo app, rendered by the browser and the ui-components terminal runtime alike"),
    );
    set_field(&mut doc, "license", json!("MIT"));
    set_field(&mut doc, "type", json!("module"));
    set_field(
        &mut doc,
        "exports",
        json!({
            ".": "./todo-app.js",
            "./todo-app.js": "./todo-app.js",
            "./todo-item.js": "./todo-item.js"
        }),
    );
    set_field(&mut doc, "peerDependencies", json!({ "lit": "^3" }));
    set_field(&mut doc, "files", json!(modules));
    fs::write(out.join("package.json"), manifest::to_pretty(&doc))
        .expect("write the tree manifest");
}
