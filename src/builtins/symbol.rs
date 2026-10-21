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

pub fn install(vm: &mut Vm) {
    let sp = vm.realm.symbol_proto.clone();
    method(vm, &sp, "toString", 0, symbol_to_string);
    method(vm, &sp, "valueOf", 0, symbol_value_of);
    let symbol = make_ctor(vm, "Symbol", 0, symbol_ctor, None, &sp);
    method(vm, &symbol, "for", 1, symbol_for);
    method(vm, &symbol, "keyFor", 1, symbol_key_for);
    vm.symbol_iterator = well_known("Symbol.iterator", ITER_KEY);
    vm.symbol_async_iterator = well_known("Symbol.asyncIterator", ASYNC_ITER_KEY);
    symbol.set_own("iterator", vm.symbol_iterator.clone());
    symbol.set_own("asyncIterator", vm.symbol_async_iterator.clone());
    vm.define_global("Symbol", Value::Object(symbol));

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

    let gp = vm.realm.generator_proto.clone();
    method(vm, &gp, "next", 1, gen_next);
    method(vm, &gp, "return", 1, gen_return);
    method(vm, &gp, "throw", 1, gen_throw);
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
    if this.is_object() {
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

fn symbol_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Symbol(s) => Ok(symbol_text(s).into()),
        _ => Err(vm.type_error("Symbol.prototype.toString requires that 'this' be a Symbol")),
    }
}

fn symbol_value_of(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this)
}

fn array_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) if matches!(o.kind, ObjKind::Array(_)) => {
            Ok(vm.new_iter(IterState::Array(o.clone(), 0)))
        }
        _ => Err(vm.type_error("Array.prototype[Symbol.iterator] called on non-array")),
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
