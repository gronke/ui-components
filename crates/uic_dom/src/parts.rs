//! The lit-parts engine: a template compiles once into a part plan, render
//! targets instantiate the prototype by cloning, and updates patch only the
//! bound parts: lit-html's architecture over the retained tree (ADR 0008).
//!
//! The dialect's holes are NAMED (`${ident}`), so unlike lit no marker
//! sentinels are injected before parsing: the raw source goes through
//! html5ever as-is and the holes are self-marking. Compilation then walks
//! the parsed prototype once: text holes split into comment-marker nodes
//! (the part's stable anchor; real node identity replaces lit's start/end
//! marker pair), bound attributes classify by their lit prefix (`.` property,
//! `?` boolean, `@` event; `<template if=${…}>` is the conditional) and are
//! removed from the prototype, and attribute-name case is recovered from the
//! source by index, exactly like lit's side array.
//!
//! The engine is value-agnostic: holes carry their raw expression text, and
//! `commit` takes one [`PartValue`] per hole; resolving expressions against
//! component state is the consumer's job. The tree owns what the tree can
//! hold (child content, attributes, boolean attributes); property and event
//! parts surface as data for the runtime glue.

use std::collections::HashMap;

use uic_core::Value;

use crate::tree::{Document, NodeData, NodeId};

/// A compiled template: the parsed prototype plus the plan locating every
/// bound part, with holes numbered in source order across all conditional
/// branches.
pub struct CompiledTemplate {
    prototype: Document,
    plan: Vec<PartSpec>,
    /// Conditional branch bodies, referenced by index from the plan.
    branches: Vec<Branch>,
    /// Raw hole expressions (`ident`, `!ident`, …) in source order.
    holes: Vec<String>,
}

/// One conditional branch: the prototype's contents fragment and the plan
/// over it.
struct Branch {
    fragment: NodeId,
    plan: Vec<PartSpec>,
}

/// One bound part in the prototype; `node` is a prototype id resolved
/// through the import map at instantiation.
struct PartSpec {
    node: NodeId,
    kind: SpecKind,
}

enum SpecKind {
    /// A comment marker in child position; committed content follows it.
    Child { hole: usize },
    /// A plain attribute, possibly assembled from several holes between
    /// static chunks (`strings.len() == holes.len() + 1`).
    Attribute {
        name: String,
        strings: Vec<String>,
        holes: Vec<usize>,
    },
    /// `?name=${x}`: present or absent.
    BooleanAttribute { name: String, hole: usize },
    /// `.name=${x}`: surfaced to the consumer, the tree holds no properties.
    Property { name: String, hole: usize },
    /// `@name=${handler}`: surfaced to the consumer at instantiation.
    Event { name: String, handler: String },
    /// `<template if=${x}>`: the anchor is the template element itself.
    Conditional { hole: usize, branch: usize },
    /// `<template for=${each} as=item>`: the anchor is the template element;
    /// the body holes live in their own space, resolved per row by the
    /// caller and delivered as a [`PartValue::List`] (ADR 0001).
    Repeat {
        hole: usize,
        branch: usize,
        item: String,
        body_holes: Vec<String>,
    },
}

/// The value a hole resolves to for one commit.
#[derive(Debug, Clone, PartialEq)]
pub enum PartValue {
    Text(String),
    Bool(bool),
    /// An object value passed through to property parts (child and
    /// attribute parts commit its display text).
    Value(Value),
    /// lit's `nothing`: clear the part (remove the attribute, empty the
    /// child position, tear down the branch).
    Nothing,
    /// lit's `noChange`: keep whatever is committed.
    NoChange,
    /// A repeat's resolved rows: one inner vector of body-hole values per
    /// row, in body-hole order (ADR 0001). Only a repeat's `each` hole
    /// carries it.
    List(Vec<Vec<PartValue>>),
}

impl PartValue {
    fn as_text(&self) -> Option<String> {
        match self {
            PartValue::Text(text) => Some(text.clone()),
            PartValue::Bool(b) => Some(b.to_string()),
            PartValue::Value(value) => Some(value.display_text()),
            PartValue::Nothing | PartValue::NoChange | PartValue::List(_) => None,
        }
    }

    fn truthy(&self) -> bool {
        match self {
            PartValue::Text(text) => !text.is_empty(),
            PartValue::Bool(b) => *b,
            PartValue::Value(value) => value.truthy(),
            PartValue::List(rows) => !rows.is_empty(),
            PartValue::Nothing | PartValue::NoChange => false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CompileError {
    #[error("unterminated hole (`${{` without `}}`)")]
    UnterminatedHole,
    #[error("`{0}` bindings take a single `${{…}}` hole")]
    CompositeBinding(String),
    #[error("bound attribute names diverge between source and parse: {0}")]
    NameRecovery(String),
}

/// A live instantiation of a compiled template inside a target document.
pub struct TemplateInstance {
    /// The cloned top-level nodes, in order.
    roots: Vec<NodeId>,
    parts: Vec<Part>,
}

enum Part {
    Child {
        marker: NodeId,
        hole: usize,
        /// The single committed text node, created on first commit.
        committed: Option<NodeId>,
        last: Option<PartValue>,
    },
    Attribute {
        node: NodeId,
        name: String,
        strings: Vec<String>,
        holes: Vec<usize>,
        last: Option<Vec<PartValue>>,
    },
    BooleanAttribute {
        node: NodeId,
        name: String,
        hole: usize,
        last: Option<bool>,
    },
    Property {
        node: NodeId,
        name: String,
        hole: usize,
        last: Option<PartValue>,
    },
    Conditional {
        anchor: NodeId,
        hole: usize,
        branch: usize,
        active: Option<ActiveBranch>,
    },
    Repeat {
        anchor: NodeId,
        hole: usize,
        branch: usize,
        /// One live branch instance per rendered row, in row order.
        instances: Vec<ActiveBranch>,
        last: Option<Vec<Vec<PartValue>>>,
    },
}

/// An instantiated conditional body: its top-level nodes (all following the
/// anchor) and the live parts inside them.
struct ActiveBranch {
    nodes: Vec<NodeId>,
    parts: Vec<Part>,
}

/// An `@event=${handler}` binding surfaced at instantiation, for the
/// consumer to wire listeners.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBinding {
    pub node: NodeId,
    pub event: String,
    pub handler: String,
}

/// A repeat's shape, so the caller can resolve the body holes per row: the
/// index (into [`CompiledTemplate::holes`]) of the array hole, the loop
/// variable, and the body-hole expressions in commit order (ADR 0001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatMeta {
    pub each_hole: usize,
    pub item: String,
    pub body_holes: Vec<String>,
    /// Repeats nested in this body (ADR 0001): their `each_hole` indexes THIS
    /// meta's `body_holes`, and their rows resolve with this loop's variable
    /// still in scope. The caller places their resolved lists at that body
    /// slot, nesting `PartValue::List` values.
    pub nested: Vec<RepeatMeta>,
}

/// A `.prop=${x}` write produced by a commit, for the consumer to apply.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyWrite {
    pub node: NodeId,
    pub name: String,
    pub value: PartValue,
}

/// What one commit asks of the consumer beyond the tree.
#[derive(Debug, Default)]
pub struct CommitEffects {
    pub property_writes: Vec<PropertyWrite>,
    /// Bindings inside conditional branches instantiated by this commit.
    pub added_events: Vec<EventBinding>,
}

impl CompiledTemplate {
    /// Parses and compiles the template source (the lit-flavored dialect).
    pub fn compile(source: &str) -> Result<CompiledTemplate, CompileError> {
        let mut prototype: Document = Document::parse_fragment(source, "body");
        let mut compiler = Compiler {
            holes: Vec::new(),
            branches: Vec::new(),
            source_names: bound_attribute_names(source)?,
            next_name: 0,
        };
        let roots: Vec<NodeId> = prototype.children(prototype.root()).collect();
        let plan = compiler.walk(&mut prototype, &roots)?;
        if compiler.next_name != compiler.source_names.len() {
            return Err(CompileError::NameRecovery(format!(
                "{} bound in source, {} found in the tree",
                compiler.source_names.len(),
                compiler.next_name
            )));
        }
        Ok(CompiledTemplate {
            prototype,
            plan,
            branches: compiler.branches,
            holes: compiler.holes,
        })
    }

    /// Compiles the already-parsed template IR: the runtime path, where
    /// components carry their (chrome-spliced) `uic_template::Template`.
    /// No HTML parse runs and no case recovery is needed: the IR preserves
    /// authored names by construction. Semantics match [`Self::compile`]:
    /// same prototype shape, same plan, same hole numbering rules.
    pub fn from_template(template: &uic_template::Template) -> CompiledTemplate {
        let mut prototype: Document = Document::new();
        let mut builder = IrBuilder {
            holes: Vec::new(),
            branches: Vec::new(),
        };
        let root = prototype.root();
        let mut plan = Vec::new();
        builder.build_nodes(&mut prototype, root, &template.roots, &mut plan);
        CompiledTemplate {
            prototype,
            plan,
            branches: builder.branches,
            holes: builder.holes,
        }
    }

    /// Raw hole expressions in commit order; `commit` takes one value each.
    pub fn holes(&self) -> &[String] {
        &self.holes
    }

    /// The repeat tree of the template, so the caller resolves the body holes
    /// per row and hands back a [`PartValue::List`] at each repeat's
    /// `each_hole` (ADR 0001). Repeats inside conditional branches are
    /// included at their level; repeats nested inside another repeat body
    /// appear under their parent's `nested`, with the parent's loop variable
    /// still in scope for their rows.
    pub fn repeats(&self) -> Vec<RepeatMeta> {
        self.collect_repeats(&self.plan)
    }

    fn collect_repeats(&self, plan: &[PartSpec]) -> Vec<RepeatMeta> {
        let mut out = Vec::new();
        for spec in plan {
            match &spec.kind {
                SpecKind::Repeat {
                    hole,
                    branch,
                    item,
                    body_holes,
                } => out.push(RepeatMeta {
                    each_hole: *hole,
                    item: item.clone(),
                    body_holes: body_holes.clone(),
                    nested: self.collect_repeats(&self.branches[*branch].plan),
                }),
                SpecKind::Conditional { branch, .. } => {
                    out.extend(self.collect_repeats(&self.branches[*branch].plan))
                }
                _ => {}
            }
        }
        out
    }

    /// Clones the prototype under `parent` and binds the plan to the copy.
    /// The returned event bindings are the top-level `@event` parts; wiring
    /// them is the consumer's job (conditional branches surface theirs
    /// through [`CommitEffects`]).
    pub fn instantiate<T: Default>(
        &self,
        doc: &mut Document<T>,
        parent: NodeId,
    ) -> (TemplateInstance, Vec<EventBinding>) {
        let mut map = HashMap::new();
        let roots: Vec<NodeId> = self
            .prototype
            .children(self.prototype.root())
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|node| {
                let copy = doc.import_node(&self.prototype, node, &mut map)?;
                doc.append_child(parent, copy);
                Some(copy)
            })
            .collect();
        let mut events = Vec::new();
        let parts = resolve_parts(&self.plan, &map, &mut events);
        (TemplateInstance { roots, parts }, events)
    }

    /// Patches the instance: one [`PartValue`] per hole (see [`Self::holes`]).
    /// Dirty-checked per part; `NoChange` always keeps the committed state.
    pub fn commit<T: Default>(
        &self,
        instance: &mut TemplateInstance,
        doc: &mut Document<T>,
        values: &[PartValue],
    ) -> CommitEffects {
        let mut effects = CommitEffects::default();
        commit_parts(self, &mut instance.parts, doc, values, &mut effects);
        effects
    }
}

impl TemplateInstance {
    /// The instantiated top-level nodes, in template order.
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }
}

// -- compilation from the parsed IR ------------------------------------------

struct IrBuilder {
    holes: Vec<String>,
    branches: Vec<Branch>,
}

impl IrBuilder {
    fn build_nodes(
        &mut self,
        doc: &mut Document,
        parent: NodeId,
        nodes: &[uic_template::Node],
        plan: &mut Vec<PartSpec>,
    ) {
        use uic_template::Node;
        for node in nodes {
            match node {
                Node::Text(text) => {
                    let text_node = doc.create_text_node(text);
                    doc.append_child(parent, text_node);
                }
                Node::TextHole(expr) => {
                    let marker = doc.create_comment("uic-part");
                    doc.append_child(parent, marker);
                    plan.push(PartSpec {
                        node: marker,
                        kind: SpecKind::Child {
                            hole: self.push_hole(expr),
                        },
                    });
                }
                Node::If { cond, then } => {
                    // The anchor clones empty; the branch body lives in a
                    // detached prototype fragment, like the parsed path.
                    let anchor = doc.create_element_named("template");
                    doc.element_mut(anchor)
                        .expect("just created")
                        .template_contents = None;
                    doc.append_child(parent, anchor);
                    let fragment = doc.create_fragment();
                    let branch = self.branches.len();
                    self.branches.push(Branch {
                        fragment,
                        plan: Vec::new(),
                    });
                    plan.push(PartSpec {
                        node: anchor,
                        kind: SpecKind::Conditional {
                            hole: self.push_hole(cond),
                            branch,
                        },
                    });
                    let mut branch_plan = Vec::new();
                    self.build_nodes(doc, fragment, then, &mut branch_plan);
                    self.branches[branch].plan = branch_plan;
                }
                Node::For { each, item, body } => {
                    let anchor = doc.create_element_named("template");
                    doc.element_mut(anchor)
                        .expect("just created")
                        .template_contents = None;
                    doc.append_child(parent, anchor);
                    let fragment = doc.create_fragment();
                    let branch = self.branches.len();
                    self.branches.push(Branch {
                        fragment,
                        plan: Vec::new(),
                    });
                    // The array reference is a top-level hole; the body holes
                    // live in their own space, numbered from zero, resolved
                    // per row by the caller (ADR 0001).
                    let each_hole = self.push_hole(each);
                    let outer = std::mem::take(&mut self.holes);
                    let mut branch_plan = Vec::new();
                    self.build_nodes(doc, fragment, body, &mut branch_plan);
                    let body_holes = std::mem::replace(&mut self.holes, outer);
                    self.branches[branch].plan = branch_plan;
                    plan.push(PartSpec {
                        node: anchor,
                        kind: SpecKind::Repeat {
                            hole: each_hole,
                            branch,
                            item: item.clone(),
                            body_holes,
                        },
                    });
                }
                Node::Element(el) => {
                    let element = doc.create_element_named(&el.tag);
                    doc.append_child(parent, element);
                    self.build_attributes(doc, element, &el.attrs, plan);
                    self.build_nodes(doc, element, &el.children, plan);
                }
            }
        }
    }

    fn build_attributes(
        &mut self,
        doc: &mut Document,
        element: NodeId,
        attrs: &[uic_template::Attribute],
        plan: &mut Vec<PartSpec>,
    ) {
        use uic_template::{AttrPart, Attribute};
        for attr in attrs {
            match attr {
                Attribute::Static { name, value } => doc.set_attribute(element, name, value),
                Attribute::Attr { name, parts } => {
                    let mut strings = vec![String::new()];
                    let mut holes = Vec::new();
                    for part in parts {
                        match part {
                            AttrPart::Static(text) => {
                                strings.last_mut().expect("seeded").push_str(text)
                            }
                            AttrPart::Expr(expr) => {
                                holes.push(self.push_hole(expr));
                                strings.push(String::new());
                            }
                        }
                    }
                    plan.push(PartSpec {
                        node: element,
                        kind: SpecKind::Attribute {
                            name: name.clone(),
                            strings,
                            holes,
                        },
                    });
                }
                Attribute::Bool { name, expr } => plan.push(PartSpec {
                    node: element,
                    kind: SpecKind::BooleanAttribute {
                        name: name.clone(),
                        hole: self.push_hole(expr),
                    },
                }),
                Attribute::Prop { name, expr } => plan.push(PartSpec {
                    node: element,
                    kind: SpecKind::Property {
                        name: name.clone(),
                        hole: self.push_hole(expr),
                    },
                }),
                Attribute::Event { name, handler } => plan.push(PartSpec {
                    node: element,
                    kind: SpecKind::Event {
                        name: name.clone(),
                        handler: handler.clone(),
                    },
                }),
            }
        }
    }

    /// Holes carry the same raw spelling the source path produces.
    fn push_hole(&mut self, expr: &uic_template::Expr) -> usize {
        self.holes.push(hole_text(expr));
        self.holes.len() - 1
    }
}

/// The raw spelling of a hole expression, matching the source path.
fn hole_text(expr: &uic_template::Expr) -> String {
    use uic_template::Expr;
    match expr {
        Expr::Ident(name) => name.clone(),
        Expr::Not(name) => format!("!{name}"),
        Expr::Member { base, field } => format!("{base}.{field}"),
    }
}

// -- compilation ------------------------------------------------------------

struct Compiler {
    holes: Vec<String>,
    branches: Vec<Branch>,
    /// Case-sensitive bound attribute names in source order (the parser
    /// lowercases; lit recovers the same way).
    source_names: Vec<String>,
    next_name: usize,
}

impl Compiler {
    fn walk(
        &mut self,
        doc: &mut Document,
        nodes: &[NodeId],
    ) -> Result<Vec<PartSpec>, CompileError> {
        let mut plan = Vec::new();
        for &node in nodes {
            self.compile_node(doc, node, &mut plan)?;
        }
        Ok(plan)
    }

    fn compile_node(
        &mut self,
        doc: &mut Document,
        node: NodeId,
        plan: &mut Vec<PartSpec>,
    ) -> Result<(), CompileError> {
        match doc.node(node) {
            Some(NodeData::Element(_)) => {
                self.compile_attributes(doc, node, plan)?;
                if doc
                    .element(node)
                    .and_then(|el| el.template_contents)
                    .is_some()
                {
                    // A conditional's body compiles into its branch plan and
                    // the anchor's contents link is severed: instances clone
                    // an empty anchor, and the body clones from the branch
                    // on demand. An unbound template keeps its contents.
                    if let Some(PartSpec {
                        node: spec_node,
                        kind: SpecKind::Conditional { branch, .. },
                    }) = plan.last()
                    {
                        if *spec_node == node {
                            let branch = *branch;
                            let fragment = self.branches[branch].fragment;
                            let children: Vec<NodeId> = doc.children(fragment).collect();
                            self.branches[branch].plan = self.walk(doc, &children)?;
                            doc.element_mut(node)
                                .expect("the anchor is an element")
                                .template_contents = None;
                        }
                    }
                    return Ok(());
                }
                let children: Vec<NodeId> = doc.children(node).collect();
                for child in children {
                    self.compile_node(doc, child, plan)?;
                }
            }
            Some(NodeData::Text(_)) => self.compile_text(doc, node, plan)?,
            _ => {}
        }
        Ok(())
    }

    fn compile_attributes(
        &mut self,
        doc: &mut Document,
        node: NodeId,
        plan: &mut Vec<PartSpec>,
    ) -> Result<(), CompileError> {
        let is_template = doc.tag_name(node).map(|tag| &**tag == "template") == Some(true);
        let attrs: Vec<(String, String)> = doc
            .element(node)
            .map(|el| {
                el.attrs()
                    .map(|(name, value)| (name.to_string(), value.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        for (parsed_name, value) in attrs {
            if !value.contains("${") {
                continue;
            }
            let name = self.recover_name(&parsed_name)?;
            let (strings, exprs) = split_holes(&value)?;
            let single = strings.len() == 2 && strings[0].is_empty() && strings[1].is_empty();
            doc.remove_attribute(node, &parsed_name);
            let kind = if is_template && name == "if" {
                if !single {
                    return Err(CompileError::CompositeBinding(name));
                }
                let branch = self.branches.len();
                let fragment = doc
                    .element(node)
                    .and_then(|el| el.template_contents)
                    .expect("templates carry a contents fragment");
                self.branches.push(Branch {
                    fragment,
                    plan: Vec::new(),
                });
                SpecKind::Conditional {
                    hole: self.push_hole(&exprs[0]),
                    branch,
                }
            } else if let Some(prop) = name.strip_prefix('.') {
                if !single {
                    return Err(CompileError::CompositeBinding(name));
                }
                SpecKind::Property {
                    name: prop.to_string(),
                    hole: self.push_hole(&exprs[0]),
                }
            } else if let Some(attr) = name.strip_prefix('?') {
                if !single {
                    return Err(CompileError::CompositeBinding(name));
                }
                SpecKind::BooleanAttribute {
                    name: attr.to_string(),
                    hole: self.push_hole(&exprs[0]),
                }
            } else if let Some(event) = name.strip_prefix('@') {
                if !single {
                    return Err(CompileError::CompositeBinding(name));
                }
                SpecKind::Event {
                    name: event.to_string(),
                    handler: exprs[0].clone(),
                }
            } else {
                SpecKind::Attribute {
                    name,
                    strings,
                    holes: exprs.iter().map(|expr| self.push_hole(expr)).collect(),
                }
            };
            plan.push(PartSpec { node, kind });
        }
        Ok(())
    }

    /// Splits a text node with holes into static text nodes and comment
    /// markers; the markers are the child parts' stable anchors.
    fn compile_text(
        &mut self,
        doc: &mut Document,
        node: NodeId,
        plan: &mut Vec<PartSpec>,
    ) -> Result<(), CompileError> {
        let Some(text) = doc.text(node).map(str::to_string) else {
            return Ok(());
        };
        if !text.contains("${") {
            return Ok(());
        }
        let (strings, exprs) = split_holes(&text)?;
        for (index, string) in strings.iter().enumerate() {
            if !string.is_empty() {
                let chunk = doc.create_text_node(string);
                doc.insert_before(chunk, node);
            }
            if index < exprs.len() {
                let marker = doc.create_comment("uic-part");
                doc.insert_before(marker, node);
                plan.push(PartSpec {
                    node: marker,
                    kind: SpecKind::Child {
                        hole: self.push_hole(&exprs[index]),
                    },
                });
            }
        }
        doc.remove(node);
        Ok(())
    }

    fn push_hole(&mut self, expr: &str) -> usize {
        self.holes.push(expr.to_string());
        self.holes.len() - 1
    }

    /// The next case-sensitive name from the source, checked against the
    /// parser's lowercased spelling.
    fn recover_name(&mut self, parsed: &str) -> Result<String, CompileError> {
        let Some(name) = self.source_names.get(self.next_name) else {
            return Err(CompileError::NameRecovery(format!(
                "no source name left for `{parsed}`"
            )));
        };
        if name.to_ascii_lowercase() != parsed {
            return Err(CompileError::NameRecovery(format!(
                "`{name}` (source) vs `{parsed}` (tree)"
            )));
        }
        self.next_name += 1;
        Ok(name.clone())
    }
}

/// Bound attribute names (value contains a hole) in source order, with
/// their original case. `\${` escapes a literal hole.
fn bound_attribute_names(source: &str) -> Result<Vec<String>, CompileError> {
    let bytes = source.as_bytes();
    let mut names = Vec::new();
    let mut i = 0;
    let mut in_tag = false;
    while i < bytes.len() {
        match bytes[i] {
            b'<' if !in_tag => in_tag = true,
            b'>' if in_tag => in_tag = false,
            b'=' if in_tag => {
                // Walk back over the attribute name.
                let mut start = i;
                while start > 0
                    && !bytes[start - 1].is_ascii_whitespace()
                    && bytes[start - 1] != b'<'
                {
                    start -= 1;
                }
                let name = &source[start..i];
                // Walk forward over the value, quoted or bare.
                let mut j = i + 1;
                let bound = match bytes.get(j) {
                    Some(b'"') | Some(b'\'') => {
                        let quote = bytes[j];
                        j += 1;
                        let value_start = j;
                        while j < bytes.len() && bytes[j] != quote {
                            j += 1;
                        }
                        source[value_start..j].contains("${")
                            && !source[value_start..j].contains("\\${")
                            || contains_unescaped_hole(&source[value_start..j])
                    }
                    _ => {
                        let value_start = j;
                        while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>'
                        {
                            j += 1;
                        }
                        contains_unescaped_hole(&source[value_start..j])
                    }
                };
                if bound && !name.is_empty() {
                    names.push(name.to_string());
                }
                i = j;
            }
            _ => {}
        }
        i += 1;
    }
    Ok(names)
}

fn contains_unescaped_hole(value: &str) -> bool {
    let bytes = value.as_bytes();
    for (index, window) in bytes.windows(2).enumerate() {
        if window == b"${" && (index == 0 || bytes[index - 1] != b'\\') {
            return true;
        }
    }
    false
}

/// Splits `pre ${a} mid ${b} post` into statics and hole expressions;
/// `\${` stays literal text.
fn split_holes(value: &str) -> Result<(Vec<String>, Vec<String>), CompileError> {
    let mut strings = Vec::new();
    let mut exprs = Vec::new();
    let mut current = String::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && value[i..].starts_with("\\${") {
            current.push_str("${");
            i += 3;
            continue;
        }
        if value[i..].starts_with("${") {
            let Some(end) = value[i + 2..].find('}') else {
                return Err(CompileError::UnterminatedHole);
            };
            strings.push(std::mem::take(&mut current));
            exprs.push(value[i + 2..i + 2 + end].trim().to_string());
            i += 2 + end + 1;
            continue;
        }
        // Advance one character (not one byte).
        let ch = value[i..].chars().next().expect("in bounds");
        current.push(ch);
        i += ch.len_utf8();
    }
    strings.push(current);
    Ok((strings, exprs))
}

// -- instantiation and commit ------------------------------------------------

fn resolve_parts(
    plan: &[PartSpec],
    map: &HashMap<NodeId, NodeId>,
    events: &mut Vec<EventBinding>,
) -> Vec<Part> {
    let mut parts = Vec::new();
    for spec in plan {
        let Some(&node) = map.get(&spec.node) else {
            continue;
        };
        match &spec.kind {
            SpecKind::Child { hole } => parts.push(Part::Child {
                marker: node,
                hole: *hole,
                committed: None,
                last: None,
            }),
            SpecKind::Attribute {
                name,
                strings,
                holes,
            } => parts.push(Part::Attribute {
                node,
                name: name.clone(),
                strings: strings.clone(),
                holes: holes.clone(),
                last: None,
            }),
            SpecKind::BooleanAttribute { name, hole } => parts.push(Part::BooleanAttribute {
                node,
                name: name.clone(),
                hole: *hole,
                last: None,
            }),
            SpecKind::Property { name, hole } => parts.push(Part::Property {
                node,
                name: name.clone(),
                hole: *hole,
                last: None,
            }),
            SpecKind::Event { name, handler } => events.push(EventBinding {
                node,
                event: name.clone(),
                handler: handler.clone(),
            }),
            SpecKind::Conditional { hole, branch } => parts.push(Part::Conditional {
                anchor: node,
                hole: *hole,
                branch: *branch,
                active: None,
            }),
            SpecKind::Repeat { hole, branch, .. } => parts.push(Part::Repeat {
                anchor: node,
                hole: *hole,
                branch: *branch,
                instances: Vec::new(),
                last: None,
            }),
        }
    }
    parts
}

/// Removes every active nested branch below the given parts: the sibling
/// insertions their anchors accumulated, which the enclosing branch's node
/// list does not cover.
fn teardown_branches<T: Default>(doc: &mut Document<T>, parts: &mut [Part]) {
    for part in parts {
        match part {
            Part::Conditional { active, .. } => {
                if let Some(mut inner) = active.take() {
                    teardown_branches(doc, &mut inner.parts);
                    for node in inner.nodes {
                        doc.remove(node);
                    }
                }
            }
            Part::Repeat { instances, .. } => {
                for mut inner in std::mem::take(instances) {
                    teardown_branches(doc, &mut inner.parts);
                    for node in inner.nodes {
                        doc.remove(node);
                    }
                }
            }
            _ => {}
        }
    }
}

fn commit_parts<T: Default>(
    template: &CompiledTemplate,
    parts: &mut [Part],
    doc: &mut Document<T>,
    values: &[PartValue],
    effects: &mut CommitEffects,
) {
    for part in parts {
        match part {
            Part::Child {
                marker,
                hole,
                committed,
                last,
            } => {
                let value = &values[*hole];
                if matches!(value, PartValue::NoChange) || last.as_ref() == Some(value) {
                    continue;
                }
                match value.as_text() {
                    Some(text) => match committed {
                        Some(node) => doc.set_text(*node, &text),
                        None => {
                            let node = doc.create_text_node(&text);
                            doc.insert_after(node, *marker);
                            *committed = Some(node);
                        }
                    },
                    None => {
                        if let Some(node) = committed.take() {
                            doc.remove(node);
                        }
                    }
                }
                *last = Some(value.clone());
            }
            Part::Attribute {
                node,
                name,
                strings,
                holes,
                last,
            } => {
                let slice: Vec<PartValue> =
                    holes.iter().map(|hole| values[*hole].clone()).collect();
                let merged: Vec<PartValue> = slice
                    .iter()
                    .enumerate()
                    .map(|(index, value)| match value {
                        PartValue::NoChange => last
                            .as_ref()
                            .map(|previous| previous[index].clone())
                            .unwrap_or(PartValue::Nothing),
                        other => other.clone(),
                    })
                    .collect();
                if last.as_ref() == Some(&merged) {
                    continue;
                }
                let single = holes.len() == 1 && strings[0].is_empty() && strings[1].is_empty();
                if single && matches!(merged[0], PartValue::Nothing) {
                    // A single-hole nothing removes the attribute, like lit.
                    doc.remove_attribute(*node, name);
                } else {
                    let mut assembled = String::new();
                    for (index, string) in strings.iter().enumerate() {
                        assembled.push_str(string);
                        if index < merged.len() {
                            // In multi-hole values nothing renders empty.
                            if let Some(text) = merged[index].as_text() {
                                assembled.push_str(&text);
                            }
                        }
                    }
                    doc.set_attribute(*node, name, &assembled);
                }
                *last = Some(merged);
            }
            Part::BooleanAttribute {
                node,
                name,
                hole,
                last,
            } => {
                let value = &values[*hole];
                if matches!(value, PartValue::NoChange) {
                    continue;
                }
                let on = value.truthy();
                if *last == Some(on) {
                    continue;
                }
                if on {
                    doc.set_attribute(*node, name, "");
                } else {
                    doc.remove_attribute(*node, name);
                }
                *last = Some(on);
            }
            Part::Property {
                node,
                name,
                hole,
                last,
            } => {
                let value = &values[*hole];
                if matches!(value, PartValue::NoChange) || last.as_ref() == Some(value) {
                    continue;
                }
                effects.property_writes.push(PropertyWrite {
                    node: *node,
                    name: name.clone(),
                    value: value.clone(),
                });
                *last = Some(value.clone());
            }
            Part::Conditional {
                anchor,
                hole,
                branch,
                active,
            } => {
                let value = &values[*hole];
                if matches!(value, PartValue::NoChange) {
                    if let Some(active) = active {
                        commit_parts(template, &mut active.parts, doc, values, effects);
                    }
                    continue;
                }
                let on = value.truthy();
                match (on, active.as_mut()) {
                    (true, Some(active)) => {
                        // Already rendered: patch the branch's own parts.
                        commit_parts(template, &mut active.parts, doc, values, effects);
                    }
                    (true, None) => {
                        let spec = &template.branches[*branch];
                        let mut map = HashMap::new();
                        let children: Vec<NodeId> =
                            template.prototype.children(spec.fragment).collect();
                        let mut nodes = Vec::new();
                        let mut previous = *anchor;
                        for child in children {
                            if let Some(copy) =
                                doc.import_node(&template.prototype, child, &mut map)
                            {
                                doc.insert_after(copy, previous);
                                previous = copy;
                                nodes.push(copy);
                            }
                        }
                        let mut parts = resolve_parts(&spec.plan, &map, &mut effects.added_events);
                        commit_parts(template, &mut parts, doc, values, effects);
                        *active = Some(ActiveBranch { nodes, parts });
                    }
                    (false, _) => {
                        if let Some(mut branch) = active.take() {
                            // Nested branches inserted their nodes as
                            // siblings beside their anchors; tear them down
                            // first, then this branch's own nodes.
                            teardown_branches(doc, &mut branch.parts);
                            for node in branch.nodes {
                                doc.remove(node);
                            }
                        }
                    }
                }
            }
            Part::Repeat {
                anchor,
                hole,
                branch,
                instances,
                last,
            } => {
                let value = &values[*hole];
                if matches!(value, PartValue::NoChange) {
                    continue;
                }
                // Anything but a row list (an empty or null array) clears.
                let rows: &[Vec<PartValue>] = match value {
                    PartValue::List(rows) => rows,
                    _ => &[],
                };
                if last.as_deref() == Some(rows) {
                    continue;
                }
                // Rebuild the rows on any change (ADR 0001): tear the old
                // instances down, then clone one branch per row after the
                // anchor, in order.
                for mut inner in std::mem::take(instances) {
                    teardown_branches(doc, &mut inner.parts);
                    for node in inner.nodes {
                        doc.remove(node);
                    }
                }
                let spec = &template.branches[*branch];
                let mut previous = *anchor;
                for row in rows {
                    let mut map = HashMap::new();
                    let children: Vec<NodeId> =
                        template.prototype.children(spec.fragment).collect();
                    let mut nodes = Vec::new();
                    for child in children {
                        if let Some(copy) = doc.import_node(&template.prototype, child, &mut map) {
                            doc.insert_after(copy, previous);
                            previous = copy;
                            nodes.push(copy);
                        }
                    }
                    let mut parts = resolve_parts(&spec.plan, &map, &mut effects.added_events);
                    commit_parts(template, &mut parts, doc, row, effects);
                    instances.push(ActiveBranch { nodes, parts });
                }
                *last = Some(rows.to_vec());
            }
        }
    }
}

#[cfg(test)]
mod repeat_tests {
    use super::*;
    use crate::html;

    fn mount(src: &str) -> (CompiledTemplate, Document, NodeId, TemplateInstance) {
        let template = CompiledTemplate::from_template(&uic_template::parse(src).expect("parses"));
        let mut doc: Document = Document::new();
        let host = doc.create_element(html::Div);
        let root = doc.root();
        doc.append_child(root, host);
        let (instance, _events) = template.instantiate(&mut doc, host);
        (template, doc, host, instance)
    }

    fn rows(names: &[&str]) -> PartValue {
        PartValue::List(
            names
                .iter()
                .map(|name| vec![PartValue::Text((*name).to_string())])
                .collect(),
        )
    }

    fn li_count(html: &str) -> usize {
        html.matches("<li>").count()
    }

    #[test]
    fn repeat_exposes_its_shape() {
        let (template, ..) =
            mount("<ul><template for=${people} as=p><li>${p.name}</li></template></ul>");
        assert_eq!(template.holes(), ["people"]);
        let repeats = template.repeats();
        assert_eq!(repeats.len(), 1);
        assert_eq!(repeats[0].each_hole, 0);
        assert_eq!(repeats[0].item, "p");
        assert_eq!(repeats[0].body_holes, ["p.name"]);
    }

    #[test]
    fn repeat_renders_one_instance_per_row_and_rebuilds_on_change() {
        let (template, mut doc, host, mut instance) =
            mount("<ul><template for=${people} as=p><li>${p.name}</li></template></ul>");

        template.commit(&mut instance, &mut doc, &[rows(&["Alice", "Bob"])]);
        let html = doc.outer_html(host);
        assert_eq!(li_count(&html), 2, "{html}");
        assert!(html.contains("Alice") && html.contains("Bob"), "{html}");

        // A shorter list tears the old rows down and rebuilds.
        template.commit(&mut instance, &mut doc, &[rows(&["Zoe"])]);
        let html = doc.outer_html(host);
        assert_eq!(li_count(&html), 1, "{html}");
        assert!(html.contains("Zoe"), "{html}");
        assert!(!html.contains("Alice") && !html.contains("Bob"), "{html}");

        // The empty list clears every row.
        template.commit(&mut instance, &mut doc, &[rows(&[])]);
        assert_eq!(li_count(&doc.outer_html(host)), 0);
    }

    #[test]
    fn nested_repeats_expose_a_tree_and_render_inner_rows() {
        let (template, mut doc, host, mut instance) = mount(
            "<template for=${cards} as=card><h3>${card.title}</h3>\
             <template for=${card.rows} as=r><p>${r.a}</p></template></template>",
        );

        // The tree: one top-level repeat whose body holds the title hole and
        // the nested repeat's each hole; the inner body holds the cell hole.
        assert_eq!(template.holes(), ["cards"]);
        let repeats = template.repeats();
        assert_eq!(repeats.len(), 1);
        assert_eq!(repeats[0].body_holes, ["card.title", "card.rows"]);
        assert_eq!(repeats[0].nested.len(), 1);
        assert_eq!(repeats[0].nested[0].each_hole, 1);
        assert_eq!(repeats[0].nested[0].body_holes, ["r.a"]);

        // Two cards, the first with two rows, the second with one.
        let card = |title: &str, rows: &[&str]| {
            vec![
                PartValue::Text(title.to_string()),
                PartValue::List(
                    rows.iter()
                        .map(|row| vec![PartValue::Text((*row).to_string())])
                        .collect(),
                ),
            ]
        };
        template.commit(
            &mut instance,
            &mut doc,
            &[PartValue::List(vec![
                card("Alpha", &["a1", "a2"]),
                card("Beta", &["b1"]),
            ])],
        );
        let html = doc.outer_html(host);
        for text in ["Alpha", "a1", "a2", "Beta", "b1"] {
            assert!(html.contains(text), "missing {text}: {html}");
        }
        assert_eq!(html.matches("<p>").count(), 3, "{html}");

        // A change rebuilds the whole tree.
        template.commit(
            &mut instance,
            &mut doc,
            &[PartValue::List(vec![card("Gamma", &["g1"])])],
        );
        let html = doc.outer_html(host);
        assert!(html.contains("Gamma") && html.contains("g1"), "{html}");
        assert!(!html.contains("Alpha") && !html.contains("b1"), "{html}");
        assert_eq!(html.matches("<p>").count(), 1, "{html}");
    }

    #[test]
    fn repeat_body_carries_several_columns() {
        let (template, mut doc, host, mut instance) = mount(
            "<table><template for=${rows} as=r><tr><td>${r.a}</td><td>${r.b}</td></tr></template></table>",
        );
        assert_eq!(template.repeats()[0].body_holes, ["r.a", "r.b"]);
        let list = PartValue::List(vec![
            vec![PartValue::Text("1".into()), PartValue::Text("2".into())],
            vec![PartValue::Text("3".into()), PartValue::Text("4".into())],
        ]);
        template.commit(&mut instance, &mut doc, &[list]);
        let html = doc.outer_html(host);
        assert_eq!(html.matches("<tr>").count(), 2, "{html}");
        for cell in ["1", "2", "3", "4"] {
            assert!(html.contains(cell), "missing {cell}: {html}");
        }
    }
}
