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

use serde_json::json;

/// The TypeScript sources (`tui-worker.ts`, `client.ts`) — an extra root
/// for a consumer's `web_modules` build.
pub fn web_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
}

/// Emits the publish-ready `@gronke/uic-worker` npm tree: the compiled
/// worker and client plus a `package.json`. Returns the emitted module names.
pub fn npm_tree(out: &Path, version: &str) -> Result<Vec<String>, String> {
    let root = web_root();
    uic_npm::emit_tree(
        &uic_npm::TreeSpec {
            web_root: &root,
            name: "@gronke/uic-worker",
            version,
            description: "The browser worker host for foreign lit elements on the ui-components terminal runtime",
            exports: json!({
                ".": "./client.js",
                "./client.js": "./client.js",
                "./tui-worker.js": "./tui-worker.js"
            }),
            peer_dependencies: None,
        },
        out,
    )
}

/// Compiles the mocked-lit runtime (`uic_js`'s `js/src`, handed in as
/// `src_root`) into a served module tree under `out`: the same per-module
/// TypeScript compile the Boa host bakes, shipped as files so the browser's
/// own engine — in a worker — imports them natively. A build script gets the
/// `rerun-if-changed` for free.
pub fn worker_runtime_tree(src_root: &Path, out: &Path) {
    println!("cargo:rerun-if-changed={}", src_root.display());
    compile_worker_tree(src_root, src_root, out);
}

fn compile_worker_tree(root: &Path, dir: &Path, out_root: &Path) {
    for entry in fs::read_dir(dir).expect("read js/src").flatten() {
        let path = entry.path();
        if path.is_dir() {
            compile_worker_tree(root, &path, out_root);
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !name.ends_with(".ts") || name.ends_with(".d.ts") {
            continue;
        }
        let relative = path.strip_prefix(root).expect("under js/src");
        let specifier = relative
            .to_string_lossy()
            .trim_end_matches(".ts")
            .to_string()
            + ".js";
        let source = fs::read_to_string(&path).expect("read runtime module");
        let compiled = web_modules::typescript::compile_str(&source, relative)
            .unwrap_or_else(|err| panic!("compile {relative:?}: {err}"));
        let target = out_root.join(&specifier);
        fs::create_dir_all(target.parent().expect("module dir")).expect("worker module dir");
        fs::write(&target, compiled).expect("write worker module");
    }
}

/// The bare specifier families the mocked runtime provides at the worker
/// tree's root — a vendored component's imports of these rewrite to relative
/// paths, since import maps do not reach workers.
const MOCK_FAMILIES: &[&str] = &["lit", "lit-html", "lit-element", "@lit/"];

/// Copies a vendored foreign package into `out` with its bare `lit*` imports
/// rewritten to relative paths into the worker tree. `base_depth` is the
/// package's nesting under the worker modules root.
pub fn rewrite_foreign_package(package_root: &Path, out: &Path, base_depth: usize) {
    copy_rewritten(package_root, package_root, out, base_depth);
}

/// Rewrites a dist module's bare `lit*` imports to relative paths into the
/// worker tree. The grammar is the finite quoted-specifier set of ES modules
/// (`from "…"`, `import "…"`, `import("…")`); web_modules' AST readers are not
/// public yet (upstream proposal pending), and the browser's own resolution
/// fails loudly on anything this misses.
fn rewrite_bare_imports(source: &str, depth: usize) -> String {
    let up = "../".repeat(depth);
    let mut out = source.to_string();
    for family in MOCK_FAMILIES {
        let family = family.trim_end_matches('/');
        for quote in ['"', '\''] {
            for lead in ["from", "import"] {
                // `from"lit"`, `from "lit/x.js"`, `import("lit")`, with or
                // without whitespace and the call parenthesis.
                for spacer in ["", " ", "("] {
                    let needle = format!("{lead}{spacer}{quote}{family}");
                    let replacement = format!("{lead}{spacer}{quote}{up}{family}");
                    out = out.replace(&needle, &replacement);
                }
            }
        }
    }
    // An extension-less rewritten family entry points at its module file.
    for family in MOCK_FAMILIES {
        let family = family.trim_end_matches('/');
        for quote in ['"', '\''] {
            let bare = format!("{up}{family}{quote}");
            let file = format!("{up}{family}.js{quote}");
            out = out.replace(&bare, &file);
        }
    }
    out
}

fn copy_rewritten(package_root: &Path, dir: &Path, out_root: &Path, base_depth: usize) {
    for entry in fs::read_dir(dir).expect("read vendored tree").flatten() {
        let path = entry.path();
        let relative = path.strip_prefix(package_root).expect("under package");
        if path.is_dir() {
            copy_rewritten(package_root, &path, out_root, base_depth);
            continue;
        }
        let target = out_root.join(relative);
        fs::create_dir_all(target.parent().expect("module dir")).expect("worker package dir");
        if path.extension().and_then(|extension| extension.to_str()) == Some("js") {
            let depth = base_depth + relative.components().count() - 1;
            let source = fs::read_to_string(&path).expect("read vendored module");
            fs::write(&target, rewrite_bare_imports(&source, depth))
                .expect("write rewritten module");
        } else {
            fs::copy(&path, &target).expect("copy vendored file");
        }
    }
}
