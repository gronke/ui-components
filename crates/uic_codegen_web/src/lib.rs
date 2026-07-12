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
//! ├── components/uic-*.ts          extra shared modules (the connectors)
//! ├── components/_<tag>.scss       component stylesheet (grass partial)
//! ├── elements.scss                aggregator, compiled to /elements.css
//! └── custom-elements.json         optional Custom Elements Manifest
//! ```

#[cfg(feature = "dist")]
mod dist;
mod manifest;
mod ts;

#[cfg(feature = "dist")]
pub use dist::{DistBuild, DistRoot};

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use uic_core::{ComponentDef, CustomElementRegistry, Notify, RegistryError};

/// Generates the web components root from every registered custom element;
/// call from a consumer `build.rs` after `ui_components::link()`.
pub struct WebCodegen {
    out: PathBuf,
    manifest: bool,
    dist_only: bool,
    extra_modules: Vec<(String, &'static str)>,
}

impl WebCodegen {
    pub fn new(out: impl Into<PathBuf>) -> Self {
        WebCodegen {
            out: out.into(),
            manifest: false,
            dist_only: false,
            extra_modules: Vec::new(),
        }
    }

    /// Also emit `custom-elements.json` (Custom Elements Manifest v1).
    pub fn manifest(mut self, on: bool) -> Self {
        self.manifest = on;
        self
    }

    /// Emit only the components with `dist = true` — the publish view.
    /// The default (everything) is the dev-server view.
    pub fn dist_only(mut self, on: bool) -> Self {
        self.dist_only = on;
        self
    }

    /// Emits an additional hand-written module into `components/` — shared
    /// TS twins that are not component partials, like the data connectors
    /// (`ui_components::connect::WEB_TS`, ADR 0014).
    pub fn extra_module(mut self, file_name: impl Into<String>, source: &'static str) -> Self {
        self.extra_modules.push((file_name.into(), source));
        self
    }

    /// Emits the full generated root, replacing previous contents.
    pub fn run(self) -> Result<GeneratedRoot, CodegenError> {
        CustomElementRegistry::assert_valid()?;
        let mut defs: Vec<&'static ComponentDef> = CustomElementRegistry::iter().collect();
        defs.sort_by_key(|def| def.tag_name);
        if self.dist_only {
            defs.retain(|def| def.dist);
            // A shipped component must not import a withheld child: the
            // generated module of a nested registered element imports it.
            for def in &defs {
                for child in def.template().custom_tags() {
                    if let Some(child_def) = CustomElementRegistry::get(child) {
                        if !child_def.dist {
                            return Err(CodegenError::DistBoundary {
                                tag: def.tag_name,
                                child: child_def.tag_name,
                            });
                        }
                    }
                }
            }
        }

        let components = self.out.join("components");
        if self.out.exists() {
            fs::remove_dir_all(&self.out)?;
        }
        fs::create_dir_all(&components)?;

        let mut scss_names = Vec::new();
        let mut shared_scss: Vec<(&'static str, &'static str, &'static str)> = Vec::new();
        let mut any_runtime = false;
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
            if let (Some(id), Some(scss)) = (def.shared_style_id, def.shared_scss) {
                match shared_scss.iter().find(|(known, _, _)| *known == id) {
                    None => shared_scss.push((id, scss, def.tag_name)),
                    Some((_, known, first)) if *known != scss => {
                        return Err(CodegenError::SharedScssConflict {
                            id,
                            first,
                            second: def.tag_name,
                        });
                    }
                    Some(_) => {}
                }
            }
            if let Some(scss) = def.scss {
                fs::write(components.join(format!("_{}.scss", def.tag_name)), scss)?;
                scss_names.push(def.tag_name);
            }
            // The runtime module carries the notify helper and the
            // SelectOption type; either use pulls it in.
            any_runtime |= def
                .properties
                .iter()
                .any(|p| !matches!(p.notify, Notify::No) || p.js_type == uic_core::JsType::Options);
        }

        // Shared stylesheets come first, so component styles can override.
        let mut use_names: Vec<&str> = Vec::new();
        for (id, scss, _) in &shared_scss {
            fs::write(components.join(format!("_{id}.scss")), scss)?;
            use_names.push(id);
        }
        use_names.extend(scss_names);

        if any_runtime {
            fs::write(components.join("uic-runtime.ts"), ts::RUNTIME_TS)?;
        }
        // The impl partials' shared helpers ride along whenever a partial
        // ships.
        if defs.iter().any(|def| def.web_impl.is_some()) {
            fs::write(
                components.join("uic-impl-helpers.ts"),
                include_str!("uic-impl-helpers.ts"),
            )?;
        }
        for (name, source) in &self.extra_modules {
            fs::write(components.join(name), source)?;
        }
        if !use_names.is_empty() {
            fs::write(self.out.join("elements.scss"), elements_scss(&use_names))?;
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
    #[error(
        "the web impl of <{tag}> has mismatched signatures: {detail}; \
         handlers take (el, event), computed properties (el)"
    )]
    ImplArity { tag: &'static str, detail: String },
    #[error(
        "shared style '{id}' has differing shared_scss sources: <{first}> and <{second}> \
         must include the same file"
    )]
    SharedScssConflict {
        id: &'static str,
        first: &'static str,
        second: &'static str,
    },
    #[error(
        "<{tag}> ships in the dist but nests <{child}>, which is dist = false; \
         both ship or neither"
    )]
    DistBoundary {
        tag: &'static str,
        child: &'static str,
    },
    #[cfg(feature = "dist")]
    #[error("dist: {0}")]
    Dist(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// JS member names of handlers/computed follow the shared naming rules.
pub(crate) use uic_template::names::camel_case;

/// Names a component's web impl must export: handlers and computed
/// properties, as their JS member names.
fn required_impl_exports(def: &ComponentDef) -> BTreeSet<String> {
    def.handlers
        .iter()
        .map(|h| camel_case(h.name))
        .chain(def.computed.iter().map(|c| camel_case(c)))
        .collect()
}

/// The parameter count of a one-line `export function <name>(…)`
/// signature; `None` for consts, multi-line signatures and anything the
/// heuristic cannot read — those stay name-checked only.
pub(crate) fn exported_arity(source: &str, name: &str) -> Option<usize> {
    for line in source.lines() {
        let line = line.trim_start();
        let Some(rest) = ["export function ", "export async function "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
        else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(name) else {
            continue;
        };
        let Some(params) = rest.strip_prefix('(') else {
            continue;
        };
        let end = params.find(')')?;
        let params = &params[..end];
        if params.trim().is_empty() {
            return Some(0);
        }
        // Commas inside generics or nested types do not separate
        // parameters (Map<PropertyKey, unknown>).
        let mut depth = 0usize;
        let mut count = 1usize;
        for ch in params.chars() {
            match ch {
                '<' | '(' | '[' | '{' => depth += 1,
                '>' | ')' | ']' | '}' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => count += 1,
                _ => {}
            }
        }
        return Some(count);
    }
    None
}

/// `export function <name>` / `export const <name>` names in a TS source.
pub(crate) fn exported_names(source: &str) -> BTreeSet<String> {
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
    if !missing.is_empty() {
        return Err(CodegenError::MissingImplExports {
            tag: def.tag_name,
            missing: missing.join(", "),
        });
    }
    // The names cover presence; simple function signatures also pin the
    // arity — handlers take (el, event), computed properties (el).
    let mut wrong = Vec::new();
    let mut check = |name: String, expected: usize| {
        if let Some(found) = exported_arity(web_impl, &name) {
            if found != expected {
                wrong.push(format!(
                    "{name} takes {found} parameters, expected {expected}"
                ));
            }
        }
    };
    for handler in def.handlers {
        check(camel_case(handler.name), 2);
    }
    for computed in def.computed {
        check(camel_case(computed), 1);
    }
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(CodegenError::ImplArity {
            tag: def.tag_name,
            detail: wrong.join("; "),
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
    fn exported_arity_reads_simple_signatures() {
        let src = "export function none(): void {}\n\
                   export function one(el: X): string { return ''; }\n\
                   export function two(el: X, e: Event): void {}\n\
                   export function generic(el: X, changed: Map<PropertyKey, unknown>): void {}\n\
                   export const asConst = (el: X) => '';\n";
        assert_eq!(exported_arity(src, "none"), Some(0));
        assert_eq!(exported_arity(src, "one"), Some(1));
        assert_eq!(exported_arity(src, "two"), Some(2));
        assert_eq!(exported_arity(src, "generic"), Some(2));
        assert_eq!(
            exported_arity(src, "asConst"),
            None,
            "consts stay name-only"
        );
        assert_eq!(exported_arity(src, "absent"), None);
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
