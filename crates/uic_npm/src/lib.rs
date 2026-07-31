//! One npm-tree emitter, shared by the crates that publish compiled
//! TypeScript. [`emit_tree`] compiles every `*.ts` under a source root to
//! `*.js` and writes a dependency-free, publish-ready `package.json`.
//!
//! Three trees ride it — `@schuhkarton/uic-sync`, `@schuhkarton/uic-worker`
//! and the lit-demo's `@schuhkarton/lit-todo` — differing only in name,
//! description, exports and (the app alone) peer dependencies; the emitted
//! bytes are unchanged from the hand-written copies this replaced.

use std::fs;
use std::path::Path;

use npm_utils::package_json::manifest::{self, remove_field, set_field};
use serde_json::{json, Value};

/// A publish-ready npm tree to emit: which sources to compile, and the
/// `package.json` fields that vary per package.
pub struct TreeSpec<'a> {
    /// The directory whose `*.ts` sources compile into the tree; every
    /// `*.d.ts` is skipped.
    pub web_root: &'a Path,
    /// The npm package name.
    pub name: &'a str,
    /// The package version.
    pub version: &'a str,
    /// The `description` field.
    pub description: &'a str,
    /// The `exports` map, as the caller builds it (`json!({ … })`).
    pub exports: Value,
    /// The `peerDependencies` map, or `None` to omit the field.
    pub peer_dependencies: Option<Value>,
}

/// Compiles every `*.ts` under `spec.web_root` (skipping `*.d.ts`) into
/// `out`, writes the `package.json`, and returns the emitted module names
/// sorted. The tree is dependency-free — `scaffold`'s empty `dependencies`
/// is dropped — and the manifest fields land in one fixed order:
/// `description`, `license`, `type`, `exports`, an optional
/// `peerDependencies`, then `files`.
pub fn emit_tree(spec: &TreeSpec, out: &Path) -> Result<Vec<String>, String> {
    fs::create_dir_all(out).map_err(|err| err.to_string())?;
    let mut modules = Vec::new();
    for entry in fs::read_dir(spec.web_root)
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
        let compiled = web_modules::typescript::compile_str(&source, Path::new(&name))
            .map_err(|err| format!("compile {name}: {err}"))?;
        let module = name.trim_end_matches(".ts").to_string() + ".js";
        fs::write(out.join(&module), compiled).map_err(|err| err.to_string())?;
        modules.push(module);
    }
    modules.sort();

    let mut doc = manifest::scaffold(spec.name, spec.version);
    // A published tree carries no dependencies; drop scaffold's empty object.
    remove_field(&mut doc, "dependencies");
    set_field(&mut doc, "description", json!(spec.description));
    set_field(&mut doc, "license", json!("MIT"));
    set_field(&mut doc, "type", json!("module"));
    set_field(&mut doc, "exports", spec.exports.clone());
    if let Some(peers) = &spec.peer_dependencies {
        set_field(&mut doc, "peerDependencies", peers.clone());
    }
    set_field(&mut doc, "files", json!(modules));
    fs::write(out.join("package.json"), manifest::to_pretty(&doc))
        .map_err(|err| err.to_string())?;
    Ok(modules)
}
