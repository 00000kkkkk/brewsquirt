pub mod array;
pub mod binary;
mod collections;
mod date;
mod extras;
pub mod intl;
mod intrinsics;
mod json;
pub mod node;
pub mod reflect;
pub mod typed;
pub use reflect::{array_like, key_value};
mod number;
mod object;
mod promise;
mod regexp;
mod stdlib;
mod string;
mod symbol;
mod system;
mod testing;
pub mod web;

use crate::inspect::inspect_plain;
use crate::object::{Native, NativeFn, Obj, ObjKind};
use crate::value::Value;
use crate::vm::Vm;
use std::rc::Rc;

pub use date::{date_to_string, iso_string};
pub use json::{json_parse, json_stringify};
pub use promise::run_prelude;
pub use regexp::make_regexp;
pub use symbol::{iter_result, symbol_from_key, symbol_text};
pub use system::exit_code;

pub fn intl_format(
    vm: &mut Vm,
    ctor: &str,
    locales: Value,
    options: Value,
    value: Value,
) -> Result<Value, Value> {
    let intl = intl::load(vm)?;
    let c = vm.get_prop(&intl, ctor)?;
    let fmt = vm.construct(&c, &[locales, options])?;
    let f = vm.get_prop(&fmt, "format")?;
    vm.call_value(&f, fmt, &[value])
}

pub fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_default()
}

pub fn native_fn(vm: &Vm, name: &str, arity: u32, func: NativeFn) -> Value {
    Value::Object(Obj::new(
        ObjKind::Native(Box::new(Native {
            name: name.into(),
            arity,
            func,
            ctor: None,
        })),
        Some(vm.realm.function_proto.clone()),
    ))
}

pub fn box_for(this: &Value, v: Value) -> Option<Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Primitive(Value::Undefined)) => Some(
            Value::Object(Obj::new(ObjKind::Primitive(v), o.proto.borrow().clone())),
        ),
        _ => None,
    }
}

pub fn constructing(this: &Value) -> bool {
    matches!(this, Value::Object(o) if matches!(o.kind, ObjKind::Primitive(Value::Undefined)))
}

pub fn method(vm: &Vm, target: &Rc<Obj>, name: &str, arity: u32, func: NativeFn) {
    target.set_own(name, native_fn(vm, name, arity, func));
}

pub fn make_ctor(
    vm: &Vm,
    name: &str,
    arity: u32,
    func: NativeFn,
    ctor: Option<fn() -> ObjKind>,
    prototype: &Rc<Obj>,
) -> Rc<Obj> {
    let c = Obj::new(
        ObjKind::Native(Box::new(Native {
            name: name.into(),
            arity,
            func,
            ctor,
        })),
        Some(vm.realm.function_proto.clone()),
    );
    c.set_with_attrs(
        "prototype",
        Value::Object(prototype.clone()),
        crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
    );
    prototype.set_with_attrs(
        "constructor",
        Value::Object(c.clone()),
        crate::object::NON_ENUM,
    );
    c
}

pub fn expect_fn(vm: &mut Vm, v: &Value) -> Result<(), Value> {
    if v.is_callable() {
        Ok(())
    } else {
        Err(vm.type_error(format!("{} is not a function", inspect_plain(v))))
    }
}

pub fn to_integer(vm: &mut Vm, v: &Value) -> Result<f64, Value> {
    let n = vm.to_number(v)?;
    Ok(if n.is_nan() { 0.0 } else { n.trunc() })
}

pub fn relative_index(n: f64, len: usize) -> usize {
    let len_f = len as f64;
    if n < 0.0 {
        (len_f + n).max(0.0) as usize
    } else {
        n.min(len_f) as usize
    }
}

pub fn install(vm: &mut Vm) {
    object::install(vm);
    array::install(vm);
    string::install(vm);
    number::install(vm);
    json::install(vm);
    collections::install(vm);
    date::install(vm);
    promise::install(vm);
    regexp::install(vm);
    symbol::install(vm);
    system::install(vm);
    web::install(vm);
    binary::install(vm);
    reflect::install(vm);
    intrinsics::install(vm);
    let global = Obj::new(ObjKind::Global, Some(vm.realm.object_proto.clone()));
    vm.define_global("globalThis", Value::Object(global));
    for name in ["Math", "JSON", "Atomics"] {
        if let Some(Value::Object(o)) = vm.globals.get(name).cloned() {
            o.set_with_attrs(
                crate::object::TO_STRING_TAG_KEY,
                name.into(),
                crate::object::NON_ENUM | crate::object::READ_ONLY,
            );
        }
    }
}

pub fn hide_builtin_members(vm: &mut Vm) {
    for name in vm.globals.names() {
        let bits = match &*name {
            "undefined" | "NaN" | "Infinity" => {
                crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG
            }
            _ => crate::object::NON_ENUM,
        };
        vm.globals.attrs.insert(name, bits);
    }
    vm.globals.readonly = true;
    let mut seen = Vec::new();
    let mut hide = |o: &Rc<Obj>| {
        if !seen.iter().any(|s| Rc::ptr_eq(s, o)) {
            o.props.borrow_mut().hide_all();
            seen.push(o.clone());
        }
    };
    let r = &vm.realm;
    for p in [
        &r.object_proto,
        &r.function_proto,
        &r.array_proto,
        &r.string_proto,
        &r.number_proto,
        &r.boolean_proto,
        &r.map_proto,
        &r.set_proto,
        &r.iter_proto,
        &r.generator_proto,
        &r.async_generator_proto,
        &r.promise_proto,
        &r.symbol_proto,
        &r.regexp_proto,
        &r.date_proto,
        &r.weakmap_proto,
        &r.weakset_proto,
        &r.weakref_proto,
        &r.array_buffer_proto,
        &r.typed_proto,
        &r.dataview_proto,
    ] {
        hide(p);
    }
    for p in r.error_protos.iter().chain(r.typed_protos.iter()) {
        hide(p);
    }
    for name in [
        "Object",
        "Function",
        "Array",
        "String",
        "Number",
        "Boolean",
        "Symbol",
        "Promise",
        "RegExp",
        "Date",
        "Map",
        "Set",
        "WeakMap",
        "WeakSet",
        "WeakRef",
        "Error",
        "TypeError",
        "RangeError",
        "ReferenceError",
        "SyntaxError",
        "URIError",
        "EvalError",
        "Math",
        "JSON",
        "Reflect",
        "ArrayBuffer",
        "DataView",
        "TextEncoder",
        "TextDecoder",
        "Int8Array",
        "Uint8Array",
        "Uint8ClampedArray",
        "Int16Array",
        "Uint16Array",
        "Int32Array",
        "Uint32Array",
        "Float32Array",
        "Float64Array",
        "BigInt64Array",
        "BigUint64Array",
        "BigInt",
        "Proxy",
    ] {
        if let Some(Value::Object(o)) = vm.globals.get(name) {
            hide(o);
            if let Some(Value::Object(p)) = o.get_own("prototype") {
                hide(&p);
            }
        }
    }
    if let Some(Value::Object(i8)) = vm.globals.get("Int8Array") {
        if let Some(ta) = i8.proto.borrow().clone() {
            hide(&ta);
        }
    }
}

pub fn builtin_module(vm: &mut Vm, name: &str) -> Option<Result<Value, Value>> {
    let v = match name {
        "fs" => system::fs_module(vm),
        "path" => stdlib::path_module(vm),
        "os" => stdlib::os_module(vm),
        "child_process" => stdlib::child_process_module(vm),
        "util" => testing::util_module(vm),
        "test" => return Some(testing::test_module(vm)),
        "http" => return Some(web::http_module(vm)),
        "process" => vm.globals.get("process").cloned()?,
        "math" => vm.globals.get("Math").cloned()?,
        "json" => vm.globals.get("JSON").cloned()?,
        _ => return None,
    };
    Some(Ok(v))
}
