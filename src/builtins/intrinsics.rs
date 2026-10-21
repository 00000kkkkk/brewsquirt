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
    for (name, f) in [
        ("__defineGetter__", define_getter as crate::object::NativeFn),
        ("__defineSetter__", define_setter),
        ("__lookupGetter__", lookup_getter),
        ("__lookupSetter__", lookup_setter),
    ] {
        method(vm, &op, name, 2, f);
        op.props.borrow_mut().set_enumerable(name, false);
    }

    let fp = vm.realm.function_proto.clone();
    fp.set_with_attrs("name", "".into(), NON_ENUM | crate::object::READ_ONLY);
    fp.set_with_attrs(
        "length",
        Value::Number(0.0),
        NON_ENUM | crate::object::READ_ONLY,
    );

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
    if let ObjKind::RegExp(_) = &o.kind {
        return Ok(vm.get_obj(o, name));
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
    let Value::Object(o) = &this else {
        return Ok(Value::Undefined);
    };
    let proto = match arg(args, 0) {
        Value::Object(p) => Some(p),
        Value::Null => None,
        _ => return Ok(Value::Undefined),
    };
    if !vm.op_set_prototype_of(o, proto)? {
        return Err(vm.type_error("Cyclic __proto__ value"));
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
    let Value::Object(o) = &this else {
        return Ok(Value::Undefined);
    };
    let key = vm.to_property_key(&arg(args, 0))?;
    let mut cur = Some(o.clone());
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
    let raw = match digits {
        Some(d) => format!("{:.*e}", d, n),
        None => {
            let shortest = crate::number::number_to_string(n.abs());
            let digits_only: String = shortest
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .trim_start_matches('0')
                .to_string();
            let sig = digits_only.trim_end_matches('0').len().max(1);
            format!("{:.*e}", sig - 1, n)
        }
    };
    let (mantissa, exp) = raw.split_once('e').unwrap_or((&raw, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let sign = if exp < 0 { "-" } else { "+" };
    format!("{mantissa}e{sign}{}", exp.abs())
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
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let a = x.abs();
    if a >= 65520.0 {
        return f64::INFINITY.copysign(x);
    }
    let e = a.log2().floor().max(-14.0);
    let step = 2f64.powf(e - 10.0);
    let q = a / step;
    let f = q.floor();
    let d = q - f;
    let r = if d > 0.5 || (d == 0.5 && f % 2.0 != 0.0) {
        f + 1.0
    } else {
        f
    };
    (r * step).copysign(x)
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
