//! The html5ever bridge: a `TreeSink` building straight into the arena, and
//! the parse entry points.
//!
//! This is lit-html's architecture ported: templates go through a real HTML5
//! parser, and the binding dialect (`?attr`, `.prop`, `@event`, `${hole}`)
//! rides through as ordinary attributes and text. Attribute names lowercase
//! on the way, per spec — the parts compiler recovers case from the template
//! source by index, exactly like lit. Malformed input never fails; the
//! diagnostics collect on [`Document::parse_errors`].

use std::borrow::Cow;
use std::cell::RefCell;

use html5ever::interface::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{ns, Attribute, LocalName, Namespace, ParseOpts, QualName};

use crate::tree::{Document, NodeId};

pub(crate) struct Sink<T> {
    doc: RefCell<Document<T>>,
}

/// `elem_name` must hand out borrows; atoms clone cheaply, so an owned copy
/// of the qualified name serves as the borrow source.
#[derive(Debug)]
pub(crate) struct SinkElemName(QualName);

impl ElemName for SinkElemName {
    fn ns(&self) -> &Namespace {
        &self.0.ns
    }

    fn local_name(&self) -> &LocalName {
        &self.0.local
    }
}

impl<T: Default> TreeSink for Sink<T> {
    type Handle = NodeId;
    type Output = Document<T>;
    type ElemName<'a>
        = SinkElemName
    where
        Self: 'a;

    fn finish(self) -> Document<T> {
        self.doc.into_inner()
    }

    fn parse_error(&self, msg: Cow<'static, str>) {
        self.doc.borrow_mut().parse_errors.push(msg.into_owned());
    }

    fn get_document(&self) -> NodeId {
        self.doc.borrow().root()
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> SinkElemName {
        SinkElemName(
            self.doc
                .borrow()
                .element(*target)
                .expect("elem_name is only called on elements")
                .name
                .clone(),
        )
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> NodeId {
        // The template-contents fragment comes from the shared element
        // constructor (by name), so the flags carry nothing extra here.
        let attrs = attrs
            .into_iter()
            .map(|attr| (attr.name, attr.value.to_string()))
            .collect();
        self.doc.borrow_mut().new_element_node(name, attrs)
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.doc.borrow_mut().create_comment(&text)
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeId {
        // HTML tokenizes `<?…>` as a bogus comment, so this only fires for
        // exotic inputs; keep the information as a comment node.
        self.doc
            .borrow_mut()
            .create_comment(&format!("?{target} {data}"))
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        let mut doc = self.doc.borrow_mut();
        match child {
            NodeOrText::AppendNode(node) => doc.append_child(*parent, node),
            NodeOrText::AppendText(text) => doc.append_text(*parent, &text),
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let has_parent = self.doc.borrow().parent(*element).is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        self.doc.borrow_mut().doctype = Some(name.to_string());
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        self.doc
            .borrow()
            .element(*target)
            .and_then(|el| el.template_contents)
            .expect("a template element carries its contents fragment")
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {
        // Quirks only steer CSS layout heuristics; nothing here consumes them.
    }

    fn append_before_sibling(&self, sibling: &NodeId, new_node: NodeOrText<NodeId>) {
        let mut doc = self.doc.borrow_mut();
        match new_node {
            NodeOrText::AppendNode(node) => doc.insert_before(node, *sibling),
            NodeOrText::AppendText(text) => doc.insert_text_before(&text, *sibling),
        }
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let mut doc = self.doc.borrow_mut();
        if let Some(el) = doc.element_mut(*target) {
            for attr in attrs {
                if el.attr(&attr.name.local).is_none() {
                    el.push_parsed_attr(attr.name, attr.value.to_string());
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.doc.borrow_mut().detach(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        self.doc.borrow_mut().reparent_children(*node, *new_parent);
    }
}

impl<T: Default> Document<T> {
    /// Parses a complete document; the spec's implied `html`/`head`/`body`
    /// scaffolding hangs under [`Document::root`].
    pub fn parse_html(source: &str) -> Self {
        let sink = Sink {
            doc: RefCell::new(Document::new()),
        };
        html5ever::parse_document(sink, ParseOpts::default()).one(source)
    }

    /// Parses a fragment the way component templates need it: no implied
    /// scaffolding; the parsed roots become [`Document::root`]'s children.
    ///
    /// `context` names the element the fragment algorithm assumes around the
    /// input — usually `"body"`; `"template"` parses template internals.
    pub fn parse_fragment(source: &str, context: &str) -> Self {
        let mut doc = Document::new();
        let context_elem = doc.new_element_node(
            QualName::new(None, ns!(html), LocalName::from(context)),
            Vec::new(),
        );
        let sink = Sink {
            doc: RefCell::new(doc),
        };
        let mut doc = html5ever::driver::parse_fragment_for_element(
            sink,
            ParseOpts::default(),
            context_elem,
            false,
            None,
        )
        .one(source);
        // The fragment algorithm parses into a synthetic `html` element
        // under the document; lift its children out and drop the scaffold
        // along with the floating context element.
        if let Some(wrapper) = doc.first_child(doc.root()) {
            let root = doc.root();
            doc.reparent_children(wrapper, root);
            doc.remove(wrapper);
        }
        doc.remove(context_elem);
        doc
    }
}
