//! The browser worker host as a reusable artifact (ADR 0007): the worker
//! that runs a foreign lit element on the browser's own engine against
//! `uic_tui_web::DomSession`, and the page-side client whose session
//! surface matches the wasm sessions'.
//!
//! Consumers integrate one of two ways: hand [`web_root`] to a
//! `web_modules` build as an extra source root (the demo's path), or emit
//! the compiled npm tree with [`npm_tree`] and install it like any package.

use std::fs;
use std::path::{Path, PathBuf};

use npm_utils::package_json::manifest::{self, remove_field, set_field};
use serde_json::json;

/// The TypeScript sources (`tui-worker.ts`, `client.ts`) — an extra root
/// for a consumer's `web_modules` build.
pub fn web_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
}

/// Emits the publish-ready npm tree: the compiled worker and client plus a
/// `package.json`. Returns the emitted module names.
pub fn npm_tree(out: &Path, version: &str) -> Result<Vec<String>, String> {
    fs::create_dir_all(out).map_err(|err| err.to_string())?;
    let mut modules = Vec::new();
    for entry in fs::read_dir(web_root())
        .map_err(|err| err.to_string())?
        .flatten()
    {
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !name.ends_with(".ts") || name.ends_with(".d.ts") {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|err| err.to_string())?;
        let relative = Path::new(&name);
        let compiled = web_modules::typescript::compile_str(&source, relative)
            .map_err(|err| format!("compile {name}: {err}"))?;
        let module = name.trim_end_matches(".ts").to_string() + ".js";
        fs::write(out.join(&module), compiled).map_err(|err| err.to_string())?;
        modules.push(module);
    }
    modules.sort();
    let mut doc = manifest::scaffold("@schuhkarton/uic-worker", version);
    // A published tree has no dependencies; drop scaffold's empty object.
    remove_field(&mut doc, "dependencies");
    set_field(
        &mut doc,
        "description",
        json!("The browser worker host for foreign lit elements on the ui-components terminal runtime"),
    );
    set_field(&mut doc, "license", json!("MIT"));
    set_field(&mut doc, "type", json!("module"));
    set_field(
        &mut doc,
        "exports",
        json!({
            ".": "./client.js",
            "./client.js": "./client.js",
            "./tui-worker.js": "./tui-worker.js"
        }),
    );
    set_field(&mut doc, "files", json!(modules));
    fs::write(out.join("package.json"), manifest::to_pretty(&doc))
        .map_err(|err| err.to_string())?;
    Ok(modules)
}
