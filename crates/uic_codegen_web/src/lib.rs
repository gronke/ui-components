//! Web codegen backend: turns registered component definitions into readable
//! TypeScript (LitElement variant) plus SCSS partials and aggregator, laid out
//! as an extra root for `web_modules::build`.
//!
//! Generated root layout:
//!
//! ```text
//! <out>/
//! ├── components/<tag>.ts          one Lit class per component
//! ├── components/<tag>.impl.ts     co-located behavior partial, copied
//! ├── components/uic-runtime.ts    LitNotify port, emitted once
//! ├── components/_<tag>.scss       component stylesheet (grass partial)
//! ├── elements.scss                aggregator, compiled to /elements.css
//! └── custom-elements.json         optional Custom Elements Manifest
//! ```

mod manifest;
mod ts;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use uic_core::{ComponentDef, CustomElementRegistry, Notify, RegistryError};

/// Generates the web components root from every registered custom element;
/// call from a consumer `build.rs` after `ui_components::link()`.
pub struct WebCodegen {
    out: PathBuf,
    manifest: bool,
}

impl WebCodegen {
    pub fn new(out: impl Into<PathBuf>) -> Self {
        WebCodegen {
            out: out.into(),
            manifest: false,
        }
    }

    /// Also emit `custom-elements.json` (Custom Elements Manifest v1).
    pub fn manifest(mut self, on: bool) -> Self {
        self.manifest = on;
        self
    }

    /// Emits the full generated root, replacing previous contents.
    pub fn run(self) -> Result<GeneratedRoot, CodegenError> {
        CustomElementRegistry::assert_valid()?;
        let mut defs: Vec<&'static ComponentDef> = CustomElementRegistry::iter().collect();
        defs.sort_by_key(|def| def.tag_name);

        let components = self.out.join("components");
        if self.out.exists() {
            fs::remove_dir_all(&self.out)?;
        }
        fs::create_dir_all(&components)?;

        let mut scss_tags = Vec::new();
        let mut any_notify = false;
        for def in &defs {
            check_impl_exports(def)?;
            let class = ts::emit_component(def);
            fs::write(components.join(format!("{}.ts", def.tag_name)), class)?;
            if let Some(web_impl) = def.web_impl {
                fs::write(
                    components.join(format!("{}.impl.ts", def.tag_name)),
                    web_impl,
                )?;
            }
            if let Some(scss) = def.scss {
                fs::write(components.join(format!("_{}.scss", def.tag_name)), scss)?;
                scss_tags.push(def.tag_name);
            }
            any_notify |= def
                .properties
                .iter()
                .any(|p| !matches!(p.notify, Notify::No));
        }

        if any_notify {
            fs::write(components.join("uic-runtime.ts"), ts::RUNTIME_TS)?;
        }
        if !scss_tags.is_empty() {
            fs::write(self.out.join("elements.scss"), elements_scss(&scss_tags))?;
        }
        if self.manifest {
            fs::write(
                self.out.join("custom-elements.json"),
                manifest::custom_elements_json(&defs),
            )?;
        }

        Ok(GeneratedRoot {
            components: defs.iter().map(|def| def.tag_name).collect(),
            root: self.out,
        })
    }
}

/// The emitted tree, ready to pass to `web_modules::build` as an extra root.
pub struct GeneratedRoot {
    pub root: PathBuf,
    /// Tags in emission order.
    pub components: Vec<&'static str>,
}

impl GeneratedRoot {
    /// Module path of a component below the served root, e.g.
    /// `components/input-date.js`.
    pub fn module_path(&self, tag: &str) -> String {
        format!("components/{tag}.js")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(
        "<{tag}> references handlers or computed properties but has no web_impl_file; \
         add one next to the component"
    )]
    MissingWebImpl { tag: &'static str },
    #[error(
        "the web impl of <{tag}> is missing exported functions: {missing}; \
         expected `export function <name>(…)` for every handler and computed property"
    )]
    MissingImplExports { tag: &'static str, missing: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// `error_message` → `errorMessage` (JS member names of handlers/computed).
pub(crate) fn camel_case(rust_name: &str) -> String {
    let mut out = String::with_capacity(rust_name.len());
    let mut upper_next = false;
    for ch in rust_name.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Names a component's web impl must export: handlers and computed
/// properties, as their JS member names.
fn required_impl_exports(def: &ComponentDef) -> BTreeSet<String> {
    def.handlers
        .iter()
        .map(|h| camel_case(h.name))
        .chain(def.computed.iter().map(|c| camel_case(c)))
        .collect()
}

/// `export function <name>` / `export const <name>` names in a TS source.
fn exported_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim_start();
        for prefix in [
            "export function ",
            "export const ",
            "export async function ",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    names.insert(name);
                }
            }
        }
    }
    names
}

/// Fails fast when the impl partial does not cover every referenced hook.
fn check_impl_exports(def: &'static ComponentDef) -> Result<(), CodegenError> {
    let required = required_impl_exports(def);
    if required.is_empty() {
        return Ok(());
    }
    let Some(web_impl) = def.web_impl else {
        return Err(CodegenError::MissingWebImpl { tag: def.tag_name });
    };
    let exported = exported_names(web_impl);
    let missing: Vec<_> = required.difference(&exported).cloned().collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(CodegenError::MissingImplExports {
            tag: def.tag_name,
            missing: missing.join(", "),
        })
    }
}

fn elements_scss(tags: &[&str]) -> String {
    let mut out = String::from(
        "// GENERATED by ui_components (uic_codegen_web). DO NOT EDIT.\n\
         // Aggregates the per-component stylesheets into /elements.css.\n",
    );
    for tag in tags {
        out.push_str(&format!("@use \"components/{tag}\";\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case_maps_rust_names() {
        assert_eq!(camel_case("value"), "value");
        assert_eq!(camel_case("error_message"), "errorMessage");
        assert_eq!(camel_case("on_change"), "onChange");
    }

    #[test]
    fn exported_names_finds_functions_and_consts() {
        let src = "export function onChange(el: X, e: Event): void {}\n\
                   export async function load(el: X) {}\n\
                   export const placeholderText = (el: X) => '';\n\
                   function privateHelper() {}\n";
        let names = exported_names(src);
        assert!(names.contains("onChange"));
        assert!(names.contains("load"));
        assert!(names.contains("placeholderText"));
        assert!(!names.contains("privateHelper"));
    }
}
