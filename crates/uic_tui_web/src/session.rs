use std::sync::Once;

use ratatui::Terminal;
use uic_core::SelectOption;
use uic_tui::{App, Control};
use wasm_bindgen::prelude::*;

use crate::backend::{Output, XtermBackend};
use crate::keymap::{translate_key, translate_mouse};

static LINK: Once = Once::new();

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

// The `inventory` registrations behind the element registry live in linker
// constructors; on wasm32-unknown-unknown nobody calls them unless the module
// does so itself (inventory's documented contract for reactor-style modules).
#[cfg(target_arch = "wasm32")]
extern "C" {
    fn __wasm_call_ctors();
}

fn js_err(err: impl std::fmt::Display) -> JsError {
    JsError::new(&err.to_string())
}

/// One terminal pane hosting mounted component roots, stacked like the
/// elements of a page. The host mounts each element by tag, replays its
/// markup attributes, and feeds DOM keys; every call returns the pending
/// ANSI for `term.write`.
#[wasm_bindgen]
pub struct TuiSession {
    app: App<XtermBackend>,
    out: Output,
    quit: bool,
}

#[wasm_bindgen]
impl TuiSession {
    /// A `cols`×`rows` in-memory terminal with no roots yet.
    #[wasm_bindgen(constructor)]
    pub fn new(cols: u16, rows: u16) -> Result<TuiSession, JsError> {
        LINK.call_once(|| {
            #[cfg(target_arch = "wasm32")]
            unsafe {
                __wasm_call_ctors()
            };
            ui_components::link();
            #[cfg(target_arch = "wasm32")]
            std::panic::set_hook(Box::new(|info| console_error(&info.to_string())));
        });
        let (backend, out) = XtermBackend::new(cols, rows);
        let terminal = Terminal::new(backend).map_err(js_err)?;
        Ok(TuiSession {
            app: App::from_terminal(terminal),
            out,
            quit: false,
        })
    }

    /// Mounts a registered custom element below the previous one and returns
    /// its root index. The first paint is deferred until the host replayed
    /// the attributes.
    pub fn mount(&mut self, tag: &str) -> Result<u32, JsError> {
        Ok(self.app.mount(tag).map_err(js_err)? as u32)
    }

    /// Replays one markup attribute; unknown names no-op like in the DOM.
    pub fn set_attr(&mut self, index: u32, name: &str, value: &str) {
        self.app.set_attr(index as usize, name, value);
    }

    /// Replays a property write from JSON: `null`, booleans, numbers,
    /// strings, object maps and arrays convert to their `Value` analogs
    /// (recursively). Malformed JSON errors; unknown property names no-op
    /// like the DOM runtime.
    pub fn set_prop_json(&mut self, index: u32, name: &str, json: &str) -> Result<(), JsError> {
        let parsed: serde_json::Value = serde_json::from_str(json).map_err(js_err)?;
        let value = uic_core::json::value_from_json(&parsed);
        self.app.set_prop(index as usize, name, value);
        Ok(())
    }

    /// Replays an option list property as JSON rows of
    /// `{value, short?, label?}` — options are their own data type, distinct
    /// from a plain array (ADR 0005): a host mounting a bare `<input-select>`
    /// feeds the rows through here so they land as `Value::Options`.
    pub fn set_options_json(&mut self, index: u32, json: &str) -> Result<(), JsError> {
        self.set_option_rows_json(index, "options", json)
    }

    /// The same replay for any option-rows property by name — a suggestion
    /// input's `suggestions`, or whatever a component declares as options.
    pub fn set_option_rows_json(
        &mut self,
        index: u32,
        name: &str,
        json: &str,
    ) -> Result<(), JsError> {
        let options = options_from_json(json).map_err(js_err)?;
        self.app.set_prop(index as usize, name, options);
        Ok(())
    }

    /// Calls back with one JSON argument per notify event:
    /// `{"type", "property", "value", "oldValue"}`. The callback must only
    /// hand the data on — calling back into the session would trip the
    /// wasm-bindgen borrow guard.
    pub fn on_notify(&mut self, index: u32, event: &str, callback: js_sys::Function) {
        self.app.on(index as usize, event, move |notify| {
            let json = serde_json::json!({
                "type": notify.event_name,
                "property": notify.property,
                "value": uic_core::json::value_to_json(&notify.value),
                "oldValue": uic_core::json::value_to_json(&notify.old_value),
            });
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&json.to_string()));
        });
    }

    /// Sets the Bootstrap color mode (`"light"` | `"dark"`) on every
    /// mounted root: the cascade resolves the matching variable block for
    /// the whole document, since custom properties inherit from the hosts.
    /// Returns the repaint ANSI; roots mounted later start light until the
    /// next call.
    pub fn set_theme(&mut self, theme: &str) -> Result<String, JsError> {
        for index in 0..self.app.mount_count() {
            self.app.set_dom_attr(index, "data-bs-theme", Some(theme));
        }
        self.app.draw().map_err(js_err)?;
        Ok(self.out.take())
    }

    /// Resizes the terminal and returns the full-repaint ANSI (a clear plus
    /// every cell): ratatui's autoresize sees the new backend size during
    /// the draw, resizes its buffers and resets the diff base. Call
    /// `term.resize(cols, rows)` before writing the result.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<String, JsError> {
        self.app.terminal_mut().backend_mut().resize(cols, rows);
        self.app.draw().map_err(js_err)?;
        Ok(self.out.take())
    }

    /// Renders and returns the pending ANSI.
    pub fn draw(&mut self) -> Result<String, JsError> {
        self.app.draw().map_err(js_err)?;
        Ok(self.out.take())
    }

    /// Feeds one DOM key and returns the ANSI the redraw produced.
    pub fn key(
        &mut self,
        key: &str,
        ctrl: bool,
        alt: bool,
        shift: bool,
    ) -> Result<String, JsError> {
        if let Some(event) = translate_key(key, ctrl, alt, shift) {
            // Draw before dispatching, like the terminal event loop: widget
            // state (option lists, popup anchors) syncs during the paint.
            self.app.draw().map_err(js_err)?;
            let event = crossterm::event::Event::Key(event);
            if self.app.handle_event(&event) == Control::Quit {
                self.quit = true;
            }
            self.app.draw().map_err(js_err)?;
        }
        Ok(self.out.take())
    }

    /// Feeds one DOM pointer gesture at a screen cell and returns the ANSI
    /// the redraw produced. Kinds: `down`, `up`, `drag`, `wheel-up`,
    /// `wheel-down`.
    pub fn mouse(&mut self, kind: &str, column: u16, row: u16) -> Result<String, JsError> {
        if let Some(kind) = translate_mouse(kind) {
            // Draw before dispatching: hit-testing reads the widget areas
            // recorded during the paint.
            self.app.draw().map_err(js_err)?;
            let event = crossterm::event::Event::Mouse(crossterm::event::MouseEvent {
                kind,
                column,
                row,
                modifiers: crossterm::event::KeyModifiers::NONE,
            });
            self.app.handle_event(&event);
            self.app.draw().map_err(js_err)?;
        }
        Ok(self.out.take())
    }

    /// Blurs the session when the pane loses focus: the focused widget
    /// commits and the ring and caret disappear.
    pub fn blur(&mut self) -> Result<String, JsError> {
        self.app.blur();
        self.app.draw().map_err(js_err)?;
        Ok(self.out.take())
    }

    /// True once after Esc or Ctrl-C; the host blurs the pane in response.
    pub fn take_quit(&mut self) -> bool {
        std::mem::take(&mut self.quit)
    }

    /// The screen as plain text rows — the assertion hook for tests.
    pub fn screen_text(&self) -> String {
        self.app.terminal().backend().screen_text()
    }
}

/// Parses JSON rows of `{value, short?, label?}` into option data
/// (ADR 0005) — the wire format of [`TuiSession::set_options_json`].
pub fn options_from_json(json: &str) -> Result<Vec<SelectOption>, serde_json::Error> {
    #[derive(serde::Deserialize)]
    struct Row {
        value: String,
        #[serde(default)]
        short: Option<String>,
        #[serde(default)]
        label: Option<String>,
    }
    let rows: Vec<Row> = serde_json::from_str(json)?;
    Ok(rows
        .into_iter()
        .map(|row| SelectOption {
            value: row.value,
            short: row.short,
            label: row.label,
        })
        .collect())
}
