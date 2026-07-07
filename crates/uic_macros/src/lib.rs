//! Proc macros for ui-components: `#[derive(CustomElement)]`.
//!
//! The derive turns a struct into a registered custom element:
//!
//! - builds the `&'static ComponentDef` (tag, properties, handlers, computed
//!   names, embedded template/scss/impl sources) and submits it to the
//!   `inventory` registry — the `customElements.define` analog,
//! - validates the template at compile time with the same `uic_template`
//!   parser the runtimes use,
//! - generates the per-component `<Name>Logic` trait carrying exactly the
//!   handler and computed-property names the template references (plus
//!   defaulted lifecycle hooks), and an `impl Behavior` that dispatches to it.
//!
//! Components always provide an `impl <Name>Logic for <Name>` block, even an
//! empty one; a template-referenced handler without an implementation is a
//! plain missing-trait-method error.

mod component;

use proc_macro::TokenStream;

#[proc_macro_derive(CustomElement, attributes(custom_element, property))]
pub fn derive_custom_element(input: TokenStream) -> TokenStream {
    let source_file = proc_macro::Span::call_site().local_file();
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    component::expand(input, source_file.as_deref())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
