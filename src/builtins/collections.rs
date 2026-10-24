use super::{arg, expect_fn, make_ctor, method};
use crate::inspect::inspect_plain;
use crate::object::{IterState, MapKey, Obj, ObjKind, WeakHold};
use crate::value::Value;
use crate::vm::Vm;
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let mp = vm.realm.map_proto.clone();
    let map_methods: &[(&str, u32, crate::object::NativeFn)] = &[
        ("get", 1, map_get),
        ("set", 2, map_set),
        ("has", 1, map_has),
        ("delete", 1, map_delete),
        ("clear", 0, map_clear),
        ("forEach", 1, map_for_each),
        ("keys", 0, map_keys),
        ("values", 0, map_values),
        ("entries", 0, map_entries),
    ];
    for (name, arity, f) in map_methods {
        method(vm, &mp, name, *arity, *f);
    }
    let map = make_ctor(
        vm,
        "Map",
        0,
        map_ctor,
        Some(|| ObjKind::Map(Box::new(RefCell::new(Default::default())))),
        &mp,
    );
    vm.define_global("Map", Value::Object(map));

    let sp = vm.realm.set_proto.clone();
    let set_methods: &[(&str, u32, crate::object::NativeFn)] = &[
        ("add", 1, set_add),
        ("has", 1, set_has),
        ("delete", 1, set_delete),
        ("clear", 0, set_clear),
        ("forEach", 1, set_for_each),
        ("values", 0, set_values),
        ("keys", 0, set_values),
        ("entries", 0, set_entries),
    ];
    for (name, arity, f) in set_methods {
        method(vm, &sp, name, *arity, *f);
    }
    let set = make_ctor(
        vm,
        "Set",
        0,
        set_ctor,
        Some(|| ObjKind::Set(Box::new(RefCell::new(Default::default())))),
        &sp,
    );
    vm.define_global("Set", Value::Object(set));

    let wm = vm.realm.weakmap_proto.clone();
    method(vm, &wm, "get", 1, weakmap_get);
    method(vm, &wm, "set", 2, weakmap_set);
    method(vm, &wm, "has", 1, weakmap_has);
    method(vm, &wm, "delete", 1, weakmap_delete);
    let weakmap = make_ctor(
        vm,
        "WeakMap",
        0,
        weakmap_ctor,
        Some(|| ObjKind::WeakMap(Box::default())),
        &wm,
    );
    vm.define_global("WeakMap", Value::Object(weakmap));

    let ws = vm.realm.weakset_proto.clone();
    method(vm, &ws, "add", 1, weakset_add);
    method(vm, &ws, "has", 1, weakset_has);
    method(vm, &ws, "delete", 1, weakset_delete);
    let weakset = make_ctor(
        vm,
        "WeakSet",
        0,
        weakset_ctor,
        Some(|| ObjKind::WeakSet(Box::default())),
        &ws,
    );
    vm.define_global("WeakSet", Value::Object(weakset));

    let wr = vm.realm.weakref_proto.clone();
    method(vm, &wr, "deref", 0, weakref_deref);
    let weakref = make_ctor(vm, "WeakRef", 1, weakref_ctor, None, &wr);
    vm.define_global("WeakRef", Value::Object(weakref));

    let protos: [(Rc<Obj>, &str, crate::object::NativeFn); 4] = [
        (
            vm.realm.iter_proto.clone(),
            "Array Iterator",
            iter_next::<0>,
        ),
        (
            vm.realm.map_iter_proto.clone(),
            "Map Iterator",
            iter_next::<1>,
        ),
        (
            vm.realm.set_iter_proto.clone(),
            "Set Iterator",
            iter_next::<2>,
        ),
        (
            vm.realm.string_iter_proto.clone(),
            "String Iterator",
            iter_next::<3>,
        ),
    ];
    for (p, tag, f) in protos {
        method(vm, &p, "next", 0, f);
        p.set_with_attrs(
            crate::object::TO_STRING_TAG_KEY,
            tag.into(),
            crate::object::NON_ENUM | crate::object::READ_ONLY,
        );
    }
    let sp = vm.realm.set_proto.clone();
    if let Some(v) = sp.get_own("values") {
        sp.set_with_attrs("keys", v, crate::object::NON_ENUM);
    }
}

pub fn receiver_desc(v: &Value) -> String {
    match v {
        Value::Object(o) => format!(
            "#<{}>",
            crate::inspect::class_name(o).unwrap_or_else(|| "Object".into())
        ),
        other => inspect_plain(other),
    }
}

fn this_map(vm: &mut Vm, this: &Value, method: &str) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Map(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method Map.prototype{method} called on incompatible receiver {}",
            receiver_desc(this)
        ))),
    }
}

fn this_set(vm: &mut Vm, this: &Value, method: &str) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Set(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method Set.prototype{method} called on incompatible receiver {}",
            receiver_desc(this)
        ))),
    }
}

macro_rules! map_of {
    ($o:expr) => {
        match &$o.kind {
            ObjKind::Map(m) => m,
            _ => unreachable!(),
        }
    };
}

macro_rules! set_of {
    ($o:expr) => {
        match &$o.kind {
            ObjKind::Set(s) => s,
            _ => unreachable!(),
        }
    };
}

fn map_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Ok(o) = this_map(vm, &this, "") else {
        return Err(vm.type_error("Constructor Map requires 'new'"));
    };
    let src = arg(args, 0);
    if src.is_nullish() {
        return Ok(this);
    }
    let adder = vm.get_prop(&this, "set")?;
    if !adder.is_callable() {
        return Err(
            vm.type_error("'set' returned for property 'set' of object '#<Map>' is not callable")
        );
    }
    let pristine = vm
        .pristine
        .as_ref()
        .is_some_and(|p| adder.strict_eq(&p.map_set));
    let it = vm.get_iterator(&src)?;
    while let Some(entry) = vm.iter_next(&it)? {
        if let Err(e) = map_entry(vm, &o, &this, &adder, pristine, &entry) {
            let _ = vm.iter_close(&it);
            return Err(e);
        }
    }
    Ok(this)
}

fn map_entry(
    vm: &mut Vm,
    o: &Rc<Obj>,
    this: &Value,
    adder: &Value,
    pristine: bool,
    entry: &Value,
) -> Result<(), Value> {
    if !entry.is_object() {
        return Err(vm.type_error(format!(
            "Iterator value {} is not an entry object",
            inspect_plain(entry)
        )));
    }
    let k = vm.get_prop(entry, "0")?;
    let v = vm.get_prop(entry, "1")?;
    if pristine {
        map_of!(o).borrow_mut().insert(MapKey::from_value(&k), v);
    } else {
        vm.call_value(adder, this.clone(), &[k, v])?;
    }
    Ok(())
}

fn map_get(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this, ".get")?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(map_of!(o).borrow().get(&k).cloned().unwrap_or_default())
}

fn map_set(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this, ".set")?;
    let k = MapKey::from_value(&arg(args, 0));
    map_of!(o).borrow_mut().insert(k, arg(args, 1));
    Ok(this)
}

fn map_has(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this, ".has")?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(Value::Bool(map_of!(o).borrow().contains_key(&k)))
}

fn map_delete(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this, ".delete")?;
    let k = MapKey::from_value(&arg(args, 0));
    let removed = map_of!(o)
        .borrow_mut()
        .shift_remove_full(&k)
        .map(|(i, _, _)| i);
    if let Some(i) = removed {
        crate::object::collection_removed(&o, i);
    }
    Ok(Value::Bool(removed.is_some()))
}

fn map_clear(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this, ".clear")?;
    map_of!(o).borrow_mut().clear();
    crate::object::collection_cleared(&o);
    Ok(Value::Undefined)
}

fn map_for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this, ".forEach")?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let cursor = crate::object::collection_cursor(&o);
    loop {
        let entry = {
            let m = map_of!(o).borrow();
            m.get_index(cursor.get())
                .map(|(k, v)| (k.to_value(), v.clone()))
        };
        let Some((k, v)) = entry else {
            break;
        };
        cursor.set(cursor.get() + 1);
        vm.call_value(&f, arg(args, 1), &[v, k, this.clone()])?;
    }
    Ok(Value::Undefined)
}

fn map_keys(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this, ".keys")?;
    {
        let c = crate::object::collection_cursor(&o);
        Ok(vm.new_iter(IterState::MapKeys(o, c)))
    }
}

fn map_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this, ".values")?;
    {
        let c = crate::object::collection_cursor(&o);
        Ok(vm.new_iter(IterState::MapValues(o, c)))
    }
}

fn map_entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this, ".entries")?;
    {
        let c = crate::object::collection_cursor(&o);
        Ok(vm.new_iter(IterState::MapEntries(o, c)))
    }
}

fn set_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Ok(o) = this_set(vm, &this, "") else {
        return Err(vm.type_error("Constructor Set requires 'new'"));
    };
    let src = arg(args, 0);
    if src.is_nullish() {
        return Ok(this);
    }
    let adder = vm.get_prop(&this, "add")?;
    if !adder.is_callable() {
        return Err(
            vm.type_error("'add' returned for property 'add' of object '#<Set>' is not callable")
        );
    }
    if vm
        .pristine
        .as_ref()
        .is_some_and(|p| adder.strict_eq(&p.set_add))
    {
        for v in vm.iterate(&src)? {
            set_of!(o).borrow_mut().insert(MapKey::from_value(&v));
        }
        return Ok(this);
    }
    let it = vm.get_iterator(&src)?;
    while let Some(v) = vm.iter_next(&it)? {
        if let Err(e) = vm.call_value(&adder, this.clone(), &[v]) {
            let _ = vm.iter_close(&it);
            return Err(e);
        }
    }
    Ok(this)
}

fn set_add(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this, ".add")?;
    set_of!(o)
        .borrow_mut()
        .insert(MapKey::from_value(&arg(args, 0)));
    Ok(this)
}

fn set_has(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this, ".has")?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(Value::Bool(set_of!(o).borrow().contains(&k)))
}

fn set_delete(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this, ".delete")?;
    let k = MapKey::from_value(&arg(args, 0));
    let removed = set_of!(o)
        .borrow_mut()
        .shift_remove_full(&k)
        .map(|(i, _)| i);
    if let Some(i) = removed {
        crate::object::collection_removed(&o, i);
    }
    Ok(Value::Bool(removed.is_some()))
}

fn set_clear(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_set(vm, &this, ".clear")?;
    set_of!(o).borrow_mut().clear();
    crate::object::collection_cleared(&o);
    Ok(Value::Undefined)
}

fn set_for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this, ".forEach")?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let cursor = crate::object::collection_cursor(&o);
    loop {
        let v = set_of!(o)
            .borrow()
            .get_index(cursor.get())
            .map(|k| k.to_value());
        let Some(v) = v else {
            break;
        };
        cursor.set(cursor.get() + 1);
        vm.call_value(&f, arg(args, 1), &[v.clone(), v, this.clone()])?;
    }
    Ok(Value::Undefined)
}

fn set_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_set(vm, &this, ".values")?;
    {
        let c = crate::object::collection_cursor(&o);
        Ok(vm.new_iter(IterState::SetValues(o, c)))
    }
}

fn set_entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_set(vm, &this, ".entries")?;
    {
        let c = crate::object::collection_cursor(&o);
        Ok(vm.new_iter(IterState::SetEntries(o, c)))
    }
}

fn iter_next<const F: usize>(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let ok = match &this {
        Value::Object(o) => match &o.kind {
            ObjKind::Iter(st) => st.borrow().family() == F,
            _ => false,
        },
        _ => false,
    };
    if !ok {
        let what = ["Array", "Map", "Set", "String"][F];
        return Err(vm.type_error(format!(
            "Method %{what}IteratorPrototype%.next called on incompatible receiver {}",
            inspect_plain(&this)
        )));
    }
    let next = vm.iter_next(&this)?;
    let o = vm.new_object();
    let done = next.is_none();
    o.set_own("value", next.unwrap_or_default());
    o.set_own("done", Value::Bool(done));
    Ok(Value::Object(o))
}

fn weak_ptr(vm: &Vm, v: &Value) -> Option<(usize, WeakHold)> {
    match v {
        Value::Object(o) => Some((Rc::as_ptr(o) as usize, WeakHold::Obj(Rc::downgrade(o)))),
        Value::Symbol(s) => {
            let registered = vm
                .symbol_registry
                .values()
                .any(|r| matches!(r, Value::Symbol(x) if Rc::ptr_eq(x, s)));
            if registered {
                None
            } else {
                Some((Rc::as_ptr(s) as usize, WeakHold::Sym(s.clone())))
            }
        }
        _ => None,
    }
}

fn weak_key(vm: &mut Vm, v: &Value, what: &str) -> Result<(usize, WeakHold), Value> {
    weak_ptr(vm, v).ok_or_else(|| vm.type_error(format!("Invalid value used {what}")))
}

fn this_weakmap(vm: &mut Vm, this: &Value) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::WeakMap(_)) => Ok(o.clone()),
        _ => Err(vm.type_error("Method WeakMap.prototype called on incompatible receiver")),
    }
}

fn this_weakset(vm: &mut Vm, this: &Value) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::WeakSet(_)) => Ok(o.clone()),
        _ => Err(vm.type_error("Method WeakSet.prototype called on incompatible receiver")),
    }
}

fn weakmap_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Ok(_) = this_weakmap(vm, &this) else {
        return Err(vm.type_error("Constructor WeakMap requires 'new'"));
    };
    let src = arg(args, 0);
    if !src.is_nullish() {
        for entry in vm.iterate(&src)? {
            let k = vm.get_prop(&entry, "0")?;
            let v = vm.get_prop(&entry, "1")?;
            weakmap_set(vm, this.clone(), &[k, v])?;
        }
    }
    Ok(this)
}

fn weakmap_get(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let m = this_weakmap(vm, &this)?;
    let Some((ptr, _)) = weak_ptr(vm, &arg(args, 0)) else {
        return Ok(Value::Undefined);
    };
    let ObjKind::WeakMap(map) = &m.kind else {
        unreachable!()
    };
    let map = map.borrow();
    Ok(match map.get(&ptr) {
        Some((w, v)) if w.alive() => v.clone(),
        _ => Value::Undefined,
    })
}

fn weakmap_set(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let m = this_weakmap(vm, &this)?;
    let (ptr, hold) = weak_key(vm, &arg(args, 0), "as weak map key")?;
    let ObjKind::WeakMap(map) = &m.kind else {
        unreachable!()
    };
    let mut map = map.borrow_mut();
    if map.len() >= 64 && map.len().is_power_of_two() {
        map.retain(|_, (w, _)| w.alive());
    }
    map.insert(ptr, (hold, arg(args, 1)));
    drop(map);
    Ok(this)
}

fn weakmap_has(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let m = this_weakmap(vm, &this)?;
    let Some((ptr, _)) = weak_ptr(vm, &arg(args, 0)) else {
        return Ok(Value::Bool(false));
    };
    let ObjKind::WeakMap(map) = &m.kind else {
        unreachable!()
    };
    let found = map.borrow().get(&ptr).is_some_and(|(w, _)| w.alive());
    Ok(Value::Bool(found))
}

fn weakmap_delete(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let m = this_weakmap(vm, &this)?;
    let Some((ptr, _)) = weak_ptr(vm, &arg(args, 0)) else {
        return Ok(Value::Bool(false));
    };
    let ObjKind::WeakMap(map) = &m.kind else {
        unreachable!()
    };
    let removed = map.borrow_mut().remove(&ptr);
    Ok(Value::Bool(removed.is_some_and(|(w, _)| w.alive())))
}

fn weakset_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Ok(_) = this_weakset(vm, &this) else {
        return Err(vm.type_error("Constructor WeakSet requires 'new'"));
    };
    let src = arg(args, 0);
    if !src.is_nullish() {
        for v in vm.iterate(&src)? {
            weakset_add(vm, this.clone(), &[v])?;
        }
    }
    Ok(this)
}

fn weakset_add(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_weakset(vm, &this)?;
    let (ptr, hold) = weak_key(vm, &arg(args, 0), "in weak set")?;
    let ObjKind::WeakSet(set) = &s.kind else {
        unreachable!()
    };
    let mut set = set.borrow_mut();
    if set.len() >= 64 && set.len().is_power_of_two() {
        set.retain(|_, w| w.alive());
    }
    set.insert(ptr, hold);
    drop(set);
    Ok(this)
}

fn weakset_has(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_weakset(vm, &this)?;
    let Some((ptr, _)) = weak_ptr(vm, &arg(args, 0)) else {
        return Ok(Value::Bool(false));
    };
    let ObjKind::WeakSet(set) = &s.kind else {
        unreachable!()
    };
    let found = set.borrow().get(&ptr).is_some_and(|w| w.alive());
    Ok(Value::Bool(found))
}

fn weakset_delete(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_weakset(vm, &this)?;
    let Some((ptr, _)) = weak_ptr(vm, &arg(args, 0)) else {
        return Ok(Value::Bool(false));
    };
    let ObjKind::WeakSet(set) = &s.kind else {
        unreachable!()
    };
    let removed = set.borrow_mut().remove(&ptr);
    Ok(Value::Bool(removed.is_some_and(|w| w.alive())))
}

fn weakref_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if this.is_undefined() && args.is_empty() {
        return Err(vm.type_error("WeakRef: target must be an object"));
    }
    let target = match arg(args, 0) {
        Value::Object(o) => o,
        v => {
            return Err(vm.type_error(format!(
                "Invalid value used as WeakRef target: {}",
                inspect_plain(&v)
            )));
        }
    };
    let weak = Rc::downgrade(&target);
    vm.kept_alive.insert(Rc::as_ptr(&target) as usize, target);
    Ok(Value::Object(Obj::new(
        ObjKind::WeakRef(weak),
        Some(vm.realm.weakref_proto.clone()),
    )))
}

fn weakref_deref(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) => match &o.kind {
            ObjKind::WeakRef(w) => match w.upgrade() {
                Some(target) => {
                    vm.kept_alive
                        .entry(Rc::as_ptr(&target) as usize)
                        .or_insert_with(|| target.clone());
                    Ok(Value::Object(target))
                }
                None => Ok(Value::Undefined),
            },
            _ => Err(vm.type_error("WeakRef.prototype.deref called on incompatible receiver")),
        },
        _ => Err(vm.type_error("WeakRef.prototype.deref called on incompatible receiver")),
    }
}
