//! The in-memory module loader: the build-compiled runtime modules (the
//! mocked `lit` and friends, generated table below) plus whatever the host
//! registers — vendored component dists, test modules. Relative specifiers
//! resolve against the referrer's path.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use boa_engine::module::{ModuleLoader, Referrer};
use boa_engine::{Context, JsNativeError, JsResult, JsString, Module, Source};

include!(concat!(env!("OUT_DIR"), "/compiled_modules.rs"));

pub(crate) struct MapLoader {
    sources: RefCell<HashMap<String, String>>,
    modules: RefCell<HashMap<String, Module>>,
}

impl MapLoader {
    pub(crate) fn new() -> Self {
        let mut sources = HashMap::new();
        for (specifier, source) in COMPILED_MODULES {
            sources.insert((*specifier).to_string(), (*source).to_string());
        }
        MapLoader {
            sources: RefCell::new(sources),
            modules: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn insert(&self, specifier: &str, source: &str) {
        let specifier = normalize(None, specifier);
        self.sources
            .borrow_mut()
            .insert(specifier, source.to_string());
    }

    pub(crate) fn resolve(&self, specifier: &str, context: &mut Context) -> JsResult<Module> {
        self.resolve_from(None, specifier, context)
    }

    fn resolve_from(
        &self,
        referrer: Option<&str>,
        specifier: &str,
        context: &mut Context,
    ) -> JsResult<Module> {
        let specifier = normalize(referrer, specifier);
        if let Some(module) = self.modules.borrow().get(&specifier) {
            return Ok(module.clone());
        }
        let sources = self.sources.borrow();
        let source = sources.get(&specifier).ok_or_else(|| {
            JsNativeError::error().with_message(format!("unknown module specifier {specifier:?}"))
        })?;
        let module = Module::parse(
            Source::from_bytes(source.as_bytes()).with_path(std::path::Path::new(&specifier)),
            None,
            context,
        )?;
        self.modules
            .borrow_mut()
            .insert(specifier.clone(), module.clone());
        Ok(module)
    }
}

impl ModuleLoader for MapLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let specifier = specifier.to_std_string_escaped();
        let referrer_path = referrer
            .path()
            .map(|path| path.to_string_lossy().to_string());
        let mut guard = context.borrow_mut();
        self.resolve_from(referrer_path.as_deref(), &specifier, &mut guard)
    }
}

/// Joins `./` and `../` segments against the referrer's directory; bare
/// specifiers pass through.
fn normalize(referrer: Option<&str>, specifier: &str) -> String {
    if !specifier.starts_with('.') {
        return specifier.to_string();
    }
    let base = referrer
        .and_then(|path| path.rsplit_once('/').map(|(dir, _)| dir))
        .unwrap_or("");
    let mut parts: Vec<&str> = if base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };
    for segment in specifier.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            segment => parts.push(segment),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn relative_specifiers_resolve_against_the_referrer() {
        assert_eq!(normalize(None, "lit"), "lit");
        assert_eq!(normalize(Some("lit.js"), "./runtime.js"), "runtime.js");
        assert_eq!(
            normalize(Some("lit/decorators.js"), "../runtime.js"),
            "runtime.js"
        );
        assert_eq!(
            normalize(Some("lit/directives/map.js"), "../../directives.js"),
            "directives.js"
        );
        assert_eq!(
            normalize(Some("json-viewer.js"), "./chunk-ABC.js"),
            "chunk-ABC.js"
        );
    }
}
