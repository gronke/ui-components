//! Builds the publish-ready npm tree into `dist/npm/`: compiled lit ESM
//! components + declarations, elements.css, the Custom Elements Manifest and
//! a package.json with lit as peer dependency.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui_components::link();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/npm");
    let dist =
        uic_codegen_web::DistBuild::new(out, "@gronke/ui-components", env!("CARGO_PKG_VERSION"))
            .repository(env!("CARGO_PKG_REPOSITORY"))
            .extra_module("uic-connectors.ts", ui_components::connect::WEB_TS)
            .extra_module("uic-icons.ts", uic_icons::WEB_TS)
            .run()?;
    println!("npm package tree: {}", dist.root.display());
    for tag in dist.components {
        println!("  <{tag}>");
    }

    // The worker host publishes beside the components (ADR 0007).
    let worker_out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/npm-worker");
    let modules = uic_worker::npm_tree(&worker_out, env!("CARGO_PKG_VERSION"))?;
    println!("worker host tree: {}", worker_out.display());
    for module in modules {
        println!("  {module}");
    }

    // The sync tooling publishes beside them (ADR 0013).
    let sync_out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/npm-sync");
    let modules = uic_sync::npm_tree(&sync_out, env!("CARGO_PKG_VERSION"))?;
    println!("sync tooling tree: {}", sync_out.display());
    for module in modules {
        println!("  {module}");
    }
    Ok(())
}
