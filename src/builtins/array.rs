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
    let fns: &[crate::object::NativeFn] = dispatch_table!(
        0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33
        34 35 36 37
    );
    for (i, (name, arity, _, _)) in METHODS.iter().enumerate() {
        method(vm, &ap, name, *arity, fns[i]);
    }
    let array = make_ctor(vm, "Array", 1, array_ctor, None, &ap);
    method(vm, &array, "isArray", 1, is_array);
    method(vm, &array, "from", 1, from);
    method(vm, &array, "of", 0, of);
    vm.define_global("Array", Value::Object(array));
}

const METHODS: &[(&str, u32, crate::object::NativeFn, u8)] = &[
    ("push", 1, push, 0),
    ("pop", 0, pop, 0),
    ("shift", 0, shift, 0),
    ("unshift", 1, unshift, 0),
    ("slice", 2, slice, 1),
    ("splice", 2, splice, 1),
    ("concat", 1, concat, 1),
    ("join", 1, join, 0),
    ("reverse", 0, reverse, 0),
    ("indexOf", 1, index_of, 0),
    ("lastIndexOf", 1, last_index_of, 0),
    ("includes", 1, includes, 0),
    ("find", 1, find, 0),
    ("findIndex", 1, find_index, 0),
    ("findLast", 1, find_last, 0),
    ("findLastIndex", 1, find_last_index, 0),
    ("filter", 1, filter, 1),
    ("map", 1, map, 1),
    ("forEach", 1, for_each, 0),
    ("reduce", 1, reduce, 0),
    ("reduceRight", 1, reduce_right, 0),
    ("some", 1, some, 0),
    ("every", 1, every, 0),
    ("sort", 1, sort, 0),
    ("toSorted", 1, to_sorted, 0),
    ("toReversed", 0, to_reversed, 0),
    ("flat", 0, flat, 1),
    ("flatMap", 1, flat_map, 1),
    ("fill", 1, fill, 0),
    ("at", 1, at, 0),
    ("keys", 0, keys, 2),
    ("values", 0, values, 2),
    ("entries", 0, entries, 2),
    ("toString", 0, to_string, 0),
    ("copyWithin", 2, copy_within, 0),
    ("toSpliced", 2, to_spliced, 0),
    ("with", 2, with, 0),
    ("toLocaleString", 0, to_locale_string, 0),
];

macro_rules! dispatch_table {
    ($($i:literal)*) => {
        &[$(dispatch::<$i> as crate::object::NativeFn),*]
    };
}
use dispatch_table;

fn dispatch<const I: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (name, _, f, mode) = METHODS[I];
    if mode == 2 {
        return f(vm, this, args);
    }
    if let Value::Object(o) = &this {
        if matches!(o.kind, ObjKind::Array(_))
            && o.flags.get() & crate::object::SLOW_ELEMENTS == 0
            && !vm.proto_indices
            && (mode == 0 || species_intact(vm, o))
        {
            return f(vm, this, args);
        }
    }
    generic(vm, name, this, args)
}

fn species_intact(vm: &Vm, o: &Obj) -> bool {
    let Some(ctor) = &vm.array_ctor else {
        return true;
    };
    let plain = o
        .proto
        .borrow()
        .as_ref()
        .is_some_and(|p| Rc::ptr_eq(p, &vm.realm.array_proto));
    if !plain || o.props.borrow().contains_key("constructor") {
        return false;
    }
    let same_ctor = matches!(
        vm.realm.array_proto.props.borrow().get("constructor"),
        Some(Value::Object(c)) if Rc::ptr_eq(c, ctor)
    );
    same_ctor
        && ctor
            .props
            .borrow()
            .get(crate::object::SPECIES_KEY)
            .is_some_and(|v| v.strict_eq(&vm.array_species))
}

fn generic(vm: &mut Vm, name: &str, this: Value, args: &[Value]) -> R {
    let g = match vm.globals.get("%ArrayGeneric%").cloned() {
        Some(g) => g,
        None => {
            let dir = std::env::current_dir().unwrap_or_default();
            let proto = vm
                .compile(
                    super::extras::ARRAY_GENERIC,
                    "<prelude>",
                    dir,
                    crate::compiler::Mode::Module,
                )
                .map_err(|e| vm.syntax_error_value(&e, "<prelude>"))?;
            vm.run_proto(proto)?;
            vm.globals
                .get("%ArrayGeneric%")
                .cloned()
                .unwrap_or_default()
        }
    };
    let f = vm.get_prop(&g, name)?;
    vm.call_value(&f, this, args)
}

fn copy_within(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    generic(vm, "copyWithin", this, args)
}

fn to_spliced(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    generic(vm, "toSpliced", this, args)
}

fn with(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    generic(vm, "with", this, args)
}

fn to_locale_string(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    generic(vm, "toLocaleString", this, args)
}

pub fn capture_species(vm: &mut Vm) {
    if let Some(Value::Object(a)) = vm.globals.get("Array").cloned() {
        vm.array_species = a
            .props
            .borrow()
            .get(crate::object::SPECIES_KEY)
            .cloned()
            .unwrap_or_default();
        vm.array_ctor = Some(a);
    }
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

fn present(o: &Obj, i: usize) -> bool {
    i < len(o) && !o.is_hole(i)
}

fn get(o: &Obj, i: usize) -> Value {
    o.array()
        .and_then(|a| a.borrow().get(i).cloned())
        .unwrap_or_default()
}

fn check_frozen(vm: &mut Vm, o: &Obj) -> Result<(), Value> {
    let flags = o.flags.get() & !crate::object::HAS_HOLES;
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
            if n > 50_000_000.0 {
                return Err(vm.range_error("Array length too large"));
            }
            let a = vm.new_array(vec![Value::Undefined; n as usize]);
            if let Value::Object(o) = &a {
                o.mark_holes(0, n as usize);
            }
            return Ok(a);
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

fn subclass_ctor(vm: &Vm, this: &Value) -> bool {
    match (this, &vm.array_ctor) {
        (Value::Object(o), Some(a)) => !Rc::ptr_eq(o, a) && vm.is_constructor(this),
        _ => false,
    }
}

fn from(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if subclass_ctor(vm, &this) {
        return generic(vm, "from", this, args);
    }
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

fn of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if subclass_ctor(vm, &this) {
        return generic(vm, "of", this, args);
    }
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
    let v = items(&o).pop().unwrap_or_default();
    let n = len(&o);
    o.truncate_holes(n);
    Ok(v)
}

fn shift(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_array(vm, &this, "shift")?;
    check_frozen(vm, &o)?;
    o.clear_holes();
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
    o.clear_holes();
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
    let result = vm.new_array(out);
    if o.has_holes() && start < end {
        if let Value::Object(r) = &result {
            for i in start..end {
                if o.is_hole(i) {
                    r.mark_holes(i - start, i - start + 1);
                }
            }
        }
    }
    Ok(result)
}

fn splice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "splice")?;
    check_frozen(vm, &o)?;
    o.clear_holes();
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

fn plain_spread(v: &Value) -> bool {
    match v {
        Value::Object(x) => {
            matches!(x.kind, ObjKind::Array(_))
                && !x.has_holes()
                && !x
                    .props
                    .borrow()
                    .contains_key(crate::object::IS_CONCAT_SPREADABLE_KEY)
        }
        _ => true,
    }
}

fn concat(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let key = crate::object::IS_CONCAT_SPREADABLE_KEY;
    let tagged = vm.realm.array_proto.get_chain(key).is_some();
    if tagged || !plain_spread(&this) || !args.iter().all(plain_spread) {
        return generic(vm, "concat", this, args);
    }
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
            crate::value::push16(&mut s, sep);
        }
        if !v.is_nullish() {
            let piece = vm.to_string(v)?;
            crate::value::push16(&mut s, &piece);
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
    o.clear_holes();
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
        .enumerate()
        .skip(start)
        .position(|(i, v)| v.strict_eq(&target) && !o.is_hole(i));
    Ok(Value::Number(
        pos.map(|p| (p + start) as f64).unwrap_or(-1.0),
    ))
}

fn last_index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "lastIndexOf")?;
    let target = arg(args, 0);
    let n = len(&o) as f64;
    if n == 0.0 {
        return Ok(Value::Number(-1.0));
    }
    let from = match args.get(1) {
        Some(v) => to_integer(vm, v)?,
        None => n - 1.0,
    };
    let k = if from >= 0.0 {
        from.min(n - 1.0)
    } else {
        n + from
    };
    if k < 0.0 {
        return Ok(Value::Number(-1.0));
    }
    let list = items(&o);
    let end = (k as usize + 1).min(list.len());
    let pos = list[..end]
        .iter()
        .enumerate()
        .rposition(|(i, v)| v.strict_eq(&target) && !o.is_hole(i));
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
    let n = len(&o);
    for i in 0..n {
        let v = get(&o, i);
        if call3(vm, &f, &t, v.clone(), i, &this)?.truthy() {
            return Ok(v);
        }
    }
    Ok(Value::Undefined)
}

fn find_index(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "findIndex")?;
    let n = len(&o);
    for i in 0..n {
        let v = get(&o, i);
        if call3(vm, &f, &t, v, i, &this)?.truthy() {
            return Ok(Value::Number(i as f64));
        }
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
    let n = len(&o);
    for i in 0..n {
        if !present(&o, i) {
            continue;
        }
        let v = get(&o, i);
        if call3(vm, &f, &t, v.clone(), i, &this)?.truthy() {
            out.push(v);
        }
    }
    Ok(vm.new_array(out))
}

fn map(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "map")?;
    let n = len(&o);
    let mut out = Vec::with_capacity(n);
    let mut holes = Vec::new();
    for i in 0..n {
        if !present(&o, i) {
            out.push(Value::Undefined);
            holes.push(i);
            continue;
        }
        let v = get(&o, i);
        out.push(call3(vm, &f, &t, v, i, &this)?);
    }
    let result = vm.new_array(out);
    if let Value::Object(r) = &result {
        for h in holes {
            r.mark_holes(h, h + 1);
        }
    }
    Ok(result)
}

fn for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "forEach")?;
    let n = len(&o);
    for i in 0..n {
        if present(&o, i) {
            let v = get(&o, i);
            call3(vm, &f, &t, v, i, &this)?;
        }
    }
    Ok(Value::Undefined)
}

fn some(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "some")?;
    let n = len(&o);
    for i in 0..n {
        if present(&o, i) {
            let v = get(&o, i);
            if call3(vm, &f, &t, v, i, &this)?.truthy() {
                return Ok(Value::Bool(true));
            }
        }
    }
    Ok(Value::Bool(false))
}

fn every(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, f, t) = callback(vm, &this, args, "every")?;
    let n = len(&o);
    for i in 0..n {
        if present(&o, i) {
            let v = get(&o, i);
            if !call3(vm, &f, &t, v, i, &this)?.truthy() {
                return Ok(Value::Bool(false));
            }
        }
    }
    Ok(Value::Bool(true))
}

fn reduce(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_array(vm, &this, "reduce")?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let n = len(&o);
    let mut i = 0;
    let mut acc = if args.len() >= 2 {
        args[1].clone()
    } else {
        while i < n && !present(&o, i) {
            i += 1;
        }
        if i >= n {
            return Err(vm.type_error("Reduce of empty array with no initial value"));
        }
        i += 1;
        get(&o, i - 1)
    };
    while i < n {
        if present(&o, i) {
            let v = get(&o, i);
            acc = vm.call_value(
                &f,
                Value::Undefined,
                &[acc, v, Value::Number(i as f64), this.clone()],
            )?;
        }
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
        while i > 0 && !present(&o, i - 1) {
            i -= 1;
        }
        if i == 0 {
            return Err(vm.type_error("Reduce of empty array with no initial value"));
        }
        i -= 1;
        get(&o, i)
    };
    while i > 0 {
        i -= 1;
        if present(&o, i) {
            let v = get(&o, i);
            acc = vm.call_value(
                &f,
                Value::Undefined,
                &[acc, v, Value::Number(i as f64), this.clone()],
            )?;
        }
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
    let all = items(&o).clone();
    let total = all.len();
    let list: Vec<Value> = all
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !o.is_hole(*i))
        .map(|(_, v)| v)
        .collect();
    let kept = list.len();
    let mut sorted = merge_sort(vm, list, &f)?;
    sorted.resize(total, Value::Undefined);
    *items(&o) = sorted;
    o.clear_holes();
    o.mark_holes(kept, total);
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
    o.clear_holes();
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
    let o = vm.to_object(&this)?;
    Ok(vm.new_iter(IterState::Array(o, 0, 0)))
}

fn values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = vm.to_object(&this)?;
    Ok(vm.new_iter(IterState::Array(o, 0, 1)))
}

fn entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = vm.to_object(&this)?;
    Ok(vm.new_iter(IterState::Array(o, 0, 2)))
}
