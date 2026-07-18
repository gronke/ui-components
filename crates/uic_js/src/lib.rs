//! Exploration #65: a Boa-embedded JS engine hosting real LitElement
//! components on the terminal runtime.
//!
//! The interception point is the `lit` module boundary: components import a
//! mocked `lit` (js/bootstrap.js) whose `LitElement` renders through flat
//! `__uic_*` natives into the retained `uic_tui::dom::DomDocument`; the
//! existing taffy layout and ratatui paint consume that document unchanged.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use boa_engine::module::{ModuleLoader, Referrer};
use boa_engine::{js_string, Context, JsNativeError, JsResult, JsString, JsValue, Module, Source};
use uic_dom::NodeId;
use uic_tui::dom::DomDocument;

const BOOTSTRAP: &str = include_str!("../js/bootstrap.js");

/// The `lit` specifiers re-export from the bootstrap's global namespace.
const LIT_SHIMS: &[(&str, &str)] = &[
    (
        "lit",
        "const m = globalThis.__uicLit;\n\
         export const html = m.html;\n\
         export const svg = m.svg;\n\
         export const css = m.css;\n\
         export const nothing = m.nothing;\n\
         export const LitElement = m.LitElement;\n",
    ),
    (
        "lit/decorators.js",
        "const m = globalThis.__uicLit;\n\
         export const property = m.property;\n\
         export const state = m.state;\n\
         export const queryAll = m.queryAll;\n",
    ),
    (
        "lit/directives/class-map.js",
        "export const classMap = globalThis.__uicLit.classMap;\n",
    ),
    (
        "lit/directives/map.js",
        "export const map = globalThis.__uicLit.map;\n",
    ),
    (
        "lit/directives/when.js",
        "export const when = globalThis.__uicLit.when;\n",
    ),
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("javascript error: {0}")]
    Js(String),
    #[error("unknown module specifier {0}")]
    UnknownModule(String),
}

impl From<boa_engine::JsError> for Error {
    fn from(err: boa_engine::JsError) -> Self {
        Error::Js(err.to_string())
    }
}

/// The document and the JS↔node handle table, shared with the natives.
pub struct HostState {
    pub doc: DomDocument,
    pub focused: Option<NodeId>,
    pub dirty: bool,
    handles: Vec<NodeId>,
    handle_of: HashMap<NodeId, usize>,
}

impl HostState {
    fn new() -> Self {
        HostState {
            doc: DomDocument::new(),
            focused: None,
            dirty: false,
            handles: Vec::new(),
            handle_of: HashMap::new(),
        }
    }

    /// The stable JS-side handle for a node.
    pub fn handle(&mut self, node: NodeId) -> usize {
        if let Some(&handle) = self.handle_of.get(&node) {
            return handle;
        }
        let handle = self.handles.len();
        self.handles.push(node);
        self.handle_of.insert(node, handle);
        handle
    }

    pub fn node(&self, handle: usize) -> Option<NodeId> {
        self.handles.get(handle).copied()
    }
}

thread_local! {
    static STATE: RefCell<Option<Rc<RefCell<HostState>>>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut HostState) -> R) -> JsResult<R> {
    let state = STATE.with(|slot| slot.borrow().clone());
    let state = state
        .ok_or_else(|| JsNativeError::error().with_message("uic_js host state is not installed"))?;
    let result = f(&mut state.borrow_mut());
    Ok(result)
}

/// In-memory module loader: specifier → source, parsed once and cached.
struct MapLoader {
    sources: RefCell<HashMap<String, String>>,
    modules: RefCell<HashMap<String, Module>>,
}

impl MapLoader {
    fn new() -> Self {
        let mut sources = HashMap::new();
        for (specifier, source) in LIT_SHIMS {
            sources.insert((*specifier).to_string(), (*source).to_string());
        }
        MapLoader {
            sources: RefCell::new(sources),
            modules: RefCell::new(HashMap::new()),
        }
    }

    fn insert(&self, specifier: &str, source: &str) {
        let specifier = specifier.trim_start_matches("./");
        self.sources
            .borrow_mut()
            .insert(specifier.to_string(), source.to_string());
    }

    fn resolve(&self, specifier: &str, context: &mut Context) -> JsResult<Module> {
        let specifier = specifier.trim_start_matches("./");
        if let Some(module) = self.modules.borrow().get(specifier) {
            return Ok(module.clone());
        }
        let sources = self.sources.borrow();
        let source = sources.get(specifier).ok_or_else(|| {
            JsNativeError::error().with_message(format!("unknown module specifier {specifier:?}"))
        })?;
        let module = Module::parse(Source::from_bytes(source.as_bytes()), None, context)?;
        self.modules
            .borrow_mut()
            .insert(specifier.to_string(), module.clone());
        Ok(module)
    }
}

impl ModuleLoader for MapLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        _referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let specifier = specifier.to_std_string_escaped();
        let mut guard = context.borrow_mut();
        self.resolve(&specifier, &mut guard)
    }
}

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
        STATE.with(|slot| *slot.borrow_mut() = Some(state.clone()));
        register_natives(&mut context)?;
        context.eval(Source::from_bytes(BOOTSTRAP.as_bytes()))?;
        Ok(JsHost {
            context,
            loader,
            state,
        })
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
        let entry_source =
            std::fs::read_to_string(dir.join(entry)).map_err(|err| Error::Js(err.to_string()))?;
        self.load_module(entry, &entry_source)
    }

    /// Registers, links and evaluates a module (a component definition).
    pub fn load_module(&mut self, specifier: &str, source: &str) -> Result<(), Error> {
        self.loader.insert(specifier, source);
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

/// The selector subset component code uses: attribute equality plus the
/// `:focus` and `:dir()` pseudo-classes.
fn matches_selector(state: &HostState, node: NodeId, selector: &str) -> JsResult<bool> {
    match selector {
        ":focus" => Ok(state.focused == Some(node)),
        ":dir(ltr)" => Ok(true),
        ":dir(rtl)" => Ok(false),
        _ => {
            let (name, value) = parse_attr_selector(selector)?;
            Ok(match state.doc.attribute(node, &name) {
                Some(actual) => value.as_deref().is_none_or(|v| v == actual),
                None => false,
            })
        }
    }
}

/// `[name]` / `[name="value"]` — the attribute-selector subset the facades
/// need; anything richer is a loud error, not silent mismatch.
fn parse_attr_selector(selector: &str) -> JsResult<(String, Option<String>)> {
    let inner = selector
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            JsNativeError::error().with_message(format!(
                "unsupported selector {selector:?} (attribute only)"
            ))
        })?;
    match inner.split_once('=') {
        Some((name, value)) => Ok((
            name.to_string(),
            Some(value.trim_matches(['"', '\'']).to_string()),
        )),
        None => Ok((inner.to_string(), None)),
    }
}

fn register_natives(context: &mut Context) -> Result<(), Error> {
    use boa_engine::NativeFunction;

    fn arg_number(args: &[JsValue], index: usize) -> JsResult<f64> {
        args.get(index).and_then(JsValue::as_number).ok_or_else(|| {
            JsNativeError::typ()
                .with_message(format!("argument {index} must be a number"))
                .into()
        })
    }

    fn arg_string(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
        Ok(args
            .get(index)
            .cloned()
            .unwrap_or_default()
            .to_string(context)?
            .to_std_string_escaped())
    }

    fn arg_node(args: &[JsValue]) -> JsResult<usize> {
        Ok(arg_number(args, 0)? as usize)
    }

    // Attribute and text access for the element facades.
    context.register_global_callable(
        js_string!("__uic_get_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            let value = with_state(|state| {
                state
                    .node(handle)
                    .and_then(|node| state.doc.attribute(node, &name).map(str::to_string))
            })?;
            Ok(value.map_or(JsValue::null(), |v| js_string!(v).into()))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_set_attr"),
        3,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            let value = arg_string(args, 2, context)?;
            with_state(|state| {
                if let Some(node) = state.node(handle) {
                    state.doc.set_attribute(node, &name, &value);
                    state.dirty = true;
                }
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_has_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            let has = with_state(|state| {
                state
                    .node(handle)
                    .is_some_and(|node| state.doc.attribute(node, &name).is_some())
            })?;
            Ok(JsValue::from(has))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_remove_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            with_state(|state| {
                if let Some(node) = state.node(handle) {
                    state.doc.remove_attribute(node, &name);
                    state.dirty = true;
                }
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_text"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_node(args)?;
            let text = with_state(|state| {
                state
                    .node(handle)
                    .map(|node| state.doc.text_content(node))
                    .unwrap_or_default()
            })?;
            Ok(js_string!(text).into())
        }),
    )?;

    // __uic_query(handle, selector) -> handles. The selector micro-matcher
    // covers what component code uses: `[name]` and `[name="value"]`.
    context.register_global_callable(
        js_string!("__uic_query"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let selector = arg_string(args, 1, context)?;
            let (name, value) = parse_attr_selector(&selector)?;
            let matches: Vec<usize> = with_state(|state| {
                let Some(root) = state.node(handle) else {
                    return Vec::new();
                };
                let nodes: Vec<NodeId> = state
                    .doc
                    .descendants(root)
                    .filter(|&node| match state.doc.attribute(node, &name) {
                        Some(actual) => value.as_deref().is_none_or(|v| v == actual),
                        None => false,
                    })
                    .collect();
                nodes.into_iter().map(|node| state.handle(node)).collect()
            })?;
            let array = boa_engine::object::builtins::JsArray::from_iter(
                matches.into_iter().map(|h: usize| JsValue::from(h as f64)),
                context,
            );
            Ok(array.into())
        }),
    )?;

    // __uic_commit(handle, html): replace the element's children with the
    // parsed fragment — the subtree-swap render path of the exploration.
    // Focus inside the swapped subtree survives by its `data-path`, the
    // component's own stable row key.
    context.register_global_callable(
        js_string!("__uic_commit"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_number(args, 0)? as usize;
            let html = arg_string(args, 1, context)?;
            with_state(|state| {
                let Some(target) = state.node(handle) else {
                    return;
                };
                let focus_path = state
                    .focused
                    .filter(|&f| f == target || state.doc.ancestors(f).any(|node| node == target));
                let focus_path = focus_path
                    .and_then(|f| state.doc.attribute(f, "data-path").map(str::to_string));
                let scratch: DomDocument = uic_dom::Document::parse_fragment(&html, "body");
                let children: Vec<NodeId> = state.doc.children(target).collect();
                for child in children {
                    state.doc.remove(child);
                }
                let sources: Vec<NodeId> = scratch.children(scratch.root()).collect();
                let mut map = HashMap::new();
                for source in sources {
                    if let Some(copy) = state.doc.import_node(&scratch, source, &mut map) {
                        state.doc.append_child(target, copy);
                    }
                }
                if let Some(focused) = state.focused {
                    if state.doc.node(focused).is_none() {
                        let resolved = focus_path.and_then(|path| {
                            state.doc.descendants(target).find(|&node| {
                                state.doc.attribute(node, "data-path") == Some(path.as_str())
                            })
                        });
                        state.focused = Some(resolved.unwrap_or(target));
                    }
                }
                state.dirty = true;
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    // Tree relations and state for the facades and the dispatcher.
    context.register_global_callable(
        js_string!("__uic_parent"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_node(args)?;
            let parent = with_state(|state| {
                let parent = state.node(handle).and_then(|node| state.doc.parent(node));
                parent
                    .filter(|&p| matches!(state.doc.node(p), Some(uic_dom::NodeData::Element(_))))
                    .map(|p| state.handle(p))
            })?;
            Ok(parent.map_or(JsValue::from(-1), |h| JsValue::from(h as f64)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_matches"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let selector = arg_string(args, 1, context)?;
            let result = with_state(|state| {
                state
                    .node(handle)
                    .map(|node| matches_selector(state, node, &selector))
            })?;
            result
                .transpose()
                .map(|m| JsValue::from(m.unwrap_or(false)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_contains"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let outer = arg_node(args)?;
            let inner = arg_number(args, 1)? as usize;
            let contains = with_state(|state| {
                let (Some(outer), Some(inner)) = (state.node(outer), state.node(inner)) else {
                    return false;
                };
                outer == inner || state.doc.ancestors(inner).any(|node| node == outer)
            })?;
            Ok(JsValue::from(contains))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_focused"),
        0,
        NativeFunction::from_fn_ptr(|_this, _args, _context| {
            let focused = with_state(|state| state.focused.map(|node| state.handle(node)))?;
            Ok(focused.map_or(JsValue::from(-1), |h| JsValue::from(h as f64)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_set_focused"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_number(args, 0)?;
            with_state(|state| {
                state.focused = if handle < 0.0 {
                    None
                } else {
                    state.node(handle as usize)
                };
                state.dirty = true;
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    // __uic_log(message): debugging visibility from scripts.
    context.register_global_callable(
        js_string!("__uic_log"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let message = arg_string(args, 0, context)?;
            eprintln!("[uic_js] {message}");
            Ok(JsValue::undefined())
        }),
    )?;

    Ok(())
}
