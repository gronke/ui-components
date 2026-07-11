//! Bakes the demo frontend into `$OUT_DIR/dist`, which `main.rs` embeds with
//! `include_dir!`: generates the web components from the Rust catalog,
//! vendors the npm dependencies from `web/package.json`, and compiles both
//! roots (`web/` and the generated one) in a single `web_modules::build`.

use std::path::PathBuf;

use web_modules::build::{build, BuildOptions};
use web_modules::vendor::specs_from_package_json;

fn main() {
    // Keep the catalog's inventory registrations linked into this build script.
    ui_components::link();

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let web = manifest.join("web");

    let generated = uic_codegen_web::WebCodegen::new(out.join("gen_web"))
        .manifest(true)
        .run()
        .expect("generate web components from the Rust catalog");

    // Browser deps come from web/package.json `dependencies` (import-map
    // entries auto-derived from each package.json).
    let specs = specs_from_package_json(&web.join("package.json"))
        .expect("read browser dependencies from web/package.json");

    build(&BuildOptions {
        specs: &specs,
        roots: &[web, generated.root],
        out: &out.join("dist"),
        // Document-relative importmap addresses: index.html always sits at
        // the site root, so the same baked dist serves the dev server, the
        // embedded binary and a GitHub project page under /<repo>/.
        mount: "./web_modules",
        html: "",
        template: None,
        processors: Default::default(),
        output: Default::default(),
    })
    .expect("build web-demo frontend");
}
