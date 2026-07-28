//! Layout from the retained DOM: taffy computes real CSS flexbox over
//! terminal cells, reading classes and mounted widgets straight off the
//! document nodes — no per-frame expansion tree in between.

use ratatui::layout::Rect;
use taffy::prelude::*;
use unicode_width::UnicodeWidthStr;

use uic_css::StyleTable;
use uic_dom::NodeData;

use super::character_manipulation::{
    collapse_whitespace, ends_spaced, longest_word, rotated_glyph, starts_spaced, wrapped_lines,
};
use super::{css, DomDocument};

/// A laid box with its computed absolute cell rectangle.
pub(crate) struct LaidNode {
    pub kind: LaidKind,
    pub rect: Rect,
    pub children: Vec<LaidNode>,
}

/// What a laid box stands for. Besides real document nodes, the tree holds
/// boxes the layout synthesized: anonymous rows wrapping inline runs and
/// `::before`/`::after` generated content (ADR 0021 stage 3).
#[derive(Clone)]
pub(crate) enum LaidKind {
    /// An element node.
    Node(uic_dom::NodeId),
    /// A text node with its flow-prepared string: interior whitespace
    /// collapsed, run-boundary spaces already decided — the paint renders
    /// it verbatim.
    Text { node: uic_dom::NodeId, text: String },
    /// A synthesized flex row wrapping a run of inline boxes; styleless and
    /// transparent to hit-testing.
    Anonymous,
    /// A generated-content box; `owner` carries the pseudo style and takes
    /// the hits (clicking the marker clicks the element).
    Generated {
        owner: uic_dom::NodeId,
        which: uic_css::PseudoElement,
        text: String,
    },
}

impl LaidKind {
    /// The node a pointer event on this box lands on.
    pub(crate) fn hit_target(&self) -> Option<uic_dom::NodeId> {
        match self {
            LaidKind::Node(node) | LaidKind::Text { node, .. } => Some(*node),
            LaidKind::Generated { owner, .. } => Some(*owner),
            LaidKind::Anonymous => None,
        }
    }
}

/// One child in an element's flow, before boxes exist: document nodes plus
/// the generated-content items the styles synthesized.
enum FlowItem<'a> {
    Text {
        node: uic_dom::NodeId,
        raw: &'a str,
    },
    Element {
        node: uic_dom::NodeId,
        inline: bool,
    },
    Generated {
        which: uic_css::PseudoElement,
        style: &'a uic_css::ComputedStyle,
    },
}

impl FlowItem<'_> {
    /// Inline-level items flow in runs; block-level items break them.
    fn is_inline(&self) -> bool {
        match self {
            FlowItem::Text { .. } | FlowItem::Generated { .. } => true,
            FlowItem::Element { inline, .. } => *inline,
        }
    }

    /// Whitespace-only text contributes separators, never content.
    fn is_blank(&self) -> bool {
        match self {
            FlowItem::Text { raw, .. } => collapse_whitespace(raw).is_empty(),
            _ => false,
        }
    }
}

enum Measured {
    /// A text leaf; the flag carries `overflow-wrap: anywhere` — an
    /// unbreakable token may then break at any character, so min-content
    /// drops to one cell instead of the longest word (the height count and
    /// the paint already break oversized words).
    Text(String, bool),
    /// A widget leaf, with its content width when the widget sizes to
    /// content (the select's closed label); `None` measures the flex
    /// default.
    Widget(Option<u16>),
}

/// A box paired with its taffy handle during the build, so the collect
/// pass can zip computed layouts back onto the laid tree.
struct Shadow {
    kind: LaidKind,
    taffy: NodeId,
    children: Vec<Shadow>,
}

pub(crate) fn compute(doc: &DomDocument, area: Rect) -> Vec<LaidNode> {
    compute_styled(doc, area, None).0
}

/// Lays the document out and returns the computed styles the paint reads.
/// `focused` feeds `:focus` selectors in the adopted component sheets.
pub(crate) fn compute_styled(
    doc: &DomDocument,
    area: Rect,
    focused: Option<uic_dom::NodeId>,
) -> (Vec<LaidNode>, StyleTable) {
    let styles = css::resolve(doc, focused);
    let mut tree: TaffyTree<Measured> = TaffyTree::new();
    let roots: Vec<Shadow> = doc
        .children(doc.root())
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|node| build(&mut tree, doc, node, true, &styles))
        .collect();
    let root_style = Style {
        display: Display::Block,
        size: Size {
            width: length(area.width as f32),
            height: length(area.height as f32),
        },
        ..Default::default()
    };
    let ids: Vec<NodeId> = roots.iter().map(|shadow| shadow.taffy).collect();
    let root = tree
        .new_with_children(root_style, &ids)
        .expect("taffy root");

    tree.compute_layout_with_measure(
        root,
        Size {
            width: AvailableSpace::Definite(area.width as f32),
            height: AvailableSpace::Definite(area.height as f32),
        },
        |known, available, _id, context, _style| match context {
            Some(Measured::Text(text, break_anywhere)) => {
                let intrinsic = text.width() as f32;
                let width = known.width.unwrap_or(match available.width {
                    AvailableSpace::Definite(limit) => intrinsic.min(limit),
                    AvailableSpace::MinContent if *break_anywhere => 1.0,
                    AvailableSpace::MinContent => longest_word(text),
                    AvailableSpace::MaxContent => intrinsic,
                });
                Size {
                    width,
                    height: known
                        .height
                        .unwrap_or_else(|| wrapped_lines(text, width) as f32),
                }
            }
            Some(Measured::Widget(intrinsic)) => Size {
                width: known
                    .width
                    .unwrap_or_else(|| intrinsic.map(f32::from).unwrap_or(12.0)),
                height: known.height.unwrap_or(1.0),
            },
            None => Size::ZERO,
        },
    )
    .expect("taffy layout");

    let laid = roots
        .iter()
        .map(|shadow| collect(&tree, shadow, (area.x as f32, area.y as f32), area))
        .collect();
    (laid, styles)
}

fn build(
    tree: &mut TaffyTree<Measured>,
    doc: &DomDocument,
    node: uic_dom::NodeId,
    root_child: bool,
    styles: &StyleTable,
) -> Option<Shadow> {
    match doc.node(node)? {
        // A root-level text node inherits nothing (the root has no style).
        NodeData::Text(text) => build_text(tree, node, collapse_whitespace(text), false),
        NodeData::Element(el) => {
            // Conditional anchors render nothing; their bodies are siblings.
            if &**el.tag() == "template" {
                return None;
            }
            let computed = styles
                .get(&node)
                .map(|e| e.style.clone())
                .unwrap_or_default();
            if computed.display == uic_css::Display::None {
                // display:none removes the subtree from layout entirely —
                // json-viewer's filtered rows, the ua sheet's [hidden].
                return None;
            }
            if el.data.widget.is_some() {
                let height = widget_height(doc, node);
                let width = widget_width(doc, node);
                let taffy = tree
                    .new_leaf_with_context(
                        widget_style(&computed, height, width),
                        Measured::Widget(width),
                    )
                    .expect("taffy widget leaf");
                return Some(Shadow {
                    kind: LaidKind::Node(node),
                    taffy,
                    children: Vec::new(),
                });
            }
            if &**el.tag() == "table" {
                if let Some(shadow) = build_table(tree, doc, node, &computed, root_child, styles) {
                    return Some(shadow);
                }
            }
            let children = build_flow(tree, doc, node, &computed, styles);
            let mut style = taffy_style(&computed);
            if root_child {
                // Mounted roots stack like block elements with one blank
                // row between them, as the host document flows.
                style.margin.bottom = length(1.0_f32);
            }
            let ids: Vec<NodeId> = children.iter().map(|shadow| shadow.taffy).collect();
            let taffy = tree
                .new_with_children(style, &ids)
                .expect("taffy container");
            Some(Shadow {
                kind: LaidKind::Node(node),
                taffy,
                children,
            })
        }
        _ => None,
    }
}

/// Builds an element's child boxes: document children plus generated
/// `::before`/`::after` items, flowed by the container's inner display.
///
/// A flex or grid container blockifies: every item becomes a direct child,
/// exactly the pre-inline behavior. A block container wraps runs of two or
/// more consecutive inline-level items into an anonymous wrapping flex row —
/// the inline formatting context, one box at a time. An inline container is
/// itself the row, so its flow items attach directly.
fn build_flow(
    tree: &mut TaffyTree<Measured>,
    doc: &DomDocument,
    node: uic_dom::NodeId,
    computed: &uic_css::ComputedStyle,
    styles: &StyleTable,
) -> Vec<Shadow> {
    use uic_css::{Display as CssDisplay, PseudoElement};

    let element_styles = styles.get(&node);
    let mut items: Vec<FlowItem<'_>> = Vec::new();
    if let Some(before) = element_styles.and_then(|e| e.before.as_ref()) {
        items.push(FlowItem::Generated {
            which: PseudoElement::Before,
            style: before,
        });
    }
    for child in doc.children(node).collect::<Vec<_>>() {
        match doc.node(child) {
            Some(NodeData::Text(raw)) => items.push(FlowItem::Text { node: child, raw }),
            Some(NodeData::Element(child_el)) => {
                if &**child_el.tag() == "template" {
                    continue;
                }
                let display = styles
                    .get(&child)
                    .map(|e| e.style.display)
                    .unwrap_or_default();
                if display == CssDisplay::None {
                    // Generates no box and never breaks a run, like the
                    // browser's inline flow around display:none.
                    continue;
                }
                // Widgets keep their block-level flow (their own row) even
                // under an inline display.
                let inline = matches!(display, CssDisplay::Inline | CssDisplay::InlineFlex)
                    && child_el.data.widget.is_none();
                items.push(FlowItem::Element {
                    node: child,
                    inline,
                });
            }
            _ => {}
        }
    }
    if let Some(after) = element_styles.and_then(|e| e.after.as_ref()) {
        items.push(FlowItem::Generated {
            which: PseudoElement::After,
            style: after,
        });
    }

    let mut children: Vec<Shadow> = Vec::new();
    if matches!(
        computed.display,
        CssDisplay::Flex | CssDisplay::InlineFlex | CssDisplay::Grid
    ) {
        // Blockified: each item is a flex/grid child of its own.
        for item in items {
            build_plain_item(tree, doc, node, item, styles, &mut children);
        }
        return children;
    }

    let container_is_row = computed.display == CssDisplay::Inline;
    let mut run: Vec<FlowItem<'_>> = Vec::new();
    for item in items {
        if item.is_inline() {
            run.push(item);
            continue;
        }
        flush_run(
            tree,
            doc,
            node,
            &mut run,
            styles,
            container_is_row,
            &mut children,
        );
        build_plain_item(tree, doc, node, item, styles, &mut children);
    }
    flush_run(
        tree,
        doc,
        node,
        &mut run,
        styles,
        container_is_row,
        &mut children,
    );
    children
}

/// Ends the pending inline run: two or more content items under a block
/// container get the anonymous flex row; an inline container takes the
/// prepared items directly (it is the row); a single item stays a plain
/// block child, the pre-inline layout.
fn flush_run(
    tree: &mut TaffyTree<Measured>,
    doc: &DomDocument,
    owner: uic_dom::NodeId,
    run: &mut Vec<FlowItem<'_>>,
    styles: &StyleTable,
    container_is_row: bool,
    out: &mut Vec<Shadow>,
) {
    if run.is_empty() {
        return;
    }
    let items = std::mem::take(run);
    let content = items.iter().filter(|item| !item.is_blank()).count();
    if content == 0 {
        return;
    }
    if container_is_row {
        out.extend(build_run_items(tree, doc, owner, items, styles));
        return;
    }
    if content == 1 {
        for item in items {
            build_plain_item(tree, doc, owner, item, styles, out);
        }
        return;
    }
    let children = build_run_items(tree, doc, owner, items, styles);
    let ids: Vec<NodeId> = children.iter().map(|shadow| shadow.taffy).collect();
    let style = Style {
        display: Display::Flex,
        flex_wrap: FlexWrap::Wrap,
        ..Default::default()
    };
    let taffy = tree
        .new_with_children(style, &ids)
        .expect("taffy anonymous row");
    out.push(Shadow {
        kind: LaidKind::Anonymous,
        taffy,
        children,
    });
}

/// Builds one flow item outside a run: text trimmed like a lone block
/// child, elements through the ordinary recursion.
fn build_plain_item(
    tree: &mut TaffyTree<Measured>,
    doc: &DomDocument,
    owner: uic_dom::NodeId,
    item: FlowItem<'_>,
    styles: &StyleTable,
    out: &mut Vec<Shadow>,
) {
    match item {
        FlowItem::Text { node, raw } => {
            let break_anywhere = owner_breaks_anywhere(styles, owner);
            out.extend(build_text(
                tree,
                node,
                collapse_whitespace(raw),
                break_anywhere,
            ));
        }
        FlowItem::Element { node, .. } => {
            out.extend(build(tree, doc, node, false, styles));
        }
        FlowItem::Generated { which, style } => {
            out.extend(build_generated(tree, owner, which, style));
        }
    }
}

/// Builds a run's boxes with inline whitespace processing: interior
/// whitespace collapses, the run's edges trim like line ends, and interior
/// boundaries keep a single space when the markup had one — whitespace-only
/// nodes between items become exactly one separator.
fn build_run_items(
    tree: &mut TaffyTree<Measured>,
    doc: &DomDocument,
    owner: uic_dom::NodeId,
    items: Vec<FlowItem<'_>>,
    styles: &StyleTable,
) -> Vec<Shadow> {
    let Some(last) = items.iter().rposition(|item| !item.is_blank()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // A per-owner constant — one lookup serves the whole run.
    let break_anywhere = owner_breaks_anywhere(styles, owner);
    // The run start behaves like text after a space: leading whitespace
    // never renders at a line start.
    let mut prev_spaced = true;
    for (index, item) in items.into_iter().enumerate() {
        if index > last {
            break;
        }
        match item {
            FlowItem::Text { node, raw } => {
                let collapsed = collapse_whitespace(raw);
                if collapsed.is_empty() {
                    if !prev_spaced {
                        out.extend(build_text(tree, node, " ".to_string(), break_anywhere));
                        prev_spaced = true;
                    }
                    continue;
                }
                let lead = starts_spaced(raw) && !prev_spaced;
                let trail = ends_spaced(raw) && index != last;
                let mut text = String::new();
                if lead {
                    text.push(' ');
                }
                text.push_str(&collapsed);
                if trail {
                    text.push(' ');
                }
                out.extend(build_text(tree, node, text, break_anywhere));
                prev_spaced = trail;
            }
            FlowItem::Element { node, .. } => {
                if let Some(shadow) = build(tree, doc, node, false, styles) {
                    out.push(shadow);
                    prev_spaced = false;
                }
            }
            FlowItem::Generated { which, style } => {
                if let Some(shadow) = build_generated(tree, owner, which, style) {
                    out.push(shadow);
                    prev_spaced = false;
                }
            }
        }
    }
    out
}

/// The owner element's inherited `overflow-wrap: anywhere` — the flag its
/// text children measure under.
fn owner_breaks_anywhere(styles: &StyleTable, owner: uic_dom::NodeId) -> bool {
    styles
        .get(&owner)
        .is_some_and(|entry| entry.style.break_anywhere)
}

/// A text leaf carrying its prepared string; empty text builds nothing.
/// `break_anywhere` is the owner's inherited `overflow-wrap: anywhere`.
fn build_text(
    tree: &mut TaffyTree<Measured>,
    node: uic_dom::NodeId,
    text: String,
    break_anywhere: bool,
) -> Option<Shadow> {
    if text.is_empty() {
        return None;
    }
    let taffy = tree
        .new_leaf_with_context(
            Style {
                flex_shrink: 0.0,
                ..Default::default()
            },
            Measured::Text(text.clone(), break_anywhere),
        )
        .expect("taffy text leaf");
    Some(Shadow {
        kind: LaidKind::Text { node, text },
        taffy,
        children: Vec::new(),
    })
}

/// A `::before`/`::after` box: the pseudo style sizes it, its `content`
/// paints, and a right-angle `transform` picks the rotated glyph at
/// synthesis — json-viewer's ▶ marker turning ▼ on expand.
fn build_generated(
    tree: &mut TaffyTree<Measured>,
    owner: uic_dom::NodeId,
    which: uic_css::PseudoElement,
    pseudo: &uic_css::ComputedStyle,
) -> Option<Shadow> {
    if pseudo.display == uic_css::Display::None {
        return None;
    }
    let content = pseudo.content.clone().filter(|text| !text.is_empty())?;
    let text = rotated_glyph(content, pseudo.rotation);
    let mut style = taffy_style(pseudo);
    style.flex_shrink = 0.0;
    let taffy = tree
        .new_leaf_with_context(style, Measured::Text(text.clone(), pseudo.break_anywhere))
        .expect("taffy generated leaf");
    Some(Shadow {
        kind: LaidKind::Generated { owner, which, text },
        taffy,
        children: Vec::new(),
    })
}

/// Lays a `<table>` out as a grid with shared column tracks (ADR 0017).
///
/// The section elements and `<tr>` are structural: the cells become the grid
/// items, placed explicitly by row and column line so a short row never
/// shifts later ones. `auto` tracks size to the largest cell across all rows
/// — the alignment separate flex rows cannot express. A `table`-classed (or
/// `w-100`) table fills its row with `minmax(auto, 1fr)` tracks, like the
/// browser's `width: 100%` table.
///
/// Returns `None` when no cells exist, falling back to the block path.
fn build_table(
    tree: &mut TaffyTree<Measured>,
    doc: &DomDocument,
    node: uic_dom::NodeId,
    computed: &uic_css::ComputedStyle,
    root_child: bool,
    styles: &StyleTable,
) -> Option<Shadow> {
    let is_tag = |candidate: uic_dom::NodeId, name: &str| {
        doc.tag_name(candidate).map(|tag| &**tag == name) == Some(true)
    };

    // Rows in document order: direct <tr> children and <tr> under the
    // section elements. Everything else (template anchors, stray text) is
    // structural noise here.
    let mut rows: Vec<uic_dom::NodeId> = Vec::new();
    for child in doc.children(node).collect::<Vec<_>>() {
        if is_tag(child, "tr") {
            rows.push(child);
        } else if ["thead", "tbody", "tfoot"]
            .iter()
            .any(|section| is_tag(child, section))
        {
            rows.extend(
                doc.children(child)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .filter(|&inner| is_tag(inner, "tr")),
            );
        }
    }

    let cells_per_row: Vec<Vec<uic_dom::NodeId>> = rows
        .iter()
        .map(|&row| {
            doc.children(row)
                .collect::<Vec<_>>()
                .into_iter()
                .filter(|&cell| is_tag(cell, "td") || is_tag(cell, "th"))
                .collect()
        })
        .collect();
    let columns = cells_per_row.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return None;
    }

    // The cells build through the ordinary recursion and then receive their
    // explicit grid placement.
    let mut children: Vec<Shadow> = Vec::new();
    for (row_index, cells) in cells_per_row.iter().enumerate() {
        for (column_index, &cell) in cells.iter().enumerate() {
            let Some(shadow) = build(tree, doc, cell, false, styles) else {
                continue;
            };
            let mut style = tree.style(shadow.taffy).expect("cell style").clone();
            style.grid_row = line(row_index as i16 + 1);
            style.grid_column = line(column_index as i16 + 1);
            tree.set_style(shadow.taffy, style).expect("cell placement");
            children.push(shadow);
        }
    }

    // Bootstrap's shrink-to-fit idiom `table w-auto` opts back out of fill.
    let table_classes: Vec<String> = doc
        .attribute(node, "class")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let fill = table_classes
        .iter()
        .any(|class| class == "table" || class == "w-100")
        && !table_classes.iter().any(|class| class == "w-auto");
    let track: GridTemplateComponent<String> = if fill {
        minmax(auto(), fr(1.0_f32))
    } else {
        auto()
    };
    let mut style = taffy_style(computed);
    style.display = Display::Grid;
    style.grid_template_columns = vec![track; columns];
    style.gap.width = length(1.0_f32);
    if fill {
        style.size.width = percent(1.0_f32);
    } else {
        // A plain table hugs its content like the browser's: packing the
        // tracks at the start keeps `auto` tracks content-sized instead of
        // stretching into the block-level free space.
        style.justify_content = Some(JustifyContent::START);
    }
    if root_child {
        style.margin.bottom = length(1.0_f32);
    }
    let ids: Vec<NodeId> = children.iter().map(|shadow| shadow.taffy).collect();
    let taffy = tree.new_with_children(style, &ids).expect("taffy table");
    Some(Shadow {
        kind: LaidKind::Node(node),
        taffy,
        children,
    })
}

/// The node's classes, with the component-state rewrites the browser gets
/// from stylesheet selectors: a `seamless` component drops its group border
/// (`input-group` renders as a plain flex row).
pub(crate) fn effective_classes(doc: &DomDocument, node: uic_dom::NodeId) -> Vec<String> {
    // Seamless chrome removal moved to the stylesheet:
    // `[seamless] .input-group` zeroes the border and inset (ADR 0021).
    doc.attribute(node, "class")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// The named attribute of the nearest enclosing custom element — the
/// component's reflected state, the way `[seamless]`/`[error]` stylesheet
/// selectors read it in the browser.
pub(crate) fn component_attr(
    doc: &DomDocument,
    node: uic_dom::NodeId,
    name: &str,
) -> Option<String> {
    for ancestor in doc.ancestors(node) {
        let Some(tag) = doc.tag_name(ancestor) else {
            continue;
        };
        if tag.contains('-') {
            return doc.attribute(ancestor, name).map(str::to_string);
        }
    }
    None
}

/// Layout height of a widget leaf: single-line widgets are one cell; a
/// textarea starts at one line like the browser's initial height and grows
/// with its content up to the component's `max-lines` attribute (10 when
/// absent).
fn widget_height(doc: &DomDocument, node: uic_dom::NodeId) -> u16 {
    let Some(widget) = doc.element(node).and_then(|el| el.data.widget.as_ref()) else {
        return 1;
    };
    let max_lines = component_attr(doc, node, "max-lines")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|lines| *lines >= 1)
        .unwrap_or(10);
    widget.adapter.intrinsic_height(max_lines)
}

/// Intrinsic width of a widget leaf that sizes to content the way the
/// browser does: a select hugs its closed label plus rat's three marker
/// cells; the text-editing widgets keep the flex default.
fn widget_width(doc: &DomDocument, node: uic_dom::NodeId) -> Option<u16> {
    let widget = doc.element(node).and_then(|el| el.data.widget.as_ref())?;
    widget.adapter.intrinsic_width()
}

/// The computed style, translated onto taffy — margins, paddings and
/// borders arrive cell-resolved from the cascade (ADR 0021).
fn taffy_style(computed: &uic_css::ComputedStyle) -> Style {
    use uic_css::{Dimension as CssDimension, Display as CssDisplay, FlexDirection as CssFlexDir};

    let mut style = Style {
        display: match computed.display {
            CssDisplay::Flex | CssDisplay::InlineFlex => Display::Flex,
            CssDisplay::Grid => Display::Grid,
            // An inline box is its own wrapping row: its children continue
            // the flow inside it (each inline box breaks lines on its own,
            // the stage-3 approximation of one shared inline context).
            CssDisplay::Inline => Display::Flex,
            // display:none subtrees are skipped by the caller.
            _ => Display::Block,
        },
        ..Default::default()
    };
    style.flex_direction = match computed.flex_direction {
        CssFlexDir::Row => FlexDirection::Row,
        CssFlexDir::Column => FlexDirection::Column,
    };
    style.flex_wrap = if computed.flex_wrap || computed.display == CssDisplay::Inline {
        FlexWrap::Wrap
    } else {
        FlexWrap::NoWrap
    };
    if let Some(grow) = computed.flex_grow {
        style.flex_grow = grow;
    }
    if let Some(shrink) = computed.flex_shrink {
        style.flex_shrink = shrink;
    }
    if computed.align_items_center {
        style.align_items = Some(AlignItems::CENTER);
    }
    if computed.align_self_center {
        style.align_self = Some(AlignItems::CENTER);
    }
    style.margin = taffy::geometry::Rect {
        top: length(computed.margin[0]),
        right: length(computed.margin[1]),
        bottom: length(computed.margin[2]),
        left: length(computed.margin[3]),
    };
    style.padding = taffy::geometry::Rect {
        top: length(computed.padding[0]),
        right: length(computed.padding[1]),
        bottom: length(computed.padding[2]),
        left: length(computed.padding[3]),
    };
    if computed.border > 0.0 {
        style.border = taffy::geometry::Rect {
            top: length(computed.border),
            right: length(computed.border),
            bottom: length(computed.border),
            left: length(computed.border),
        };
    }
    style.gap = Size {
        width: length(computed.gap.1),
        height: length(computed.gap.0),
    };
    let dimension = |value: &CssDimension| match value {
        CssDimension::Cells(cells) => length(*cells),
        CssDimension::Percent(unit) => percent(*unit),
        CssDimension::Auto => auto(),
    };
    style.size = Size {
        width: dimension(&computed.width),
        height: dimension(&computed.height),
    };
    if !matches!(computed.min_width, CssDimension::Auto) {
        style.min_size.width = dimension(&computed.min_width);
    }
    if !matches!(computed.max_width, CssDimension::Auto) {
        style.max_size.width = dimension(&computed.max_width);
    }
    style
}

fn widget_style(computed: &uic_css::ComputedStyle, height: u16, width: Option<u16>) -> Style {
    let height = height.max(1) as f32;
    // A content-sized widget may not shrink below its content, like
    // fit-content in the browser; the others keep the editing minimum.
    let min_width = width.map(f32::from).unwrap_or(12.0_f32);
    let mut style = Style {
        size: Size {
            width: auto(),
            height: length(height),
        },
        min_size: Size {
            width: length(min_width),
            height: length(height),
        },
        ..Default::default()
    };
    if let Some(grow) = computed.flex_grow {
        style.flex_grow = grow;
    }
    if let Some(shrink) = computed.flex_shrink {
        style.flex_shrink = shrink;
    }
    if let uic_css::Dimension::Percent(unit) = computed.width {
        style.size.width = percent(unit);
    }
    style
}

fn collect(
    tree: &TaffyTree<Measured>,
    shadow: &Shadow,
    origin: (f32, f32),
    bounds: Rect,
) -> LaidNode {
    let layout = tree.layout(shadow.taffy).expect("computed layout");
    let x = origin.0 + layout.location.x;
    let y = origin.1 + layout.location.y;
    let rect = clamp_rect(x, y, layout.size.width, layout.size.height, bounds);
    let children = shadow
        .children
        .iter()
        .map(|child| collect(tree, child, (x, y), bounds))
        .collect();
    LaidNode {
        kind: shadow.kind.clone(),
        rect,
        children,
    }
}

/// Rounds to whole cells and clips to the drawable area (rounding lives here
/// in one place).
fn clamp_rect(x: f32, y: f32, width: f32, height: f32, bounds: Rect) -> Rect {
    let x0 = (x.round().max(0.0) as u16).min(bounds.right());
    let y0 = (y.round().max(0.0) as u16).min(bounds.bottom());
    let x1 = ((x + width).round().max(0.0) as u16).min(bounds.right());
    let y1 = ((y + height).round().max(0.0) as u16).min(bounds.bottom());
    Rect {
        x: x0,
        y: y0,
        width: x1.saturating_sub(x0),
        height: y1.saturating_sub(y0),
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;
    use crate::dom::DomDocument;

    fn add_span(doc: &mut DomDocument, parent: uic_dom::NodeId, text: &str) -> uic_dom::NodeId {
        let span = doc.create_element_named("span");
        doc.set_attribute(span, "class", "d-inline");
        let content = doc.create_text_node(text);
        doc.append_child(span, content);
        doc.append_child(parent, span);
        span
    }

    /// Collects every text box with its rect, in paint order.
    fn text_boxes(laid: &[LaidNode], out: &mut Vec<(String, Rect)>) {
        for node in laid {
            if let LaidKind::Text { text, .. } = &node.kind {
                out.push((text.clone(), node.rect));
            }
            text_boxes(&node.children, out);
        }
    }

    #[test]
    fn an_inline_run_shares_a_row_and_keeps_the_markup_space() {
        let mut doc = DomDocument::new();
        let div = doc.create_element_named("div");
        let root = doc.root();
        doc.append_child(root, div);
        add_span(&mut doc, div, "issue:");
        let separator = doc.create_text_node("\n    ");
        doc.append_child(div, separator);
        add_span(&mut doc, div, "65");

        let laid = compute(&doc, Rect::new(0, 0, 40, 4));
        // The two spans and the separator wrap into one anonymous row.
        let anonymous = &laid[0].children[0];
        assert!(matches!(anonymous.kind, LaidKind::Anonymous));
        let mut texts = Vec::new();
        text_boxes(&laid, &mut texts);
        let issue = texts.iter().find(|(t, _)| t == "issue:").expect("issue:");
        let value = texts.iter().find(|(t, _)| t == "65").expect("65");
        assert_eq!(issue.1.y, value.1.y, "inline boxes share the row");
        // The whitespace-only node between them became exactly one cell.
        assert_eq!(value.1.x, 7, "issue: (6) + one separator space");
    }

    #[test]
    fn boundary_whitespace_trims_and_a_lone_inline_child_stays_plain() {
        let mut doc = DomDocument::new();
        let div = doc.create_element_named("div");
        let root = doc.root();
        doc.append_child(root, div);
        let leading = doc.create_text_node("\n  ");
        doc.append_child(div, leading);
        let span = add_span(&mut doc, div, "alone");
        let trailing = doc.create_text_node("  \n");
        doc.append_child(div, trailing);

        let laid = compute(&doc, Rect::new(0, 0, 40, 4));
        // One content item: no anonymous wrapper, the span is the block
        // child itself and the blank edges drop like line ends.
        assert_eq!(laid[0].children.len(), 1);
        match laid[0].children[0].kind {
            LaidKind::Node(node) => assert_eq!(node, span),
            _ => panic!("a lone inline child lays out as a plain block child"),
        }
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;
    use crate::dom::DomDocument;

    fn add_cell(doc: &mut DomDocument, row: uic_dom::NodeId, tag: &str, text: &str) {
        let cell = doc.create_element_named(tag);
        let content = doc.create_text_node(text);
        doc.append_child(cell, content);
        doc.append_child(row, cell);
    }

    /// Builds a `<table><tbody>` with one `<tr>` per row slice.
    fn add_table(doc: &mut DomDocument, class: Option<&str>, rows: &[&[&str]]) {
        let table = doc.create_element_named("table");
        if let Some(class) = class {
            doc.set_attribute(table, "class", class);
        }
        let root = doc.root();
        doc.append_child(root, table);
        let tbody = doc.create_element_named("tbody");
        doc.append_child(table, tbody);
        for cells in rows {
            let tr = doc.create_element_named("tr");
            doc.append_child(tbody, tr);
            for text in *cells {
                add_cell(doc, tr, "td", text);
            }
        }
    }

    #[test]
    fn columns_share_tracks_across_rows() {
        let mut doc = DomDocument::new();
        add_table(
            &mut doc,
            None,
            &[
                &["a", "considerably-longer-cell", "x"],
                &["wider-first-cell", "b", "y"],
            ],
        );
        let laid = compute(&doc, Rect::new(0, 0, 80, 12));
        let table = &laid[0];
        // The cells are the table's direct laid children, in row-major order.
        assert_eq!(table.children.len(), 6);
        for column in 0..3 {
            assert_eq!(
                table.children[column].rect.x,
                table.children[3 + column].rect.x,
                "column {column} shares its track across rows",
            );
        }
        // The widest cell wins its column: the second column fits the long text.
        let second = &table.children[1];
        assert!(
            second.rect.width >= "considerably-longer-cell".len() as u16,
            "track fits the widest cell: {:?}",
            second.rect,
        );
        // Without the fill classes the tracks hug their content: the last
        // column ends well before the row does.
        let last = &table.children[2];
        assert!(
            last.rect.x + last.rect.width < 60,
            "content-sized tracks: {:?}",
            last.rect,
        );
    }

    #[test]
    fn classed_table_fills_the_row_and_ragged_rows_stay_placed() {
        let mut doc = DomDocument::new();
        add_table(
            &mut doc,
            Some("table"),
            &[&["alpha", "beta", "gamma"], &["delta", "epsilon"]],
        );
        let laid = compute(&doc, Rect::new(0, 0, 60, 12));
        let table = &laid[0];
        assert_eq!(table.rect.width, 60, "the table class fills the row");
        assert_eq!(table.children.len(), 5);
        // The short row's cells keep their columns; nothing drifts into the
        // missing third slot.
        assert_eq!(table.children[0].rect.x, table.children[3].rect.x);
        assert_eq!(table.children[1].rect.x, table.children[4].rect.x);
    }

    #[test]
    fn header_rows_count_like_body_rows() {
        let mut doc = DomDocument::new();
        let table = doc.create_element_named("table");
        let root = doc.root();
        doc.append_child(root, table);
        let thead = doc.create_element_named("thead");
        doc.append_child(table, thead);
        let head_row = doc.create_element_named("tr");
        doc.append_child(thead, head_row);
        add_cell(&mut doc, head_row, "th", "Name");
        add_cell(&mut doc, head_row, "th", "State");
        let tbody = doc.create_element_named("tbody");
        doc.append_child(table, tbody);
        let body_row = doc.create_element_named("tr");
        doc.append_child(tbody, body_row);
        add_cell(&mut doc, body_row, "td", "a-much-longer-name");
        add_cell(&mut doc, body_row, "td", "ok");

        let laid = compute(&doc, Rect::new(0, 0, 80, 12));
        let table = &laid[0];
        assert_eq!(table.children.len(), 4);
        assert_eq!(table.children[0].rect.x, table.children[2].rect.x);
        assert_eq!(table.children[1].rect.x, table.children[3].rect.x);
        // The header sits above the body row.
        assert!(table.children[0].rect.y < table.children[2].rect.y);
    }

    #[test]
    fn w_auto_opts_a_classed_table_back_into_hugging() {
        let mut doc = DomDocument::new();
        add_table(
            &mut doc,
            Some("table w-auto"),
            &[&["alpha", "beta"], &["a", "b"]],
        );
        let laid = compute(&doc, Rect::new(0, 0, 60, 8));
        let table = &laid[0];
        let last = &table.children[1];
        assert!(
            last.rect.x + last.rect.width < 30,
            "content-sized tracks: {:?}",
            last.rect,
        );
        assert_eq!(table.children[0].rect.x, table.children[2].rect.x);
    }

    #[test]
    fn a_table_without_cells_stays_a_block() {
        let mut doc = DomDocument::new();
        let table = doc.create_element_named("table");
        let root = doc.root();
        doc.append_child(root, table);
        let text = doc.create_text_node("just text");
        doc.append_child(table, text);

        let laid = compute(&doc, Rect::new(0, 0, 40, 4));
        // The block fallback keeps the text as an ordinary laid child.
        assert_eq!(laid[0].children.len(), 1);
    }
}
