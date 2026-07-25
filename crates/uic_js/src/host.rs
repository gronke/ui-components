//! The embedded engine: one Boa context, the module loader, and the entry
//! points the demos and tests drive — mount, property writes, key and click
//! delivery, focus.

use std::cell::RefCell;
use std::rc::Rc;

use boa_engine::{Context, JsValue, Source};
use uic_dom::NodeId;
use uic_tui::KeyStroke;

use crate::error::Error;
use crate::loader::MapLoader;
use crate::natives::register_natives;
use crate::state::{self, HostState};

/// The embedded engine plus the shared document state.
pub struct JsHost {
    context: Context,
    loader: Rc<MapLoader>,
    pub state: Rc<RefCell<HostState>>,
}

impl JsHost {
    pub fn new() -> Result<Self, Error> {
        let loader = Rc::new(MapLoader::new());
        let mut context = Context::builder()
            .module_loader(loader.clone())
            .build()
            .map_err(|err| Error::Js(err.to_string()))?;
        let state = Rc::new(RefCell::new(HostState::new()));
        state::install(state.clone());
        register_natives(&mut context)?;
        let mut host = JsHost {
            context,
            loader,
            state,
        };
        // The runtime's entry module publishes customElements and the
        // __uic* globals the host calls by name.
        host.load_registered("main.js")?;
        Ok(host)
    }

    /// Registers a module source under a specifier without evaluating it.
    pub fn register_module(&self, specifier: &str, source: &str) {
        self.loader.insert(specifier, source);
    }

    /// Registers every `.js` module of a vendored dist tree — subdirectories
    /// included, each under its dist-root-relative path — and loads the
    /// entry (itself possibly a subpath): the byte-unmodified npm package
    /// enters the engine here.
    pub fn load_dist_dir(&mut self, dir: &std::path::Path, entry: &str) -> Result<(), Error> {
        self.register_dist_tree(dir, dir)?;
        self.load_registered(entry)
    }

    fn register_dist_tree(
        &self,
        root: &std::path::Path,
        dir: &std::path::Path,
    ) -> Result<(), Error> {
        let entries = std::fs::read_dir(dir).map_err(|err| Error::Js(err.to_string()))?;
        for file in entries.flatten() {
            let path = file.path();
            if path.is_dir() {
                self.register_dist_tree(root, &path)?;
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("js") {
                continue;
            }
            let specifier = path
                .strip_prefix(root)
                .map_err(|err| Error::Js(err.to_string()))?
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let source =
                std::fs::read_to_string(&path).map_err(|err| Error::Js(err.to_string()))?;
            self.loader.insert(&specifier, &source);
        }
        Ok(())
    }

    /// Loads a vendored npm package by name: the ESM entry derives from its
    /// own manifest (`exports` "." → `module` → `main`), the package tree
    /// registers path-preserving, and the entry evaluates — any lit element
    /// enters the engine here, no per-package knowledge required.
    pub fn load_package(
        &mut self,
        vendor_root: &std::path::Path,
        package: &str,
    ) -> Result<(), Error> {
        let root = vendor_root.join(package);
        let manifest_path = root.join("package.json");
        let manifest = std::fs::read_to_string(&manifest_path)
            .map_err(|err| Error::Js(format!("read {}: {err}", manifest_path.display())))?;
        let entry = package_entry(&manifest).ok_or_else(|| {
            Error::Js(format!(
                "{package}: no ESM entry in package.json (exports \".\", module, main)"
            ))
        })?;
        self.load_dist_dir(&root, &entry)
    }

    /// Registers, links and evaluates a module (a component definition).
    pub fn load_module(&mut self, specifier: &str, source: &str) -> Result<(), Error> {
        self.loader.insert(specifier, source);
        self.load_registered(specifier)
    }

    /// Links and evaluates an already-registered module.
    fn load_registered(&mut self, specifier: &str) -> Result<(), Error> {
        let module = self
            .loader
            .resolve(specifier, &mut self.context)
            .map_err(|err| Error::Js(err.to_string()))?;
        let promise = module.load_link_evaluate(&mut self.context);
        self.context.run_jobs()?;
        match promise.state() {
            boa_engine::builtins::promise::PromiseState::Fulfilled(_) => Ok(()),
            boa_engine::builtins::promise::PromiseState::Rejected(err) => {
                Err(Error::Js(err.display().to_string()))
            }
            boa_engine::builtins::promise::PromiseState::Pending => {
                Err(Error::Js(format!("module {specifier:?} did not settle")))
            }
        }
    }

    /// Creates the element node with its markup attributes, instantiates the
    /// registered class over it, and runs the resulting update jobs.
    pub fn mount(&mut self, tag: &str, attrs: &[(&str, &str)]) -> Result<NodeId, Error> {
        let (node, handle) = {
            let mut state = self.state.borrow_mut();
            let handle = state.create_root(tag, attrs);
            let node = state.node(handle).expect("freshly created root");
            (node, handle)
        };
        self.eval(&format!("__uicMount({tag:?}, {handle})"))?;
        self.run_jobs()?;
        Ok(node)
    }

    /// Writes a component property through the mocked accessor (JSON value).
    pub fn set_prop(&mut self, node: NodeId, name: &str, json: &str) -> Result<(), Error> {
        let handle = self.state.borrow_mut().handle(node);
        self.eval(&format!("__uicSetProp({handle}, {name:?}, {json})"))?;
        self.run_jobs()?;
        Ok(())
    }

    /// Reads a component property through the mocked accessor as JSON text —
    /// the outbound mirror of [`set_prop`](Self::set_prop).
    pub fn prop_json(&mut self, node: NodeId, name: &str) -> Result<String, Error> {
        let handle = self.state.borrow_mut().handle(node);
        let value = self.eval(&format!("JSON.stringify(__uicGetProp({handle}, {name:?}))"))?;
        value
            .as_string()
            .map(|text| text.to_std_string_escaped())
            .ok_or_else(|| Error::Js(format!("property {name:?} did not serialize to JSON")))
    }

    /// Moves the DOM focus (dispatching focusout/focusin) and runs the jobs.
    pub fn focus(&mut self, node: NodeId) -> Result<(), Error> {
        let handle = self.state.borrow_mut().handle(node);
        self.eval(&format!("__uicFocus({handle})"))?;
        self.run_jobs()
    }

    /// Delivers a keydown to the focused node; returns defaultPrevented.
    pub fn dispatch_key(&mut self, key: &str) -> Result<bool, Error> {
        self.dispatch(&KeyStroke::new(key))
    }

    /// Delivers a keydown carrying the shift modifier state.
    pub fn dispatch_key_shift(&mut self, key: &str, shift: bool) -> Result<bool, Error> {
        let mut stroke = KeyStroke::new(key);
        stroke.shift = shift;
        self.dispatch(&stroke)
    }

    /// Delivers a keydown in the shared vocabulary (`uic_tui::keys`) —
    /// every modifier flag reaches the runtime's event. An uncancelled
    /// keydown then runs the focused widget as the browser's editing
    /// default action; a text change synthesizes the bubbling `input`,
    /// whose listeners read the live text through `event.target.value`.
    pub fn dispatch(&mut self, stroke: &KeyStroke) -> Result<bool, Error> {
        let Some(focused) = self.state.borrow().focused else {
            return Ok(false);
        };
        let handle = self.state.borrow_mut().handle(focused);
        let KeyStroke {
            key,
            shift,
            ctrl,
            alt,
            meta,
        } = stroke;
        let prevented = self.eval(&format!(
            "__uicDeliver({handle}, 'keydown', {{ key: {key:?}, shiftKey: {shift}, ctrlKey: {ctrl}, altKey: {alt}, metaKey: {meta} }})"
        ))?;
        let prevented = prevented.as_boolean().unwrap_or(false);
        if !prevented {
            let changed = self
                .state
                .borrow_mut()
                .widget_default_action(stroke)
                .is_some();
            if changed {
                self.eval(&format!("__uicDeliver({handle}, 'input', {{}})"))?;
            }
        }
        self.run_jobs()?;
        Ok(prevented)
    }

    /// Delivers a bubbling click at the node — the pointer entry after a
    /// `uic_tui::dom::hit_test`.
    pub fn click(&mut self, node: NodeId) -> Result<(), Error> {
        let handle = self.state.borrow_mut().handle(node);
        self.eval(&format!("__uicDeliver({handle}, 'click', {{}})"))?;
        self.run_jobs()
    }

    /// The pointer entry with the cell it landed on: a widget node takes
    /// focus first and the caret drops under the pointer — the browser's
    /// click-into-an-input semantics — then the bubbling click delivers.
    pub fn click_at(&mut self, node: NodeId, column: u16, row: u16) -> Result<(), Error> {
        let (handle, widgeted) = {
            let mut state = self.state.borrow_mut();
            let handle = state.handle(node);
            (handle, state.has_widget(handle))
        };
        if widgeted {
            self.eval(&format!("__uicFocus({handle})"))?;
            self.state.borrow_mut().place_caret(handle, column, row);
        }
        self.eval(&format!("__uicDeliver({handle}, 'click', {{}})"))?;
        self.run_jobs()
    }

    /// Delivers a bubbling double click — hosts with a clock synthesize it
    /// after two quick clicks on one node, the browser's own order.
    pub fn dblclick(&mut self, node: NodeId) -> Result<(), Error> {
        let handle = self.state.borrow_mut().handle(node);
        self.eval(&format!("__uicDeliver({handle}, 'dblclick', {{}})"))?;
        self.run_jobs()
    }

    pub fn eval(&mut self, source: &str) -> Result<JsValue, Error> {
        Ok(self.context.eval(Source::from_bytes(source.as_bytes()))?)
    }

    /// Drains the microtask queue — lit schedules updates on it.
    pub fn run_jobs(&mut self) -> Result<(), Error> {
        self.context.run_jobs()?;
        Ok(())
    }
}

/// The package's ESM entry per its manifest: `exports` "." (conditions
/// `import`/`module`/`default`, nested), then `module`, then `main` —
/// normalized without the leading `./`.
fn package_entry(manifest: &str) -> Option<String> {
    fn export_target(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(path) => Some(path.clone()),
            serde_json::Value::Object(conditions) => ["import", "module", "default"]
                .iter()
                .find_map(|key| conditions.get(*key).and_then(export_target)),
            _ => None,
        }
    }
    let json: serde_json::Value = serde_json::from_str(manifest).ok()?;
    let from_exports = json.get("exports").and_then(|exports| match exports {
        serde_json::Value::String(_) => export_target(exports),
        serde_json::Value::Object(map) => match map.get(".") {
            Some(dot) => export_target(dot),
            None => export_target(exports),
        },
        _ => None,
    });
    from_exports
        .or_else(|| {
            json.get("module")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .or_else(|| {
            json.get("main")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .map(|entry| entry.trim_start_matches("./").to_string())
}

#[cfg(test)]
mod entry_tests {
    use super::package_entry;

    #[test]
    fn entries_derive_from_exports_module_and_main() {
        assert_eq!(
            package_entry(r#"{"exports": {".": {"import": "./dist/x.js"}}}"#).as_deref(),
            Some("dist/x.js")
        );
        assert_eq!(
            package_entry(r#"{"exports": "./index.js"}"#).as_deref(),
            Some("index.js")
        );
        assert_eq!(
            package_entry(r#"{"exports": {"import": "./esm/y.js"}}"#).as_deref(),
            Some("esm/y.js")
        );
        assert_eq!(
            package_entry(r#"{"module": "./m.js", "main": "./c.js"}"#).as_deref(),
            Some("m.js")
        );
        assert_eq!(
            package_entry(r#"{"main": "c.js"}"#).as_deref(),
            Some("c.js")
        );
        assert_eq!(package_entry(r#"{"name": "bare"}"#), None);
    }
}
