//! Lit-flavored template string parser and IR.
//!
//! One parser serves both authoring forms — inline template strings and
//! `.mhtml` files — and both render targets (generated Lit TypeScript, TUI).
//!
//! The dialect is an HTML subset with lit-html binding sigils:
//!
//! - `${name}` — text hole referencing a property or computed property
//! - `attr="a ${name} b"` / `attr=${name}` — attribute value with holes
//! - `?attr=${name}` / `?attr=${!name}` — boolean attribute
//! - `.prop=${name}` — element property
//! - `@event=${handler}` — event handler reference
//! - `<template if=${name}>…</template>` — conditional subtree (`!name` negates;
//!   nesting expresses AND)
//!
//! Hole expressions are deliberately closed to bare identifiers and `!ident`:
//! anything richer belongs in a named computed property or handler on the
//! component, which keeps every template executable by both render targets.
//! The `|` character is reserved inside holes for future formatters.
//!
//! `\${` escapes a literal `${` in text and quoted attribute values.
//! HTML comments (`<!-- … -->`) are skipped.
//! Character entities (`&amp;` …) pass through undecoded.

mod parser;

use std::collections::BTreeSet;

pub use parser::parse;

/// A parsed template: the ordered root nodes of the fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub roots: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    /// Raw text, whitespace preserved (targets normalize as they see fit).
    Text(String),
    /// `${name}` in text position.
    TextHole(Expr),
    /// `<template if=${…}>…</template>`.
    If {
        cond: Expr,
        then: Vec<Node>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// Lowercase tag name; a dash marks a custom element.
    pub tag: String,
    /// Attributes and bindings in authoring order.
    pub attrs: Vec<Attribute>,
    pub children: Vec<Node>,
}

impl Element {
    /// Whether the tag names a custom element (contains a dash).
    pub fn is_custom(&self) -> bool {
        self.tag.contains('-')
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    /// `name`, `name="value"`, or `name=value` without holes.
    Static { name: String, value: String },
    /// `name="a ${x} b"` or `name=${x}` — value assembled from parts.
    Attr { name: String, parts: Vec<AttrPart> },
    /// `?name=${x}`.
    Bool { name: String, expr: Expr },
    /// `.name=${x}`.
    Prop { name: String, expr: Expr },
    /// `@name=${handler}`.
    Event { name: String, handler: String },
}

impl Attribute {
    pub fn name(&self) -> &str {
        match self {
            Attribute::Static { name, .. }
            | Attribute::Attr { name, .. }
            | Attribute::Bool { name, .. }
            | Attribute::Prop { name, .. }
            | Attribute::Event { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrPart {
    Static(String),
    Expr(Expr),
}

/// A hole expression: a property/computed reference, optionally negated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Ident(String),
    Not(String),
}

impl Expr {
    /// The referenced property or computed-property name.
    pub fn ident(&self) -> &str {
        match self {
            Expr::Ident(name) | Expr::Not(name) => name,
        }
    }
}

impl Template {
    /// All property/computed names referenced from holes, in sorted order.
    pub fn referenced_idents(&self) -> BTreeSet<&str> {
        let mut set = BTreeSet::new();
        collect(&self.roots, &mut |node| match node {
            NodeRef::Expr(expr) => {
                set.insert(expr.ident());
            }
            NodeRef::Handler(_) => {}
        });
        set
    }

    /// All handler names referenced from `@event` bindings, in sorted order.
    pub fn referenced_handlers(&self) -> BTreeSet<&str> {
        let mut set = BTreeSet::new();
        collect(&self.roots, &mut |node| match node {
            NodeRef::Expr(_) => {}
            NodeRef::Handler(name) => {
                set.insert(name);
            }
        });
        set
    }

    /// All custom-element tags (containing a dash) used in the template.
    pub fn custom_tags(&self) -> BTreeSet<&str> {
        fn walk<'t>(nodes: &'t [Node], set: &mut BTreeSet<&'t str>) {
            for node in nodes {
                match node {
                    Node::Element(el) => {
                        if el.is_custom() {
                            set.insert(el.tag.as_str());
                        }
                        walk(&el.children, set);
                    }
                    Node::If { then, .. } => walk(then, set),
                    Node::Text(_) | Node::TextHole(_) => {}
                }
            }
        }
        let mut set = BTreeSet::new();
        walk(&self.roots, &mut set);
        set
    }
}

enum NodeRef<'t> {
    Expr(&'t Expr),
    Handler(&'t str),
}

fn collect<'t>(nodes: &'t [Node], f: &mut impl FnMut(NodeRef<'t>)) {
    for node in nodes {
        match node {
            Node::Text(_) => {}
            Node::TextHole(expr) => f(NodeRef::Expr(expr)),
            Node::If { cond, then } => {
                f(NodeRef::Expr(cond));
                collect(then, f);
            }
            Node::Element(el) => {
                for attr in &el.attrs {
                    match attr {
                        Attribute::Static { .. } => {}
                        Attribute::Attr { parts, .. } => {
                            for part in parts {
                                if let AttrPart::Expr(expr) = part {
                                    f(NodeRef::Expr(expr));
                                }
                            }
                        }
                        Attribute::Bool { expr, .. } | Attribute::Prop { expr, .. } => {
                            f(NodeRef::Expr(expr))
                        }
                        Attribute::Event { handler, .. } => f(NodeRef::Handler(handler)),
                    }
                }
                collect(&el.children, f);
            }
        }
    }
}

/// A parse failure, located by byte offset plus 1-based line and column.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message} at line {line}, column {column}")]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpliceError {
    #[error("chrome template contains no <slot/>")]
    NoSlot,
    #[error("chrome template contains more than one <slot/>")]
    MultipleSlots,
    #[error("<slot/> takes no attributes or children")]
    SlotNotEmpty,
}

/// Replaces the single `<slot/>` element of a chrome template with the inner
/// template's roots — the composition primitive behind `wraps_file`.
pub fn splice(chrome: &Template, inner: &Template) -> Result<Template, SpliceError> {
    let mut replaced = false;
    let roots = splice_nodes(&chrome.roots, &inner.roots, &mut replaced)?;
    if !replaced {
        return Err(SpliceError::NoSlot);
    }
    Ok(Template { roots })
}

fn splice_nodes(
    nodes: &[Node],
    inner: &[Node],
    replaced: &mut bool,
) -> Result<Vec<Node>, SpliceError> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Node::Element(el) if el.tag == "slot" => {
                if !el.attrs.is_empty() || !el.children.is_empty() {
                    return Err(SpliceError::SlotNotEmpty);
                }
                if *replaced {
                    return Err(SpliceError::MultipleSlots);
                }
                *replaced = true;
                out.extend(inner.iter().cloned());
            }
            Node::Element(el) => out.push(Node::Element(Element {
                tag: el.tag.clone(),
                attrs: el.attrs.clone(),
                children: splice_nodes(&el.children, inner, replaced)?,
            })),
            Node::If { cond, then } => out.push(Node::If {
                cond: cond.clone(),
                then: splice_nodes(then, inner, replaced)?,
            }),
            Node::Text(_) | Node::TextHole(_) => out.push(node.clone()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod splice_tests {
    use super::*;

    fn t(src: &str) -> Template {
        parse(src).expect("template parses")
    }

    #[test]
    fn slot_parses_as_a_plain_element() {
        let template = t("<slot></slot><slot />");
        assert_eq!(template.roots.len(), 2);
        assert!(template.custom_tags().is_empty());
    }

    #[test]
    fn splice_replaces_the_slot_in_place() {
        let chrome = t("<label>${label}</label><div class=\"wrap\"><slot></slot></div>after");
        let inner = t("<input type=\"text\"><b>x</b>");
        let merged = splice(&chrome, &inner).expect("splices");
        let Node::Element(wrap) = &merged.roots[1] else {
            panic!("expected wrap div");
        };
        assert_eq!(wrap.children.len(), 2);
        assert!(matches!(&wrap.children[0], Node::Element(el) if el.tag == "input"));
        assert_eq!(merged.roots[2], Node::Text("after".into()));
    }

    #[test]
    fn splice_reaches_into_conditionals() {
        let chrome = t("<template if=${label}><slot/></template>");
        let inner = t("<p>x</p>");
        let merged = splice(&chrome, &inner).expect("splices");
        let Node::If { then, .. } = &merged.roots[0] else {
            panic!("expected if");
        };
        assert!(matches!(&then[0], Node::Element(el) if el.tag == "p"));
    }

    #[test]
    fn splice_errors() {
        let inner = t("<p>x</p>");
        assert_eq!(
            splice(&t("<div></div>"), &inner).unwrap_err(),
            SpliceError::NoSlot
        );
        assert_eq!(
            splice(&t("<slot/><slot/>"), &inner).unwrap_err(),
            SpliceError::MultipleSlots
        );
        assert_eq!(
            splice(&t("<slot name=\"input\"></slot>"), &inner).unwrap_err(),
            SpliceError::SlotNotEmpty
        );
        assert_eq!(
            splice(&t("<slot>fallback</slot>"), &inner).unwrap_err(),
            SpliceError::SlotNotEmpty
        );
    }
}
