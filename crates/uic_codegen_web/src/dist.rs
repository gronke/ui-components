//! The npm-distributable build: plain lit ESM web components produced by the
//! pure-Rust toolchain, ready for `npm publish`.
//!
//! Output tree:
//!
//! ```text
//! <out>/
//! ├── components/<tag>.js + <tag>.d.ts     compiled ESM + declarations
//! ├── components/<tag>.impl.js + .d.ts
//! ├── components/uic-runtime.js + .d.ts
//! ├── elements.css                          compiled component styles
//! ├── custom-elements.json                  Custom Elements Manifest
//! └── package.json                          type: module, lit as peer
//! ```
//!
//! The emitted modules import only the bare `lit` specifier — consumable via
//! any bundler, an import map, or a web_modules vendor tree.

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use crate::{CodegenError, WebCodegen};

pub struct DistBuild {
    out: PathBuf,
    package_name: String,
    version: String,
    repository: Option<String>,
}

pub struct DistRoot {
    pub root: PathBuf,
    /// Tags in emission order.
    pub components: Vec<&'static str>,
}

impl DistBuild {
    pub fn new(
        out: impl Into<PathBuf>,
        package_name: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        DistBuild {
            out: out.into(),
            package_name: package_name.into(),
            version: version.into(),
            repository: None,
        }
    }

    /// Source repository URL; fills the package's `repository`, `homepage`
    /// and `bugs` fields.
    pub fn repository(mut self, url: impl Into<String>) -> Self {
        self.repository = Some(url.into());
        self
    }

    pub fn run(self) -> Result<DistRoot, CodegenError> {
        // Stage the TypeScript tree next to the final output, then compile.
        let staging = self.out.with_extension("gen");
        let generated = WebCodegen::new(&staging).manifest(true).run()?;

        if self.out.exists() {
            fs::remove_dir_all(&self.out)?;
        }
        let components = self.out.join("components");
        fs::create_dir_all(&components)?;

        for entry in fs::read_dir(staging.join("components"))? {
            let path = entry?.path();
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("ts") => {
                    let source = fs::read_to_string(&path)?;
                    let stem = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .expect("generated names are UTF-8");
                    let js = web_modules::typescript::compile_str(&source, &path)
                        .map_err(|err| CodegenError::Dist(err.to_string()))?;
                    fs::write(components.join(format!("{stem}.js")), js)?;
                    let dts = web_modules::dts::emit_dts(&source, &path)
                        .map_err(|err| CodegenError::Dist(err.to_string()))?;
                    fs::write(components.join(format!("{stem}.d.ts")), dts)?;
                }
                _ => continue,
            }
        }

        let elements = staging.join("elements.scss");
        if elements.exists() {
            let css = web_modules::scss::compile_str(
                &fs::read_to_string(&elements)?,
                &[staging.as_path()],
            )
            .map_err(|err| CodegenError::Dist(err.to_string()))?;
            fs::write(self.out.join("elements.css"), css)?;
        }

        fs::copy(
            staging.join("custom-elements.json"),
            self.out.join("custom-elements.json"),
        )?;

        fs::write(
            self.out.join("package.json"),
            self.package_json(&generated.components),
        )?;
        fs::write(
            self.out.join("README.md"),
            self.readme(&generated.components),
        )?;

        fs::remove_dir_all(&staging)?;
        Ok(DistRoot {
            root: self.out,
            components: generated.components,
        })
    }

    fn package_json(&self, components: &[&str]) -> String {
        let mut exports = serde_json::Map::new();
        for tag in components {
            exports.insert(
                format!("./{tag}.js"),
                json!({
                    "types": format!("./components/{tag}.d.ts"),
                    "default": format!("./components/{tag}.js"),
                }),
            );
        }
        exports.insert("./elements.css".into(), json!("./elements.css"));
        exports.insert(
            "./custom-elements.json".into(),
            json!("./custom-elements.json"),
        );
        exports.insert("./package.json".into(), json!("./package.json"));

        let mut package = json!({
            "name": self.package_name,
            "version": self.version,
            "description": "Web components generated from Rust definitions (ui-components)",
            "license": "MIT",
            "keywords": ["web-components", "custom-elements", "lit", "esm"],
            "type": "module",
            // Importing a component registers it with customElements.
            "sideEffects": true,
            "customElements": "custom-elements.json",
            "exports": exports,
            "peerDependencies": { "lit": "^3" },
            // Scoped packages default to restricted on the public registry.
            "publishConfig": { "access": "public" },
        });
        if let Some(url) = &self.repository {
            let fields = package.as_object_mut().expect("package is an object");
            fields.insert(
                "repository".into(),
                json!({ "type": "git", "url": format!("git+{url}.git") }),
            );
            fields.insert("homepage".into(), json!(format!("{url}#readme")));
            fields.insert("bugs".into(), json!(format!("{url}/issues")));
        }
        let mut out = serde_json::to_string_pretty(&package).expect("package.json serializes");
        out.push('\n');
        out
    }

    /// The package README shown on the registry page.
    fn readme(&self, components: &[&str]) -> String {
        let name = &self.package_name;
        let first = components.first().copied().unwrap_or("input-date");
        let mut imports = String::new();
        for tag in components {
            imports.push_str(&format!("import '{name}/{tag}.js';\n"));
        }
        format!(
            "# {name}\n\n\
            Web components generated from Rust definitions — plain lit ESM, no build step.\n\
            Importing a module registers its element with `customElements`.\n\n\
            ```sh\nnpm install {name} lit\n```\n\n\
            ```js\n{imports}```\n\n\
            ```html\n<{first} label=\"Label\"></{first}>\n```\n\n\
            The elements render in light DOM and expect Bootstrap plus this package's \
            `{name}/elements.css` on the page.\n\
            Property changes fire LitNotify-style `<attribute>-changed` events.\n\
            `custom-elements.json` ships for editor and tooling integration.\n"
        )
    }
}
