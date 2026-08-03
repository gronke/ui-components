//! The selectors-crate integration: a `SelectorImpl` for the terminal's
//! dialect and an `Element` view over `uic_dom::Document<T>`.
//!
//! Unknown pseudo-classes parse and never match (`:hover` on a terminal),
//! so a rule with one keeps its other selectors alive: the degradation
//! contract applied to selector space.

use std::borrow::Borrow;
use std::fmt;

use cssparser::{match_ignore_ascii_case, ToCss};
use precomputed_hash::PrecomputedHash;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::{matches_selector, ElementSelectorFlags};
use selectors::parser::{
    NonTSPseudoClass as NonTSPseudoClassTrait, ParseRelative, PseudoElement as PseudoElementTrait,
    SelectorImpl, SelectorList, SelectorParseErrorKind,
};
use selectors::OpaqueElement;
use uic_dom::{Document, NodeData, NodeId};

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";

/// The string type for every selector part: a plain owned string.
/// Documents are small and sheets parse once; atoms are a later knob.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CssString(pub String);

impl<'a> From<&'a str> for CssString {
    fn from(value: &'a str) -> Self {
        CssString(value.to_string())
    }
}

impl Borrow<str> for CssString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CssString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl ToCss for CssString {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl PrecomputedHash for CssString {
    fn precomputed_hash(&self) -> u32 {
        // FNV-1a over the bytes; stable and cheap, no cached field needed.
        let mut hash: u32 = 0x811c_9dc5;
        for byte in self.0.as_bytes() {
            hash ^= u32::from(*byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        hash
    }
}

/// Pseudo-classes the dialect knows. `:focus` matches the focused node once
/// the runtime feeds one; the pointer states parse and never match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PseudoClass {
    Focus,
    FocusWithin,
    FocusVisible,
    Hover,
    Active,
    Disabled,
    Checked,
    /// `:dir(ltr)` / `:dir(rtl)`: the terminal renders left-to-right, so
    /// ltr matches and rtl never does.
    Dir(bool),
}

impl NonTSPseudoClassTrait for PseudoClass {
    type Impl = TuiSelectors;

    fn is_active_or_hover(&self) -> bool {
        matches!(self, PseudoClass::Hover | PseudoClass::Active)
    }

    fn is_user_action_state(&self) -> bool {
        matches!(
            self,
            PseudoClass::Focus
                | PseudoClass::FocusWithin
                | PseudoClass::FocusVisible
                | PseudoClass::Hover
                | PseudoClass::Active
        )
    }
}

impl ToCss for PseudoClass {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str(match self {
            PseudoClass::Focus => ":focus",
            PseudoClass::FocusWithin => ":focus-within",
            PseudoClass::FocusVisible => ":focus-visible",
            PseudoClass::Hover => ":hover",
            PseudoClass::Active => ":active",
            PseudoClass::Disabled => ":disabled",
            PseudoClass::Checked => ":checked",
            PseudoClass::Dir(true) => ":dir(ltr)",
            PseudoClass::Dir(false) => ":dir(rtl)",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PseudoElement {
    Before,
    After,
}

impl PseudoElementTrait for PseudoElement {
    type Impl = TuiSelectors;
}

impl ToCss for PseudoElement {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str(match self {
            PseudoElement::Before => "::before",
            PseudoElement::After => "::after",
        })
    }
}

#[derive(Clone, Debug)]
pub struct TuiSelectors;

impl SelectorImpl for TuiSelectors {
    type ExtraMatchingData<'a> = ();
    type AttrValue = CssString;
    type Identifier = CssString;
    type LocalName = CssString;
    type NamespaceUrl = CssString;
    type NamespacePrefix = CssString;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = PseudoClass;
    type PseudoElement = PseudoElement;
}

/// The selector parser: `:host`, `:is()`/`:where()` and the known
/// pseudo-classes are in; everything else fails the selector (the list's
/// other selectors survive via forgiving parsing at the sheet level).
pub struct TuiSelectorParser;

impl<'i> selectors::parser::Parser<'i> for TuiSelectorParser {
    type Impl = TuiSelectors;
    type Error = SelectorParseErrorKind<'i>;

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_host(&self) -> bool {
        true
    }

    fn parse_non_ts_functional_pseudo_class<'t>(
        &self,
        name: cssparser::CowRcStr<'i>,
        parser: &mut cssparser::Parser<'i, 't>,
        _after_part: bool,
    ) -> Result<PseudoClass, cssparser::ParseError<'i, Self::Error>> {
        if name.eq_ignore_ascii_case("dir") {
            let location = parser.current_source_location();
            let direction = parser.expect_ident()?.to_ascii_lowercase();
            return match direction.as_str() {
                "ltr" => Ok(PseudoClass::Dir(true)),
                "rtl" => Ok(PseudoClass::Dir(false)),
                _ => Err(location.new_custom_error(
                    SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                )),
            };
        }
        Err(
            parser.new_custom_error(SelectorParseErrorKind::UnsupportedPseudoClassOrElement(
                name,
            )),
        )
    }

    fn parse_non_ts_pseudo_class(
        &self,
        location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoClass, cssparser::ParseError<'i, Self::Error>> {
        let class = match_ignore_ascii_case! { &name,
            "focus" => PseudoClass::Focus,
            "focus-within" => PseudoClass::FocusWithin,
            "focus-visible" => PseudoClass::FocusVisible,
            "hover" => PseudoClass::Hover,
            "active" => PseudoClass::Active,
            "disabled" => PseudoClass::Disabled,
            "checked" => PseudoClass::Checked,
            _ => {
                return Err(location.new_custom_error(
                    SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                ))
            }
        };
        Ok(class)
    }

    fn parse_pseudo_element(
        &self,
        location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoElement, cssparser::ParseError<'i, Self::Error>> {
        let element = match_ignore_ascii_case! { &name,
            "before" => PseudoElement::Before,
            "after" => PseudoElement::After,
            _ => {
                return Err(location.new_custom_error(
                    SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                ))
            }
        };
        Ok(element)
    }
}

/// Parses one selector list in the dialect.
pub fn parse_selector_list(source: &str) -> Result<SelectorList<TuiSelectors>, String> {
    let mut input = cssparser::ParserInput::new(source);
    let mut parser = cssparser::Parser::new(&mut input);
    SelectorList::parse(&TuiSelectorParser, &mut parser, ParseRelative::No)
        .map_err(|err| format!("{err:?}"))
}

/// An element view for matching: the document, a node, and the component
/// scope the walk must not escape (component-sheet matching).
pub struct El<'a, T> {
    pub doc: &'a Document<T>,
    pub node: NodeId,
    pub scope: Option<NodeId>,
    pub focused: Option<NodeId>,
    /// Set when matching for a pseudo-element style
    /// (`MatchingMode::ForStatelessPseudoElement`).
    pub pseudo: Option<PseudoElement>,
}

impl<T> Clone for El<'_, T> {
    fn clone(&self) -> Self {
        El {
            doc: self.doc,
            node: self.node,
            scope: self.scope,
            focused: self.focused,
            pseudo: self.pseudo.clone(),
        }
    }
}

impl<T> fmt::Debug for El<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "El({:?})", self.node)
    }
}

impl<'a, T> El<'a, T> {
    fn wrap(&self, node: NodeId) -> Self {
        El {
            doc: self.doc,
            node,
            scope: self.scope,
            focused: self.focused,
            // Ancestors and siblings are ordinary elements; the pseudo
            // target applies to the subject only.
            pseudo: None,
        }
    }

    fn tag(&self) -> Option<&str> {
        match self.doc.node(self.node) {
            Some(NodeData::Element(el)) => Some(el.tag()),
            _ => None,
        }
    }

    fn element_around(&self, mut step: impl FnMut(NodeId) -> Option<NodeId>) -> Option<Self> {
        let mut current = self.node;
        while let Some(next) = step(current) {
            match self.doc.node(next) {
                Some(NodeData::Element(_)) => return Some(self.wrap(next)),
                Some(_) => current = next,
                None => return None,
            }
        }
        None
    }
}

impl<T> selectors::Element for El<'_, T> {
    type Impl = TuiSelectors;

    fn opaque(&self) -> OpaqueElement {
        opaque_of(self.node)
    }

    fn parent_element(&self) -> Option<Self> {
        // Component-sheet matching clamps at the scope root: ancestor
        // combinators stay inside the component subtree.
        if self.scope == Some(self.node) {
            return None;
        }
        let parent = self.doc.parent(self.node)?;
        match self.doc.node(parent) {
            Some(NodeData::Element(_)) => Some(self.wrap(parent)),
            _ => None,
        }
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.element_around(|node| self.doc.previous_sibling(node))
    }

    fn next_sibling_element(&self) -> Option<Self> {
        self.element_around(|node| self.doc.next_sibling(node))
    }

    fn first_element_child(&self) -> Option<Self> {
        let mut child = self.doc.first_child(self.node);
        while let Some(node) = child {
            if matches!(self.doc.node(node), Some(NodeData::Element(_))) {
                return Some(self.wrap(node));
            }
            child = self.doc.next_sibling(node);
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        true
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.tag() == Some(local_name)
    }

    fn has_namespace(&self, ns: &str) -> bool {
        ns.is_empty() || ns == HTML_NS
    }

    fn is_same_type(&self, other: &Self) -> bool {
        self.tag() == other.tag()
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&CssString>,
        local_name: &CssString,
        operation: &AttrSelectorOperation<&CssString>,
    ) -> bool {
        if matches!(ns, NamespaceConstraint::Specific(url) if !url.0.is_empty()) {
            return false;
        }
        match self.doc.attribute(self.node, &local_name.0) {
            Some(value) => operation.eval_str(value),
            None => false,
        }
    }

    fn has_attr_in_no_namespace(&self, local_name: &CssString) -> bool {
        self.doc.attribute(self.node, &local_name.0).is_some()
    }

    fn match_non_ts_pseudo_class(
        &self,
        pc: &PseudoClass,
        _context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        match pc {
            PseudoClass::Focus => self.focused == Some(self.node),
            PseudoClass::Dir(ltr) => *ltr,
            PseudoClass::FocusWithin => {
                let Some(focused) = self.focused else {
                    return false;
                };
                focused == self.node || self.doc.ancestors(focused).any(|node| node == self.node)
            }
            // No pointer, no forms machinery: parsed but never matched.
            _ => false,
        }
    }

    fn match_pseudo_element(
        &self,
        pe: &PseudoElement,
        _context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        self.pseudo.as_ref() == Some(pe)
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {}

    fn is_link(&self) -> bool {
        self.tag() == Some("a") && self.doc.attribute(self.node, "href").is_some()
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &CssString, case_sensitivity: CaseSensitivity) -> bool {
        match self.doc.attribute(self.node, "id") {
            Some(value) => case_sensitivity.eq(value.as_bytes(), id.0.as_bytes()),
            None => false,
        }
    }

    fn has_class(&self, name: &CssString, case_sensitivity: CaseSensitivity) -> bool {
        self.doc
            .classes(self.node)
            .any(|class| case_sensitivity.eq(class.as_bytes(), name.0.as_bytes()))
    }

    fn has_custom_state(&self, _name: &CssString) -> bool {
        false
    }

    fn imported_part(&self, _name: &CssString) -> Option<CssString> {
        None
    }

    fn is_part(&self, _name: &CssString) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        let mut child = self.doc.first_child(self.node);
        while let Some(node) = child {
            match self.doc.node(node) {
                Some(NodeData::Element(_)) => return false,
                Some(NodeData::Text(text)) if !text.trim().is_empty() => return false,
                _ => {}
            }
            child = self.doc.next_sibling(node);
        }
        true
    }

    fn is_root(&self) -> bool {
        match self.doc.parent(self.node) {
            Some(parent) => matches!(self.doc.node(parent), Some(NodeData::Document)),
            None => true,
        }
    }

    fn add_element_unique_hashes(&self, _filter: &mut BloomFilter) -> bool {
        // No bloom acceleration in v1; matching stays exact without it.
        false
    }
}

pub(crate) fn opaque_of(node: NodeId) -> OpaqueElement {
    // Identity only, never dereferenced: the arena index as a pointer.
    let index = usize::from(std::num::NonZeroUsize::from(node));
    let ptr = std::ptr::NonNull::new(index as *mut ()).expect("nonzero arena index");
    OpaqueElement::from_non_null_ptr(ptr)
}

/// Whether the node matches any selector of the list. `scope` clamps
/// ancestor walks and provides `:host` (component sheets).
pub fn matches<T>(
    doc: &Document<T>,
    node: NodeId,
    selectors: &SelectorList<TuiSelectors>,
    scope: Option<NodeId>,
    focused: Option<NodeId>,
) -> bool {
    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    context.current_host = scope.map(opaque_of);
    let element = El {
        doc,
        node,
        scope,
        focused,
        pseudo: None,
    };
    selectors
        .slice()
        .iter()
        .any(|selector| matches_selector(selector, 0, None, &element, &mut context))
}
