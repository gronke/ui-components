//! The native engine's host boundary: a wasm session exposing the shared
//! host operations (`uic_tui::dom::HostState`) so the UNCHANGED mocked-lit
//! runtime runs on the browser's own JS engine — in a worker — against the
//! retained document, cascade and paint. Boa stays the host for real
//! terminals; here the browser is the engine and this is its `__uic_*`
//! surface.

use ratatui::layout::Rect;
use ratatui::Terminal;
use uic_tui::dom::HostState;
use wasm_bindgen::prelude::*;

use crate::backend::{Output, XtermBackend};

fn js_err(err: impl std::fmt::Display) -> JsError {
    JsError::new(&err.to_string())
}

/// One terminal pane whose document a native-JS runtime mutates through
/// the flat host operations. The worker shims each method onto the
/// matching `globalThis.__uic_*` name and imports the runtime modules;
/// event dispatch and mounting stay in JS (`__uicDeliver`, `__uicMount`).
#[wasm_bindgen]
pub struct DomSession {
    state: HostState,
    terminal: Terminal<XtermBackend>,
    out: Output,
}

#[wasm_bindgen]
impl DomSession {
    /// A `cols`×`rows` terminal over an empty document.
    #[wasm_bindgen(constructor)]
    pub fn new(cols: u16, rows: u16) -> Result<DomSession, JsError> {
        let (backend, out) = XtermBackend::new(cols, rows);
        let terminal = Terminal::new(backend).map_err(js_err)?;
        Ok(DomSession {
            state: HostState::new(),
            terminal,
            out,
        })
    }

    // ---- the __uic_* operations -------------------------------------

    /// `__uic_commit`: the subtree-swap render path; focus survives by
    /// its `data-path`.
    pub fn commit(&mut self, handle: u32, html: &str) {
        self.state.commit(handle as usize, html);
    }

    /// `__uic_get_attr`.
    pub fn get_attr(&self, handle: u32, name: &str) -> Option<String> {
        self.state.attribute(handle as usize, name)
    }

    /// `__uic_set_attr`.
    pub fn set_attr(&mut self, handle: u32, name: &str, value: &str) {
        self.state.set_attribute(handle as usize, name, value);
    }

    /// `__uic_has_attr`.
    pub fn has_attr(&self, handle: u32, name: &str) -> bool {
        self.state.has_attribute(handle as usize, name)
    }

    /// `__uic_remove_attr`.
    pub fn remove_attr(&mut self, handle: u32, name: &str) {
        self.state.remove_attribute(handle as usize, name);
    }

    /// `__uic_text`.
    pub fn text(&self, handle: u32) -> String {
        self.state.text(handle as usize)
    }

    /// `__uic_query`: the matching handles. Crosses to JS as a typed
    /// array — the worker shim wraps it in `Array.from(...)`, since the
    /// runtime maps object facades over the result.
    pub fn query(&mut self, handle: u32, selector: &str) -> Result<Vec<u32>, JsError> {
        let matches = self
            .state
            .query(handle as usize, selector)
            .map_err(|message| JsError::new(&message))?;
        Ok(matches.into_iter().map(|h| h as u32).collect())
    }

    /// `__uic_matches`.
    pub fn matches(&self, handle: u32, selector: &str) -> Result<bool, JsError> {
        self.state
            .matches(handle as usize, selector)
            .map_err(|message| JsError::new(&message))
    }

    /// `__uic_contains`.
    pub fn contains(&self, outer: u32, inner: u32) -> bool {
        self.state.contains(outer as usize, inner as usize)
    }

    /// `__uic_parent`: the nearest element parent, `-1` at the top.
    pub fn parent(&mut self, handle: u32) -> i32 {
        self.state.parent(handle as usize).map_or(-1, |h| h as i32)
    }

    /// `__uic_focused`: the focus handle, `-1` when nothing holds it.
    pub fn focused(&mut self) -> i32 {
        self.state.focused_handle().map_or(-1, |h| h as i32)
    }

    /// `__uic_set_focused`; a negative handle blurs.
    pub fn set_focused(&mut self, handle: i32) {
        self.state
            .set_focused_handle((handle >= 0).then_some(handle as usize));
    }

    /// `__uic_adopt_styles`: the component's static styles enter the
    /// cascade, scoped per instance; returns the dropped-declaration count.
    pub fn adopt_styles(&mut self, tag: &str, css_text: &str) -> u32 {
        uic_tui::dom::adopt_component_sheet(tag, css_text) as u32
    }

    /// `__uic_widget_value`: a mounted widget's live text, `None` (null)
    /// on plain nodes — the facade falls back to the value attribute.
    pub fn widget_value(&self, handle: u32) -> Option<String> {
        self.state.widget_value(handle as usize)
    }

    /// `__uic_set_widget_value` — echo-skipped like the commit sync.
    pub fn set_widget_value(&mut self, handle: u32, text: &str) {
        self.state.set_widget_value(handle as usize, text);
    }

    /// The keydown's editing default action: routes the key into the
    /// focused widget; true when the text changed and the worker should
    /// deliver the bubbling `input`.
    pub fn widget_key(&mut self, key: &str, shift: bool, ctrl: bool, alt: bool) -> bool {
        let stroke = uic_tui::KeyStroke {
            key: key.to_string(),
            shift,
            ctrl,
            alt,
            meta: false,
        };
        self.state.widget_default_action(&stroke).is_some()
    }

    /// The paste default action: the pane's clipboard text into the focused
    /// widget as one bulk insert; true when the text changed and the worker
    /// should deliver the single bubbling `input` a browser paste fires.
    pub fn widget_paste(&mut self, text: &str) -> bool {
        self.state.widget_paste(text).is_some()
    }

    /// Whether the node carries a mounted widget — the pointer-focus guard.
    pub fn widget_at(&self, handle: i32) -> bool {
        handle >= 0 && self.state.has_widget(handle as usize)
    }

    /// The caret under the pointer — the browser's click-into-an-input.
    pub fn place_caret(&mut self, handle: u32, col: u16, row: u16) {
        self.state.place_caret(handle as usize, col, row);
    }

    // ---- host driving ------------------------------------------------

    /// Creates and appends the host element — the node half of a mount;
    /// the worker then calls `__uicMount(tag, handle)`.
    pub fn create_root(&mut self, tag: &str, attrs_json: &str) -> Result<u32, JsError> {
        let parsed: serde_json::Value = serde_json::from_str(attrs_json).map_err(js_err)?;
        let attrs: Vec<(String, String)> = parsed
            .as_object()
            .map(|attrs| {
                attrs
                    .iter()
                    .map(|(name, value)| {
                        let value = match value {
                            serde_json::Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                        (name.clone(), value)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let attrs: Vec<(&str, &str)> = attrs
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        Ok(self.state.create_root(tag, &attrs) as u32)
    }

    /// The deepest node under the cell, as a handle (`-1` misses) — the
    /// worker picks the click target and calls `__uicDeliver` itself.
    pub fn hit_test(&mut self, col: u16, row: u16) -> i32 {
        use ratatui::backend::Backend as _;
        let size = self.terminal.backend().size().expect("backend size");
        let target = uic_tui::dom::hit_test(
            &self.state.doc,
            Rect::new(0, 0, size.width, size.height),
            col,
            row,
        );
        match target {
            Some(node) => self.state.handle(node) as i32,
            None => -1,
        }
    }

    /// Sets the Bootstrap color mode on every document root; the cascade
    /// resolves the matching variable block. Returns the repaint ANSI.
    pub fn set_theme(&mut self, theme: &str) -> Result<String, JsError> {
        let root = self.state.doc.root();
        let roots: Vec<_> = self.state.doc.children(root).collect();
        for node in roots {
            self.state.doc.set_attribute(node, "data-bs-theme", theme);
        }
        self.draw()
    }

    /// Renders and returns the pending ANSI.
    pub fn draw(&mut self) -> Result<String, JsError> {
        let state = &mut self.state;
        let focused = state.focused;
        state.dirty = false;
        self.terminal
            .draw(|frame| {
                uic_tui::dom::paint_document(frame, frame.area(), &mut state.doc, focused);
            })
            .map_err(js_err)?;
        Ok(self.out.take())
    }

    /// Resizes the terminal and returns the full-repaint ANSI; call
    /// `term.resize(cols, rows)` before writing it.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<String, JsError> {
        self.terminal.backend_mut().resize(cols, rows);
        self.draw()
    }

    /// Whether a host operation changed the document since the last draw.
    pub fn dirty(&self) -> bool {
        self.state.dirty
    }

    /// The shadow grid as text, for assertions.
    pub fn screen_text(&self) -> String {
        self.terminal.backend().screen_text()
    }
}
