use super::{arg, make_ctor, method};
use crate::event_loop::{ResumeMode, Step};
use crate::inspect::inspect_plain;
use crate::object::{ASYNC_ITER_KEY, ITER_KEY, IterState, Obj, ObjKind, Symbol};
use crate::value::Value;
use crate::vm::Vm;
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn well_known(description: &str, key: &str) -> Value {
    Value::Symbol(Rc::new(Symbol {
        description: Some(description.into()),
        key: key.into(),
    }))
}

fn function_has_instance(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Value::Object(c) = &this else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(vm.ordinary_has_instance(&arg(args, 0), c)?))
}

pub fn symbol_from_key(key: &str) -> Value {
    let desc = crate::object::symbol_key_description(key);
    Value::Symbol(Rc::new(Symbol {
        description: Some(desc.into()),
        key: key.into(),
    }))
}

pub fn install(vm: &mut Vm) {
    let sp = vm.realm.symbol_proto.clone();
    method(vm, &sp, "toString", 0, symbol_to_string);
    method(vm, &sp, "valueOf", 0, symbol_value_of);
    let tp = super::native_fn(vm, crate::object::TO_PRIMITIVE_KEY, 1, symbol_to_primitive);
    sp.set_with_attrs(
        crate::object::TO_PRIMITIVE_KEY,
        tp,
        crate::object::NON_ENUM | crate::object::READ_ONLY,
    );
    let symbol = make_ctor(
        vm,
        "Symbol",
        0,
        symbol_ctor,
        Some(crate::object::wrapper_marker),
        &sp,
    );
    method(vm, &symbol, "for", 1, symbol_for);
    method(vm, &symbol, "keyFor", 1, symbol_key_for);
    vm.symbol_iterator = well_known("Symbol.iterator", ITER_KEY);
    vm.symbol_async_iterator = well_known("Symbol.asyncIterator", ASYNC_ITER_KEY);
    symbol.set_own("iterator", vm.symbol_iterator.clone());
    symbol.set_own("asyncIterator", vm.symbol_async_iterator.clone());
    use crate::object::{
        HAS_INSTANCE_KEY, IS_CONCAT_SPREADABLE_KEY, MATCH_ALL_KEY, MATCH_KEY, REPLACE_KEY,
        SEARCH_KEY, SPECIES_KEY, SPLIT_KEY, TO_PRIMITIVE_KEY, TO_STRING_TAG_KEY, UNSCOPABLES_KEY,
    };
    for (name, key) in [
        ("toStringTag", TO_STRING_TAG_KEY),
        ("toPrimitive", TO_PRIMITIVE_KEY),
        ("hasInstance", HAS_INSTANCE_KEY),
        ("species", SPECIES_KEY),
        ("isConcatSpreadable", IS_CONCAT_SPREADABLE_KEY),
        ("unscopables", UNSCOPABLES_KEY),
        ("match", MATCH_KEY),
        ("matchAll", MATCH_ALL_KEY),
        ("replace", REPLACE_KEY),
        ("search", SEARCH_KEY),
        ("split", SPLIT_KEY),
        ("dispose", "\u{0}@@dispose:Symbol.dispose"),
        ("asyncDispose", "\u{0}@@asyncDispose:Symbol.asyncDispose"),
    ] {
        symbol.set_with_attrs(
            name,
            well_known(&format!("Symbol.{name}"), key),
            crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
        );
    }
    let has_instance = super::native_fn(vm, "[Symbol.hasInstance]", 1, function_has_instance);
    vm.default_has_instance = has_instance.clone();
    vm.realm.function_proto.set_with_attrs(
        HAS_INSTANCE_KEY,
        has_instance,
        crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
    );
    let tags: Vec<(Rc<Obj>, &str)> = vec![
        (vm.realm.map_proto.clone(), "Map"),
        (vm.realm.set_proto.clone(), "Set"),
        (vm.realm.promise_proto.clone(), "Promise"),
        (vm.realm.symbol_proto.clone(), "Symbol"),
        (vm.realm.weakmap_proto.clone(), "WeakMap"),
        (vm.realm.weakset_proto.clone(), "WeakSet"),
        (vm.realm.weakref_proto.clone(), "WeakRef"),
        (vm.realm.array_buffer_proto.clone(), "ArrayBuffer"),
        (vm.realm.dataview_proto.clone(), "DataView"),
        (vm.realm.generator_proto.clone(), "Generator"),
        (vm.realm.async_generator_proto.clone(), "AsyncGenerator"),
    ];
    for (p, tag) in tags {
        p.set_with_attrs(
            TO_STRING_TAG_KEY,
            tag.into(),
            crate::object::NON_ENUM | crate::object::READ_ONLY,
        );
    }
    vm.define_global("Symbol", Value::Object(symbol));

    let ib = vm.realm.iterator_base.clone();
    method(vm, &ib, ITER_KEY, 0, iter_self);
    ib.props.borrow_mut().set_enumerable(ITER_KEY, false);
    let aib = vm.realm.async_iterator_base.clone();
    method(vm, &aib, ASYNC_ITER_KEY, 0, iter_self);
    aib.props.borrow_mut().set_enumerable(ASYNC_ITER_KEY, false);
    let iterable: [(&Rc<Obj>, crate::object::NativeFn); 6] = [
        (&vm.realm.array_proto.clone(), array_values),
        (&vm.realm.string_proto.clone(), string_iter),
        (&vm.realm.map_proto.clone(), map_entries),
        (&vm.realm.set_proto.clone(), set_values),
        (&vm.realm.iter_proto.clone(), iter_self),
        (&vm.realm.generator_proto.clone(), iter_self),
    ];
    for (proto, f) in iterable {
        method(vm, proto, ITER_KEY, 0, f);
    }
    for (proto, name) in [
        (vm.realm.array_proto.clone(), "values"),
        (vm.realm.map_proto.clone(), "entries"),
        (vm.realm.set_proto.clone(), "values"),
    ] {
        if let Some(f) = proto.get_own(name) {
            proto.set_own(ITER_KEY, f);
        }
    }

    let gp = vm.realm.generator_proto.clone();
    method(vm, &gp, "next", 1, gen_next);
    method(vm, &gp, "return", 1, gen_return);
    method(vm, &gp, "throw", 1, gen_throw);

    let ap = vm.realm.async_generator_proto.clone();
    method(vm, &ap, "next", 1, async_gen_next);
    method(vm, &ap, "return", 1, async_gen_return);
    method(vm, &ap, "throw", 1, async_gen_throw);
    method(vm, &ap, ASYNC_ITER_KEY, 0, iter_self);
    vm.async_from_sync_next = super::native_fn(vm, "next", 0, async_from_sync_next);
    vm.iter_value_fn = super::native_fn(vm, "", 1, iter_value);
}

fn async_gen_step(vm: &mut Vm, this: Value, value: Value, mode: ResumeMode) -> R {
    match &this {
        Value::Object(o) if matches!(&o.kind, ObjKind::Coroutine(c) if c.is_generator && c.is_async) =>
        {
            let o = o.clone();
            Ok(Value::Object(vm.async_gen_enqueue(&o, value, mode)))
        }
        _ => {
            let p = vm.new_promise();
            let e = vm.type_error("next method called on incompatible receiver");
            vm.reject_promise(&p, e);
            Ok(Value::Object(p))
        }
    }
}

fn async_gen_next(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    async_gen_step(vm, this, arg(args, 0), ResumeMode::Next)
}

fn async_gen_return(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    async_gen_step(vm, this, arg(args, 0), ResumeMode::Return)
}

fn async_gen_throw(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    async_gen_step(vm, this, arg(args, 0), ResumeMode::Throw)
}

fn iter_value(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(iter_result(vm, arg(args, 0), false))
}

fn async_from_sync_next(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match vm.iter_next(&this) {
        Ok(Some(v)) => {
            let p = vm.to_promise(v);
            let f = vm.iter_value_fn.clone();
            Ok(Value::Object(vm.promise_then(&p, f, Value::Undefined)))
        }
        Ok(None) => {
            let p = vm.new_promise();
            let r = iter_result(vm, Value::Undefined, true);
            vm.resolve_promise(&p, r);
            Ok(Value::Object(p))
        }
        Err(e) => {
            let p = vm.new_promise();
            vm.reject_promise(&p, e);
            Ok(Value::Object(p))
        }
    }
}

fn new_symbol(vm: &mut Vm, description: Option<Rc<str>>) -> Value {
    vm.symbol_seq += 1;
    let key = format!(
        "\u{0}@@{}:{}",
        vm.symbol_seq,
        description.as_deref().unwrap_or("")
    );
    Value::Symbol(Rc::new(Symbol {
        description,
        key: key.into(),
    }))
}

fn symbol_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if super::constructing(&this) {
        return Err(vm.type_error("Symbol is not a constructor"));
    }
    let description = match arg(args, 0) {
        Value::Undefined => None,
        v => Some(vm.to_string(&v)?),
    };
    Ok(new_symbol(vm, description))
}

fn symbol_for(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let key = vm.to_string(&arg(args, 0))?;
    if let Some(s) = vm.symbol_registry.get(&key) {
        return Ok(s.clone());
    }
    let s = new_symbol(vm, Some(key.clone()));
    vm.symbol_registry.insert(key, s.clone());
    Ok(s)
}

fn symbol_key_for(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    for (k, v) in &vm.symbol_registry {
        if v.strict_eq(&target) {
            return Ok(Value::Str(k.clone()));
        }
    }
    Ok(Value::Undefined)
}

pub fn symbol_text(s: &Symbol) -> String {
    format!("Symbol({})", s.description.as_deref().unwrap_or(""))
}

fn symbol_to_primitive(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match crate::object::unwrap_primitive(&this)
        .as_ref()
        .unwrap_or(&this)
    {
        Value::Symbol(s) => Ok(Value::Symbol(s.clone())),
        _ => Err(
            vm.type_error("Symbol.prototype[Symbol.toPrimitive] requires that 'this' be a Symbol")
        ),
    }
}

fn symbol_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match crate::object::unwrap_primitive(&this)
        .as_ref()
        .unwrap_or(&this)
    {
        Value::Symbol(s) => Ok(symbol_text(s).into()),
        _ => Err(vm.type_error("Symbol.prototype.toString requires that 'this' be a Symbol")),
    }
}

fn symbol_value_of(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match crate::object::unwrap_primitive(&this).unwrap_or(this) {
        Value::Symbol(s) => Ok(Value::Symbol(s)),
        _ => Err(vm.type_error("Symbol.prototype.valueOf requires that 'this' be a Symbol")),
    }
}

fn array_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) if matches!(o.kind, ObjKind::Array(_)) => {
            Ok(vm.new_iter(IterState::Array(o.clone(), 0, 1)))
        }
        _ => {
            let o = vm.to_object(&this)?;
            Ok(vm.new_iter(IterState::Array(o, 0, 1)))
        }
    }
}

fn string_iter(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let s = vm.to_string(&this)?;
    Ok(vm.new_iter(IterState::Str(s, 0)))
}

fn map_entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) if matches!(o.kind, ObjKind::Map(_)) => {
            Ok(vm.new_iter(IterState::MapEntries(o.clone(), 0)))
        }
        _ => Err(vm.type_error("Map.prototype[Symbol.iterator] called on incompatible receiver")),
    }
}

fn set_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) if matches!(o.kind, ObjKind::Set(_)) => {
            Ok(vm.new_iter(IterState::SetValues(o.clone(), 0)))
        }
        _ => Err(vm.type_error("Set.prototype[Symbol.iterator] called on incompatible receiver")),
    }
}

fn iter_self(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this)
}

pub fn iter_result(vm: &mut Vm, value: Value, done: bool) -> Value {
    let o = vm.new_object();
    o.set_own("value", value);
    o.set_own("done", Value::Bool(done));
    Value::Object(o)
}

fn this_generator(vm: &mut Vm, this: &Value) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(&o.kind, ObjKind::Coroutine(c) if c.is_generator) => {
            Ok(o.clone())
        }
        _ => Err(vm.type_error(format!(
            "next method called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn gen_step(vm: &mut Vm, this: Value, value: Value, mode: ResumeMode) -> R {
    let g = this_generator(vm, &this)?;
    match vm.resume(&g, value, mode)? {
        Step::Yield(v) => Ok(iter_result(vm, v, false)),
        Step::YieldRaw(v) => Ok(v),
        Step::Done(v) | Step::Await(v) => Ok(iter_result(vm, v, true)),
    }
}

fn gen_next(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    gen_step(vm, this, arg(args, 0), ResumeMode::Next)
}

fn gen_return(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    gen_step(vm, this, arg(args, 0), ResumeMode::Return)
}

fn gen_throw(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    gen_step(vm, this, arg(args, 0), ResumeMode::Throw)
}
