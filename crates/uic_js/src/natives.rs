//! The flat `__uic_*` natives the runtime modules call, as thin Boa
//! wrappers over the shared host operations (`uic_tui::dom::HostState`) —
//! the browser host exposes the same bodies through its wasm session.

use boa_engine::{js_string, Context, JsNativeError, JsResult, JsValue, NativeFunction};

use crate::error::Error;
use crate::state::with_state;

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

fn selector_error(message: String) -> boa_engine::JsError {
    JsNativeError::error().with_message(message).into()
}

pub(crate) fn register_natives(context: &mut Context) -> Result<(), Error> {
    // __uic_commit(handle, html): the subtree-swap render path; focus
    // survives by data-path (see HostState::commit).
    context.register_global_callable(
        js_string!("__uic_commit"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let html = arg_string(args, 1, context)?;
            with_state(|state| state.commit(handle, &html))?;
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
            let value = with_state(|state| state.attribute(handle, &name))?;
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
            with_state(|state| state.set_attribute(handle, &name, &value))?;
            Ok(JsValue::undefined())
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_has_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            let has = with_state(|state| state.has_attribute(handle, &name))?;
            Ok(JsValue::from(has))
        }),
    )?;

    // The input facade: a mounted terminal widget's live text behind
    // `el.value` — null on plain nodes, so the facade can fall back to the
    // value attribute.
    context.register_global_callable(
        js_string!("__uic_widget_value"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_node(args)?;
            let value = with_state(|state| state.widget_value(handle))?;
            Ok(value.map_or(JsValue::null(), |v| js_string!(v).into()))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_set_widget_value"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let text = arg_string(args, 1, context)?;
            with_state(|state| state.set_widget_value(handle, &text))?;
            Ok(JsValue::undefined())
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_remove_attr"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let name = arg_string(args, 1, context)?;
            with_state(|state| state.remove_attribute(handle, &name))?;
            Ok(JsValue::undefined())
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_text"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_node(args)?;
            let text = with_state(|state| state.text(handle))?;
            Ok(js_string!(text).into())
        }),
    )?;

    // __uic_query(handle, selector) -> handles.
    context.register_global_callable(
        js_string!("__uic_query"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let selector = arg_string(args, 1, context)?;
            let matches =
                with_state(|state| state.query(handle, &selector))?.map_err(selector_error)?;
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
            let parent = with_state(|state| state.parent(handle))?;
            Ok(parent.map_or(JsValue::from(-1), |h| JsValue::from(h as f64)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_matches"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, context| {
            let handle = arg_node(args)?;
            let selector = arg_string(args, 1, context)?;
            let result =
                with_state(|state| state.matches(handle, &selector))?.map_err(selector_error)?;
            Ok(JsValue::from(result))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_contains"),
        2,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let outer = arg_node(args)?;
            let inner = arg_number(args, 1)? as usize;
            let contains = with_state(|state| state.contains(outer, inner))?;
            Ok(JsValue::from(contains))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_focused"),
        0,
        NativeFunction::from_fn_ptr(|_this, _args, _context| {
            let focused = with_state(|state| state.focused_handle())?;
            Ok(focused.map_or(JsValue::from(-1), |h| JsValue::from(h as f64)))
        }),
    )?;

    context.register_global_callable(
        js_string!("__uic_set_focused"),
        1,
        NativeFunction::from_fn_ptr(|_this, args, _context| {
            let handle = arg_number(args, 0)?;
            with_state(|state| {
                state.set_focused_handle((handle >= 0.0).then_some(handle as usize));
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
