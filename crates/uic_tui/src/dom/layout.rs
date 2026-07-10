//! Layout from the retained DOM: taffy computes real CSS flexbox over
//! terminal cells, reading classes and `data-tui` markers straight off the
//! document nodes — no per-frame expansion tree in between.

use ratatui::layout::Rect;
use taffy::prelude::*;
use unicode_width::UnicodeWidthStr;

use uic_dom::NodeData;

use super::widget::WidgetState;
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
        |known, _available, _id, context, _style| match context {
            Some(Measured::Text(text)) => Size {
                width: known.width.unwrap_or(text.width() as f32),
                height: known.height.unwrap_or(1.0),
            },
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
    match &widget.state {
        WidgetState::TextArea(state) => {
            let max_lines = component_attr(doc, node, "max-lines")
                .and_then(|value| value.parse::<u16>().ok())
                .filter(|lines| *lines >= 1)
                .unwrap_or(10);
            // rat's text is newline-terminated: the count includes an empty
            // tail line that never shows in the browser.
            let lines = (state.len_lines() as u16).saturating_sub(1).max(1);
            lines.clamp(1, max_lines.max(1))
        }
        _ => 1,
    }
}

/// Intrinsic width of a widget leaf that sizes to content the way the
/// browser does: a select hugs its closed label plus rat's three marker
/// cells; the text-editing widgets keep the flex default.
fn widget_width(doc: &DomDocument, node: uic_dom::NodeId) -> Option<u16> {
    let widget = doc.element(node).and_then(|el| el.data.widget.as_ref())?;
    match &widget.state {
        WidgetState::Select(state) => {
            let value = state.value();
            let closed = widget
                .options
                .iter()
                .find(|option| option.value == value)
                .map(|option| option.short_label())
                .unwrap_or_default();
            Some((closed.width() as u16).saturating_add(3))
        }
        _ => None,
    }
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
            // Bootstrap's fractional-row spacers round down in cells: mt-1
            // is a quarter rem in the browser, less than any terminal row.
            "mt-1" => {}
            "p-0" => style.padding = taffy::geometry::Rect::zero(),
            _ => {}
        }
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
