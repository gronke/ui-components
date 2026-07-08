//! Expansion of `#[input_shared]`: the shared input contract.
//!
//! Injects the core contract fields (label, hint, error_message, disabled,
//! name, required) and appends a `#[custom_element(...)]` attribute wiring
//! the shared chrome template and stylesheet. Must be placed above
//! `#[derive(CustomElement)]`, so the derive expands the rewritten struct.

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_quote, Fields, ItemStruct};

/// The injected field names, kept in sync with `injected_fields`.
const CONTRACT_FIELDS: &[&str] = &[
    "label",
    "hint",
    "error_message",
    "disabled",
    "name",
    "required",
    "error",
    "suggested",
    "seamless",
];

pub fn expand(mut item: ItemStruct) -> syn::Result<TokenStream> {
    let Fields::Named(fields) = &mut item.fields else {
        return Err(syn::Error::new_spanned(
            &item.ident,
            "#[input_shared] requires a struct with named fields",
        ));
    };

    for field in &fields.named {
        let name = field.ident.as_ref().expect("named field").to_string();
        if CONTRACT_FIELDS.contains(&name.as_str()) {
            return Err(syn::Error::new_spanned(
                field.ident.as_ref().expect("named field"),
                format!("field `{name}` is provided by #[input_shared]"),
            ));
        }
    }

    let injected: syn::FieldsNamed = parse_quote!({
        /// Label rendered above the input.
        #[property]
        pub label: ::core::option::Option<::std::string::String>,
        /// Hint rendered below the input while there is no error.
        #[property]
        pub hint: ::core::option::Option<::std::string::String>,
        /// Validation error rendered below the input.
        #[property]
        pub error_message: ::core::option::Option<::std::string::String>,
        #[property(reflect)]
        pub disabled: bool,
        /// Form-field name.
        #[property]
        pub name: ::core::option::Option<::std::string::String>,
        #[property(reflect)]
        pub required: bool,
        /// Invalid-input state; styles the input with the danger outline.
        #[property(reflect)]
        pub error: bool,
        /// Suggested state; styles the input with the accent outline.
        #[property(reflect)]
        pub suggested: bool,
        /// Borderless flush rendering for embedding in card headers etc.
        #[property(reflect)]
        pub seamless: bool,
    });
    fields.named.extend(injected.named);

    // A second custom_element attribute; ElementArgs folds all of them and
    // errors when the user set any of these options too.
    item.attrs.push(parse_quote!(#[custom_element(
        wraps_file = "_shared/chrome.mhtml",
        shared_style = "input-default",
        shared_scss = "_shared/input-default.scss"
    )]));

    Ok(quote!(#item))
}

/// Guards against the attribute being placed with no derive to consume it —
/// the injected options only mean something to `#[derive(CustomElement)]`.
pub fn has_custom_element_derive(item: &ItemStruct) -> bool {
    item.attrs.iter().any(|attr| {
        attr.path().is_ident("derive")
            && attr
                .meta
                .to_token_stream()
                .to_string()
                .contains("CustomElement")
    })
}
