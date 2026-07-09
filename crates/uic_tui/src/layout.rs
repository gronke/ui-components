//! Layout: taffy computes real CSS flexbox over terminal cells, so the
//! terminal arrangement coarsely matches what the same Bootstrap-ish classes
//! produce in the browser.

use ratatui::layout::Rect;
use taffy::prelude::*;
use unicode_width::UnicodeWidthStr;

use crate::expand::RNode;

/// An `RNode` with its computed absolute cell rectangle.
pub(crate) struct Laid<'r> {
    pub rnode: &'r RNode,
    pub rect: Rect,
    pub children: Vec<Laid<'r>>,
}

enum Measured {
    Text(String),
    Widget,
}

pub(crate) fn compute<'r>(roots: &'r [RNode], area: Rect) -> Vec<Laid<'r>> {
    let mut tree: TaffyTree<Measured> = TaffyTree::new();

    let child_ids: Vec<NodeId> = roots.iter().map(|node| build(&mut tree, node)).collect();
    // The host element is a block, like in the browser: children stack and
    // flex-* classes only take effect inside d-flex containers.
    let root_style = Style {
        display: Display::Block,
        size: Size {
            width: length(area.width as f32),
            height: length(area.height as f32),
        },
        ..Default::default()
    };
    let root = tree
        .new_with_children(root_style, &child_ids)
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
            Some(Measured::Widget) => Size {
                width: known.width.unwrap_or(12.0),
                height: known.height.unwrap_or(1.0),
            },
            None => Size::ZERO,
        },
    )
    .expect("taffy layout");

    roots
        .iter()
        .zip(tree.children(root).expect("root children"))
        .map(|(rnode, id)| collect(&tree, id, rnode, (area.x as f32, area.y as f32), area))
        .collect()
}

fn build(tree: &mut TaffyTree<Measured>, rnode: &RNode) -> NodeId {
    match rnode {
        RNode::Text(text) => tree
            .new_leaf_with_context(
                Style {
                    flex_shrink: 0.0,
                    ..Default::default()
                },
                Measured::Text(text.clone()),
            )
            .expect("taffy text leaf"),
        RNode::Element {
            classes,
            slot,
            widget_height,
            children,
        } => {
            if slot.is_some() {
                return tree
                    .new_leaf_with_context(widget_style(classes, *widget_height), Measured::Widget)
                    .expect("taffy widget leaf");
            }
            let child_ids: Vec<NodeId> = children.iter().map(|child| build(tree, child)).collect();
            tree.new_with_children(container_style(classes), &child_ids)
                .expect("taffy container")
        }
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

fn widget_style(classes: &[String], height: u16) -> Style {
    let height = height.max(1) as f32;
    let mut style = Style {
        size: Size {
            width: auto(),
            height: length(height),
        },
        min_size: Size {
            width: length(12.0_f32),
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

fn collect<'r>(
    tree: &TaffyTree<Measured>,
    id: NodeId,
    rnode: &'r RNode,
    origin: (f32, f32),
    bounds: Rect,
) -> Laid<'r> {
    let layout = tree.layout(id).expect("computed layout");
    let x = origin.0 + layout.location.x;
    let y = origin.1 + layout.location.y;
    let rect = clamp_rect(x, y, layout.size.width, layout.size.height, bounds);

    let children = match rnode {
        RNode::Element { children, slot, .. } if slot.is_none() => tree
            .children(id)
            .expect("taffy children")
            .into_iter()
            .zip(children)
            .map(|(child_id, child)| collect(tree, child_id, child, (x, y), bounds))
            .collect(),
        _ => Vec::new(),
    };

    Laid {
        rnode,
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
