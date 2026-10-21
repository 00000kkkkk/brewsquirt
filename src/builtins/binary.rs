use super::{arg, expect_fn, make_ctor, method, native_fn, relative_index, to_integer};
use crate::inspect::inspect_plain;
use crate::object::{ITER_KEY, MapKey, NativeFn, Obj, ObjKind, TypedKind, View};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

const MAX_BYTES: f64 = 4294967296.0;

pub fn new_buffer(vm: &Vm, bytes: Vec<u8>) -> Rc<Obj> {
    Obj::new(
        ObjKind::ArrayBuffer(RefCell::new(bytes)),
        Some(vm.realm.array_buffer_proto.clone()),
    )
}

pub fn new_typed(vm: &Vm, kind: TypedKind, buffer: Rc<Obj>, offset: usize, length: usize) -> Value {
    Value::Object(Obj::new(
        ObjKind::Typed(Box::new(View {
            kind,
            buffer,
            offset,
            length,
        })),
        Some(vm.realm.typed_protos[kind as usize].clone()),
    ))
}

pub fn new_uint8(vm: &Vm, bytes: Vec<u8>) -> Value {
    let len = bytes.len();
    let buffer = new_buffer(vm, bytes);
    new_typed(vm, TypedKind::Uint8, buffer, 0, len)
}

fn filled(vm: &Vm, kind: TypedKind, values: &[f64]) -> Value {
    let size = kind.size();
    let mut bytes = vec![0u8; values.len() * size];
    for (i, &n) in values.iter().enumerate() {
        kind.write(&mut bytes[i * size..], n, true);
    }
    let buffer = new_buffer(vm, bytes);
    new_typed(vm, kind, buffer, 0, values.len())
}

pub fn buffer_source_bytes(v: &Value) -> Option<Vec<u8>> {
    let Value::Object(o) = v else {
        return None;
    };
    match &o.kind {
        ObjKind::ArrayBuffer(b) => Some(b.borrow().clone()),
        ObjKind::Typed(t) => Some(t.bytes()),
        ObjKind::DataView(t) => {
            let b = t.store().borrow();
            let end = (t.offset + t.length).min(b.len());
            Some(b[t.offset.min(end)..end].to_vec())
        }
        _ => None,
    }
}

fn to_index(vm: &mut Vm, v: &Value, what: &str) -> Result<usize, Value> {
    if v.is_undefined() {
        return Ok(0);
    }
    let n = to_integer(vm, v)?;
    if !(0.0..MAX_BYTES).contains(&n) {
        return Err(vm.range_error(format!("Invalid {what}: {}", inspect_plain(v))));
    }
    Ok(n as usize)
}

fn this_view(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Typed(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method %TypedArray%.prototype.{name} called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn view(o: &Obj) -> &View {
    match &o.kind {
        ObjKind::Typed(t) | ObjKind::DataView(t) => t,
        _ => unreachable!("not a view"),
    }
}

fn kind_of_ctor(vm: &Vm, ctor: &Value) -> Option<TypedKind> {
    let Value::Object(c) = ctor else {
        return None;
    };
    let Some(Value::Object(p)) = c.get_own("prototype") else {
        return None;
    };
    let mut cur = Some(p);
    while let Some(p) = cur {
        if let Some(i) = vm.realm.typed_protos.iter().position(|t| Rc::ptr_eq(t, &p)) {
            return Some(TypedKind::ALL[i]);
        }
        cur = p.proto.borrow().clone();
    }
    None
}

pub fn install(vm: &mut Vm) {
    let abp = vm.realm.array_buffer_proto.clone();
    method(vm, &abp, "slice", 2, ab_slice);
    let ab = make_ctor(
        vm,
        "ArrayBuffer",
        1,
        ab_ctor,
        Some(|| ObjKind::Ordinary),
        &abp,
    );
    method(vm, &ab, "isView", 1, ab_is_view);
    vm.define_global("ArrayBuffer", Value::Object(ab));

    let tp = vm.realm.typed_proto.clone();
    let methods: [(&str, u32, NativeFn); 33] = [
        ("at", 1, ta_at),
        ("fill", 1, ta_fill),
        ("set", 1, ta_set),
        ("subarray", 2, ta_subarray),
        ("slice", 2, ta_slice),
        ("join", 1, ta_join),
        ("toString", 0, ta_to_string),
        ("toLocaleString", 0, ta_to_string),
        ("indexOf", 1, ta_index_of),
        ("lastIndexOf", 1, ta_last_index_of),
        ("includes", 1, ta_includes),
        ("reverse", 0, ta_reverse),
        ("toReversed", 0, ta_to_reversed),
        ("sort", 1, ta_sort),
        ("toSorted", 1, ta_to_sorted),
        ("with", 2, ta_with),
        ("map", 1, ta_map),
        ("filter", 1, ta_filter),
        ("forEach", 1, ta_for_each),
        ("every", 1, ta_every),
        ("some", 1, ta_some),
        ("find", 1, ta_find),
        ("findIndex", 1, ta_find_index),
        ("findLast", 1, ta_find_last),
        ("findLastIndex", 1, ta_find_last_index),
        ("reduce", 1, ta_reduce),
        ("reduceRight", 1, ta_reduce_right),
        ("copyWithin", 2, ta_copy_within),
        ("keys", 0, ta_keys),
        ("values", 0, ta_values),
        ("entries", 0, ta_entries),
        ("toHex", 0, ta_to_hex),
        ("toBase64", 0, ta_to_base64),
    ];
    for (name, arity, f) in methods {
        method(vm, &tp, name, arity, f);
    }
    let values = native_fn(vm, "values", 0, ta_values);
    tp.set_own(ITER_KEY, values);

    let ctors: [NativeFn; 9] = [
        typed_ctor::<0>,
        typed_ctor::<1>,
        typed_ctor::<2>,
        typed_ctor::<3>,
        typed_ctor::<4>,
        typed_ctor::<5>,
        typed_ctor::<6>,
        typed_ctor::<7>,
        typed_ctor::<8>,
    ];
    for (i, kind) in TypedKind::ALL.iter().enumerate() {
        let proto = vm.realm.typed_protos[i].clone();
        let c = make_ctor(
            vm,
            kind.name(),
            3,
            ctors[i],
            Some(|| ObjKind::Ordinary),
            &proto,
        );
        let size = Value::Number(kind.size() as f64);
        c.set_own("BYTES_PER_ELEMENT", size.clone());
        proto.set_own("BYTES_PER_ELEMENT", size);
        method(vm, &c, "from", 1, ta_from);
        method(vm, &c, "of", 0, ta_of);
        if *kind == TypedKind::Uint8 {
            method(vm, &c, "fromHex", 1, u8_from_hex);
            method(vm, &c, "fromBase64", 1, u8_from_base64);
        }
        vm.define_global(kind.name(), Value::Object(c));
    }

    let dvp = vm.realm.dataview_proto.clone();
    let getters: [(&str, NativeFn, &str, NativeFn); 8] = [
        ("getInt8", dv_get::<0>, "setInt8", dv_set::<0>),
        ("getUint8", dv_get::<1>, "setUint8", dv_set::<1>),
        ("getInt16", dv_get::<3>, "setInt16", dv_set::<3>),
        ("getUint16", dv_get::<4>, "setUint16", dv_set::<4>),
        ("getInt32", dv_get::<5>, "setInt32", dv_set::<5>),
        ("getUint32", dv_get::<6>, "setUint32", dv_set::<6>),
        ("getFloat32", dv_get::<7>, "setFloat32", dv_set::<7>),
        ("getFloat64", dv_get::<8>, "setFloat64", dv_set::<8>),
    ];
    for (g, gf, s, sf) in getters {
        method(vm, &dvp, g, 1, gf);
        method(vm, &dvp, s, 2, sf);
    }
    let dv = make_ctor(vm, "DataView", 1, dv_ctor, Some(|| ObjKind::Ordinary), &dvp);
    vm.define_global("DataView", Value::Object(dv));

    let tep = vm.new_object();
    method(vm, &tep, "encode", 0, te_encode);
    method(vm, &tep, "encodeInto", 2, te_encode_into);
    let te = make_ctor(
        vm,
        "TextEncoder",
        0,
        te_ctor,
        Some(|| ObjKind::Ordinary),
        &tep,
    );
    vm.define_global("TextEncoder", Value::Object(te));
    let tdp = vm.new_object();
    method(vm, &tdp, "decode", 1, td_decode);
    let td = make_ctor(
        vm,
        "TextDecoder",
        0,
        td_ctor,
        Some(|| ObjKind::Ordinary),
        &tdp,
    );
    vm.define_global("TextDecoder", Value::Object(td));

    let f = native_fn(vm, "atob", 1, atob);
    vm.define_global("atob", f);
    let f = native_fn(vm, "btoa", 1, btoa);
    vm.define_global("btoa", f);
    let f = native_fn(vm, "structuredClone", 1, structured_clone);
    vm.define_global("structuredClone", f);

    let crypto = vm.new_object();
    method(vm, &crypto, "getRandomValues", 1, get_random_values);
    method(vm, &crypto, "randomUUID", 0, random_uuid);
    vm.define_global("crypto", Value::Object(crypto));
}

fn ab_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !this.is_object() {
        return Err(vm.type_error("Constructor ArrayBuffer requires 'new'"));
    }
    let len = to_index(vm, &arg(args, 0), "array buffer length")?;
    let buf = new_buffer(vm, vec![0; len]);
    if let Value::Object(t) = &this {
        *buf.proto.borrow_mut() = t.proto.borrow().clone();
    }
    Ok(Value::Object(buf))
}

fn ab_is_view(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(matches!(
        arg(args, 0),
        Value::Object(o) if matches!(o.kind, ObjKind::Typed(_) | ObjKind::DataView(_))
    )))
}

fn ab_slice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Value::Object(o) = &this else {
        return Err(vm.type_error("ArrayBuffer.prototype.slice called on incompatible receiver"));
    };
    let ObjKind::ArrayBuffer(b) = &o.kind else {
        return Err(vm.type_error("ArrayBuffer.prototype.slice called on incompatible receiver"));
    };
    let len = b.borrow().len();
    let (start, end) = range(vm, args, 0, len)?;
    let bytes = b.borrow()[start..end.max(start)].to_vec();
    Ok(Value::Object(new_buffer(vm, bytes)))
}

fn range(vm: &mut Vm, args: &[Value], at: usize, len: usize) -> Result<(usize, usize), Value> {
    let s = arg(args, at);
    let e = arg(args, at + 1);
    let start = relative_index(to_integer(vm, &s)?, len);
    let end = if e.is_undefined() {
        len
    } else {
        relative_index(to_integer(vm, &e)?, len)
    };
    Ok((start, end.max(start)))
}

fn collect_numbers(vm: &mut Vm, src: &Value) -> Result<Vec<f64>, Value> {
    if let Value::Object(o) = src {
        if let ObjKind::Typed(t) = &o.kind {
            return Ok(t.values());
        }
    }
    let iter = vm.get_prop(src, ITER_KEY)?;
    let items = if iter.is_callable() {
        vm.iterate(src)?
    } else {
        let len = vm.get_prop(src, "length")?;
        let len = to_integer(vm, &len)?.max(0.0) as usize;
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(vm.get_prop(src, &i.to_string())?);
        }
        out
    };
    let mut out = Vec::with_capacity(items.len());
    for v in &items {
        out.push(vm.to_number(v)?);
    }
    Ok(out)
}

fn typed_ctor<const K: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let kind = TypedKind::ALL[K];
    let Value::Object(t) = &this else {
        return Err(vm.type_error(format!("Constructor {} requires 'new'", kind.name())));
    };
    let proto = t.proto.borrow().clone();
    let size = kind.size();
    let a0 = arg(args, 0);
    let made = match &a0 {
        Value::Object(o) => match &o.kind {
            ObjKind::ArrayBuffer(b) => {
                let len = b.borrow().len();
                let offset = to_index(vm, &arg(args, 1), "typed array offset")?;
                if offset % size != 0 {
                    return Err(vm.range_error(format!(
                        "start offset of {} should be a multiple of {size}",
                        kind.name()
                    )));
                }
                let length = if arg(args, 2).is_undefined() {
                    if len % size != 0 {
                        return Err(vm.range_error(format!(
                            "byte length of {} should be a multiple of {size}",
                            kind.name()
                        )));
                    }
                    if offset > len {
                        return Err(vm.range_error(format!(
                            "Start offset {offset} is outside the bounds of the buffer"
                        )));
                    }
                    (len - offset) / size
                } else {
                    let l = to_index(vm, &arg(args, 2), "typed array length")?;
                    if offset + l * size > len {
                        return Err(vm.range_error(format!("Invalid typed array length: {l}")));
                    }
                    l
                };
                new_typed(vm, kind, o.clone(), offset, length)
            }
            _ => {
                let values = collect_numbers(vm, &a0)?;
                filled(vm, kind, &values)
            }
        },
        Value::Undefined | Value::Null => filled(vm, kind, &[]),
        other => {
            let n = vm.to_number(other)?;
            let n = if n.is_nan() { 0.0 } else { n.trunc() };
            if !(0.0..MAX_BYTES).contains(&n) {
                return Err(vm.range_error(format!(
                    "Invalid typed array length: {}",
                    crate::number::number_to_string(n)
                )));
            }
            let buffer = new_buffer(vm, vec![0; n as usize * size]);
            new_typed(vm, kind, buffer, 0, n as usize)
        }
    };
    if let Value::Object(m) = &made {
        *m.proto.borrow_mut() = proto;
    }
    Ok(made)
}

fn ta_from(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Some(kind) = kind_of_ctor(vm, &this) else {
        return Err(vm.type_error("this is not a typed array constructor"));
    };
    let mut values = collect_numbers(vm, &arg(args, 0))?;
    let f = arg(args, 1);
    if !f.is_undefined() {
        expect_fn(vm, &f)?;
        let this_arg = arg(args, 2);
        for (i, x) in values.iter_mut().enumerate() {
            let r = vm.call_value(
                &f,
                this_arg.clone(),
                &[Value::Number(*x), (i as f64).into()],
            )?;
            *x = vm.to_number(&r)?;
        }
    }
    Ok(filled(vm, kind, &values))
}

fn ta_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Some(kind) = kind_of_ctor(vm, &this) else {
        return Err(vm.type_error("this is not a typed array constructor"));
    };
    let mut values = Vec::with_capacity(args.len());
    for a in args {
        values.push(vm.to_number(a)?);
    }
    Ok(filled(vm, kind, &values))
}

fn same_kind(vm: &Vm, o: &Obj, values: &[f64]) -> Value {
    let v = view(o);
    let out = filled(vm, v.kind, values);
    if let Value::Object(n) = &out {
        *n.proto.borrow_mut() = o.proto.borrow().clone();
    }
    out
}

fn ta_at(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "at")?;
    let len = view(&o).length as f64;
    let n = to_integer(vm, &arg(args, 0))?;
    let i = if n < 0.0 { len + n } else { n };
    if i < 0.0 || i >= len {
        return Ok(Value::Undefined);
    }
    Ok(view(&o)
        .get(i as usize)
        .map(Value::Number)
        .unwrap_or_default())
}

fn ta_fill(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "fill")?;
    let n = vm.to_number(&arg(args, 0))?;
    let len = view(&o).length;
    let (start, end) = range(vm, args, 1, len)?;
    for i in start..end {
        view(&o).set(i, n);
    }
    Ok(this)
}

fn ta_set(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "set")?;
    let values = collect_numbers(vm, &arg(args, 0))?;
    let offset = to_integer(vm, &arg(args, 1))?;
    if offset < 0.0 {
        return Err(vm.range_error("offset is out of bounds"));
    }
    let offset = offset as usize;
    let v = view(&o);
    if offset + values.len() > v.length {
        return Err(vm.range_error("offset is out of bounds"));
    }
    for (i, x) in values.into_iter().enumerate() {
        v.set(offset + i, x);
    }
    Ok(Value::Undefined)
}

fn ta_subarray(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "subarray")?;
    let len = view(&o).length;
    let (start, end) = range(vm, args, 0, len)?;
    let v = view(&o);
    let out = new_typed(
        vm,
        v.kind,
        v.buffer.clone(),
        v.offset + start * v.kind.size(),
        end - start,
    );
    if let Value::Object(n) = &out {
        *n.proto.borrow_mut() = o.proto.borrow().clone();
    }
    Ok(out)
}

fn ta_slice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "slice")?;
    let values = view(&o).values();
    let (start, end) = range(vm, args, 0, values.len())?;
    Ok(same_kind(vm, &o, &values[start..end]))
}

fn number_text(n: f64) -> String {
    crate::number::number_to_string(n)
}

fn ta_join(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "join")?;
    let sep = match arg(args, 0) {
        Value::Undefined => Rc::from(","),
        v => vm.to_string(&v)?,
    };
    let parts: Vec<String> = view(&o).values().into_iter().map(number_text).collect();
    Ok(parts.join(&sep).into())
}

fn ta_to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    ta_join(vm, this, &[])
}

fn ta_index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "indexOf")?;
    let values = view(&o).values();
    let Value::Number(target) = arg(args, 0) else {
        return Ok(Value::Number(-1.0));
    };
    let from = relative_index(to_integer(vm, &arg(args, 1))?, values.len());
    Ok(Value::Number(
        values
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, x)| **x == target)
            .map_or(-1.0, |(i, _)| i as f64),
    ))
}

fn ta_last_index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "lastIndexOf")?;
    let values = view(&o).values();
    let Value::Number(target) = arg(args, 0) else {
        return Ok(Value::Number(-1.0));
    };
    let len = values.len() as f64;
    let from = if args.len() > 1 {
        let n = to_integer(vm, &args[1])?;
        if n < 0.0 { len + n } else { n.min(len - 1.0) }
    } else {
        len - 1.0
    };
    if from < 0.0 {
        return Ok(Value::Number(-1.0));
    }
    Ok(Value::Number(
        (0..=from as usize)
            .rev()
            .find(|&i| values[i] == target)
            .map_or(-1.0, |i| i as f64),
    ))
}

fn ta_includes(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "includes")?;
    let values = view(&o).values();
    let Value::Number(target) = arg(args, 0) else {
        return Ok(Value::Bool(false));
    };
    let from = relative_index(to_integer(vm, &arg(args, 1))?, values.len());
    Ok(Value::Bool(
        values
            .iter()
            .skip(from)
            .any(|x| *x == target || (x.is_nan() && target.is_nan())),
    ))
}

fn ta_reverse(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_view(vm, &this, "reverse")?;
    let mut values = view(&o).values();
    values.reverse();
    for (i, x) in values.into_iter().enumerate() {
        view(&o).set(i, x);
    }
    Ok(this)
}

fn ta_to_reversed(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_view(vm, &this, "toReversed")?;
    let mut values = view(&o).values();
    values.reverse();
    Ok(same_kind(vm, &o, &values))
}

fn sorted(vm: &mut Vm, o: &Obj, f: &Value) -> Result<Vec<f64>, Value> {
    let mut values = view(o).values();
    if f.is_undefined() {
        values.sort_by(|a, b| a.total_cmp(b));
        return Ok(values);
    }
    expect_fn(vm, f)?;
    let list = values.into_iter().map(Value::Number).collect();
    let out = super::array::merge_sort(vm, list, f)?;
    Ok(out
        .into_iter()
        .map(|v| match v {
            Value::Number(n) => n,
            _ => f64::NAN,
        })
        .collect())
}

fn ta_sort(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "sort")?;
    let values = sorted(vm, &o, &arg(args, 0))?;
    for (i, x) in values.into_iter().enumerate() {
        view(&o).set(i, x);
    }
    Ok(this)
}

fn ta_to_sorted(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "toSorted")?;
    let values = sorted(vm, &o, &arg(args, 0))?;
    Ok(same_kind(vm, &o, &values))
}

fn ta_with(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "with")?;
    let mut values = view(&o).values();
    let len = values.len() as f64;
    let n = to_integer(vm, &arg(args, 0))?;
    let i = if n < 0.0 { len + n } else { n };
    let x = vm.to_number(&arg(args, 1))?;
    if i < 0.0 || i >= len {
        return Err(vm.range_error("Invalid typed array index"));
    }
    values[i as usize] = x;
    Ok(same_kind(vm, &o, &values))
}

fn each(
    vm: &mut Vm,
    this: &Value,
    name: &str,
    args: &[Value],
    mut step: impl FnMut(usize, f64, &Value) -> Option<Value>,
) -> Result<Option<Value>, Value> {
    let o = this_view(vm, this, name)?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let this_arg = arg(args, 1);
    let len = view(&o).length;
    for i in 0..len {
        let x = view(&o).get(i).unwrap_or(f64::NAN);
        let r = vm.call_value(
            &f,
            this_arg.clone(),
            &[Value::Number(x), (i as f64).into(), this.clone()],
        )?;
        if let Some(v) = step(i, x, &r) {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

fn ta_map(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let mut results = Vec::new();
    each(vm, &this, "map", args, |_, _, r| {
        results.push(r.clone());
        None
    })?;
    let mut values = Vec::with_capacity(results.len());
    for r in &results {
        values.push(vm.to_number(r)?);
    }
    let o = this_view(vm, &this, "map")?;
    Ok(same_kind(vm, &o, &values))
}

fn ta_filter(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let mut kept = Vec::new();
    each(vm, &this, "filter", args, |_, x, r| {
        if r.truthy() {
            kept.push(x);
        }
        None
    })?;
    let o = this_view(vm, &this, "filter")?;
    Ok(same_kind(vm, &o, &kept))
}

fn ta_for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    each(vm, &this, "forEach", args, |_, _, _| None)?;
    Ok(Value::Undefined)
}

fn ta_every(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = each(vm, &this, "every", args, |_, _, r| {
        (!r.truthy()).then_some(Value::Bool(false))
    })?;
    Ok(r.unwrap_or(Value::Bool(true)))
}

fn ta_some(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = each(vm, &this, "some", args, |_, _, r| {
        r.truthy().then_some(Value::Bool(true))
    })?;
    Ok(r.unwrap_or(Value::Bool(false)))
}

fn ta_find(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = each(vm, &this, "find", args, |_, x, r| {
        r.truthy().then_some(Value::Number(x))
    })?;
    Ok(r.unwrap_or_default())
}

fn ta_find_index(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = each(vm, &this, "findIndex", args, |i, _, r| {
        r.truthy().then_some(Value::Number(i as f64))
    })?;
    Ok(r.unwrap_or(Value::Number(-1.0)))
}

fn find_last(vm: &mut Vm, this: &Value, args: &[Value], index: bool) -> R {
    let o = this_view(vm, this, "findLast")?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let this_arg = arg(args, 1);
    let len = view(&o).length;
    for i in (0..len).rev() {
        let x = view(&o).get(i).unwrap_or(f64::NAN);
        let r = vm.call_value(
            &f,
            this_arg.clone(),
            &[Value::Number(x), (i as f64).into(), this.clone()],
        )?;
        if r.truthy() {
            return Ok(Value::Number(if index { i as f64 } else { x }));
        }
    }
    Ok(if index {
        Value::Number(-1.0)
    } else {
        Value::Undefined
    })
}

fn ta_find_last(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    find_last(vm, &this, args, false)
}

fn ta_find_last_index(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    find_last(vm, &this, args, true)
}

fn reduce_impl(vm: &mut Vm, this: &Value, args: &[Value], right: bool) -> R {
    let o = this_view(vm, this, if right { "reduceRight" } else { "reduce" })?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let values = view(&o).values();
    let order: Vec<usize> = if right {
        (0..values.len()).rev().collect()
    } else {
        (0..values.len()).collect()
    };
    let mut it = order.into_iter();
    let mut acc = if args.len() > 1 {
        args[1].clone()
    } else {
        match it.next() {
            Some(i) => Value::Number(values[i]),
            None => return Err(vm.type_error("Reduce of empty array with no initial value")),
        }
    };
    for i in it {
        acc = vm.call_value(
            &f,
            Value::Undefined,
            &[
                acc,
                Value::Number(values[i]),
                (i as f64).into(),
                this.clone(),
            ],
        )?;
    }
    Ok(acc)
}

fn ta_reduce(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    reduce_impl(vm, &this, args, false)
}

fn ta_reduce_right(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    reduce_impl(vm, &this, args, true)
}

fn ta_copy_within(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_view(vm, &this, "copyWithin")?;
    let values = view(&o).values();
    let len = values.len();
    let target = relative_index(to_integer(vm, &arg(args, 0))?, len);
    let (start, end) = range(vm, args, 1, len)?;
    for (k, i) in (start..end).enumerate() {
        if target + k >= len {
            break;
        }
        view(&o).set(target + k, values[i]);
    }
    Ok(this)
}

fn ta_keys(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_view(vm, &this, "keys")?;
    let items = (0..view(&o).length)
        .map(|i| Value::Number(i as f64))
        .collect();
    Ok(vm.new_iter(crate::object::IterState::List(items, 0)))
}

fn ta_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_view(vm, &this, "values")?;
    let items = view(&o).values().into_iter().map(Value::Number).collect();
    Ok(vm.new_iter(crate::object::IterState::List(items, 0)))
}

fn ta_entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_view(vm, &this, "entries")?;
    let items = view(&o)
        .values()
        .into_iter()
        .enumerate()
        .map(|(i, x)| vm.new_array(vec![(i as f64).into(), Value::Number(x)]))
        .collect();
    Ok(vm.new_iter(crate::object::IterState::List(items, 0)))
}

fn this_u8(vm: &mut Vm, this: &Value, name: &str) -> Result<Vec<u8>, Value> {
    match this {
        Value::Object(o) => match &o.kind {
            ObjKind::Typed(t) if t.kind == TypedKind::Uint8 => Ok(t.bytes()),
            _ => Err(vm.type_error(format!(
                "Uint8Array.prototype.{name} called on incompatible receiver"
            ))),
        },
        _ => Err(vm.type_error(format!(
            "Uint8Array.prototype.{name} called on incompatible receiver"
        ))),
    }
}

fn ta_to_hex(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let bytes = this_u8(vm, &this, "toHex")?;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s.into())
}

fn ta_to_base64(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let bytes = this_u8(vm, &this, "toBase64")?;
    let url = base64_url_option(vm, &arg(args, 0))?;
    let mut s = base64_encode(&bytes, url);
    if arg(args, 0).is_object() && vm.get_prop(&arg(args, 0), "omitPadding")?.truthy() {
        while s.ends_with('=') {
            s.pop();
        }
    }
    Ok(s.into())
}

fn base64_url_option(vm: &mut Vm, options: &Value) -> Result<bool, Value> {
    if !options.is_object() {
        return Ok(false);
    }
    let a = vm.get_prop(options, "alphabet")?;
    Ok(matches!(&a, Value::Str(s) if &**s == "base64url"))
}

fn u8_from_hex(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    if s.len() % 2 != 0 {
        return Err(vm.make_error(
            ErrorKind::SyntaxError,
            "Input string must contain hex characters in even length",
        ));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let b = s.as_bytes();
    for pair in b.chunks(2) {
        let hi = (pair[0] as char).to_digit(16);
        let lo = (pair[1] as char).to_digit(16);
        match (hi, lo) {
            (Some(h), Some(l)) => out.push((h * 16 + l) as u8),
            _ => {
                return Err(vm.make_error(
                    ErrorKind::SyntaxError,
                    "Input string must contain hex characters in even length",
                ));
            }
        }
    }
    Ok(new_uint8(vm, out))
}

fn u8_from_base64(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    let url = base64_url_option(vm, &arg(args, 1))?;
    match base64_decode(&s, url) {
        Some(bytes) => Ok(new_uint8(vm, bytes)),
        None => Err(vm.make_error(ErrorKind::SyntaxError, "Invalid base64 string")),
    }
}

fn this_dataview(vm: &mut Vm, this: &Value) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::DataView(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Receiver is not a DataView: {}",
            inspect_plain(this)
        ))),
    }
}

fn dv_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Value::Object(t) = &this else {
        return Err(vm.type_error("Constructor DataView requires 'new'"));
    };
    let buffer = match arg(args, 0) {
        Value::Object(o) if matches!(o.kind, ObjKind::ArrayBuffer(_)) => o,
        _ => {
            return Err(
                vm.type_error("First argument to DataView constructor must be an ArrayBuffer")
            );
        }
    };
    let len = match &buffer.kind {
        ObjKind::ArrayBuffer(b) => b.borrow().len(),
        _ => 0,
    };
    let offset = to_index(vm, &arg(args, 1), "DataView offset")?;
    if offset > len {
        return Err(vm.range_error(format!(
            "Start offset {offset} is outside the bounds of the buffer"
        )));
    }
    let length = if arg(args, 2).is_undefined() {
        len - offset
    } else {
        let l = to_index(vm, &arg(args, 2), "DataView length")?;
        if offset + l > len {
            return Err(vm.range_error(format!("Invalid DataView length {l}")));
        }
        l
    };
    Ok(Value::Object(Obj::new(
        ObjKind::DataView(Box::new(View {
            kind: TypedKind::Uint8,
            buffer,
            offset,
            length,
        })),
        t.proto.borrow().clone(),
    )))
}

fn dv_get<const K: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let kind = TypedKind::ALL[K];
    let o = this_dataview(vm, &this)?;
    let at = to_index(vm, &arg(args, 0), "offset")?;
    let little = arg(args, 1).truthy();
    let v = view(&o);
    if at + kind.size() > v.length {
        return Err(vm.range_error("Offset is outside the bounds of the DataView"));
    }
    let b = v.store().borrow();
    let start = v.offset + at;
    Ok(Value::Number(
        kind.read(&b[start..start + kind.size()], little),
    ))
}

fn dv_set<const K: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let kind = TypedKind::ALL[K];
    let o = this_dataview(vm, &this)?;
    let at = to_index(vm, &arg(args, 0), "offset")?;
    let n = vm.to_number(&arg(args, 1))?;
    let little = arg(args, 2).truthy();
    let v = view(&o);
    if at + kind.size() > v.length {
        return Err(vm.range_error("Offset is outside the bounds of the DataView"));
    }
    let mut b = v.store().borrow_mut();
    let start = v.offset + at;
    kind.write(&mut b[start..start + kind.size()], n, little);
    Ok(Value::Undefined)
}

fn te_ctor(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let Value::Object(o) = &this else {
        return Err(vm.type_error("Constructor TextEncoder requires 'new'"));
    };
    o.set_own("encoding", "utf-8".into());
    Ok(this)
}

fn te_encode(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = match arg(args, 0) {
        Value::Undefined => Rc::from(""),
        v => vm.to_string(&v)?,
    };
    Ok(new_uint8(vm, s.as_bytes().to_vec()))
}

fn te_encode_into(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    let target = match arg(args, 1) {
        Value::Object(o) if matches!(&o.kind, ObjKind::Typed(t) if t.kind == TypedKind::Uint8) => o,
        _ => return Err(vm.type_error("The \"dest\" argument must be an instance of Uint8Array.")),
    };
    let v = view(&target);
    let mut read = 0usize;
    let mut written = 0usize;
    for c in s.chars() {
        let mut buf = [0u8; 4];
        let bytes = c.encode_utf8(&mut buf).as_bytes();
        if written + bytes.len() > v.length {
            break;
        }
        for (i, b) in bytes.iter().enumerate() {
            v.set(written + i, *b as f64);
        }
        written += bytes.len();
        read += c.len_utf16();
    }
    let r = vm.new_object();
    r.set_own("read", (read as f64).into());
    r.set_own("written", (written as f64).into());
    Ok(Value::Object(r))
}

fn td_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Value::Object(o) = &this else {
        return Err(vm.type_error("Constructor TextDecoder requires 'new'"));
    };
    let label = match arg(args, 0) {
        Value::Undefined => Rc::from("utf-8"),
        v => vm.to_string(&v)?,
    };
    let norm = label.trim().to_ascii_lowercase();
    if !matches!(norm.as_str(), "utf-8" | "utf8" | "unicode-1-1-utf-8") {
        return Err(vm.range_error(format!("The \"{label}\" encoding is not supported")));
    }
    let options = arg(args, 1);
    let (fatal, ignore_bom) = if options.is_object() {
        let f = vm.get_prop(&options, "fatal")?.truthy();
        let b = vm.get_prop(&options, "ignoreBOM")?.truthy();
        (f, b)
    } else {
        (false, false)
    };
    o.set_own("encoding", "utf-8".into());
    o.set_own("fatal", Value::Bool(fatal));
    o.set_own("ignoreBOM", Value::Bool(ignore_bom));
    Ok(this)
}

fn td_decode(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let input = arg(args, 0);
    let bytes = if input.is_undefined() {
        Vec::new()
    } else {
        match buffer_source_bytes(&input) {
            Some(b) => b,
            None => {
                return Err(vm.type_error(
                    "The \"input\" argument must be an instance of ArrayBuffer or ArrayBufferView",
                ));
            }
        }
    };
    let fatal = vm.get_prop(&this, "fatal")?.truthy();
    let ignore_bom = vm.get_prop(&this, "ignoreBOM")?.truthy();
    let body = if !ignore_bom && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        &bytes[..]
    };
    if fatal {
        return match std::str::from_utf8(body) {
            Ok(s) => Ok(s.into()),
            Err(_) => Err(vm.type_error("The encoded data was not valid for encoding utf-8")),
        };
    }
    Ok(String::from_utf8_lossy(body).into_owned().into())
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64_encode(bytes: &[u8], url: bool) -> String {
    let table = if url { B64_URL } else { B64 };
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(table[(n >> 18) as usize & 63] as char);
        out.push(table[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            table[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            table[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(s: &str, url: bool) -> Option<Vec<u8>> {
    let mut vals = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_whitespace() {
            continue;
        }
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' if !url => 62,
            '/' if !url => 63,
            '-' if url => 62,
            '_' if url => 63,
            '=' => 64,
            _ => return None,
        };
        vals.push(v);
    }
    while vals.last() == Some(&64) {
        vals.pop();
    }
    if vals.contains(&64) || vals.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() {
            n |= v << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

fn dom_error(vm: &mut Vm, name: &str, msg: &str) -> Value {
    let e = vm.make_error(ErrorKind::Error, msg);
    if let Value::Object(o) = &e {
        o.set_hidden("name", name.into());
        o.set_hidden("stack", format!("{name}: {msg}").into());
    }
    e
}

fn btoa(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c as u32 > 255 {
            return Err(dom_error(vm, "InvalidCharacterError", "Invalid character"));
        }
        bytes.push(c as u8);
    }
    Ok(base64_encode(&bytes, false).into())
}

fn atob(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    match base64_decode(&s, false) {
        Some(bytes) => Ok(bytes
            .into_iter()
            .map(|b| b as char)
            .collect::<String>()
            .into()),
        None => Err(dom_error(
            vm,
            "InvalidCharacterError",
            "The string to be decoded is not correctly encoded.",
        )),
    }
}

fn fill_random(buf: &mut [u8]) {
    if getrandom::getrandom(buf).is_err() {
        let mut x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15)
            | 1;
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
}

fn get_random_values(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    let Value::Object(o) = &target else {
        return Err(vm.type_error("The \"typedArray\" argument must be an integer typed array"));
    };
    let ObjKind::Typed(t) = &o.kind else {
        return Err(vm.type_error("The \"typedArray\" argument must be an integer typed array"));
    };
    if t.kind.is_float() {
        return Err(dom_error(
            vm,
            "TypeMismatchError",
            "The data argument must be an integer-type TypedArray",
        ));
    }
    let n = t.byte_len();
    if n > 65536 {
        return Err(dom_error(
            vm,
            "QuotaExceededError",
            &format!(
                "The ArrayBufferView's byte length ({n}) exceeds the number of bytes of entropy available via this API (65536)"
            ),
        ));
    }
    let mut buf = vec![0u8; n];
    fill_random(&mut buf);
    let mut store = t.store().borrow_mut();
    store[t.offset..t.offset + n].copy_from_slice(&buf);
    drop(store);
    Ok(target)
}

fn random_uuid(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let mut b = [0u8; 16];
    fill_random(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    )
    .into())
}

fn structured_clone(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut memo: Vec<(*const Obj, Value)> = Vec::new();
    clone_value(vm, &arg(args, 0), &mut memo)
}

fn not_cloneable(vm: &mut Vm, v: &Value) -> Value {
    dom_error(
        vm,
        "DataCloneError",
        &format!("{} could not be cloned.", inspect_plain(v)),
    )
}

fn clone_value(vm: &mut Vm, v: &Value, memo: &mut Vec<(*const Obj, Value)>) -> R {
    let o = match v {
        Value::Symbol(_) => return Err(not_cloneable(vm, v)),
        Value::Object(o) => o,
        other => return Ok(other.clone()),
    };
    let ptr = Rc::as_ptr(o);
    if let Some((_, c)) = memo.iter().find(|(p, _)| *p == ptr) {
        return Ok(c.clone());
    }
    let out = match &o.kind {
        ObjKind::Array(a) => {
            let out = vm.new_array(Vec::new());
            memo.push((ptr, out.clone()));
            let items = a.borrow().clone();
            let mut cloned = Vec::with_capacity(items.len());
            for x in &items {
                cloned.push(clone_value(vm, x, memo)?);
            }
            if let Value::Object(n) = &out {
                if let ObjKind::Array(na) = &n.kind {
                    *na.borrow_mut() = cloned;
                }
            }
            return Ok(out);
        }
        ObjKind::Ordinary | ObjKind::Error => {
            let n = if matches!(o.kind, ObjKind::Error) {
                let e = vm.make_error(ErrorKind::Error, "");
                if let Value::Object(e) = &e {
                    for key in ["name", "message", "stack"] {
                        if let Some(x) = o.get_chain(key) {
                            if matches!(x, Value::Str(_)) {
                                e.set_hidden(key, x);
                            }
                        }
                    }
                    if let Some(Value::Str(name)) = o.get_chain("name") {
                        if let Some(Value::Object(c)) = vm.globals.get(&name).cloned() {
                            if let Some(Value::Object(p)) = c.get_own("prototype") {
                                *e.proto.borrow_mut() = Some(p);
                            }
                        }
                    }
                }
                e
            } else {
                Value::Object(vm.new_object())
            };
            memo.push((ptr, n.clone()));
            let entries = vm.own_entries_resolved(v, false)?;
            for (k, x) in entries {
                if x.is_callable() {
                    return Err(not_cloneable(vm, &x));
                }
                let c = clone_value(vm, &x, memo)?;
                if let Value::Object(no) = &n {
                    no.set_own(k, c);
                }
            }
            return Ok(n);
        }
        ObjKind::Map(m) => {
            let out = Obj::new(
                ObjKind::Map(Box::default()),
                Some(vm.realm.map_proto.clone()),
            );
            let outv = Value::Object(out.clone());
            memo.push((ptr, outv.clone()));
            let entries: Vec<(Value, Value)> = m
                .borrow()
                .iter()
                .map(|(k, v)| (k.to_value(), v.clone()))
                .collect();
            for (k, x) in entries {
                let ck = clone_value(vm, &k, memo)?;
                let cx = clone_value(vm, &x, memo)?;
                if let ObjKind::Map(nm) = &out.kind {
                    nm.borrow_mut().insert(MapKey::from_value(&ck), cx);
                }
            }
            return Ok(outv);
        }
        ObjKind::Set(s) => {
            let out = Obj::new(
                ObjKind::Set(Box::default()),
                Some(vm.realm.set_proto.clone()),
            );
            let outv = Value::Object(out.clone());
            memo.push((ptr, outv.clone()));
            let items: Vec<Value> = s.borrow().iter().map(|k| k.to_value()).collect();
            for x in items {
                let cx = clone_value(vm, &x, memo)?;
                if let ObjKind::Set(ns) = &out.kind {
                    ns.borrow_mut().insert(MapKey::from_value(&cx));
                }
            }
            return Ok(outv);
        }
        ObjKind::Date(t) => Value::Object(Obj::new(
            ObjKind::Date(std::cell::Cell::new(t.get())),
            Some(vm.realm.date_proto.clone()),
        )),
        ObjKind::RegExp(r) => crate::builtins::make_regexp(vm, &r.data.source, &r.data.flags)?,
        ObjKind::ArrayBuffer(b) => {
            let bytes = b.borrow().clone();
            Value::Object(new_buffer(vm, bytes))
        }
        ObjKind::Typed(t) => {
            let buf = clone_value(vm, &Value::Object(t.buffer.clone()), memo)?;
            let Value::Object(buf) = buf else {
                return Err(not_cloneable(vm, v));
            };
            new_typed(vm, t.kind, buf, t.offset, t.length)
        }
        ObjKind::DataView(t) => {
            let buf = clone_value(vm, &Value::Object(t.buffer.clone()), memo)?;
            let Value::Object(buf) = buf else {
                return Err(not_cloneable(vm, v));
            };
            Value::Object(Obj::new(
                ObjKind::DataView(Box::new(View {
                    kind: TypedKind::Uint8,
                    buffer: buf,
                    offset: t.offset,
                    length: t.length,
                })),
                Some(vm.realm.dataview_proto.clone()),
            ))
        }
        _ => return Err(not_cloneable(vm, v)),
    };
    memo.push((ptr, out.clone()));
    Ok(out)
}
