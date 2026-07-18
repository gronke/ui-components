//! The flat `__uic_*` natives the runtime modules call: document mutation,
//! attribute and text access, the selector micro-matcher, tree relations
//! and the focus state.

use std::collections::HashMap;

use boa_engine::{js_string, Context, JsNativeError, JsResult, JsValue, NativeFunction};
use uic_dom::NodeId;
use uic_tui::dom::DomDocument;

use crate::error::Error;
use crate::state::{with_state, HostState};

/// The selector subset component code uses: attribute equality plus the
/// `:focus` and `:dir()` pseudo-classes.
fn matches_selector(state: &HostState, node: NodeId, selector: &str) -> JsResult<bool> {
    match selector {
        ":focus" => Ok(state.focused == Some(node)),
        ":dir(ltr)" => Ok(true),
        ":dir(rtl)" => Ok(false),
        _ => {
            let (name, value) = parse_attr_selector(selector)?;
            Ok(match state.doc.attribute(node, &name) {
                Some(actual) => value.as_deref().is_none_or(|v| v == actual),
                None => false,
            })
        }
    }
}

/// `[name]` / `[name="value"]` — the attribute-selector subset the facades
/// need; anything richer is a loud error, not silent mismatch.
fn parse_attr_selector(selector: &str) -> JsResult<(String, Option<String>)> {
    let inner = selector
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            JsNativeError::error().with_message(format!(
                "unsupported selector {selector:?} (attribute only)"
            ))
        })?;
    match inner.split_once('=') {
        Some((name, value)) => Ok((
            name.to_string(),
            Some(value.trim_matches(['"', '\'']).to_string()),
        )),
        None => Ok((inner.to_string(), None)),
    }
}

fn arg_number(args: &[JsValue], index: usize) -> JsResult<f64> {
    args.get(index).and_then(JsValue::as_number).ok_or_else(|| {
        JsNativeError::typ()
            .with_message(format!("argument {index} must be a number"))
            .into()
    })
}

fn arg_string(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    Ok(args
        .get(index)
        .cloned()
        .unwrap_or_default()
        .to_string(context)?
        .to_std_string_escaped())
}

fn arg_node(args: &[JsValue]) -> JsResult<usize> {
    Ok(arg_number(args, 0)? as usize)
}

pub(crate) fn register_natives(context: &mut Context) -> Result<(), Error> {
    // __uic_commit(handle, html): replace the element's children with the
    // parsed fragment — the subtree-swap render path. Focus inside the
    // swapped subtree survives by its `data-path`, the component's own
    // stable row key.
    context.register_global_callable(
        js_string!("__uic_commit"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_number(args, 0)? as usize;
            let html = arg_string(args, 1, context)?;
            with_state(|state| {
                let Some(target) = state.node(handle) else {
                    return;
                };
                let focus_path = state
                    .focused
                    .filter(|&f| f == target || state.doc.ancestors(f).any(|node| node == target));
                let focus_path = focus_path
                    .and_then(|f| state.doc.attribute(f, "data-path").map(str::to_string));
                let scratch: DomDocument = uic_dom::Document::parse_fragment(&html, "body");
                let children: Vec<NodeId> = state.doc.children(target).collect();
                for child in children {
                    state.doc.remove(child);
                }
                let sources: Vec<NodeId> = scratch.children(scratch.root()).collect();
                let mut map = HashMap::new();
                for source in sources {
                    if let Some(copy) = state.doc.import_node(&scratch, source, &mut map) {
                        state.doc.append_child(target, copy);
                    }
                }
                if let Some(focused) = state.focused {
                    if state.doc.node(focused).is_none() {
                        let resolved = focus_path.and_then(|path| {
                            state.doc.descendants(target).find(|&node| {
                                state.doc.attribute(node, "data-path") == Some(path.as_str())
                            })
                        });
                        state.focused = Some(resolved.unwrap_or(target));
                    }
                }
                state.dirty = true;
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    // Attribute and text access for the element facades.
    context.register_global_callable(
        js_string!("__uic_get_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            let value = with_state(|state| {
                state
                    .node(handle)
                    .and_then(|node| state.doc.attribute(node, &name).map(str::to_string))
            })?;
            Ok(value.map_or(JsValue::null(), |v| js_string!(v).into()))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_set_attr"),
        3,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            let value = arg_string(args, 2, context)?;
            with_state(|state| {
                if let Some(node) = state.node(handle) {
                    state.doc.set_attribute(node, &name, &value);
                    state.dirty = true;
                }
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_has_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            let has = with_state(|state| {
                state
                    .node(handle)
                    .is_some_and(|node| state.doc.attribute(node, &name).is_some())
            })?;
            Ok(JsValue::from(has))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_remove_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            with_state(|state| {
                if let Some(node) = state.node(handle) {
                    state.doc.remove_attribute(node, &name);
                    state.dirty = true;
                }
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_text"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_node(args)?;
            let text = with_state(|state| {
                state
                    .node(handle)
                    .map(|node| state.doc.text_content(node))
                    .unwrap_or_default()
            })?;
            Ok(js_string!(text).into())
        }),
    )?;

    // __uic_query(handle, selector) -> handles. The selector micro-matcher
    // covers what component code uses: `[name]` and `[name="value"]`.
    context.register_global_callable(
        js_string!("__uic_query"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let selector = arg_string(args, 1, context)?;
            let (name, value) = parse_attr_selector(&selector)?;
            let matches: Vec<usize> = with_state(|state| {
                let Some(root) = state.node(handle) else {
                    return Vec::new();
                };
                let nodes: Vec<NodeId> = state
                    .doc
                    .descendants(root)
                    .filter(|&node| match state.doc.attribute(node, &name) {
                        Some(actual) => value.as_deref().is_none_or(|v| v == actual),
                        None => false,
                    })
                    .collect();
                nodes.into_iter().map(|node| state.handle(node)).collect()
            })?;
            let array = boa_engine::object::builtins::JsArray::from_iter(
                matches.into_iter().map(|h: usize| JsValue::from(h as f64)),
                context,
            );
            Ok(array.into())
        }),
    )?;

    // Tree relations and state for the facades and the dispatcher.
    context.register_global_callable(
        js_string!("__uic_parent"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_node(args)?;
            let parent = with_state(|state| {
                let parent = state.node(handle).and_then(|node| state.doc.parent(node));
                parent
                    .filter(|&p| matches!(state.doc.node(p), Some(uic_dom::NodeData::Element(_))))
                    .map(|p| state.handle(p))
            })?;
            Ok(parent.map_or(JsValue::from(-1), |h| JsValue::from(h as f64)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_matches"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let selector = arg_string(args, 1, context)?;
            let result = with_state(|state| {
                state
                    .node(handle)
                    .map(|node| matches_selector(state, node, &selector))
            })?;
            result
                .transpose()
                .map(|m| JsValue::from(m.unwrap_or(false)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_contains"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let outer = arg_node(args)?;
            let inner = arg_number(args, 1)? as usize;
            let contains = with_state(|state| {
                let (Some(outer), Some(inner)) = (state.node(outer), state.node(inner)) else {
                    return false;
                };
                outer == inner || state.doc.ancestors(inner).any(|node| node == outer)
            })?;
            Ok(JsValue::from(contains))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_focused"),
        0,
        NativeFunction::from_fn_ptr(|_this, _args, _context| {
            let focused = with_state(|state| state.focused.map(|node| state.handle(node)))?;
            Ok(focused.map_or(JsValue::from(-1), |h| JsValue::from(h as f64)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_set_focused"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_number(args, 0)?;
            with_state(|state| {
                state.focused = if handle < 0.0 {
                    None
                } else {
                    state.node(handle as usize)
                };
                state.dirty = true;
            })?;
            Ok(JsValue::undefined())
        }),
    )?;

    // __uic_adopt_styles(tag, cssText): a component's static styles enter
    // the cascade, scoped per instance (ADR 0021 stage 2). The dropped
    // count feeds the degradation report.
    context.register_global_callable(
        js_string!("__uic_adopt_styles"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let tag = arg_string(args, 0, context)?;
            let css_text = arg_string(args, 1, context)?;
            let dropped = uic_tui::dom::adopt_component_sheet(&tag, &css_text);
            Ok(JsValue::from(dropped as f64))
        }),
    )?;

    // __uic_log(message): debugging visibility from scripts.
    context.register_global_callable(
        js_string!("__uic_log"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let message = arg_string(args, 0, context)?;
            eprintln!("[uic_js] {message}");
            Ok(JsValue::undefined())
        }),
    )?;

    Ok(())
}
