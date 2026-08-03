//! A retained DOM for the terminal runtime: LitElement's substrate, in Rust.
//!
//! One arena-backed tree with the web's element operations, spec-grade
//! parsing through html5ever's `TreeSink` (the architecture lit-html itself
//! uses in the browser: real HTML parsing, with the binding dialect riding
//! through as plain attributes and text), and the whatwg event-dispatch
//! subset with capture/bubble propagation.
//!
//! This crate is the foundation layer; the template parts compiler, the
//! reactive update lifecycle and the TUI integration build on it (ADR 0008).
//!
//! ```
//! use uic_dom::{html, Document, Event, ListenerOptions};
//!
//! let mut doc: Document = Document::new();
//! let form = doc.create_element(html::Form);
//! let input = doc.create_element(html::Input);
//! let root = doc.root();
//! doc.append_child(root, form);
//! doc.append_child(form, input);
//! doc.set_attribute(input, "placeholder", "free text");
//!
//! doc.add_event_listener(form, "change", ListenerOptions::default(), |doc, event| {
//!     let target = event.target().expect("dispatched events carry a target");
//!     doc.add_class(target, "was-changed");
//! });
//! doc.dispatch_event(input, &mut Event::change());
//!
//! assert!(doc.has_class(input, "was-changed"));
//! assert_eq!(
//!     doc.outer_html(form),
//!     "<form><input placeholder=\"free text\" class=\"was-changed\"></form>",
//! );
//! ```

mod event;
pub mod html;
pub mod parts;
mod serialize;
mod sink;
mod tree;

pub use event::{Event, EventPhase, ListenerId, ListenerOptions};
pub use html::ElementKind;
pub use tree::{Document, ElementData, NodeData, NodeId};
