use super::{arg, make_ctor, method};
use crate::inspect::error_header;
use crate::object::{Bound, Obj, ObjKind};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let op = vm.realm.object_proto.clone();
    method(vm, &op, "hasOwnProperty", 1, has_own_property);
    method(vm, &op, "toString", 0, object_to_string);
    method(vm, &op, "toLocaleString", 0, object_to_string);
    method(vm, &op, "valueOf", 0, value_of);
    method(vm, &op, "isPrototypeOf", 1, is_prototype_of);
    let object = make_ctor(vm, "Object", 1, object_ctor, None, &op);
    method(vm, &object, "keys", 1, keys);
    method(vm, &object, "values", 1, values);
    method(vm, &object, "entries", 1, entries);
    method(vm, &object, "assign", 2, assign);
    method(vm, &object, "fromEntries", 1, from_entries);
    method(vm, &object, "freeze", 1, freeze);
    method(vm, &object, "isFrozen", 1, is_frozen);
    method(vm, &object, "create", 2, create);
    method(vm, &object, "getPrototypeOf", 1, get_prototype_of);
    method(vm, &object, "setPrototypeOf", 2, set_prototype_of);
    method(vm, &object, "hasOwn", 2, has_own);
    method(vm, &object, "getOwnPropertyNames", 1, keys);
    method(vm, &object, "defineProperty", 3, define_property);
    method(
        vm,
        &object,
        "getOwnPropertyDescriptor",
        2,
        get_own_property_descriptor,
    );
    method(vm, &object, "is", 2, object_is);
    vm.define_global("Object", Value::Object(object));

    let fp = vm.realm.function_proto.clone();
    method(vm, &fp, "call", 1, fn_call);
    method(vm, &fp, "apply", 2, fn_apply);
    method(vm, &fp, "bind", 1, fn_bind);
    method(vm, &fp, "toString", 0, fn_to_string);
    let function = make_ctor(vm, "Function", 1, function_ctor, None, &fp);
    vm.define_global("Function", Value::Object(function));

    let ctors: [crate::object::NativeFn; 5] = [
        error_ctor::<0>,
        error_ctor::<1>,
        error_ctor::<2>,
        error_ctor::<3>,
        error_ctor::<4>,
    ];
    let mut base_ctor = None;
    for (i, kind) in ErrorKind::ALL.iter().enumerate() {
        let proto = vm.realm.error_protos[i].clone();
        proto.set_own("name", kind.name().into());
        proto.set_own("message", "".into());
        if i == 0 {
            method(vm, &proto, "toString", 0, error_to_string);
        }
        let c = make_ctor(
            vm,
            kind.name(),
            1,
            ctors[i],
            Some(|| ObjKind::Error),
            &proto,
        );
        if let Some(base) = &base_ctor {
            *c.proto.borrow_mut() = Some(Rc::clone(base));
        } else {
            base_ctor = Some(c.clone());
        }
        vm.define_global(kind.name(), Value::Object(c));
    }

    let bp = vm.realm.boolean_proto.clone();
    method(vm, &bp, "toString", 0, bool_to_string);
    method(vm, &bp, "valueOf", 0, bool_value_of);
    let boolean = make_ctor(vm, "Boolean", 1, boolean_ctor, None, &bp);
    vm.define_global("Boolean", Value::Object(boolean));
}

fn to_object(vm: &mut Vm, v: &Value) -> Result<Rc<Obj>, Value> {
    match v {
        Value::Object(o) => Ok(o.clone()),
        Value::Undefined | Value::Null => {
            Err(vm.type_error("Cannot convert undefined or null to object"))
        }
        _ => Ok(vm.new_object()),
    }
}

fn has_own_property(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let k = vm.to_property_key(&arg(args, 0))?;
    has_own_key(vm, &this, &k)
}

fn has_own_key(vm: &mut Vm, target: &Value, k: &str) -> R {
    Ok(Value::Bool(match target {
        Value::Object(o) => match &o.kind {
            ObjKind::Array(a) => match crate::object::array_index(k) {
                Some(i) => i < a.borrow().len(),
                None => k == "length" || o.has_own(k),
            },
            _ => o.has_own(k),
        },
        Value::Str(s) => {
            crate::object::array_index(k).is_some_and(|i| i < crate::value::len16(s))
                || k == "length"
        }
        Value::Undefined | Value::Null => {
            return Err(vm.type_error("Cannot convert undefined or null to object"));
        }
        _ => false,
    }))
}

fn has_own(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let k = vm.to_property_key(&arg(args, 1))?;
    has_own_key(vm, &arg(args, 0), &k)
}

fn object_to_string(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let tag = match &this {
        Value::Undefined => "Undefined",
        Value::Null => "Null",
        Value::Bool(_) => "Boolean",
        Value::Number(_) => "Number",
        Value::Str(_) => "String",
        Value::Object(o) => match &o.kind {
            ObjKind::Array(_) => "Array",
            ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_) => "Function",
            ObjKind::Error => "Error",
            ObjKind::Map(_) => "Map",
            ObjKind::Set(_) => "Set",
            ObjKind::Iter(_) => "Iterator",
            ObjKind::Promise(_) => "Promise",
            ObjKind::RegExp(_) => "RegExp",
            ObjKind::Date(_) => "Date",
            ObjKind::WeakMap(_) => "WeakMap",
            ObjKind::WeakSet(_) => "WeakSet",
            ObjKind::WeakRef(_) => "WeakRef",
            ObjKind::Coroutine(_) => "Generator",
            ObjKind::Accessor(_) | ObjKind::Ordinary => "Object",
        },
        Value::Symbol(_) => "Symbol",
    };
    Ok(format!("[object {tag}]").into())
}

fn value_of(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this)
}

fn is_prototype_of(_vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (Value::Object(p), Value::Object(o)) = (&this, &arg(args, 0)) else {
        return Ok(Value::Bool(false));
    };
    let mut cur = o.proto.borrow().clone();
    while let Some(c) = cur {
        if Rc::ptr_eq(&c, p) {
            return Ok(Value::Bool(true));
        }
        cur = c.proto.borrow().clone();
    }
    Ok(Value::Bool(false))
}

fn object_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    match arg(args, 0) {
        Value::Object(o) => Ok(Value::Object(o)),
        _ => Ok(Value::Object(vm.new_object())),
    }
}

fn keys(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if v.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    let k = vm.own_keys(&v);
    Ok(vm.new_array(k))
}

fn values(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if v.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    let vals = vm
        .own_entries_resolved(&v, false)?
        .into_iter()
        .map(|(_, v)| v)
        .collect();
    Ok(vm.new_array(vals))
}

fn entries(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if v.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    let pairs: Vec<Value> = vm
        .own_entries_resolved(&v, false)?
        .into_iter()
        .map(|(k, v)| vm.new_array(vec![Value::Str(k), v]))
        .collect();
    Ok(vm.new_array(pairs))
}

fn assign(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    to_object(vm, &target)?;
    for src in args.iter().skip(1) {
        for (k, v) in vm.own_entries_resolved(src, true)? {
            vm.set_prop(&target, k, v)?;
        }
    }
    Ok(target)
}

fn from_entries(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let items = vm.iterate(&arg(args, 0))?;
    let o = vm.new_object();
    for item in items {
        let k = vm.get_prop(&item, "0")?;
        let v = vm.get_prop(&item, "1")?;
        let k = vm.to_property_key(&k)?;
        o.set_own(k, v);
    }
    Ok(Value::Object(o))
}

fn freeze(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if let Value::Object(o) = &v {
        o.frozen.set(true);
    }
    Ok(v)
}

fn is_frozen(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(match arg(args, 0) {
        Value::Object(o) => o.frozen.get(),
        _ => true,
    }))
}

fn create(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let proto = match arg(args, 0) {
        Value::Object(p) => Some(p),
        Value::Null => None,
        other => {
            return Err(vm.type_error(format!(
                "Object prototype may only be an Object or null: {}",
                crate::inspect::inspect_plain(&other)
            )));
        }
    };
    let o = Obj::new(ObjKind::Ordinary, proto);
    if let Value::Object(_) = arg(args, 1) {
        for (k, d) in vm.own_entries(&arg(args, 1)) {
            let v = vm.get_prop(&d, "value")?;
            o.set_own(k, v);
        }
    }
    Ok(Value::Object(o))
}

fn get_prototype_of(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(match arg(args, 0) {
        Value::Object(o) => o
            .proto
            .borrow()
            .clone()
            .map(Value::Object)
            .unwrap_or(Value::Null),
        Value::Str(_) => Value::Object(vm.realm.string_proto.clone()),
        Value::Number(_) => Value::Object(vm.realm.number_proto.clone()),
        Value::Bool(_) => Value::Object(vm.realm.boolean_proto.clone()),
        Value::Symbol(_) => Value::Object(vm.realm.symbol_proto.clone()),
        _ => return Err(vm.type_error("Cannot convert undefined or null to object")),
    })
}

fn set_prototype_of(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    if let Value::Object(o) = &target {
        match arg(args, 1) {
            Value::Object(p) => *o.proto.borrow_mut() = Some(p),
            Value::Null => *o.proto.borrow_mut() = None,
            _ => return Err(vm.type_error("Object prototype may only be an Object or null")),
        }
    }
    Ok(target)
}

fn define_property(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    if !target.is_object() {
        return Err(vm.type_error("Object.defineProperty called on non-object"));
    }
    let k = vm.to_property_key(&arg(args, 1))?;
    let desc = arg(args, 2);
    if desc.is_object() {
        let getter = vm.get_prop(&desc, "get")?;
        let setter = vm.get_prop(&desc, "set")?;
        if getter.is_callable() || setter.is_callable() {
            if let Value::Object(o) = &target {
                if getter.is_callable() {
                    vm.define_accessor(o, k.clone(), getter, false);
                }
                if setter.is_callable() {
                    vm.define_accessor(o, k, setter, true);
                }
            }
            return Ok(target);
        }
        let v = vm.get_prop(&desc, "value")?;
        if let Value::Object(o) = &target {
            o.props.borrow_mut().insert(k, v);
        }
    }
    Ok(target)
}

fn get_own_property_descriptor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    let k = vm.to_property_key(&arg(args, 1))?;
    let Value::Object(o) = &target else {
        return Ok(Value::Undefined);
    };
    let raw = match &o.kind {
        ObjKind::Array(a) => match crate::object::array_index(&k) {
            Some(i) => a.borrow().get(i).cloned(),
            None if &*k == "length" => Some(Value::Number(a.borrow().len() as f64)),
            None => o.get_own(&k),
        },
        _ => o.get_own(&k),
    };
    let Some(raw) = raw else {
        return Ok(Value::Undefined);
    };
    let d = vm.new_object();
    match &raw {
        Value::Object(a) if a.is_accessor() => {
            if let ObjKind::Accessor(acc) = &a.kind {
                d.set_own("get", acc.get.borrow().clone());
                d.set_own("set", acc.set.borrow().clone());
            }
        }
        _ => {
            d.set_own("value", raw.clone());
            d.set_own("writable", Value::Bool(!o.frozen.get()));
        }
    }
    d.set_own("enumerable", Value::Bool(true));
    d.set_own("configurable", Value::Bool(!o.frozen.get()));
    Ok(Value::Object(d))
}

fn object_is(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let a = arg(args, 0);
    let b = arg(args, 1);
    Ok(Value::Bool(match (&a, &b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.is_nan() && y.is_nan()) || (x == y && x.is_sign_negative() == y.is_sign_negative())
        }
        _ => a.strict_eq(&b),
    }))
}

fn fn_call(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };
    vm.call_value(&this, arg(args, 0), rest)
}

fn fn_apply(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let list = arg(args, 1);
    let items = if list.is_nullish() {
        Vec::new()
    } else {
        vm.iterate(&list)?
    };
    vm.call_value(&this, arg(args, 0), &items)
}

fn fn_bind(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !this.is_callable() {
        return Err(vm.type_error("Bind must be called on a function"));
    }
    let bound = Bound {
        target: this,
        this: arg(args, 0),
        args: args.iter().skip(1).cloned().collect(),
    };
    Ok(Value::Object(Obj::new(
        ObjKind::Bound(Box::new(bound)),
        Some(vm.realm.function_proto.clone()),
    )))
}

fn fn_to_string(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let Value::Object(o) = &this else {
        return Ok("".into());
    };
    let name = o.function_name().unwrap_or_else(|| "".into());
    Ok(match &o.kind {
        ObjKind::Function(c) if c.proto.is_class() => format!("class {name} {{ }}"),
        _ => format!("function {name}() {{ [native code] }}"),
    }
    .into())
}

fn function_ctor(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Err(vm.make_error(
        ErrorKind::Error,
        "The Function constructor is not supported",
    ))
}

fn error_ctor<const K: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let kind = ErrorKind::ALL[K];
    let msg = arg(args, 0);
    let msg = if msg.is_undefined() {
        None
    } else {
        Some(vm.to_string(&msg)?)
    };
    let target = match &this {
        Value::Object(o) if matches!(o.kind, ObjKind::Error) => o.clone(),
        _ => {
            let v = vm.make_error(kind, "");
            let Value::Object(o) = v else { unreachable!() };
            o.props.borrow_mut().shift_remove("message");
            o
        }
    };
    if let Some(m) = msg {
        target.set_own("message", Value::Str(m));
    }
    let options = arg(args, 1);
    if options.is_object() {
        let cause = vm.get_prop(&options, "cause")?;
        if !cause.is_undefined() {
            target.set_own("cause", cause);
        }
    }
    let stack = format!("{}{}", error_header(&target), vm.stack_trace());
    target.set_own("stack", stack.into());
    Ok(Value::Object(target))
}

fn error_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) => Ok(error_header(o).into()),
        _ => Err(vm.type_error("Error.prototype.toString called on non-object")),
    }
}

fn boolean_ctor(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(arg(args, 0).truthy()))
}

fn bool_to_string(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(if this.truthy() { "true" } else { "false" }.into())
}

fn bool_value_of(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(Value::Bool(this.truthy()))
}
