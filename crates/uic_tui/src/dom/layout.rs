//! Layout from the retained DOM: taffy computes real CSS flexbox over
//! terminal cells, reading classes and `data-tui` markers straight off the
//! document nodes — no per-frame expansion tree in between.

use ratatui::layout::Rect;
use taffy::prelude::*;
use unicode_width::UnicodeWidthStr;

use uic_dom::NodeData;

use super::DomDocument;

/// A document node with its computed absolute cell rectangle.
pub(crate) struct LaidNode {
    pub node: uic_dom::NodeId,
    pub rect: Rect,
    pub children: Vec<LaidNode>,
}

enum Measured {
    Text(String),
    /// A widget leaf, with its content width when the widget sizes to
    /// content (the select's closed label); `None` measures the flex
    /// default.
    Widget(Option<u16>),
}

/// A node paired with its taffy handle during the build, so the collect
/// pass can zip computed layouts back onto document nodes.
struct Shadow {
    node: uic_dom::NodeId,
    taffy: NodeId,
    children: Vec<Shadow>,
}

pub(crate) fn compute(doc: &DomDocument, area: Rect) -> Vec<LaidNode> {
    let mut tree: TaffyTree<Measured> = TaffyTree::new();
    let roots: Vec<Shadow> = doc
        .children(doc.root())
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|node| build(&mut tree, doc, node, true))
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
            Some(Measured::Text(text)) => {
                let intrinsic = text.width() as f32;
                let width = known.width.unwrap_or(match available.width {
                    AvailableSpace::Definite(limit) => intrinsic.min(limit),
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

    roots
        .iter()
        .map(|shadow| collect(&tree, shadow, (area.x as f32, area.y as f32), area))
        .collect()
}

fn build(
    tree: &mut TaffyTree<Measured>,
    doc: &DomDocument,
    node: uic_dom::NodeId,
    root_child: bool,
) -> Option<Shadow> {
    match doc.node(node)? {
        NodeData::Text(text) => {
            let collapsed = collapse_whitespace(text);
            if collapsed.is_empty() {
                return None;
            }
            let taffy = tree
                .new_leaf_with_context(
                    Style {
                        flex_shrink: 0.0,
                        ..Default::default()
                    },
                    Measured::Text(collapsed),
                )
                .expect("taffy text leaf");
            Some(Shadow {
                node,
                taffy,
                children: Vec::new(),
            })
        }
        NodeData::Element(el) => {
            // Conditional anchors render nothing; their bodies are siblings.
            if &**el.tag() == "template" {
                return None;
            }
            let classes = effective_classes(doc, node);
            if el.attr("data-tui").is_some() {
                let height = widget_height(doc, node);
                let width = widget_width(doc, node);
                let taffy = tree
                    .new_leaf_with_context(
                        widget_style(&classes, height, width),
                        Measured::Widget(width),
                    )
                    .expect("taffy widget leaf");
                return Some(Shadow {
                    node,
                    taffy,
                    children: Vec::new(),
                });
            }
            let children: Vec<Shadow> = doc
                .children(node)
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|child| build(tree, doc, child, false))
                .collect();
            let mut style = container_style(&classes);
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
                node,
                taffy,
                children,
            })
        }
        _ => None,
    }
}

/// The node's classes, with the component-state rewrites the browser gets
/// from stylesheet selectors: a `seamless` component drops its group border
/// (`input-group` renders as a plain flex row).
pub(crate) fn effective_classes(doc: &DomDocument, node: uic_dom::NodeId) -> Vec<String> {
    let mut classes: Vec<String> = doc
        .attribute(node, "class")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if classes.iter().any(|class| class == "input-group")
        && component_attr(doc, node, "seamless").is_some()
    {
        for class in &mut classes {
            if class == "input-group" {
                *class = "d-flex".to_string();
            }
        }
    }
    classes
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

/// The coarse Bootstrap-class → CSS mapping shared with the browser target.
fn container_style(classes: &[String]) -> Style {
    // Elements are blocks unless a class opts into flex, like in CSS.
    let mut style = Style {
        display: Display::Block,
        ..Default::default()
    };
    for class in classes {
        match class.as_str() {
            "d-flex" | "input-group" => {
                style.display = Display::Flex;
                style.flex_direction = FlexDirection::Row;
            }
            "flex-column" => style.flex_direction = FlexDirection::Column,
            "flex-row" => style.flex_direction = FlexDirection::Row,
            "flex-nowrap" => style.flex_wrap = FlexWrap::NoWrap,
            "flex-wrap" => style.flex_wrap = FlexWrap::Wrap,
            "flex-grow-0" => style.flex_grow = 0.0,
            "flex-grow-1" => style.flex_grow = 1.0,
            "flex-shrink-0" => style.flex_shrink = 0.0,
            "flex-shrink-1" => style.flex_shrink = 1.0,
            "w-100" => style.size.width = percent(1.0_f32),
            // Half a rem is under one cell, but a zero gap would fuse the
            // items; one cell is the closest readable analog.
            "gap-2" => {
                style.gap = Size {
                    width: length(1.0_f32),
                    height: length(0.0_f32),
                }
            }
            "align-self-center" => style.align_self = Some(AlignItems::CENTER),
            // Bootstrap's spacers in rows (a terminal row reads like
            // ~1.5rem): 1 and 2 stay sub-row, 3 and 4 round to one row,
            // 5 to two — the margins push the following flow down, like
            // the browser's.
            "mt-1" | "mt-2" | "mb-0" | "mb-1" | "mb-2" => {}
            "mt-3" | "mt-4" => style.margin.top = length(1.0_f32),
            "mb-3" | "mb-4" => style.margin.bottom = length(1.0_f32),
            "mt-5" => style.margin.top = length(2.0_f32),
            "mb-5" => style.margin.bottom = length(2.0_f32),
            // The browser pads the input-group affix (the number's unit);
            // one cell keeps it off the value.
            "input-group-text" => style.padding.left = length(1.0_f32),
            // One blank row between the card's cap and its content, the
            // closest cell to the body's inset (ADR 0017).
            "card-body" => style.padding.top = length(1.0_f32),
            "p-0" => style.padding = taffy::geometry::Rect::zero(),
            _ => {}
        }
    }
    if classes.iter().any(|c| c == "card") {
        // The card is the generic bordered container (ADR 0017): reserve
        // the border cells plus one cell of horizontal padding, mirroring
        // the input group's treatment below.
        style.border = taffy::geometry::Rect {
            left: length(1.0_f32),
            right: length(1.0_f32),
            top: length(1.0_f32),
            bottom: length(1.0_f32),
        };
        style.padding.left = length(1.0_f32);
        style.padding.right = length(1.0_f32);
    }
    if classes.iter().any(|c| c == "input-group") {
        // The input group draws a border block; reserve the cells, plus one
        // cell of horizontal padding — the closest cells get to the
        // form-control's side padding in the browser.
        style.border = taffy::geometry::Rect {
            left: length(1.0_f32),
            right: length(1.0_f32),
            top: length(1.0_f32),
            bottom: length(1.0_f32),
        };
        style.padding = taffy::geometry::Rect {
            left: length(1.0_f32),
            right: length(1.0_f32),
            top: length(0.0_f32),
            bottom: length(0.0_f32),
        };
    }
    style
}

fn widget_style(classes: &[String], height: u16, width: Option<u16>) -> Style {
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
    for class in classes {
        match class.as_str() {
            "flex-grow-1" => style.flex_grow = 1.0,
            "flex-shrink-0" => style.flex_shrink = 0.0,
            "w-100" => style.size.width = percent(1.0_f32),
            _ => {}
        }
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
        node: shadow.node,
        rect,
        children,
    }
}

pub(crate) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Greedy word wrap in cells, the reservation behind ratatui's trimmed
/// `Wrap`: words fill each line up to the width and an oversized word
/// breaks across lines.
fn wrapped_lines(text: &str, width: f32) -> u16 {
    let width = width.round().max(1.0) as usize;
    let mut lines: u16 = 1;
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let len = word.width();
        if used > 0 && used + 1 + len <= width {
            used += 1 + len;
            continue;
        }
        if used == 0 && len <= width {
            used = len;
            continue;
        }
        if used > 0 {
            lines += 1;
        }
        let mut rest = len;
        while rest > width {
            lines += 1;
            rest -= width;
        }
        used = rest;
    }
    lines
}

/// The widest single word — the narrowest a text can measure (MinContent).
fn longest_word(text: &str) -> f32 {
    text.split_whitespace()
        .map(|word| word.width())
        .max()
        .unwrap_or(0) as f32
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
