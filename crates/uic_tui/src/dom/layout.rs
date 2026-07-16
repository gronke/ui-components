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
            if &**el.tag() == "table" {
                if let Some(shadow) = build_table(tree, doc, node, &classes, root_child) {
                    return Some(shadow);
                }
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

/// Lays a `<table>` out as a grid with shared column tracks (ADR 0019).
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
    classes: &[String],
    root_child: bool,
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
            let Some(shadow) = build(tree, doc, cell, false) else {
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
    let fill = classes
        .iter()
        .any(|class| class == "table" || class == "w-100")
        && !classes.iter().any(|class| class == "w-auto");
    let track: GridTemplateComponent<String> = if fill {
        minmax(auto(), fr(1.0_f32))
    } else {
        auto()
    };
    let mut style = container_style(classes);
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
        node,
        taffy,
        children,
    })
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

/// Collapses runs of ASCII whitespace to single spaces and trims the ends,
/// like the browser flows prose. Non-breaking spaces are content, not
/// separators — the browser renders `&nbsp;`, so the terminal keeps it too
/// (indentation would otherwise collapse away).
pub(crate) fn collapse_whitespace(text: &str) -> String {
    words(text).collect::<Vec<_>>().join(" ")
}

/// The wrap words of a text: runs unbroken by ASCII whitespace, so a
/// non-breaking space glues its neighbors into one word.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_ascii_whitespace())
        .filter(|word| !word.is_empty())
}

/// Greedy word wrap in cells, the reservation behind ratatui's `Wrap`:
/// words fill each line up to the width and an oversized word breaks
/// across lines.
fn wrapped_lines(text: &str, width: f32) -> u16 {
    let width = width.round().max(1.0) as usize;
    let mut lines: u16 = 1;
    let mut used = 0usize;
    for word in words(text) {
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
    words(text).map(|word| word.width()).max().unwrap_or(0) as f32
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
mod text_tests {
    use super::*;

    #[test]
    fn ascii_whitespace_collapses_and_trims() {
        assert_eq!(collapse_whitespace("  a \n\t b  "), "a b");
    }

    #[test]
    fn non_breaking_spaces_survive_the_collapse() {
        assert_eq!(
            collapse_whitespace("\n  \u{a0}\u{a0}indented rest\n"),
            "\u{a0}\u{a0}indented rest"
        );
    }

    #[test]
    fn non_breaking_spaces_glue_wrap_words() {
        // One 8-cell word (two NBSPs plus "abcdef"): it wraps as a unit, so
        // a 10-cell line holds it and pushes the next word down.
        assert_eq!(wrapped_lines("\u{a0}\u{a0}abcdef xyz", 10.0), 2);
        assert_eq!(longest_word("\u{a0}\u{a0}abcdef xyz"), 8.0);
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
