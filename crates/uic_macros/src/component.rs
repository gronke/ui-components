//! Expansion of `#[derive(CustomElement)]`.

use std::path::Path;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Expr, ExprLit, Fields, Lit, LitStr, Meta, Type};

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

    let parsed = uic_template::parse(&template.content).map_err(|err| {
        syn::Error::new(
            template.span,
            match &args.template_file {
                Some(file) => format!("template {}: {err}", file.value()),
                None => format!("template: {err}"),
            },
        )
    })?;

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

    let handler_names = &handlers;
    let computed_names = &computed;

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
                    scss: #scss,
                    web_impl: #web_impl,
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

/// The `#[custom_element(...)]` options.
struct ElementArgs {
    tag: String,
    style: Option<String>,
    template_inline: Option<LitStr>,
    template_file: Option<LitStr>,
    scss_file: Option<LitStr>,
    web_impl_file: Option<LitStr>,
}

impl ElementArgs {
    fn parse(input: &DeriveInput) -> syn::Result<Self> {
        let mut tag: Option<LitStr> = None;
        let mut style: Option<LitStr> = None;
        let mut template_inline = None;
        let mut template_file = None;
        let mut scss_file = None;
        let mut web_impl_file = None;

        for attr in &input.attrs {
            if !attr.path().is_ident("custom_element") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                let set = |slot: &mut Option<LitStr>| -> syn::Result<()> {
                    *slot = Some(meta.value()?.parse()?);
                    Ok(())
                };
                if meta.path.is_ident("tag") {
                    set(&mut tag)
                } else if meta.path.is_ident("style") {
                    set(&mut style)
                } else if meta.path.is_ident("template") {
                    set(&mut template_inline)
                } else if meta.path.is_ident("template_file") {
                    set(&mut template_file)
                } else if meta.path.is_ident("scss_file") {
                    set(&mut scss_file)
                } else if meta.path.is_ident("web_impl_file") {
                    set(&mut web_impl_file)
                } else {
                    Err(meta.error(
                        "unknown custom_element option; expected tag, style, template, \
                         template_file, scss_file or web_impl_file",
                    ))
                }
            })?;
        }

        let Some(tag) = tag else {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[custom_element(tag = \"…\")] is required",
            ));
        };
        let tag_value = tag.value();
        let valid_tag = tag_value.contains('-')
            && tag_value
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
            && tag_value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid_tag {
            return Err(syn::Error::new(
                tag.span(),
                "custom-element tags are lowercase and contain a dash, e.g. \"input-date\"",
            ));
        }
        if template_inline.is_some() && template_file.is_some() {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "use either template = \"…\" or template_file = \"…\", not both",
            ));
        }
        if template_inline.is_none() && template_file.is_none() {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[custom_element] requires template = \"…\" or template_file = \"…\"",
            ));
        }

        Ok(ElementArgs {
            tag: tag_value,
            style: style.map(|s| s.value()),
            template_inline,
            template_file,
            scss_file,
            web_impl_file,
        })
    }
}

struct LoadedTemplate {
    content: String,
    /// Span for template diagnostics (the template/template_file literal).
    span: proc_macro2::Span,
    /// Tokens embedding the source into the definition:
    /// the inline literal, or `include_str!` for file templates so cargo
    /// rebuilds when the `.mhtml` changes.
    src_tokens: TokenStream,
}

fn load_template(args: &ElementArgs, source_file: Option<&Path>) -> syn::Result<LoadedTemplate> {
    if let Some(inline) = &args.template_inline {
        return Ok(LoadedTemplate {
            content: inline.value(),
            span: inline.span(),
            src_tokens: quote!(#inline),
        });
    }
    let file = args.template_file.as_ref().expect("checked by ElementArgs");
    let content = read_relative(file, source_file, "template_file")?;
    Ok(LoadedTemplate {
        content,
        span: file.span(),
        src_tokens: quote!(include_str!(#file)),
    })
}

/// Reads a path relative to the source file containing the derive.
fn read_relative(file: &LitStr, source_file: Option<&Path>, option: &str) -> syn::Result<String> {
    let Some(dir) = source_file.and_then(Path::parent) else {
        return Err(syn::Error::new(
            file.span(),
            format!("cannot resolve {option} relative to this source file"),
        ));
    };
    let path = dir.join(file.value());
    std::fs::read_to_string(&path)
        .map_err(|err| syn::Error::new(file.span(), format!("{option} {}: {err}", path.display())))
}

/// `Some(include_str!("…"))` for a co-located asset, `None` when absent.
fn optional_include(
    file: &Option<LitStr>,
    source_file: Option<&Path>,
    option: &str,
) -> syn::Result<TokenStream> {
    match file {
        None => Ok(quote!(::core::option::Option::None)),
        Some(file) => {
            // Read now for an early, well-spanned error; embed via include_str!
            // so cargo tracks the file.
            read_relative(file, source_file, option)?;
            Ok(quote!(::core::option::Option::Some(include_str!(#file))))
        }
    }
}

enum JsTypeArg {
    String,
    Number,
    Boolean,
}

enum NotifyArg {
    No,
    Auto,
    Named(String),
}

/// One `#[property]` field.
struct Prop {
    ident: syn::Ident,
    rust_name: String,
    js_name: String,
    attribute: String,
    js_type: JsTypeArg,
    optional: bool,
    reflect: bool,
    notify: NotifyArg,
    doc: String,
}

impl Prop {
    fn meta_tokens(&self) -> TokenStream {
        let rust_name = &self.rust_name;
        let js_name = &self.js_name;
        let attribute = &self.attribute;
        let js_type = match self.js_type {
            JsTypeArg::String => quote!(::uic_core::JsType::String),
            JsTypeArg::Number => quote!(::uic_core::JsType::Number),
            JsTypeArg::Boolean => quote!(::uic_core::JsType::Boolean),
        };
        let reflect = self.reflect;
        let notify = match &self.notify {
            NotifyArg::No => quote!(::uic_core::Notify::No),
            NotifyArg::Auto => quote!(::uic_core::Notify::Auto),
            NotifyArg::Named(name) => quote!(::uic_core::Notify::Named(#name)),
        };
        let default = if self.optional {
            quote!(::uic_core::DefaultValue::Undefined)
        } else {
            match self.js_type {
                JsTypeArg::String => quote!(::uic_core::DefaultValue::Str("")),
                JsTypeArg::Number => quote!(::uic_core::DefaultValue::Num(0.0f64)),
                JsTypeArg::Boolean => quote!(::uic_core::DefaultValue::Bool(false)),
            }
        };
        let doc = &self.doc;
        quote! {
            ::uic_core::PropertyMeta {
                rust_name: #rust_name,
                js_name: #js_name,
                attribute: ::core::option::Option::Some(#attribute),
                js_type: #js_type,
                reflect: #reflect,
                notify: #notify,
                default: #default,
                doc: #doc,
            }
        }
    }
}

fn parse_properties(input: &DeriveInput) -> syn::Result<Vec<Prop>> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[derive(CustomElement)] requires a struct",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[derive(CustomElement)] requires named fields",
        ));
    };

    let mut props = Vec::new();
    for field in &fields.named {
        let Some(property_attr) = field.attrs.iter().find(|a| a.path().is_ident("property")) else {
            // Fields without #[property] are plain Rust state.
            continue;
        };

        let mut reflect = false;
        let mut notify = NotifyArg::No;
        let mut attribute: Option<String> = None;
        if let Meta::List(_) = &property_attr.meta {
            property_attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("reflect") {
                    reflect = true;
                    Ok(())
                } else if meta.path.is_ident("notify") {
                    notify = if meta.input.peek(syn::Token![=]) {
                        NotifyArg::Named(meta.value()?.parse::<LitStr>()?.value())
                    } else {
                        NotifyArg::Auto
                    };
                    Ok(())
                } else if meta.path.is_ident("attribute") {
                    attribute = Some(meta.value()?.parse::<LitStr>()?.value());
                    Ok(())
                } else {
                    Err(meta.error(
                        "unknown property option; expected reflect, notify, notify = \"…\" \
                         or attribute = \"…\"",
                    ))
                }
            })?;
        }

        let ident = field.ident.clone().expect("named field");
        let rust_name = ident.to_string();
        let (js_type, optional) = js_type_of(&field.ty).ok_or_else(|| {
            syn::Error::new_spanned(
                &field.ty,
                "unsupported property type; use String, bool, a number type, \
                 or Option of one of those",
            )
        })?;

        props.push(Prop {
            js_name: camel_case(&rust_name),
            attribute: attribute.unwrap_or_else(|| dash_case(&rust_name)),
            ident,
            rust_name,
            js_type,
            optional,
            reflect,
            notify,
            doc: doc_of(&field.attrs),
        });
    }
    Ok(props)
}

fn js_type_of(ty: &Type) -> Option<(JsTypeArg, bool)> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let name = segment.ident.to_string();
    if name == "Option" {
        let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
            return None;
        };
        let inner = args.args.iter().find_map(|arg| match arg {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })?;
        let (js_type, optional) = js_type_of(inner)?;
        if optional {
            return None;
        }
        return Some((js_type, true));
    }
    let js_type = match name.as_str() {
        "String" => JsTypeArg::String,
        "bool" => JsTypeArg::Boolean,
        "f64" | "f32" | "i64" | "i32" | "i16" | "i8" | "u64" | "u32" | "u16" | "u8" | "usize"
        | "isize" => JsTypeArg::Number,
        _ => return None,
    };
    Some((js_type, false))
}

/// `error_message` → `errorMessage`.
fn camel_case(rust_name: &str) -> String {
    let mut out = String::with_capacity(rust_name.len());
    let mut upper_next = false;
    for ch in rust_name.chars() {
        if ch == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// `error_message` → `error-message`.
fn dash_case(rust_name: &str) -> String {
    rust_name.replace('_', "-")
}

fn doc_of(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let Meta::NameValue(nv) = &attr.meta {
            if let Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) = &nv.value
            {
                lines.push(s.value().trim().to_string());
            }
        }
    }
    lines.join("\n")
}
