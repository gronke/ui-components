//! Builds the publish-ready npm tree into `dist/npm/`: compiled lit ESM
//! components + declarations, elements.css, the Custom Elements Manifest and
//! a package.json with lit as peer dependency.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui_components::link();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/npm");
    let dist = uic_codegen_web::DistBuild::new(
        out,
        "@schuhkarton/ui-components",
        env!("CARGO_PKG_VERSION"),
    )
    .repository("https://github.com/schuhkarton/ui-components")
    .run()?;
    println!("npm package tree: {}", dist.root.display());
    for tag in dist.components {
        println!("  <{tag}>");
    }
    Ok(())
}
