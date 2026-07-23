//! The `#[custom_element(...)]` options and the co-located file inputs.

use std::path::Path;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, LitStr};

/// The `#[custom_element(...)]` options, folded over every such attribute
/// (`#[input_shared]` appends its own `#[custom_element(...)]`).
pub(super) struct ElementArgs {
    pub(super) tag: String,
    pub(super) style: Option<String>,
    pub(super) template_inline: Option<LitStr>,
    pub(super) template_file: Option<LitStr>,
    pub(super) scss_file: Option<LitStr>,
    pub(super) web_impl_file: Option<LitStr>,
    pub(super) wraps_file: Option<LitStr>,
    pub(super) shared_style: Option<LitStr>,
    pub(super) shared_scss: Option<LitStr>,
    /// Ships in the npm dist (default); `dist = false` for demo compositions.
    pub(super) dist: bool,
}

impl ElementArgs {
    pub(super) fn parse(input: &DeriveInput) -> syn::Result<Self> {
        let mut tag: Option<LitStr> = None;
        let mut style: Option<LitStr> = None;
        let mut template_inline = None;
        let mut template_file = None;
        let mut scss_file = None;
        let mut web_impl_file = None;
        let mut wraps_file = None;
        let mut shared_style = None;
        let mut shared_scss = None;
        let mut dist: Option<bool> = None;

        for attr in &input.attrs {
            if !attr.path().is_ident("custom_element") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                let set = |slot: &mut Option<LitStr>| -> syn::Result<()> {
                    if slot.is_some() {
                        return Err(meta.error("custom_element option set twice"));
                    }
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
                } else if meta.path.is_ident("wraps_file") {
                    set(&mut wraps_file)
                } else if meta.path.is_ident("shared_style") {
                    set(&mut shared_style)
                } else if meta.path.is_ident("shared_scss") {
                    set(&mut shared_scss)
                } else if meta.path.is_ident("dist") {
                    if dist.is_some() {
                        return Err(meta.error("custom_element option set twice"));
                    }
                    dist = Some(meta.value()?.parse::<syn::LitBool>()?.value);
                    Ok(())
                } else {
                    Err(meta.error(
                        "unknown custom_element option; expected tag, style, template, \
                         template_file, scss_file, web_impl_file, wraps_file, \
                         shared_style, shared_scss or dist",
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
        if let (Some(scss), None) = (&shared_scss, &shared_style) {
            return Err(syn::Error::new(
                scss.span(),
                "shared_scss requires shared_style = \"…\" naming the style it backs",
            ));
        }

        Ok(ElementArgs {
            tag: tag_value,
            style: style.map(|s| s.value()),
            template_inline,
            template_file,
            scss_file,
            web_impl_file,
            wraps_file,
            shared_style,
            shared_scss,
            dist: dist.unwrap_or(true),
        })
    }
}

pub(super) struct LoadedTemplate {
    pub(super) content: String,
    /// Span for template diagnostics (the template/template_file literal).
    pub(super) span: proc_macro2::Span,
    /// Tokens embedding the source into the definition:
    /// the inline literal, or `include_str!` for file templates so cargo
    /// rebuilds when the `.html` changes.
    pub(super) src_tokens: TokenStream,
}

pub(super) fn load_template(
    args: &ElementArgs,
    source_file: Option<&Path>,
) -> syn::Result<LoadedTemplate> {
    if let Some(inline) = &args.template_inline {
        return Ok(LoadedTemplate {
            content: inline.value(),
            span: inline.span(),
            src_tokens: quote!(#inline),
        });
    }
    let file = args.template_file.as_ref().expect("checked by ElementArgs");
    let content = read_relative(file, source_file, "template_file")?;
    let src_tokens = include_tokens(file, source_file, "template_file")?;
    Ok(LoadedTemplate {
        content,
        span: file.span(),
        src_tokens,
    })
}

/// Reads a co-located asset: resolved beside the source file containing the
/// derive, then upward through its ancestors, stopping at the directory
/// holding the crate's Cargo.toml. The search serves shared assets like the
/// input chrome, which components reference from varying directory depths
/// (ADR 0015); the nearest match wins.
pub(super) fn read_relative(
    file: &LitStr,
    source_file: Option<&Path>,
    option: &str,
) -> syn::Result<String> {
    let (path, _) = resolve_relative(file, source_file, option)?;
    std::fs::read_to_string(&path)
        .map_err(|err| syn::Error::new(file.span(), format!("{option} {}: {err}", path.display())))
}

/// The found asset path and its `../` depth above the source file's
/// directory (see [`read_relative`] for the search).
fn resolve_relative(
    file: &LitStr,
    source_file: Option<&Path>,
    option: &str,
) -> syn::Result<(std::path::PathBuf, usize)> {
    let Some(dir) = source_file.and_then(Path::parent) else {
        return Err(syn::Error::new(
            file.span(),
            format!("cannot resolve {option} relative to this source file"),
        ));
    };
    let mut current = dir.to_path_buf();
    let mut depth = 0;
    loop {
        let candidate = current.join(file.value());
        if candidate.is_file() {
            return Ok((candidate, depth));
        }
        if current.join("Cargo.toml").is_file() || !current.pop() {
            return Err(syn::Error::new(
                file.span(),
                format!(
                    "{option} {}: not found beside the source file or in its \
                     ancestors up to the crate root",
                    file.value()
                ),
            ));
        }
        depth += 1;
    }
}

/// The `include_str!` tokens for an asset, prefixed with the `../` steps the
/// upward search took, so cargo tracks the found file.
fn include_tokens(
    file: &LitStr,
    source_file: Option<&Path>,
    option: &str,
) -> syn::Result<TokenStream> {
    let (_, depth) = resolve_relative(file, source_file, option)?;
    if depth == 0 {
        return Ok(quote!(include_str!(#file)));
    }
    let prefixed = format!("{}{}", "../".repeat(depth), file.value());
    Ok(quote!(include_str!(#prefixed)))
}

/// `Some(include_str!("…"))` for a co-located asset, `None` when absent.
pub(super) fn optional_include(
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
            let tokens = include_tokens(file, source_file, option)?;
            Ok(quote!(::core::option::Option::Some(#tokens)))
        }
    }
}
