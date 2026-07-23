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
mod input_shared;

use proc_macro::TokenStream;

#[proc_macro_derive(CustomElement, attributes(custom_element, property))]
pub fn derive_custom_element(input: TokenStream) -> TokenStream {
    let source_file = proc_macro::Span::call_site().local_file();
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    component::expand(input, source_file.as_deref())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// The shared input contract: injects the label/hint/error_message/disabled/
/// name/required properties and wires the shared chrome template and
/// stylesheet (`_shared/chrome.html`, `_shared/input-default.scss`, resolved
/// next to the component's module). Place above `#[derive(CustomElement)]`.
#[proc_macro_attribute]
pub fn input_shared(args: TokenStream, input: TokenStream) -> TokenStream {
    let item = syn::parse_macro_input!(input as syn::ItemStruct);
    if !args.is_empty() {
        return syn::Error::new_spanned(
            proc_macro2::TokenStream::from(args),
            "#[input_shared] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    if !input_shared::has_custom_element_derive(&item) {
        return syn::Error::new_spanned(
            &item.ident,
            "#[input_shared] must sit above #[derive(CustomElement)] on the same struct",
        )
        .to_compile_error()
        .into();
    }
    input_shared::expand(item)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
