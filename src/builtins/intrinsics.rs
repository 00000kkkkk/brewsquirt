use super::{arg, method, native_fn, to_integer};
use crate::object::{Accessor, NON_ENUM, Obj, ObjKind};
use crate::value::Value;
use crate::vm::Vm;
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn raw_accessor(get: Value, set: Value) -> Value {
    Value::Object(Obj::new(
        ObjKind::Accessor(Box::new(Accessor {
            get: RefCell::new(get),
            set: RefCell::new(set),
        })),
        None,
    ))
}

fn getter(vm: &mut Vm, target: &Rc<Obj>, name: &str, f: crate::object::NativeFn) {
    let g = native_fn(vm, &format!("get {name}"), 0, f);
    target.set_with_attrs(name, raw_accessor(g, Value::Undefined), NON_ENUM);
}

pub fn install(vm: &mut Vm) {
    let mp = vm.realm.map_proto.clone();
    getter(vm, &mp, "size", collection_size);
    let sp = vm.realm.set_proto.clone();
    getter(vm, &sp, "size", collection_size);

    let rp = vm.realm.regexp_proto.clone();
    let regexp_getters: [crate::object::NativeFn; 10] = [
        regexp_prop::<0>,
        regexp_prop::<1>,
        regexp_prop::<2>,
        regexp_prop::<3>,
        regexp_prop::<4>,
        regexp_prop::<5>,
        regexp_prop::<6>,
        regexp_prop::<7>,
        regexp_prop::<8>,
        regexp_prop::<9>,
    ];
    for (name, f) in REGEXP_PROPS.iter().zip(regexp_getters) {
        getter(vm, &rp, name, f);
    }

    let syp = vm.realm.symbol_proto.clone();
    getter(vm, &syp, "description", symbol_description);

    let op = vm.realm.object_proto.clone();
    let get = native_fn(vm, "get __proto__", 0, proto_get);
    let set = native_fn(vm, "set __proto__", 1, proto_set);
    let acc = raw_accessor(get, set);
    vm.proto_accessor = acc.clone();
    op.set_with_attrs("__proto__", acc, NON_ENUM);
    for (name, arity, f) in [
        (
            "__defineGetter__",
            2,
            define_getter as crate::object::NativeFn,
        ),
        ("__defineSetter__", 2, define_setter),
        ("__lookupGetter__", 1, lookup_getter),
        ("__lookupSetter__", 1, lookup_setter),
    ] {
        method(vm, &op, name, arity, f);
        op.props.borrow_mut().set_enumerable(name, false);
    }

    let fp = vm.realm.function_proto.clone();
    let thrower = native_fn(vm, "", 0, throw_type_error);
    if let Value::Object(t) = &thrower {
        t.set_with_attrs(
            "length",
            Value::Number(0.0),
            crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
        );
        t.set_with_attrs(
            "name",
            "".into(),
            crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
        );
        t.flags.set(t.flags.get() | crate::object::NOT_EXTENSIBLE);
    }
    vm.thrower = thrower.clone();
    for name in ["caller", "arguments"] {
        fp.set_with_attrs(
            name,
            raw_accessor(thrower.clone(), thrower.clone()),
            NON_ENUM,
        );
    }
    fp.set_with_attrs(
        "length",
        Value::Number(0.0),
        NON_ENUM | crate::object::READ_ONLY,
    );
    fp.set_with_attrs("name", "".into(), NON_ENUM | crate::object::READ_ONLY);

    let np = vm.realm.number_proto.clone();
    method(vm, &np, "toExponential", 1, to_exponential);
    np.props.borrow_mut().set_enumerable("toExponential", false);

    if let Some(Value::Object(math)) = vm.globals.get("Math").cloned() {
        for (name, arity, f) in [
            ("acosh", 1, math_acosh as crate::object::NativeFn),
            ("asinh", 1, math_asinh),
            ("atanh", 1, math_atanh),
            ("imul", 2, math_imul),
            ("f16round", 1, math_f16round),
        ] {
            method(vm, &math, name, arity, f);
        }
    }

    if let Some(Value::Object(err)) = vm.globals.get("Error").cloned() {
        method(vm, &err, "isError", 1, error_is_error);
    }

    let strp = vm.realm.string_proto.clone();
    method(vm, &strp, "normalize", 0, string_normalize);
    method(vm, &strp, "isWellFormed", 0, string_is_well_formed);
    method(vm, &strp, "toWellFormed", 0, string_to_well_formed);
}

fn collection_size(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) => match &o.kind {
            ObjKind::Map(m) => Ok(Value::Number(m.borrow().len() as f64)),
            ObjKind::Set(s) => Ok(Value::Number(s.borrow().len() as f64)),
            _ => Err(vm.type_error("Method get size called on incompatible receiver")),
        },
        _ => Err(vm.type_error("Method get size called on incompatible receiver")),
    }
}

const REGEXP_PROPS: [&str; 10] = [
    "flags",
    "source",
    "global",
    "ignoreCase",
    "multiline",
    "dotAll",
    "unicode",
    "unicodeSets",
    "sticky",
    "hasIndices",
];

fn regexp_prop<const I: usize>(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let name = REGEXP_PROPS[I];
    let Value::Object(o) = &this else {
        return Err(vm.type_error(format!(
            "RegExp.prototype.{name} getter called on non-object {}",
            crate::inspect::inspect_plain(&this)
        )));
    };
    if Rc::ptr_eq(o, &vm.realm.regexp_proto) {
        return Ok(match name {
            "source" => "(?:)".into(),
            "flags" => "".into(),
            _ => Value::Undefined,
        });
    }
    if let ObjKind::RegExp(r) = &o.kind {
        let d = r.data.borrow().clone();
        let flag = |c: char| Value::Bool(d.flags.contains(c));
        return Ok(match name {
            "source" => crate::regexp::escape_source(&d.source).into(),
            "flags" => Value::Str(d.flags.clone()),
            "global" => flag('g'),
            "ignoreCase" => flag('i'),
            "multiline" => flag('m'),
            "dotAll" => flag('s'),
            "unicode" => flag('u'),
            "unicodeSets" => flag('v'),
            "sticky" => flag('y'),
            _ => flag('d'),
        });
    }
    if name == "flags" {
        let mut out = String::new();
        for (c, p) in [
            ('d', "hasIndices"),
            ('g', "global"),
            ('i', "ignoreCase"),
            ('m', "multiline"),
            ('s', "dotAll"),
            ('u', "unicode"),
            ('v', "unicodeSets"),
            ('y', "sticky"),
        ] {
            if vm.get_prop(&this, p)?.truthy() {
                out.push(c);
            }
        }
        return Ok(out.into());
    }
    Err(vm.type_error(format!(
        "RegExp.prototype.{name} getter called on non-RegExp object"
    )))
}

fn throw_type_error(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Err(vm.type_error(
        "'caller', 'callee', and 'arguments' properties may not be accessed on strict mode functions or the arguments objects for calls to them",
    ))
}

fn symbol_description(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Symbol(s) => Ok(s.description.clone().map(Value::Str).unwrap_or_default()),
        v if crate::object::unwrap_primitive(v).is_some() => {
            match crate::object::unwrap_primitive(v) {
                Some(Value::Symbol(s)) => {
                    Ok(s.description.clone().map(Value::Str).unwrap_or_default())
                }
                _ => Err(
                    vm.type_error("Symbol.prototype.description requires that 'this' be a Symbol")
                ),
            }
        }
        _ => Err(vm.type_error("Symbol.prototype.description requires that 'this' be a Symbol")),
    }
}

fn proto_get(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) => Ok(vm
            .op_get_prototype_of(o)?
            .map(Value::Object)
            .unwrap_or(Value::Null)),
        Value::Undefined | Value::Null => {
            Err(vm.type_error("Cannot convert undefined or null to object"))
        }
        other => vm
            .get_prop(other, "constructor")
            .and_then(|c| vm.get_prop(&c, "prototype")),
    }
}

fn proto_set(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if this.is_nullish() {
        return Err(vm.type_error("Object.prototype.__proto__ called on null or undefined"));
    }
    let proto = match arg(args, 0) {
        Value::Object(p) => Some(p),
        Value::Null => None,
        _ => return Ok(Value::Undefined),
    };
    let Value::Object(o) = &this else {
        return Ok(Value::Undefined);
    };
    if !vm.op_set_prototype_of(o, proto)? {
        return Err(super::object::proto_set_error(vm, o));
    }
    Ok(Value::Undefined)
}

fn define_accessor_legacy(vm: &mut Vm, this: Value, args: &[Value], setter: bool) -> R {
    let Value::Object(o) = &this else {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    };
    let f = arg(args, 1);
    if !f.is_callable() {
        return Err(vm.type_error("Object.prototype.__defineGetter__: Expecting function"));
    }
    let key = vm.to_property_key(&arg(args, 0))?;
    let mut d = crate::props::PropDesc {
        enumerable: Some(true),
        configurable: Some(true),
        ..Default::default()
    };
    if setter {
        d.set = Some(f);
    } else {
        d.get = Some(f);
    }
    vm.op_define_own_property(o, key, d, true)?;
    Ok(Value::Undefined)
}

fn define_getter(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    define_accessor_legacy(vm, this, args, false)
}

fn define_setter(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    define_accessor_legacy(vm, this, args, true)
}

fn lookup_accessor(vm: &mut Vm, this: Value, args: &[Value], setter: bool) -> R {
    let o = vm.to_object(&this)?;
    let key = vm.to_property_key(&arg(args, 0))?;
    let mut cur = Some(o);
    while let Some(c) = cur {
        if let Some(d) = vm.op_get_own_property(&c, &key)? {
            return Ok(if setter { d.set } else { d.get }.unwrap_or_default());
        }
        cur = vm.op_get_prototype_of(&c)?;
    }
    Ok(Value::Undefined)
}

fn lookup_getter(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    lookup_accessor(vm, this, args, false)
}

fn lookup_setter(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    lookup_accessor(vm, this, args, true)
}

fn this_number(vm: &mut Vm, this: &Value) -> Result<f64, Value> {
    match crate::object::unwrap_primitive(this)
        .as_ref()
        .unwrap_or(this)
    {
        Value::Number(n) => Ok(*n),
        _ => Err(vm.type_error("Number.prototype.toExponential requires that 'this' be a Number")),
    }
}

pub fn exponential(n: f64, digits: Option<usize>) -> String {
    let x = if n == 0.0 { 0.0 } else { n };
    let a = x.abs();
    let d = match digits {
        Some(d) => d,
        None => {
            let s = crate::number::number_to_string(a);
            let m = s.split('e').next().unwrap_or(&s);
            let sig: String = m.chars().filter(|c| c.is_ascii_digit()).collect();
            let sig = sig.trim_start_matches('0').trim_end_matches('0');
            sig.len().max(1) - 1
        }
    };
    let (mant, exp) = round_half_up(a, d);
    format!(
        "{}{mant}e{}{}",
        if x < 0.0 { "-" } else { "" },
        if exp < 0 { "-" } else { "+" },
        exp.abs()
    )
}

fn round_half_up(a: f64, d: usize) -> (String, i32) {
    if a == 0.0 {
        let m = if d == 0 {
            "0".to_string()
        } else {
            format!("0.{}", "0".repeat(d))
        };
        return (m, 0);
    }
    let s = format!("{:.*e}", d + 30, a);
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let mut exp: i32 = e.parse().unwrap_or(0);
    let digits: Vec<u8> = m
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    let mut keep: Vec<u8> = digits[..d + 1].to_vec();
    if digits[d + 1] >= 5 {
        let mut i = d + 1;
        loop {
            if i == 0 {
                keep.insert(0, 1);
                keep.pop();
                exp += 1;
                break;
            }
            i -= 1;
            if keep[i] == 9 {
                keep[i] = 0;
            } else {
                keep[i] += 1;
                break;
            }
        }
    }
    let mut out = String::new();
    out.push((b'0' + keep[0]) as char);
    if d > 0 {
        out.push('.');
        for k in &keep[1..] {
            out.push((b'0' + k) as char);
        }
    }
    (out, exp)
}

fn to_exponential(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let n = this_number(vm, &this)?;
    let d = arg(args, 0);
    let digits = if d.is_undefined() {
        None
    } else {
        Some(to_integer(vm, &d)?)
    };
    if !n.is_finite() {
        return Ok(crate::number::number_to_string(n).into());
    }
    if let Some(f) = digits {
        if !(0.0..=100.0).contains(&f) {
            return Err(vm.range_error("toExponential() argument must be between 0 and 100"));
        }
    }
    Ok(exponential(n, digits.map(|f| f as usize)).into())
}

fn num_arg(vm: &mut Vm, args: &[Value], i: usize) -> Result<f64, Value> {
    vm.to_number(&arg(args, i))
}

fn math_acosh(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Number(num_arg(vm, args, 0)?.acosh()))
}

fn math_asinh(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let x = num_arg(vm, args, 0)?;
    Ok(Value::Number(if x == 0.0 { x } else { x.asinh() }))
}

fn math_atanh(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Number(num_arg(vm, args, 0)?.atanh()))
}

fn math_imul(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let a = crate::number::to_int32(num_arg(vm, args, 0)?);
    let b = crate::number::to_int32(num_arg(vm, args, 1)?);
    Ok(Value::Number(a.wrapping_mul(b) as f64))
}

fn math_f16round(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let x = num_arg(vm, args, 0)?;
    Ok(Value::Number(f16_round(x)))
}

fn f16_round(x: f64) -> f64 {
    crate::object::f16_round(x)
}

fn error_is_error(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(matches!(
        arg(args, 0),
        Value::Object(o) if matches!(o.kind, ObjKind::Error)
    )))
}

fn this_string(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<str>, Value> {
    if this.is_nullish() {
        return Err(vm.type_error(format!(
            "String.prototype.{name} called on null or undefined"
        )));
    }
    vm.to_string(this)
}

fn string_normalize(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    use unicode_normalization::UnicodeNormalization;
    let s = this_string(vm, &this, "normalize")?;
    let form = match arg(args, 0) {
        Value::Undefined => Rc::from("NFC"),
        v => vm.to_string(&v)?,
    };
    let out: String = match &*form {
        "NFC" => s.nfc().collect(),
        "NFD" => s.nfd().collect(),
        "NFKC" => s.nfkc().collect(),
        "NFKD" => s.nfkd().collect(),
        _ => {
            return Err(
                vm.range_error("The normalization form should be one of NFC, NFD, NFKC, NFKD.")
            );
        }
    };
    Ok(out.into())
}

fn string_is_well_formed(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let s = this_string(vm, &this, "isWellFormed")?;
    Ok(Value::Bool(crate::value::is_well_formed(&s)))
}

fn string_to_well_formed(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let s = this_string(vm, &this, "toWellFormed")?;
    Ok(crate::value::to_well_formed(&s).into())
}
