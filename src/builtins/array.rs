use super::{arg, expect_fn, make_ctor, method, relative_index, to_integer};
use crate::inspect::inspect_plain;
use crate::object::{IterState, Obj, ObjKind};
use crate::value::Value;
use crate::vm::Vm;
use std::cmp::Ordering;
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let ap = vm.realm.array_proto.clone();
    let methods: &[(&str, u32, crate::object::NativeFn)] = &[
        ("push", 1, push),
        ("pop", 0, pop),
        ("shift", 0, shift),
        ("unshift", 1, unshift),
        ("slice", 2, slice),
        ("splice", 2, splice),
        ("concat", 1, concat),
        ("join", 1, join),
        ("reverse", 0, reverse),
        ("indexOf", 1, index_of),
        ("lastIndexOf", 1, last_index_of),
        ("includes", 1, includes),
        ("find", 1, find),
        ("findIndex", 1, find_index),
        ("findLast", 1, find_last),
        ("findLastIndex", 1, find_last_index),
        ("filter", 1, filter),
        ("map", 1, map),
        ("forEach", 1, for_each),
        ("reduce", 1, reduce),
        ("reduceRight", 1, reduce_right),
        ("some", 1, some),
        ("every", 1, every),
        ("sort", 1, sort),
        ("toSorted", 1, to_sorted),
        ("toReversed", 0, to_reversed),
        ("flat", 0, flat),
        ("flatMap", 1, flat_map),
        ("fill", 1, fill),
        ("at", 1, at),
        ("keys", 0, keys),
        ("values", 0, values),
        ("entries", 0, entries),
        ("toString", 0, to_string),
    ];
    for (name, arity, f) in methods {
        method(vm, &ap, name, *arity, *f);
    }
    let array = make_ctor(vm, "Array", 1, array_ctor, None, &ap);
    method(vm, &array, "isArray", 1, is_array);
    method(vm, &array, "from", 1, from);
    method(vm, &array, "of", 0, of);
    vm.define_global("Array", Value::Object(array));
}

fn this_array(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    if let Value::Object(o) = this {
        if matches!(o.kind, ObjKind::Array(_)) {
            return Ok(o.clone());
        }
    }
    Err(vm.type_error(format!(
        "Array.prototype.{name} called on non-array {}",
        inspect_plain(this)
    )))
}

fn items(o: &Obj) -> std::cell::RefMut<'_, Vec<Value>> {
    o.array().expect("array").borrow_mut()
}

fn len(o: &Obj) -> usize {
    o.array().map(|a| a.borrow().len()).unwrap_or(0)
}

fn get(o: &Obj, i: usize) -> Value {
    o.array()
        .and_then(|a| a.borrow().get(i).cloned())
        .unwrap_or_default()
}

fn check_frozen(vm: &mut Vm, o: &Obj) -> Result<(), Value> {
    let flags = o.flags.get();
    if flags & crate::object::ELEMENTS_FROZEN != 0 {
        Err(vm.type_error("Cannot assign to read only property '0' of object '[object Array]'"))
    } else if flags != 0 {
        let len = match &o.kind {
            ObjKind::Array(a) => a.borrow().len(),
            _ => 0,
        };
        Err(vm.type_error(format!(
            "Cannot add property {len}, object is not extensible"
        )))
    } else {
        Ok(())
    }
}

fn array_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if args.len() == 1 {
        if let Value::Number(n) = args[0] {
            if n < 0.0 || n.fract() != 0.0 || n > 4294967295.0 {
                return Err(vm.range_error("Invalid array length"));
            }
            return Ok(vm.new_array(vec![Value::Undefined; n as usize]));
        }
    }
    Ok(vm.new_array(args.to_vec()))
}

fn is_array(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut v = arg(args, 0);
    loop {
        if let Value::Object(o) = &v {
            if let ObjKind::Proxy(p) = &o.kind {
                let t = p.target.borrow().clone();
                match t {
                    Some(t) => {
                        v = Value::Object(t);
                        continue;
                    }
                    None => {
                        return Err(vm.type_error(
                            "Cannot perform 'IsArray' on a proxy that has been revoked",
                        ));
                    }
                }
            }
        }
        return Ok(Value::Bool(v.is_array()));
    }
}

fn from(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let src = arg(args, 0);
    let list = match &src {
        Value::Object(o)
            if matches!(o.kind, ObjKind::Ordinary)
                && o.get_chain(crate::object::ITER_KEY).is_none() =>
        {
            let n = vm.get_prop(&src, "length")?;
            let n = to_integer(vm, &n)?.max(0.0) as usize;
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                out.push(vm.get_prop(&src, &i.to_string())?);
            }
            out
        }
        Value::Undefined | Value::Null => {
            return Err(vm.type_error(format!("{} is not iterable", inspect_plain(&src))));
        }
        _ => vm.iterate(&src)?,
    };
    let f = arg(args, 1);
    if f.is_undefined() {
        return Ok(vm.new_array(list));
    }
    expect_fn(vm, &f)?;
    let mut out = Vec::with_capacity(list.len());
    for (i, v) in list.into_iter().enumerate() {
        out.push(vm.call_value(&f, Value::Undefined, &[v, Value::Number(i as f64)])?);
    }
    Ok(vm.new_array(out))
}

fn of(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(vm.new_array(args.to_vec()))
}

fn push(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "push")?;
    check_frozen(vm, &o)?;
    let mut a = items(&o);
    a.extend_from_slice(args);
    Ok(Value::Number(a.len() as f64))
}

fn pop(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "pop")?;
    check_frozen(vm, &o)?;
    Ok(items(&o).pop().unwrap_or_default())
}

fn shift(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "shift")?;
    check_frozen(vm, &o)?;
    let mut a = items(&o);
    if a.is_empty() {
        Ok(Value::Undefined)
    } else {
        Ok(a.remove(0))
    }
}

fn unshift(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "unshift")?;
    check_frozen(vm, &o)?;
    let mut a = items(&o);
    a.splice(0..0, args.iter().cloned());
    Ok(Value::Number(a.len() as f64))
}

fn range_args(
    vm: &mut Vm,
    args: &[Value],
    n: usize,
    start_i: usize,
) -> Result<(usize, usize), Value> {
    let start = match args.get(start_i) {
        Some(v) if !v.is_undefined() => relative_index(to_integer(vm, v)?, n),
        _ => 0,
    };
    let end = match args.get(start_i + 1) {
        Some(v) if !v.is_undefined() => relative_index(to_integer(vm, v)?, n),
        _ => n,
    };
    Ok((start, end))
}

fn slice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "slice")?;
    let n = len(&o);
    let (start, end) = range_args(vm, args, n, 0)?;
    let out = if start < end {
        items(&o)[start..end].to_vec()
    } else {
        Vec::new()
    };
    Ok(vm.new_array(out))
}

fn splice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "splice")?;
    check_frozen(vm, &o)?;
    let n = len(&o);
    let start = match args.first() {
        Some(v) => relative_index(to_integer(vm, v)?, n),
        None => n,
    };
    let delete = match args.get(1) {
        Some(v) if !v.is_undefined() => {
            to_integer(vm, v)?.max(0.0).min((n - start) as f64) as usize
        }
        Some(_) => 0,
        None => n - start,
    };
    let inserted: Vec<Value> = args.iter().skip(2).cloned().collect();
    let removed: Vec<Value> = items(&o).splice(start..start + delete, inserted).collect();
    Ok(vm.new_array(removed))
}

fn concat(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "concat")?;
    let mut out = items(&o).clone();
    for a in args {
        match a {
            Value::Object(x) if matches!(x.kind, ObjKind::Array(_)) => {
                out.extend(items(x).iter().cloned());
            }
            _ => out.push(a.clone()),
        }
    }
    Ok(vm.new_array(out))
}

pub fn join_values(vm: &mut Vm, list: &[Value], sep: &str) -> Result<String, Value> {
    let mut s = String::new();
    for (i, v) in list.iter().enumerate() {
        if i > 0 {
            s.push_str(sep);
        }
        if !v.is_nullish() {
            s.push_str(&vm.to_string(v)?);
        }
    }
    Ok(s)
}

fn join(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "join")?;
    let sep = match arg(args, 0) {
        Value::Undefined => Rc::from(","),
        v => vm.to_string(&v)?,
    };
    let list = items(&o).clone();
    Ok(join_values(vm, &list, &sep)?.into())
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) if matches!(o.kind, ObjKind::Array(_)) => {
            let list = items(o).clone();
            Ok(join_values(vm, &list, ",")?.into())
        }
        _ => Ok("[object Object]".into()),
    }
}

fn reverse(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "reverse")?;
    check_frozen(vm, &o)?;
    items(&o).reverse();
    Ok(this)
}

fn to_reversed(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "toReversed")?;
    let mut list = items(&o).clone();
    list.reverse();
    Ok(vm.new_array(list))
}

fn from_index(vm: &mut Vm, args: &[Value], n: usize) -> Result<usize, Value> {
    match args.get(1) {
        Some(v) if !v.is_undefined() => Ok(relative_index(to_integer(vm, v)?, n)),
        _ => Ok(0),
    }
}

fn index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "indexOf")?;
    let target = arg(args, 0);
    let start = from_index(vm, args, len(&o))?;
    let pos = items(&o)
        .iter()
        .skip(start)
        .position(|v| v.strict_eq(&target));
    Ok(Value::Number(
        pos.map(|p| (p + start) as f64).unwrap_or(-1.0),
    ))
}

fn last_index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "lastIndexOf")?;
    let target = arg(args, 0);
    let pos = items(&o).iter().rposition(|v| v.strict_eq(&target));
    Ok(Value::Number(pos.map(|p| p as f64).unwrap_or(-1.0)))
}

fn includes(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "includes")?;
    let target = arg(args, 0);
    let start = from_index(vm, args, len(&o))?;
    let found = items(&o)
        .iter()
        .skip(start)
        .any(|v| v.same_value_zero(&target));
    Ok(Value::Bool(found))
}

fn callback(
    vm: &mut Vm,
    this: &Value,
    args: &[Value],
    name: &str,
) -> Result<(Rc<Obj>, Value, Value), Value> {
    let o = this_array(vm, this, name)?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    Ok((o, f, arg(args, 1)))
}

fn call3(vm: &mut Vm, f: &Value, this_arg: &Value, v: Value, i: usize, arr: &Value) -> R {
    vm.call_value(
        f,
        this_arg.clone(),
        &[v, Value::Number(i as f64), arr.clone()],
    )
}

fn find(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "find")?;
    let mut i = 0;
    while i < len(&o) {
        let v = get(&o, i);
        if call3(vm, &f, &t, v.clone(), i, &this)?.truthy() {
            return Ok(v);
        }
        i += 1;
    }
    Ok(Value::Undefined)
}

fn find_index(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "findIndex")?;
    let mut i = 0;
    while i < len(&o) {
        let v = get(&o, i);
        if call3(vm, &f, &t, v, i, &this)?.truthy() {
            return Ok(Value::Number(i as f64));
        }
        i += 1;
    }
    Ok(Value::Number(-1.0))
}

fn find_last(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "findLast")?;
    let mut i = len(&o);
    while i > 0 {
        i -= 1;
        let v = get(&o, i);
        if call3(vm, &f, &t, v.clone(), i, &this)?.truthy() {
            return Ok(v);
        }
    }
    Ok(Value::Undefined)
}

fn find_last_index(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "findLastIndex")?;
    let mut i = len(&o);
    while i > 0 {
        i -= 1;
        let v = get(&o, i);
        if call3(vm, &f, &t, v, i, &this)?.truthy() {
            return Ok(Value::Number(i as f64));
        }
    }
    Ok(Value::Number(-1.0))
}

fn filter(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "filter")?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < len(&o) {
        let v = get(&o, i);
        if call3(vm, &f, &t, v.clone(), i, &this)?.truthy() {
            out.push(v);
        }
        i += 1;
    }
    Ok(vm.new_array(out))
}

fn map(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "map")?;
    let n = len(&o);
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n.min(len(&o)) {
        let v = get(&o, i);
        out.push(call3(vm, &f, &t, v, i, &this)?);
        i += 1;
    }
    Ok(vm.new_array(out))
}

fn for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "forEach")?;
    let mut i = 0;
    while i < len(&o) {
        let v = get(&o, i);
        call3(vm, &f, &t, v, i, &this)?;
        i += 1;
    }
    Ok(Value::Undefined)
}

fn some(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "some")?;
    let mut i = 0;
    while i < len(&o) {
        let v = get(&o, i);
        if call3(vm, &f, &t, v, i, &this)?.truthy() {
            return Ok(Value::Bool(true));
        }
        i += 1;
    }
    Ok(Value::Bool(false))
}

fn every(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "every")?;
    let mut i = 0;
    while i < len(&o) {
        let v = get(&o, i);
        if !call3(vm, &f, &t, v, i, &this)?.truthy() {
            return Ok(Value::Bool(false));
        }
        i += 1;
    }
    Ok(Value::Bool(true))
}

fn reduce(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "reduce")?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let mut i = 0;
    let mut acc = if args.len() >= 2 {
        args[1].clone()
    } else {
        if len(&o) == 0 {
            return Err(vm.type_error("Reduce of empty array with no initial value"));
        }
        i = 1;
        get(&o, 0)
    };
    while i < len(&o) {
        let v = get(&o, i);
        acc = vm.call_value(
            &f,
            Value::Undefined,
            &[acc, v, Value::Number(i as f64), this.clone()],
        )?;
        i += 1;
    }
    Ok(acc)
}

fn reduce_right(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "reduceRight")?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let mut i = len(&o);
    let mut acc = if args.len() >= 2 {
        args[1].clone()
    } else {
        if i == 0 {
            return Err(vm.type_error("Reduce of empty array with no initial value"));
        }
        i -= 1;
        get(&o, i)
    };
    while i > 0 {
        i -= 1;
        let v = get(&o, i);
        acc = vm.call_value(
            &f,
            Value::Undefined,
            &[acc, v, Value::Number(i as f64), this.clone()],
        )?;
    }
    Ok(acc)
}

fn compare_values(vm: &mut Vm, f: &Value, a: &Value, b: &Value) -> Result<Ordering, Value> {
    match (a.is_undefined(), b.is_undefined()) {
        (true, true) => return Ok(Ordering::Equal),
        (true, false) => return Ok(Ordering::Greater),
        (false, true) => return Ok(Ordering::Less),
        _ => {}
    }
    if f.is_undefined() {
        let x = vm.to_string(a)?;
        let y = vm.to_string(b)?;
        return Ok(crate::value::compare_strings(&x, &y));
    }
    let r = vm.call_value(f, Value::Undefined, &[a.clone(), b.clone()])?;
    let n = vm.to_number(&r)?;
    Ok(if n < 0.0 {
        Ordering::Less
    } else if n > 0.0 {
        Ordering::Greater
    } else {
        Ordering::Equal
    })
}

pub fn merge_sort(vm: &mut Vm, list: Vec<Value>, f: &Value) -> Result<Vec<Value>, Value> {
    if list.len() <= 1 {
        return Ok(list);
    }
    let mut src = list;
    let n = src.len();
    let mut width = 1;
    while width < n {
        let mut dst = Vec::with_capacity(n);
        let mut lo = 0;
        while lo < n {
            let mid = (lo + width).min(n);
            let hi = (lo + 2 * width).min(n);
            let (mut i, mut j) = (lo, mid);
            while i < mid && j < hi {
                if compare_values(vm, f, &src[j], &src[i])? == Ordering::Less {
                    dst.push(src[j].clone());
                    j += 1;
                } else {
                    dst.push(src[i].clone());
                    i += 1;
                }
            }
            dst.extend_from_slice(&src[i..mid]);
            dst.extend_from_slice(&src[j..hi]);
            lo = hi;
        }
        src = dst;
        width *= 2;
    }
    Ok(src)
}

fn sort(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "sort")?;
    check_frozen(vm, &o)?;
    let f = arg(args, 0);
    if !f.is_undefined() {
        expect_fn(vm, &f)?;
    }
    let list = items(&o).clone();
    let sorted = merge_sort(vm, list, &f)?;
    *items(&o) = sorted;
    Ok(this)
}

fn to_sorted(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "toSorted")?;
    let f = arg(args, 0);
    if !f.is_undefined() {
        expect_fn(vm, &f)?;
    }
    let list = items(&o).clone();
    let sorted = merge_sort(vm, list, &f)?;
    Ok(vm.new_array(sorted))
}

fn flatten(list: &[Value], depth: f64, out: &mut Vec<Value>) {
    for v in list {
        match v {
            Value::Object(o) if depth >= 1.0 && matches!(o.kind, ObjKind::Array(_)) => {
                let inner = items(o).clone();
                flatten(&inner, depth - 1.0, out);
            }
            _ => out.push(v.clone()),
        }
    }
}

fn flat(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "flat")?;
    let depth = match arg(args, 0) {
        Value::Undefined => 1.0,
        v => to_integer(vm, &v)?,
    };
    let list = items(&o).clone();
    let mut out = Vec::new();
    flatten(&list, depth, &mut out);
    Ok(vm.new_array(out))
}

fn flat_map(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "flatMap")?;
    let mut mapped = Vec::new();
    let mut i = 0;
    while i < len(&o) {
        let v = get(&o, i);
        mapped.push(call3(vm, &f, &t, v, i, &this)?);
        i += 1;
    }
    let mut out = Vec::new();
    flatten(&mapped, 1.0, &mut out);
    Ok(vm.new_array(out))
}

fn fill(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "fill")?;
    check_frozen(vm, &o)?;
    let n = len(&o);
    let (start, end) = range_args(vm, args, n, 1)?;
    let v = arg(args, 0);
    let mut a = items(&o);
    for slot in a.iter_mut().take(end).skip(start) {
        *slot = v.clone();
    }
    drop(a);
    Ok(this)
}

fn at(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "at")?;
    let n = len(&o) as f64;
    let i = to_integer(vm, &arg(args, 0))?;
    let i = if i < 0.0 { n + i } else { i };
    if i < 0.0 || i >= n {
        return Ok(Value::Undefined);
    }
    Ok(get(&o, i as usize))
}

fn keys(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "keys")?;
    let list = (0..len(&o)).map(|i| Value::Number(i as f64)).collect();
    Ok(vm.new_iter(IterState::List(list, 0)))
}

fn values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "values")?;
    Ok(vm.new_iter(IterState::Array(o, 0)))
}

fn entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "entries")?;
    let list = items(&o).clone();
    let pairs = list
        .into_iter()
        .enumerate()
        .map(|(i, v)| vm.new_array(vec![Value::Number(i as f64), v]))
        .collect();
    Ok(vm.new_iter(IterState::List(pairs, 0)))
}
