//! The embedded engine: one Boa context, the module loader, and the entry
//! points the demos and tests drive — mount, property writes, key and click
//! delivery, focus.

use std::cell::RefCell;
use std::rc::Rc;

use boa_engine::{Context, JsValue, Source};
use uic_dom::NodeId;

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

    /// Registers every `.js` module of a vendored dist directory and loads
    /// the entry — the byte-unmodified npm package enters the engine here.
    pub fn load_dist_dir(&mut self, dir: &std::path::Path, entry: &str) -> Result<(), Error> {
        let entries = std::fs::read_dir(dir).map_err(|err| Error::Js(err.to_string()))?;
        for file in entries.flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            if name.ends_with(".js") {
                let source = std::fs::read_to_string(file.path())
                    .map_err(|err| Error::Js(err.to_string()))?;
                self.loader.insert(&name, &source);
            }
        }
        self.load_registered(entry)
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
            let root = state.doc.root();
            let node = state.doc.create_element_named(tag);
            for (name, value) in attrs {
                state.doc.set_attribute(node, name, value);
            }
            state.doc.append_child(root, node);
            state.dirty = true;
            let handle = state.handle(node);
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

    /// Moves the DOM focus (dispatching focusout/focusin) and runs the jobs.
    pub fn focus(&mut self, node: NodeId) -> Result<(), Error> {
        let handle = self.state.borrow_mut().handle(node);
        self.eval(&format!("__uicFocus({handle})"))?;
        self.run_jobs()
    }

    /// Delivers a keydown to the focused node; returns defaultPrevented.
    pub fn dispatch_key(&mut self, key: &str) -> Result<bool, Error> {
        let Some(focused) = self.state.borrow().focused else {
            return Ok(false);
        };
        let handle = self.state.borrow_mut().handle(focused);
        let prevented = self.eval(&format!(
            "__uicDeliver({handle}, 'keydown', {{ key: {key:?} }})"
        ))?;
        self.run_jobs()?;
        Ok(prevented.as_boolean().unwrap_or(false))
    }

    /// Delivers a bubbling click at the node — the pointer entry after a
    /// `uic_tui::dom::hit_test`.
    pub fn click(&mut self, node: NodeId) -> Result<(), Error> {
        let handle = self.state.borrow_mut().handle(node);
        self.eval(&format!("__uicDeliver({handle}, 'click', {{}})"))?;
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
