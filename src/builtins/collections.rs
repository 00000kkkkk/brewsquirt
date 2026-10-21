use super::{arg, expect_fn, make_ctor, method};
use crate::inspect::inspect_plain;
use crate::object::{IterState, MapKey, Obj, ObjKind};
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

    let ip = vm.realm.iter_proto.clone();
    method(vm, &ip, "next", 0, iter_next);
    method(vm, &ip, "toArray", 0, iter_to_array);
}

fn this_map(vm: &mut Vm, this: &Value) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Map(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method Map.prototype called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn this_set(vm: &mut Vm, this: &Value) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Set(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method Set.prototype called on incompatible receiver {}",
            inspect_plain(this)
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
    let Ok(o) = this_map(vm, &this) else {
        return Err(vm.type_error("Constructor Map requires 'new'"));
    };
    let src = arg(args, 0);
    if !src.is_nullish() {
        for entry in vm.iterate(&src)? {
            if !entry.is_object() {
                return Err(vm.type_error(format!(
                    "Iterator value {} is not an entry object",
                    inspect_plain(&entry)
                )));
            }
            let k = vm.get_prop(&entry, "0")?;
            let v = vm.get_prop(&entry, "1")?;
            map_of!(o).borrow_mut().insert(MapKey::from_value(&k), v);
        }
    }
    Ok(this)
}

fn map_get(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(map_of!(o).borrow().get(&k).cloned().unwrap_or_default())
}

fn map_set(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    let k = MapKey::from_value(&arg(args, 0));
    map_of!(o).borrow_mut().insert(k, arg(args, 1));
    Ok(this)
}

fn map_has(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(Value::Bool(map_of!(o).borrow().contains_key(&k)))
}

fn map_delete(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(Value::Bool(
        map_of!(o).borrow_mut().shift_remove(&k).is_some(),
    ))
}

fn map_clear(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    map_of!(o).borrow_mut().clear();
    Ok(Value::Undefined)
}

fn map_for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let mut i = 0;
    loop {
        let entry = {
            let m = map_of!(o).borrow();
            m.get_index(i).map(|(k, v)| (k.to_value(), v.clone()))
        };
        let Some((k, v)) = entry else {
            break;
        };
        vm.call_value(&f, arg(args, 1), &[v, k, this.clone()])?;
        i += 1;
    }
    Ok(Value::Undefined)
}

fn map_keys(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    Ok(vm.new_iter(IterState::MapKeys(o, 0)))
}

fn map_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    Ok(vm.new_iter(IterState::MapValues(o, 0)))
}

fn map_entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_map(vm, &this)?;
    Ok(vm.new_iter(IterState::MapEntries(o, 0)))
}

fn set_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Ok(o) = this_set(vm, &this) else {
        return Err(vm.type_error("Constructor Set requires 'new'"));
    };
    let src = arg(args, 0);
    if !src.is_nullish() {
        for v in vm.iterate(&src)? {
            set_of!(o).borrow_mut().insert(MapKey::from_value(&v));
        }
    }
    Ok(this)
}

fn set_add(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this)?;
    set_of!(o)
        .borrow_mut()
        .insert(MapKey::from_value(&arg(args, 0)));
    Ok(this)
}

fn set_has(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this)?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(Value::Bool(set_of!(o).borrow().contains(&k)))
}

fn set_delete(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this)?;
    let k = MapKey::from_value(&arg(args, 0));
    Ok(Value::Bool(set_of!(o).borrow_mut().shift_remove(&k)))
}

fn set_clear(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_set(vm, &this)?;
    set_of!(o).borrow_mut().clear();
    Ok(Value::Undefined)
}

fn set_for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_set(vm, &this)?;
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let mut i = 0;
    loop {
        let v = set_of!(o).borrow().get_index(i).map(|k| k.to_value());
        let Some(v) = v else {
            break;
        };
        vm.call_value(&f, arg(args, 1), &[v.clone(), v, this.clone()])?;
        i += 1;
    }
    Ok(Value::Undefined)
}

fn set_values(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_set(vm, &this)?;
    Ok(vm.new_iter(IterState::SetValues(o, 0)))
}

fn set_entries(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_set(vm, &this)?;
    let vals: Vec<Value> = set_of!(o).borrow().iter().map(|k| k.to_value()).collect();
    let pairs = vals
        .into_iter()
        .map(|v| vm.new_array(vec![v.clone(), v]))
        .collect();
    Ok(vm.new_iter(IterState::List(pairs, 0)))
}

fn iter_next(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let next = vm.iter_next(&this)?;
    let o = vm.new_object();
    let done = next.is_none();
    o.set_own("value", next.unwrap_or_default());
    o.set_own("done", Value::Bool(done));
    Ok(Value::Object(o))
}

fn iter_to_array(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let items = vm.iterate(&this)?;
    Ok(vm.new_array(items))
}
