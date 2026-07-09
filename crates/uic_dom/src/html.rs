//! The typed element vocabulary: unit structs for the tag slice the
//! components use, so trees build as `document.create_element(html::Input)`.
//!
//! The trait keeps room for per-element typed attributes and a generated
//! full vocabulary (@webref-style) later, without changing call sites.

use html5ever::{local_name, LocalName};

/// A statically-known element kind; anything dynamic goes through
/// [`crate::Document::create_element_named`].
pub trait ElementKind {
    fn local_name(&self) -> LocalName;
}

macro_rules! element_kinds {
    ($($(#[$doc:meta])* $name:ident => $tag:tt,)*) => {
        $(
            $(#[$doc])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct $name;

            impl ElementKind for $name {
                fn local_name(&self) -> LocalName {
                    local_name!($tag)
                }
            }
        )*
    };
}

element_kinds! {
    Div => "div",
    Span => "span",
    Label => "label",
    Input => "input",
    TextArea => "textarea",
    Select => "select",
    /// `<option>`; named to stay clear of `std::option::Option`.
    OptionEl => "option",
    Form => "form",
    Button => "button",
    P => "p",
    Pre => "pre",
    H1 => "h1",
    H2 => "h2",
    H3 => "h3",
    H4 => "h4",
    H5 => "h5",
    H6 => "h6",
    A => "a",
    /// `<template>`; its children live in the contents fragment.
    Template => "template",
}
