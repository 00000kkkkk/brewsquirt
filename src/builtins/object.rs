use super::{arg, make_ctor, method, native_fn};
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
    method(vm, &op, "toLocaleString", 0, object_to_locale_string);
    method(vm, &op, "valueOf", 0, value_of);
    method(vm, &op, "isPrototypeOf", 1, is_prototype_of);
    method(vm, &op, "propertyIsEnumerable", 1, property_is_enumerable);
    let object = make_ctor(vm, "Object", 1, object_ctor, None, &op);
    method(vm, &object, "keys", 1, keys);
    method(vm, &object, "values", 1, values);
    method(vm, &object, "entries", 1, entries);
    method(vm, &object, "assign", 2, assign);
    method(vm, &object, "fromEntries", 1, from_entries);
    method(vm, &object, "freeze", 1, freeze);
    method(vm, &object, "isFrozen", 1, is_frozen);
    method(vm, &object, "seal", 1, seal);
    method(vm, &object, "isSealed", 1, is_sealed);
    method(vm, &object, "preventExtensions", 1, prevent_extensions);
    method(vm, &object, "isExtensible", 1, is_extensible);
    method(
        vm,
        &object,
        "getOwnPropertySymbols",
        1,
        get_own_property_symbols,
    );
    method(
        vm,
        &object,
        "getOwnPropertyDescriptors",
        1,
        get_own_property_descriptors,
    );
    method(vm, &object, "create", 2, create);
    method(vm, &object, "getPrototypeOf", 1, get_prototype_of);
    method(vm, &object, "setPrototypeOf", 2, set_prototype_of);
    method(vm, &object, "hasOwn", 2, has_own);
    method(vm, &object, "getOwnPropertyNames", 1, own_property_names);
    method(vm, &object, "defineProperty", 3, define_property);
    method(vm, &object, "defineProperties", 2, define_properties);
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
    vm.define_global("Function", Value::Object(function.clone()));
    let kinds: [(&str, NativeFnPtr, Rc<Obj>, Rc<Obj>); 3] = [
        (
            "GeneratorFunction",
            generator_function_ctor,
            vm.realm.generator_function_proto.clone(),
            vm.realm.generator_proto.clone(),
        ),
        (
            "AsyncGeneratorFunction",
            async_generator_function_ctor,
            vm.realm.async_generator_function_proto.clone(),
            vm.realm.async_generator_proto.clone(),
        ),
        (
            "AsyncFunction",
            async_function_ctor,
            vm.realm.async_function_proto.clone(),
            vm.realm.async_function_proto.clone(),
        ),
    ];
    for (name, f, fproto, instance_proto) in kinds {
        let c = make_ctor(vm, name, 1, f, None, &fproto);
        *c.proto.borrow_mut() = Some(function.clone());
        let ro = crate::object::NON_ENUM | crate::object::READ_ONLY;
        fproto.set_with_attrs("constructor", Value::Object(c.clone()), ro);
        fproto.set_with_attrs(crate::object::TO_STRING_TAG_KEY, name.into(), ro);
        if !Rc::ptr_eq(&fproto, &instance_proto) {
            fproto.set_with_attrs("prototype", Value::Object(instance_proto.clone()), ro);
            instance_proto.set_with_attrs("constructor", Value::Object(fproto.clone()), ro);
        }
    }
    let eval = native_fn(vm, "eval", 1, global_eval);
    vm.eval_fn = eval.as_object().cloned();
    vm.define_global("eval", eval);
    let script = native_fn(vm, "evalScript", 1, eval_script);
    vm.define_global("%evalScript%", script);

    let ctors: [crate::object::NativeFn; 7] = [
        error_ctor::<0>,
        error_ctor::<1>,
        error_ctor::<2>,
        error_ctor::<3>,
        error_ctor::<4>,
        error_ctor::<5>,
        error_ctor::<6>,
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
            method(vm, &c, "captureStackTrace", 1, capture_stack_trace);
            c.set_own("stackTraceLimit", Value::Number(10.0));
        }
        vm.define_global(kind.name(), Value::Object(c));
    }
    let agg_proto = Obj::new(ObjKind::Ordinary, Some(vm.realm.error_protos[0].clone()));
    agg_proto.set_hidden("name", "AggregateError".into());
    agg_proto.set_hidden("message", "".into());
    let agg = make_ctor(
        vm,
        "AggregateError",
        2,
        aggregate_error_ctor,
        None,
        &agg_proto,
    );
    if let Some(base) = &base_ctor {
        *agg.proto.borrow_mut() = Some(Rc::clone(base));
    }
    vm.define_global("AggregateError", Value::Object(agg));

    let bp = vm.realm.boolean_proto.clone();
    method(vm, &bp, "toString", 0, bool_to_string);
    method(vm, &bp, "valueOf", 0, bool_value_of);
    let boolean = make_ctor(
        vm,
        "Boolean",
        1,
        boolean_ctor,
        Some(crate::object::wrapper_marker),
        &bp,
    );
    vm.define_global("Boolean", Value::Object(boolean));
}

fn to_object(vm: &mut Vm, v: &Value) -> Result<Rc<Obj>, Value> {
    vm.to_object(v)
}

fn property_is_enumerable(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let k = vm.to_property_key(&arg(args, 0))?;
    if let Value::Object(o) = &this {
        if crate::builtins::reflect::is_proxy(o) {
            let d = vm.op_get_own_property(o, &k)?;
            return Ok(Value::Bool(d.is_some_and(|d| d.enumerable == Some(true))));
        }
    }
    let own = has_own_key(vm, &this, &k)?.truthy();
    Ok(Value::Bool(
        own && match &this {
            Value::Object(o) => match &o.kind {
                ObjKind::Array(_) if &*k == "length" => false,
                _ => o.props.borrow().is_enumerable(&k),
            },
            _ => true,
        },
    ))
}

fn own_property_names(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if v.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    if let Value::Object(o) = &v {
        if crate::builtins::reflect::is_proxy(o) || matches!(o.kind, ObjKind::Global) {
            let keys = vm.op_own_keys(o)?;
            let list = keys
                .into_iter()
                .filter(|k| !crate::object::is_symbol_key(k))
                .map(Value::Str)
                .collect();
            return Ok(vm.new_array(list));
        }
    }
    let k = vm.own_property_names(&v);
    Ok(vm.new_array(k))
}

fn has_own_property(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let k = vm.to_property_key(&arg(args, 0))?;
    has_own_key(vm, &this, &k)
}

fn has_own_key(vm: &mut Vm, target: &Value, k: &str) -> R {
    Ok(Value::Bool(match target {
        Value::Object(o) if crate::builtins::reflect::is_proxy(o) => {
            vm.op_get_own_property(o, k)?.is_some()
        }
        Value::Object(o) => vm.own_property(o, k).is_some(),
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

fn object_to_locale_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    if this.is_nullish() {
        return Err(vm.type_error("Object.prototype.toLocaleString called on null or undefined"));
    }
    let f = vm.get_prop(&this, "toString")?;
    if !f.is_callable() {
        return Err(vm.type_error(format!(
            "{} is not a function",
            crate::inspect::inspect_plain(&f)
        )));
    }
    vm.call_value(&f, this, &[])
}

fn object_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let builtin = match &this {
        Value::Undefined | Value::Uninit => return Ok("[object Undefined]".into()),
        Value::Null => return Ok("[object Null]".into()),
        Value::Bool(_) => "Boolean",
        Value::Number(_) => "Number",
        Value::Str(_) => "String",
        Value::Symbol(_) => "Symbol",
        Value::BigInt(_) => "BigInt",
        Value::Object(o) => match &o.kind {
            ObjKind::Array(_) => "Array",
            ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_) => "Function",
            ObjKind::Error => "Error",
            ObjKind::RegExp(_) => "RegExp",
            ObjKind::Date(_) => "Date",
            ObjKind::Typed(t) => t.kind.name(),
            ObjKind::Primitive(Value::Str(_)) => "String",
            ObjKind::Primitive(Value::Number(_)) => "Number",
            ObjKind::Primitive(Value::Bool(_)) => "Boolean",
            ObjKind::Ordinary if o.flags.get() & crate::object::ARGUMENTS != 0 => "Arguments",
            ObjKind::Proxy(p) => {
                let t = p.target.borrow().clone();
                match t {
                    Some(t) if matches!(t.kind, ObjKind::Array(_)) => "Array",
                    _ if p.callable => "Function",
                    _ => "Object",
                }
            }
            _ => "Object",
        },
    };
    let tag = match &this {
        Value::Object(_)
        | Value::Symbol(_)
        | Value::Str(_)
        | Value::Number(_)
        | Value::Bool(_)
        | Value::BigInt(_) => vm.get_prop(&this, crate::object::TO_STRING_TAG_KEY)?,
        _ => Value::Undefined,
    };
    let tag = match &tag {
        Value::Str(s) => s.to_string(),
        _ => builtin.to_string(),
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
        Value::Undefined | Value::Null => Ok(Value::Object(vm.new_object())),
        v => Ok(Value::Object(vm.wrap_primitive(v))),
    }
}

fn keys(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if v.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    if let Value::Object(o) = &v {
        if crate::builtins::reflect::is_proxy(o) {
            let keys = vm.op_own_enumerable_keys(o)?;
            let list = keys.into_iter().map(Value::Str).collect();
            return Ok(vm.new_array(list));
        }
    }
    let k = vm.own_keys(&v);
    Ok(vm.new_array(k))
}

fn proxy_entries(vm: &mut Vm, o: &Rc<Obj>) -> Result<Vec<(Rc<str>, Value)>, Value> {
    let keys = vm.op_own_enumerable_keys(o)?;
    let receiver = Value::Object(o.clone());
    let mut out = Vec::with_capacity(keys.len());
    for k in keys {
        let v = vm.op_get(o, &k, &receiver)?;
        out.push((k, v));
    }
    Ok(out)
}

fn entries_of(vm: &mut Vm, v: &Value, symbols: bool) -> Result<Vec<(Rc<str>, Value)>, Value> {
    if let Value::Object(o) = v {
        if crate::builtins::reflect::is_proxy(o) {
            return proxy_entries(vm, o);
        }
    }
    vm.own_entries_resolved(v, symbols)
}

fn values(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if v.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    let vals = entries_of(vm, &v, false)?
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
    let pairs: Vec<Value> = entries_of(vm, &v, false)?
        .into_iter()
        .map(|(k, v)| vm.new_array(vec![Value::Str(k), v]))
        .collect();
    Ok(vm.new_array(pairs))
}

fn assign(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    to_object(vm, &target)?;
    for src in args.iter().skip(1) {
        for (k, v) in entries_of(vm, src, true)? {
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

fn freeze(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if let Value::Object(o) = &v {
        if crate::builtins::reflect::is_proxy(o) {
            integrity_via_ops(vm, o, true)?;
        } else {
            vm.freeze(o)?;
        }
    }
    Ok(v)
}

fn integrity_via_ops(vm: &mut Vm, o: &Rc<Obj>, frozen: bool) -> Result<(), Value> {
    if !vm.op_prevent_extensions(o)? {
        return Err(vm.type_error("Cannot prevent extensions"));
    }
    for k in vm.op_own_keys(o)? {
        let mut d = crate::props::PropDesc {
            configurable: Some(false),
            ..Default::default()
        };
        if frozen {
            if let Some(cur) = vm.op_get_own_property(o, &k)? {
                if !cur.is_accessor() {
                    d.writable = Some(false);
                }
            }
        }
        vm.op_define_own_property(o, k, d, true)?;
    }
    Ok(())
}

fn seal(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if let Value::Object(o) = &v {
        if crate::builtins::reflect::is_proxy(o) {
            integrity_via_ops(vm, o, false)?;
        } else {
            vm.seal(o);
        }
    }
    Ok(v)
}

fn prevent_extensions(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let v = arg(args, 0);
    if let Value::Object(o) = &v {
        if !vm.op_prevent_extensions(o)? {
            return Err(vm.type_error("Cannot prevent extensions"));
        }
    }
    Ok(v)
}

fn is_sealed(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(match arg(args, 0) {
        Value::Object(o) => vm.test_integrity(&o, false),
        _ => true,
    }))
}

fn is_extensible(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(match arg(args, 0) {
        Value::Object(o) => vm.op_is_extensible(&o)?,
        _ => false,
    }))
}

fn is_frozen(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(match arg(args, 0) {
        Value::Object(o) => vm.test_integrity(&o, true),
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
    let props = arg(args, 1);
    if !props.is_undefined() {
        define_all(vm, &o, &props)?;
    }
    Ok(Value::Object(o))
}

fn define_all(vm: &mut Vm, o: &Rc<Obj>, props: &Value) -> Result<(), Value> {
    if props.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    let entries = vm.own_entries_resolved(props, true)?;
    let mut descs = Vec::with_capacity(entries.len());
    for (k, d) in entries {
        descs.push((k, vm.to_prop_desc(&d)?));
    }
    for (k, d) in descs {
        vm.op_define_own_property(o, k, d, true)?;
    }
    Ok(())
}

fn get_prototype_of(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(match arg(args, 0) {
        Value::Object(o) => vm
            .op_get_prototype_of(&o)?
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
    if target.is_nullish() {
        return Err(vm.type_error("Object.setPrototypeOf called on null or undefined"));
    }
    let proto = match arg(args, 1) {
        Value::Object(p) => Some(p),
        Value::Null => None,
        other => {
            return Err(vm.type_error(format!(
                "Object prototype may only be an Object or null: {}",
                crate::inspect::inspect_plain(&other)
            )));
        }
    };
    if let Value::Object(o) = &target {
        if !vm.op_set_prototype_of(o, proto)? {
            return Err(vm.type_error("Cyclic __proto__ value"));
        }
    }
    Ok(target)
}

fn define_one(vm: &mut Vm, target: &Value, k: Rc<str>, desc: &Value) -> Result<(), Value> {
    let Value::Object(o) = target else {
        return Ok(());
    };
    let d = vm.to_prop_desc(desc)?;
    vm.op_define_own_property(o, k, d, true)?;
    Ok(())
}

fn define_property(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    if !target.is_object() {
        return Err(vm.type_error("Object.defineProperty called on non-object"));
    }
    let k = vm.to_property_key(&arg(args, 1))?;
    define_one(vm, &target, k, &arg(args, 2))?;
    Ok(target)
}

fn define_properties(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    let Value::Object(o) = &target else {
        return Err(vm.type_error("Object.defineProperties called on non-object"));
    };
    define_all(vm, o, &arg(args, 1))?;
    Ok(target)
}

fn get_own_property_descriptor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    if target.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    let k = vm.to_property_key(&arg(args, 1))?;
    let o = match &target {
        Value::Object(o) => o.clone(),
        Value::Str(s) => {
            if &*k == "length" {
                let d = crate::props::PropDesc::data(
                    Value::Number(crate::value::len16(s) as f64),
                    crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
                );
                return Ok(vm.from_prop_desc(&d));
            }
            if let Some(i) = crate::object::array_index(&k) {
                if let Some(c) = crate::value::char_at16(s, i) {
                    let d = crate::props::PropDesc::data(
                        Value::from(c),
                        crate::object::READ_ONLY | crate::object::NON_CONFIG,
                    );
                    return Ok(vm.from_prop_desc(&d));
                }
            }
            return Ok(Value::Undefined);
        }
        _ => return Ok(Value::Undefined),
    };
    match vm.op_get_own_property(&o, &k)? {
        Some(d) => Ok(vm.from_prop_desc(&d)),
        None => Ok(Value::Undefined),
    }
}

fn get_own_property_descriptors(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    let Value::Object(o) = &target else {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    };
    let out = vm.new_object();
    let mut keys = vm.own_property_names(&target);
    keys.extend(own_symbols(o));
    for k in keys {
        let k = vm.to_property_key(&k)?;
        if let Some(d) = vm.own_descriptor(o, &k) {
            let v = vm.from_prop_desc(&d);
            out.set_own(k, v);
        }
    }
    Ok(Value::Object(out))
}

fn own_symbols(o: &Obj) -> Vec<Value> {
    o.props
        .borrow()
        .keys()
        .filter(|k| crate::object::is_symbol_key(k) && !crate::object::is_private_key(k))
        .map(|k| crate::builtins::symbol_from_key(k))
        .collect()
}

fn get_own_property_symbols(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let list = match arg(args, 0) {
        Value::Object(o) if crate::builtins::reflect::is_proxy(&o) => vm
            .op_own_keys(&o)?
            .into_iter()
            .filter(|k| crate::object::is_symbol_key(k))
            .map(|k| crate::builtins::symbol_from_key(&k))
            .collect(),
        Value::Object(o) => own_symbols(&o),
        Value::Undefined | Value::Null => {
            return Err(vm.type_error("Cannot convert undefined or null to object"));
        }
        _ => Vec::new(),
    };
    Ok(vm.new_array(list))
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

fn fn_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let Value::Object(o) = &this else {
        return Err(vm.type_error("Function.prototype.toString requires that 'this' be a Function"));
    };
    if !o.is_callable() {
        return Err(vm.type_error("Function.prototype.toString requires that 'this' be a Function"));
    }
    let name = o.function_name().unwrap_or_else(|| "".into());
    if let ObjKind::Function(c) = &o.kind {
        let (a, b) = c.proto.span;
        let src = &c.proto.module.source;
        if b > a && (b as usize) <= src.len() {
            if let Some(text) = src.get(a as usize..b as usize) {
                return Ok(text.into());
            }
        }
        if c.proto.is_class() {
            return Ok(format!("class {name} {{ }}").into());
        }
    }
    Ok(format!("function {name}() {{ [native code] }}").into())
}

type NativeFnPtr = crate::object::NativeFn;

fn generator_function_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    dynamic_function(vm, args, "function*")
}

fn async_generator_function_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    dynamic_function(vm, args, "async function*")
}

fn async_function_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    dynamic_function(vm, args, "async function")
}

fn function_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    dynamic_function(vm, args, "function")
}

fn dynamic_function(vm: &mut Vm, args: &[Value], kind: &str) -> R {
    let mut params = Vec::new();
    for a in args.iter().take(args.len().saturating_sub(1)) {
        params.push(vm.to_string(a)?.to_string());
    }
    let body = match args.last() {
        Some(b) => vm.to_string(b)?.to_string(),
        None => String::new(),
    };
    let src = format!(
        "({kind} anonymous({}
) {{
{}
}})",
        params.join(","),
        body
    );
    let dir = std::env::current_dir().unwrap_or_default();
    let trimmed = body.trim_start();
    let sloppy = !(trimmed.starts_with("\"use strict\"") || trimmed.starts_with("'use strict'"));
    let proto = match vm.compile_script(&src, "<anonymous>", dir, sloppy) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<anonymous>")),
    };
    vm.run_proto(proto)
}

fn eval_script(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let src = vm.to_string(&arg(args, 0))?;
    vm.run_script(&src, "<script>")
}

fn global_eval(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let Value::Str(code) = arg(args, 0) else {
        return Ok(arg(args, 0));
    };
    vm.indirect_eval(&code)
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
        target.set_hidden("message", Value::Str(m));
    }
    let options = arg(args, 1);
    install_cause(vm, &target, &options)?;
    let stack = format!("{}{}", error_header(&target), vm.stack_trace());
    target.set_hidden("stack", stack.into());
    Ok(Value::Object(target))
}

fn install_cause(vm: &mut Vm, target: &Rc<Obj>, options: &Value) -> Result<(), Value> {
    if options.is_object() && vm.has_property(options, &Value::Str("cause".into()))? {
        let cause = vm.get_prop(options, "cause")?;
        target.set_hidden("cause", cause);
    }
    Ok(())
}

fn aggregate_error_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let proto = match &this {
        Value::Object(o) => o.proto.borrow().clone(),
        _ => vm
            .globals
            .get("AggregateError")
            .cloned()
            .and_then(|c| c.as_object().and_then(|c| c.get_own("prototype")))
            .and_then(|p| p.as_object().cloned()),
    };
    let target = Obj::new(ObjKind::Error, proto);
    let msg = arg(args, 1);
    if !msg.is_undefined() {
        let m = vm.to_string(&msg)?;
        target.set_hidden("message", Value::Str(m));
    }
    install_cause(vm, &target, &arg(args, 2))?;
    let errors = vm.iterate(&arg(args, 0))?;
    let list = vm.new_array(errors);
    target.set_hidden("errors", list);
    let stack = format!("{}{}", error_header(&target), vm.stack_trace());
    target.set_hidden("stack", stack.into());
    Ok(Value::Object(target))
}

fn capture_stack_trace(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let Value::Object(o) = arg(args, 0) else {
        return Err(vm.type_error("Invalid argument"));
    };
    let header = match o.kind {
        ObjKind::Error => error_header(&o),
        _ => {
            let name = vm.get_prop(&Value::Object(o.clone()), "name")?;
            let msg = vm.get_prop(&Value::Object(o.clone()), "message")?;
            let name = if name.is_undefined() {
                "Error".into()
            } else {
                vm.to_string(&name)?
            };
            let msg = if msg.is_undefined() {
                "".into()
            } else {
                vm.to_string(&msg)?
            };
            if msg.is_empty() {
                name.to_string()
            } else {
                format!("{name}: {msg}")
            }
        }
    };
    let stack = format!("{header}{}", vm.stack_trace());
    o.set_hidden("stack", stack.into());
    Ok(Value::Undefined)
}

fn error_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) => Ok(error_header(o).into()),
        _ => Err(vm.type_error("Error.prototype.toString called on non-object")),
    }
}

fn boolean_ctor(_vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let b = Value::Bool(arg(args, 0).truthy());
    Ok(super::box_for(&this, b.clone()).unwrap_or(b))
}

fn this_bool(vm: &mut Vm, this: &Value, name: &str) -> Result<bool, Value> {
    match crate::object::unwrap_primitive(this)
        .as_ref()
        .unwrap_or(this)
    {
        Value::Bool(b) => Ok(*b),
        _ => Err(vm.type_error(format!(
            "Boolean.prototype.{name} requires that 'this' be a Boolean"
        ))),
    }
}

fn bool_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(if this_bool(vm, &this, "toString")? {
        "true"
    } else {
        "false"
    }
    .into())
}

fn bool_value_of(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(Value::Bool(this_bool(vm, &this, "valueOf")?))
}
