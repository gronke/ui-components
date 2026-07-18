//! The cascade: four origins, importance, specificity, source order —
//! resolved into a per-node computed-style table over the document.
//!
//! Origin dominates importance (unlike full CSS's author rules): the
//! overrides sheet must reliably beat generated Bootstrap, `!important`
//! included — the model that makes a curated overlay trustworthy.

use std::collections::HashMap;

use selectors::matching::matches_selector;
use uic_dom::{Document, NodeData, NodeId};

use crate::computed::ComputedStyle;
use crate::parse::Stylesheet;
use crate::select::El;
use crate::value::parse_value;

/// The cascade origins, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    Ua,
    Target,
    Component,
    App,
}

/// One sheet in the cascade; component sheets carry their scope root.
pub struct SheetRef<'a> {
    pub origin: Origin,
    pub sheet: &'a Stylesheet,
    pub scope: Option<NodeId>,
}

/// The resolved table: element nodes to computed styles.
pub type StyleTable = HashMap<NodeId, ComputedStyle>;

/// Resolves the whole document against the sheet set.
pub fn resolve_document<T>(
    doc: &Document<T>,
    sheets: &[SheetRef<'_>],
    focused: Option<NodeId>,
) -> StyleTable {
    let mut table = StyleTable::new();
    let root_style = ComputedStyle::default();
    let children: Vec<NodeId> = doc.children(doc.root()).collect();
    for child in children {
        resolve_into(doc, child, &root_style, sheets, focused, &mut table);
    }
    table
}

fn resolve_into<T>(
    doc: &Document<T>,
    node: NodeId,
    parent: &ComputedStyle,
    sheets: &[SheetRef<'_>],
    focused: Option<NodeId>,
    table: &mut StyleTable,
) {
    if !matches!(doc.node(node), Some(NodeData::Element(_))) {
        return;
    }
    let style = resolve_element(doc, node, parent, sheets, focused);
    let children: Vec<NodeId> = doc.children(node).collect();
    for child in children {
        resolve_into(doc, child, &style, sheets, focused, table);
    }
    table.insert(node, style);
}

/// (origin, importance, specificity, source order): ascending application,
/// the last write wins.
type SortKey = (Origin, bool, u32, u32);

fn resolve_element<T>(
    doc: &Document<T>,
    node: NodeId,
    parent: &ComputedStyle,
    sheets: &[SheetRef<'_>],
    focused: Option<NodeId>,
) -> ComputedStyle {
    let mut matched: Vec<(SortKey, &str, &str)> = Vec::new();
    for sheet_ref in sheets {
        // A scoped sheet only applies inside its component subtree.
        if let Some(scope) = sheet_ref.scope {
            let inside = node == scope || doc.ancestors(node).any(|a| a == scope);
            if !inside {
                continue;
            }
        }
        for rule in &sheet_ref.sheet.rules {
            let specificity = best_matching_specificity(doc, node, rule, sheet_ref.scope, focused);
            let Some(specificity) = specificity else {
                continue;
            };
            for declaration in &rule.declarations {
                matched.push((
                    (
                        sheet_ref.origin,
                        declaration.important,
                        specificity,
                        rule.source_order,
                    ),
                    &declaration.name,
                    &declaration.value,
                ));
            }
        }
    }
    matched.sort_by_key(|(key, ..)| *key);

    let mut style = parent.inherited();
    // Custom properties resolve first: later value substitution sees the
    // cascaded map (inherited entries already present).
    for (_, name, value) in &matched {
        if name.starts_with("--") {
            style
                .custom
                .insert((*name).to_string(), (*value).to_string());
        }
    }
    for (_, name, value) in &matched {
        if name.starts_with("--") {
            continue;
        }
        let Some(substituted) = substitute(value, &style.custom, 0) else {
            continue;
        };
        let Some(resolved) = resolve_calc(&substituted) else {
            continue;
        };
        if let Some(typed) = parse_value(name, &resolved) {
            style.apply(name, &typed);
        }
    }
    style
}

fn best_matching_specificity<T>(
    doc: &Document<T>,
    node: NodeId,
    rule: &crate::parse::Rule,
    scope: Option<NodeId>,
    focused: Option<NodeId>,
) -> Option<u32> {
    use selectors::context::{
        MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
        SelectorCaches,
    };

    let mut caches = SelectorCaches::default();
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        QuirksMode::NoQuirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    context.current_host = scope.map(crate::select::opaque_of);
    let element = El {
        doc,
        node,
        scope,
        focused,
    };
    let mut best: Option<u32> = None;
    for selector in rule.selectors.slice() {
        if matches_selector(selector, 0, None, &element, &mut context) {
            let specificity = selector.specificity();
            best = Some(best.map_or(specificity, |b| b.max(specificity)));
        }
    }
    best
}

/// `var(--name[, fallback])` substitution over raw value text.
/// Unresolvable variables invalidate the declaration (CSS behavior).
fn substitute(value: &str, custom: &HashMap<String, String>, depth: u8) -> Option<String> {
    if depth > 8 || !value.contains("var(") {
        return Some(value.to_string());
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("var(") {
        out.push_str(&rest[..start]);
        let inner_start = start + 4;
        let inner_end = matching_paren(&rest[inner_start..])?;
        let inner = &rest[inner_start..inner_start + inner_end];
        let (name, fallback) = match inner.find(',') {
            Some(comma) => (inner[..comma].trim(), Some(inner[comma + 1..].trim())),
            None => (inner.trim(), None),
        };
        match custom.get(name) {
            Some(resolved) => out.push_str(resolved.trim()),
            None => out.push_str(fallback?),
        }
        rest = &rest[inner_start + inner_end + 1..];
    }
    out.push_str(rest);
    substitute(&out, custom, depth + 1)
}

fn matching_paren(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Single-level additive `calc()` over lengths, folded to px.
fn resolve_calc(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let Some(inner) = trimmed
        .strip_prefix("calc(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return Some(value.to_string());
    };
    let mut total_px = 0.0f32;
    let mut sign = 1.0f32;
    for token in inner.split_whitespace() {
        match token {
            "+" => sign = 1.0,
            "-" => sign = -1.0,
            _ => {
                let px = length_px(token)?;
                total_px += sign * px;
                sign = 1.0;
            }
        }
    }
    Some(format!("{total_px}px"))
}

fn length_px(token: &str) -> Option<f32> {
    let (number, unit) = token
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .map(|pos| token.split_at(pos))?;
    let number: f32 = number.parse().ok()?;
    match unit {
        "px" => Some(number),
        "rem" | "em" => Some(number * 16.0),
        "ch" => Some(number * crate::value::PX_PER_COLUMN),
        "lh" => Some(number * crate::value::PX_PER_ROW),
        _ => None,
    }
}
