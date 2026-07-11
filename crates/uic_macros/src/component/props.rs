//! The `#[property]` model: field parsing, per-type shape rules and
//! `PropertyMeta` token emission.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Expr, ExprLit, Fields, Lit, LitStr, Meta, Type};
use uic_template::names::{camel_case, dash_case};

enum JsTypeArg {
    String,
    Number,
    Boolean,
    Zoned,
    Options,
    Object,
}

/// The shape rule of a property-only (object-valued) type: how the Rust
/// field must be declared and the diagnostics when it is not. One table row
/// per type replaces a hand-kept rejection block; the texts are pinned by
/// the trybuild goldens.
struct ShapeRule {
    /// The type's name in diagnostics.
    name: &'static str,
    /// Whether the Rust field must be `Option<…>` (Zoned) or plain
    /// (Options/Object).
    optional: bool,
    /// The violation text of the optionality rule.
    optionality_msg: &'static str,
    /// How the default-less value starts, for the default rejection text.
    starts: &'static str,
}

impl JsTypeArg {
    /// Mirrors `uic_core::JsType::is_property_only` at macro-expansion time.
    fn is_property_only(&self) -> bool {
        self.shape_rule().is_some()
    }

    fn shape_rule(&self) -> Option<ShapeRule> {
        match self {
            JsTypeArg::Zoned => Some(ShapeRule {
                name: "Zoned",
                optional: true,
                optionality_msg: "Zoned properties must be Option<Zoned>; \
                     there is no non-null default for an object value",
                starts: "undefined",
            }),
            JsTypeArg::Options => Some(ShapeRule {
                name: "Options",
                optional: false,
                optionality_msg: "Options properties are plain Vec<SelectOption>; \
                     the empty state is the empty list, not None",
                starts: "empty",
            }),
            JsTypeArg::Object => Some(ShapeRule {
                name: "Object",
                optional: false,
                optionality_msg: "Object properties are plain ObjectMap; \
                     the empty state is the empty object, not None",
                starts: "empty",
            }),
            JsTypeArg::String | JsTypeArg::Number | JsTypeArg::Boolean => None,
        }
    }
}

/// Enforces a property-only type's shape: the declared optionality, no
/// attribute serialization, no default.
fn validate_shape(
    rule: &ShapeRule,
    field: &syn::Field,
    property_attr: &syn::Attribute,
    optional: bool,
    reflect: bool,
    attribute: &Option<String>,
    default_lit: &Option<Lit>,
) -> syn::Result<()> {
    if optional != rule.optional {
        return Err(syn::Error::new_spanned(&field.ty, rule.optionality_msg));
    }
    if reflect || attribute.is_some() {
        return Err(syn::Error::new_spanned(
            property_attr,
            format!(
                "{} properties are property-only; \
                 no attribute serialization exists (drop reflect/attribute)",
                rule.name
            ),
        ));
    }
    if let Some(lit) = default_lit {
        return Err(syn::Error::new_spanned(
            lit,
            format!(
                "{} properties cannot take a default; they start {}",
                rule.name, rule.starts
            ),
        ));
    }
    Ok(())
}

enum NotifyArg {
    No,
    Auto,
    Named(String),
}

/// A validated `default = …` literal.
enum DefaultArg {
    Str(String),
    Num(f64),
    Bool(bool),
}

/// One `#[property]` field.
pub(super) struct Prop {
    pub(super) ident: syn::Ident,
    pub(super) rust_name: String,
    js_name: String,
    attribute: String,
    js_type: JsTypeArg,
    optional: bool,
    reflect: bool,
    notify: NotifyArg,
    default: Option<DefaultArg>,
    doc: String,
}

impl Prop {
    pub(super) fn meta_tokens(&self) -> TokenStream {
        let rust_name = &self.rust_name;
        let js_name = &self.js_name;
        // Property-only (object-valued) types carry no observed attribute.
        let attribute = if self.js_type.is_property_only() {
            quote!(::core::option::Option::None)
        } else {
            let attribute = &self.attribute;
            quote!(::core::option::Option::Some(#attribute))
        };
        let js_type = match self.js_type {
            JsTypeArg::String => quote!(::uic_core::JsType::String),
            JsTypeArg::Number => quote!(::uic_core::JsType::Number),
            JsTypeArg::Boolean => quote!(::uic_core::JsType::Boolean),
            JsTypeArg::Zoned => quote!(::uic_core::JsType::Zoned),
            JsTypeArg::Options => quote!(::uic_core::JsType::Options),
            JsTypeArg::Object => quote!(::uic_core::JsType::Object),
        };
        let reflect = self.reflect;
        let notify = match &self.notify {
            NotifyArg::No => quote!(::uic_core::Notify::No),
            NotifyArg::Auto => quote!(::uic_core::Notify::Auto),
            NotifyArg::Named(name) => quote!(::uic_core::Notify::Named(#name)),
        };
        let default = match (&self.default, self.optional) {
            (Some(DefaultArg::Str(s)), _) => quote!(::uic_core::DefaultValue::Str(#s)),
            (Some(DefaultArg::Num(n)), _) => quote!(::uic_core::DefaultValue::Num(#n)),
            (Some(DefaultArg::Bool(b)), _) => quote!(::uic_core::DefaultValue::Bool(#b)),
            (None, true) => quote!(::uic_core::DefaultValue::Undefined),
            (None, false) => match self.js_type {
                JsTypeArg::String => quote!(::uic_core::DefaultValue::Str("")),
                JsTypeArg::Number => quote!(::uic_core::DefaultValue::Num(0.0f64)),
                JsTypeArg::Boolean => quote!(::uic_core::DefaultValue::Bool(false)),
                // Unreachable: Zoned properties must be Option<Zoned>.
                JsTypeArg::Zoned => quote!(::uic_core::DefaultValue::Undefined),
                JsTypeArg::Options => quote!(::uic_core::DefaultValue::EmptyOptions),
                JsTypeArg::Object => quote!(::uic_core::DefaultValue::EmptyObject),
            },
        };
        let optional = self.optional;
        let doc = &self.doc;
        quote! {
            ::uic_core::PropertyMeta {
                rust_name: #rust_name,
                js_name: #js_name,
                attribute: #attribute,
                js_type: #js_type,
                optional: #optional,
                reflect: #reflect,
                notify: #notify,
                default: #default,
                doc: #doc,
            }
        }
    }
}

pub(super) fn parse_properties(input: &DeriveInput) -> syn::Result<Vec<Prop>> {
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
        let mut default_lit: Option<Lit> = None;
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
                } else if meta.path.is_ident("default") {
                    default_lit = Some(meta.value()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error(
                        "unknown property option; expected reflect, notify, notify = \"…\", \
                         attribute = \"…\" or default = …",
                    ))
                }
            })?;
        }

        let ident = field.ident.clone().expect("named field");
        // `unraw` lets keyword fields like `r#type` map to the JS name `type`.
        let rust_name = ident.unraw().to_string();
        let (js_type, optional) = js_type_of(&field.ty).ok_or_else(|| {
            syn::Error::new_spanned(
                &field.ty,
                "unsupported property type; use String, bool, a number type, Zoned, \
                 Vec<SelectOption>, ObjectMap, or Option of one of those",
            )
        })?;
        if let Some(rule) = js_type.shape_rule() {
            validate_shape(
                &rule,
                field,
                property_attr,
                optional,
                reflect,
                &attribute,
                &default_lit,
            )?;
        }
        let default = default_lit
            .map(|lit| default_arg(&lit, &js_type))
            .transpose()?;

        props.push(Prop {
            js_name: camel_case(&rust_name),
            attribute: attribute.unwrap_or_else(|| dash_case(&rust_name)),
            ident,
            rust_name,
            js_type,
            optional,
            reflect,
            notify,
            default,
            doc: doc_of(&field.attrs),
        });
    }
    Ok(props)
}

/// Validates a `default = …` literal against the property's JS type.
fn default_arg(lit: &Lit, js_type: &JsTypeArg) -> syn::Result<DefaultArg> {
    match (js_type, lit) {
        (JsTypeArg::String, Lit::Str(s)) => Ok(DefaultArg::Str(s.value())),
        (JsTypeArg::Boolean, Lit::Bool(b)) => Ok(DefaultArg::Bool(b.value)),
        (JsTypeArg::Number, Lit::Int(n)) => Ok(DefaultArg::Num(n.base10_parse()?)),
        (JsTypeArg::Number, Lit::Float(n)) => Ok(DefaultArg::Num(n.base10_parse()?)),
        (js_type, lit) => Err(syn::Error::new_spanned(
            lit,
            format!(
                "default literal does not match the property type ({})",
                match js_type {
                    JsTypeArg::String => "String expects a string literal",
                    JsTypeArg::Number => "Number expects a number literal",
                    JsTypeArg::Boolean => "Boolean expects true or false",
                    JsTypeArg::Zoned => "Zoned takes no default",
                    JsTypeArg::Options => "Options takes no default",
                    JsTypeArg::Object => "Object takes no default",
                }
            ),
        )),
    }
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
    if name == "Vec" {
        let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
            return None;
        };
        let inner = args.args.iter().find_map(|arg| match arg {
            syn::GenericArgument::Type(Type::Path(path)) => path.path.segments.last(),
            _ => None,
        })?;
        if inner.ident == "SelectOption" {
            return Some((JsTypeArg::Options, false));
        }
        return None;
    }
    let js_type = match name.as_str() {
        "String" => JsTypeArg::String,
        "bool" => JsTypeArg::Boolean,
        "f64" | "f32" | "i64" | "i32" | "i16" | "i8" | "u64" | "u32" | "u16" | "u8" | "usize"
        | "isize" => JsTypeArg::Number,
        "Zoned" => JsTypeArg::Zoned,
        "ObjectMap" => JsTypeArg::Object,
        _ => return None,
    };
    Some((js_type, false))
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
