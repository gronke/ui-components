//! html5ever-backed serialization: `outer_html`/`inner_html` for tests,
//! debugging and the future codegen path. Escaping, void elements and
//! raw-text tags follow the spec via `HtmlSerializer`.

use std::io;

use html5ever::serialize::{serialize, Serialize, SerializeOpts, Serializer, TraversalScope};

use crate::tree::{Document, NodeData, NodeId};

struct SerializableNode<'a, T> {
    doc: &'a Document<T>,
    node: NodeId,
}

impl<T> Serialize for SerializableNode<'_, T> {
    fn serialize<S>(&self, serializer: &mut S, traversal_scope: TraversalScope) -> io::Result<()>
    where
        S: Serializer,
    {
        serialize_node(self.doc, self.node, serializer, traversal_scope)
    }
}

fn serialize_node<T, S>(
    doc: &Document<T>,
    node: NodeId,
    serializer: &mut S,
    scope: TraversalScope,
) -> io::Result<()>
where
    S: Serializer,
{
    let include_node = matches!(scope, TraversalScope::IncludeNode);
    match doc.node(node) {
        Some(NodeData::Element(el)) => {
            if include_node {
                serializer.start_elem(el.name.clone(), el.qual_attrs())?;
            }
            // A template's children live in its contents fragment.
            let children_of = el.template_contents.unwrap_or(node);
            for child in doc.children(children_of) {
                serialize_node(doc, child, serializer, TraversalScope::IncludeNode)?;
            }
            if include_node {
                serializer.end_elem(el.name.clone())?;
            }
        }
        Some(NodeData::Text(text)) => serializer.write_text(text)?,
        Some(NodeData::Comment(text)) => serializer.write_comment(text)?,
        Some(NodeData::Document | NodeData::Fragment) => {
            for child in doc.children(node) {
                serialize_node(doc, child, serializer, TraversalScope::IncludeNode)?;
            }
        }
        None => {}
    }
    Ok(())
}

impl<T> Document<T> {
    /// The node and its subtree as HTML text; the document root and
    /// fragments render their children only.
    pub fn outer_html(&self, node: NodeId) -> String {
        let scope = match self.node(node) {
            Some(NodeData::Element(_)) => TraversalScope::IncludeNode,
            _ => TraversalScope::ChildrenOnly(None),
        };
        self.serialize_scope(node, scope)
    }

    /// The subtree below the node as HTML text.
    pub fn inner_html(&self, node: NodeId) -> String {
        self.serialize_scope(node, TraversalScope::ChildrenOnly(None))
    }

    fn serialize_scope(&self, node: NodeId, traversal_scope: TraversalScope) -> String {
        let mut out = Vec::new();
        serialize(
            &mut out,
            &SerializableNode { doc: self, node },
            SerializeOpts {
                traversal_scope,
                ..Default::default()
            },
        )
        .expect("in-memory serialization does not fail");
        String::from_utf8(out).expect("the serializer emits UTF-8")
    }
}
