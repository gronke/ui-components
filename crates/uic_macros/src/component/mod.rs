//! Expansion of `#[derive(CustomElement)]`: orchestration, template
//! validation and token emission. The `#[custom_element(...)]` options live
//! in `args`, the `#[property]` model in `props`.

mod args;
mod props;

use std::path::Path;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::DeriveInput;

use args::{load_template, optional_include, read_relative, ElementArgs};
use props::{parse_properties, Prop};

pub fn expand(input: DeriveInput, source_file: Option<&Path>) -> syn::Result<TokenStream> {
    let struct_ident = &input.ident;
    let vis = &input.vis;

    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "custom elements cannot be generic",
        ));
    }

    let args = ElementArgs::parse(&input)?;
    let props = parse_properties(&input)?;
    let template = load_template(&args, source_file)?;

    let inner = uic_template::parse(&template.content).map_err(|err| {
        syn::Error::new(
            template.span,
            match &args.template_file {
                Some(file) => format!("template {}: {err}", file.value()),
                None => format!("template: {err}"),
            },
        )
    })?;

    // Splice the chrome around the component's template; all validation below
    // runs on the merged tree, exactly what both render targets execute.
    let parsed = match &args.wraps_file {
        None => inner,
        Some(wraps_file) => {
            let chrome_content = read_relative(wraps_file, source_file, "wraps_file")?;
            let chrome = uic_template::parse(&chrome_content).map_err(|err| {
                syn::Error::new(
                    wraps_file.span(),
                    format!("chrome template {}: {err}", wraps_file.value()),
                )
            })?;
            if chrome_has_data_tui(&chrome.roots) {
                return Err(syn::Error::new(
                    wraps_file.span(),
                    "chrome template must not contain data-tui widgets; \
                     they belong to the wrapped component",
                ));
            }
            uic_template::splice(&chrome, &inner).map_err(|err| {
                syn::Error::new(
                    wraps_file.span(),
                    format!("chrome template {}: {err}", wraps_file.value()),
                )
            })?
        }
    };

    if let Err(msg) = validate_options_bindings(&parsed.roots) {
        return Err(syn::Error::new(template.span, msg));
    }

    // Split referenced names: declared properties stay properties, the rest
    // become computed getters on the Logic trait.
    let prop_names: Vec<&str> = props.iter().map(|p| p.rust_name.as_str()).collect();
    let computed: Vec<String> = parsed
        .referenced_idents()
        .into_iter()
        .filter(|name| !prop_names.contains(name))
        .map(str::to_string)
        .collect();
    let handlers: Vec<String> = parsed
        .referenced_handlers()
        .into_iter()
        .map(str::to_string)
        .collect();
    if let Some(clash) = computed.iter().find(|c| handlers.contains(c)) {
        return Err(syn::Error::new(
            template.span,
            format!("'{clash}' is used both as a value hole and an event handler"),
        ));
    }
    if let Some(clash) = handlers.iter().find(|h| prop_names.contains(&h.as_str())) {
        return Err(syn::Error::new(
            template.span,
            format!("'{clash}' is a property and cannot be bound as an event handler"),
        ));
    }

    let idents_of = |names: &[String], span_msg: &str| -> syn::Result<Vec<syn::Ident>> {
        names
            .iter()
            .map(|name| {
                syn::parse_str::<syn::Ident>(name).map_err(|_| {
                    syn::Error::new(
                        template.span,
                        format!("{span_msg} '{name}' is not a usable Rust method name"),
                    )
                })
            })
            .collect()
    };
    let handler_idents = idents_of(&handlers, "handler")?;
    let computed_idents = idents_of(&computed, "computed property")?;

    let tag = &args.tag;
    let style_id = args.style.clone().unwrap_or_else(|| args.tag.clone());
    let class_name = struct_ident.to_string();
    let logic_ident = format_ident!("{struct_ident}Logic");
    let template_src = &template.src_tokens;

    let property_metas = props.iter().map(Prop::meta_tokens);
    let field_idents = props.iter().map(|p| &p.ident);

    let scss = optional_include(&args.scss_file, source_file, "scss_file")?;
    let web_impl = optional_include(&args.web_impl_file, source_file, "web_impl_file")?;
    let wraps_src = optional_include(&args.wraps_file, source_file, "wraps_file")?;
    let shared_scss = optional_include(&args.shared_scss, source_file, "shared_scss")?;
    let shared_style_id = match &args.shared_style {
        Some(id) => {
            let id = id.value();
            quote!(::core::option::Option::Some(#id))
        }
        None => quote!(::core::option::Option::None),
    };

    let handler_names = &handlers;
    let computed_names = &computed;
    let dist = args.dist;

    Ok(quote! {
        impl #struct_ident {
            pub const TAG_NAME: &'static str = #tag;

            /// The registered definition of this custom element.
            pub fn definition() -> &'static ::uic_core::ComponentDef {
                fn new_behavior() -> ::std::boxed::Box<dyn ::uic_core::Behavior> {
                    ::std::boxed::Box::new(<#struct_ident as ::core::default::Default>::default())
                }
                static PROPERTIES: &[::uic_core::PropertyMeta] = &[#(#property_metas),*];
                static HANDLERS: &[::uic_core::HandlerMeta] = &[
                    #(::uic_core::HandlerMeta {
                        name: #handler_names,
                        kind: ::uic_core::HandlerKind::PerTarget,
                    }),*
                ];
                static COMPUTED: &[&str] = &[#(#computed_names),*];
                static DEF: ::uic_core::ComponentDef = ::uic_core::ComponentDef {
                    tag_name: #tag,
                    class_name: #class_name,
                    style_id: #style_id,
                    properties: PROPERTIES,
                    handlers: HANDLERS,
                    computed: COMPUTED,
                    template_src: #template_src,
                    wraps_src: #wraps_src,
                    shared_style_id: #shared_style_id,
                    shared_scss: #shared_scss,
                    scss: #scss,
                    web_impl: #web_impl,
                    dist: #dist,
                    module_path: ::core::module_path!(),
                    new_behavior,
                    template_cache: ::std::sync::OnceLock::new(),
                };
                &DEF
            }

            #[doc(hidden)]
            #[allow(dead_code)]
            fn __uic_property_fields(&self) {
                #(let _ = &self.#field_idents;)*
            }
        }

        ::uic_core::inventory::submit! {
            ::uic_core::Registration(#struct_ident::definition)
        }

        /// Behavior hooks of the component: template-referenced handlers and
        /// computed properties (required), lifecycle hooks (defaulted).
        /// Components implement this trait, possibly with an empty block.
        #vis trait #logic_ident {
            #(fn #handler_idents(
                &mut self,
                ctx: &mut ::uic_core::Ctx,
                event: &::uic_core::UiEvent,
            );)*

            #(fn #computed_idents(
                &self,
                store: &::uic_core::PropertyStore,
            ) -> ::uic_core::Value;)*

            fn connected(&mut self, _ctx: &mut ::uic_core::Ctx) {}
            fn disconnected(&mut self, _ctx: &mut ::uic_core::Ctx) {}
            fn attribute_changed(
                &mut self,
                _ctx: &mut ::uic_core::Ctx,
                _name: &str,
                _old: ::core::option::Option<&str>,
                _new: ::core::option::Option<&str>,
            ) {
            }
            fn will_update(&mut self, _ctx: &mut ::uic_core::Ctx, _changed: &::uic_core::Changed) {}
            fn updated(&mut self, _ctx: &mut ::uic_core::Ctx, _changed: &::uic_core::Changed) {}
        }

        impl ::uic_core::Behavior for #struct_ident {
            fn def(&self) -> &'static ::uic_core::ComponentDef {
                #struct_ident::definition()
            }

            fn connected(&mut self, ctx: &mut ::uic_core::Ctx) {
                <Self as #logic_ident>::connected(self, ctx)
            }

            fn disconnected(&mut self, ctx: &mut ::uic_core::Ctx) {
                <Self as #logic_ident>::disconnected(self, ctx)
            }

            fn attribute_changed(
                &mut self,
                ctx: &mut ::uic_core::Ctx,
                name: &str,
                old: ::core::option::Option<&str>,
                new: ::core::option::Option<&str>,
            ) {
                <Self as #logic_ident>::attribute_changed(self, ctx, name, old, new)
            }

            fn will_update(&mut self, ctx: &mut ::uic_core::Ctx, changed: &::uic_core::Changed) {
                <Self as #logic_ident>::will_update(self, ctx, changed)
            }

            fn updated(&mut self, ctx: &mut ::uic_core::Ctx, changed: &::uic_core::Changed) {
                <Self as #logic_ident>::updated(self, ctx, changed)
            }

            fn handle(
                &mut self,
                ctx: &mut ::uic_core::Ctx,
                handler: &str,
                event: &::uic_core::UiEvent,
            ) {
                match handler {
                    #(#handler_names => <Self as #logic_ident>::#handler_idents(self, ctx, event),)*
                    _ => {}
                }
            }

            fn compute(&self, store: &::uic_core::PropertyStore, name: &str) -> ::uic_core::Value {
                match name {
                    #(#computed_names => <Self as #logic_ident>::#computed_idents(self, store),)*
                    _ => ::uic_core::Value::Undefined,
                }
            }
        }
    })
}

/// `.options` bindings carry option lists (ADR 0006): they belong on
/// `<select>` elements — whose children then come from the data — on custom
/// elements, which receive the list as a property, or on `data-tui` widgets,
/// whose adapters store the rows (ADR 0015).
fn validate_options_bindings(nodes: &[uic_template::Node]) -> Result<(), String> {
    use uic_template::{Attribute, Node};
    for node in nodes {
        match node {
            Node::Element(el) => {
                let has_options = el
                    .attrs
                    .iter()
                    .any(|attr| matches!(attr, Attribute::Prop { name, .. } if name == "options"));
                let is_widget = el.attrs.iter().any(
                    |attr| matches!(attr, Attribute::Static { name, .. } if name == "data-tui"),
                );
                if has_options && el.tag != "select" && !el.is_custom() && !is_widget {
                    return Err(format!(
                        "'.options' bindings belong on <select> elements, data-tui widgets \
                         or custom elements, not <{}>",
                        el.tag
                    ));
                }
                if has_options && el.tag == "select" && !el.children.is_empty() {
                    return Err(
                        "a <select> with '.options' takes no children; the option list is data"
                            .to_string(),
                    );
                }
                validate_options_bindings(&el.children)?;
            }
            Node::If { then, .. } => validate_options_bindings(then)?,
            Node::Text(_) | Node::TextHole(_) => {}
        }
    }
    Ok(())
}

/// Whether any element in the tree carries a `data-tui` marker.
fn chrome_has_data_tui(nodes: &[uic_template::Node]) -> bool {
    use uic_template::{Attribute, Node};
    nodes.iter().any(|node| match node {
        Node::Element(el) => {
            el.attrs
                .iter()
                .any(|attr| matches!(attr, Attribute::Static { name, .. } if name == "data-tui"))
                || chrome_has_data_tui(&el.children)
        }
        Node::If { then, .. } => chrome_has_data_tui(then),
        Node::Text(_) | Node::TextHole(_) => false,
    })
}
