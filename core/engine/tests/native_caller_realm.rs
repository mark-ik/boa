#![allow(unused_crate_dependencies)]
//! Native caller-realm provenance survives nested native exceptions and
//! call/apply trampolines.

use boa_engine::{Context, JsNativeError, JsResult, JsValue, NativeFunction, Source, js_string};

#[allow(clippy::unnecessary_wraps)]
fn caller_differs(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let caller = context
        .native_caller_realm()
        .expect("native caller")
        .clone();
    // A nested native exception must restore this invocation's provenance.
    assert!(context.eval(Source::from_bytes("thrower()")).is_err());
    assert_eq!(context.native_caller_realm(), Some(&caller));
    Ok(JsValue::from(&caller != context.realm()))
}

fn thrower(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(JsNativeError::error().with_message("nested throw").into())
}

fn relay(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arguments[0]
        .as_callable()
        .expect("callback")
        .call(&JsValue::undefined(), &[], context)
}

#[test]
fn native_caller_is_preserved_across_realm_switch_and_nested_throw() {
    let mut context = Context::default();
    assert!(context.native_caller_realm().is_none());
    let child = context.create_realm().unwrap();
    let parent = context.enter_realm(child);
    context
        .register_global_callable(
            js_string!("probe"),
            0,
            NativeFunction::from_fn_ptr(caller_differs),
        )
        .unwrap();
    context
        .register_global_callable(
            js_string!("thrower"),
            0,
            NativeFunction::from_fn_ptr(thrower),
        )
        .unwrap();
    context
        .register_global_callable(js_string!("relay"), 1, NativeFunction::from_fn_ptr(relay))
        .unwrap();
    let relay_function = context.eval(Source::from_bytes("relay")).unwrap();
    let child_callback = context
        .eval(Source::from_bytes(
            "(function(){ return probe.call(null); })",
        ))
        .unwrap();
    let probe = context.eval(Source::from_bytes("probe")).unwrap();
    context.enter_realm(parent);
    context
        .register_global_property(
            js_string!("probe"),
            probe,
            boa_engine::property::Attribute::all(),
        )
        .unwrap();

    assert_eq!(
        context.eval(Source::from_bytes("probe()")).unwrap(),
        JsValue::from(true)
    );
    assert!(context.native_caller_realm().is_none());
    context
        .register_global_property(
            js_string!("relay"),
            relay_function,
            boa_engine::property::Attribute::all(),
        )
        .unwrap();
    context
        .register_global_property(
            js_string!("childCallback"),
            child_callback,
            boa_engine::property::Attribute::all(),
        )
        .unwrap();
    // Native call/apply trampolines retain their nearest authored caller.
    assert_eq!(
        context
            .eval(Source::from_bytes("probe.call(null)"))
            .unwrap(),
        JsValue::from(true)
    );
    assert_eq!(
        context
            .eval(Source::from_bytes("probe.apply(null, [])"))
            .unwrap(),
        JsValue::from(true)
    );
    assert!(context.native_caller_realm().is_none());
    assert_eq!(
        context
            .eval(Source::from_bytes("relay(childCallback)"))
            .unwrap(),
        JsValue::from(false)
    );
    assert_eq!(
        context
            .eval(Source::from_bytes(
                "relay(function(){return probe.apply(null,[]);})"
            ))
            .unwrap(),
        JsValue::from(true)
    );
    context.eval(Source::from_bytes("new probe()")).unwrap();
    assert!(context.native_caller_realm().is_none());
}
