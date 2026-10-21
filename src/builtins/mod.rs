mod array;
mod collections;
mod date;
mod json;
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
pub use symbol::{iter_result, symbol_text};

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
    c.set_own("prototype", Value::Object(prototype.clone()));
    prototype.set_own("constructor", Value::Object(c.clone()));
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
}

pub fn hide_builtin_members(vm: &mut Vm) {
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
    ] {
        hide(p);
    }
    for p in &r.error_protos {
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
    ] {
        if let Some(Value::Object(o)) = vm.globals.get(name) {
            hide(o);
            if let Some(Value::Object(p)) = o.get_own("prototype") {
                hide(&p);
            }
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
