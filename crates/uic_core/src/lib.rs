//! Component model: `ComponentDef`, `PropertyMeta`, `Behavior`, notify
//! semantics, and the inventory-backed custom-element registry.
//!
//! The vocabulary mirrors vanilla WebComponents (`customElements.define`,
//! HTMLElement/ReactiveElement lifecycle); LitElement is one output variant
//! with fixed assumptions, produced by `uic_codegen_web`.

mod behavior;
#[cfg(feature = "json")]
pub mod json;
mod meta;
mod notify;
mod object;
mod registry;
mod select;
mod value;
mod zoned;

pub use behavior::{Behavior, Ctx, NotifyEvent, UiEvent};
pub use meta::{
    ComponentDef, DefaultValue, HandlerKind, HandlerMeta, JsType, Notify, PropertyMeta,
};
pub use notify::notify_events;
pub use object::ObjectMap;
pub use registry::{CustomElementRegistry, Registration, RegistryError};
pub use select::SelectOption;
pub use value::{attribute_to_value, Changed, PropertyStore, Value};
pub use zoned::Zoned;

/// The derive macro that turns a struct into a registered custom element.
pub use uic_macros::CustomElement;

/// The shared input contract: injected properties + chrome, see `uic_macros`.
pub use uic_macros::input_shared;

/// Re-exported for the derive-generated code.
#[doc(hidden)]
pub use inventory;

pub mod prelude {
    pub use crate::behavior::{Behavior, Ctx, UiEvent};
    pub use crate::meta::ComponentDef;
    pub use crate::value::{Changed, PropertyStore, Value};
    pub use crate::CustomElement;
}
