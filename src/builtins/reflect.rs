use super::{arg, make_ctor, method, native_fn};
use crate::inspect::inspect_plain;
use crate::object::{Obj, ObjKind, ProxyData, is_private_key, is_symbol_key};
use crate::props::{PropDesc, accessor_parts};
use crate::value::Value;
use crate::vm::Vm;
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn key_value(k: &str) -> Value {
    if is_symbol_key(k) {
        super::symbol_from_key(k)
    } else {
        Value::Str(k.into())
    }
}

fn proxy_parts(vm: &mut Vm, o: &Obj, op: &str) -> Result<(Rc<Obj>, Rc<Obj>), Value> {
    let ObjKind::Proxy(p) = &o.kind else {
        unreachable!("not a proxy")
    };
    let target = p.target.borrow().clone();
    let handler = p.handler.borrow().clone();
    match (target, handler) {
        (Some(t), Some(h)) => Ok((t, h)),
        _ => Err(vm.type_error(format!(
            "Cannot perform '{op}' on a proxy that has been revoked"
        ))),
    }
}

fn trap(vm: &mut Vm, handler: &Rc<Obj>, name: &str) -> Result<Option<Value>, Value> {
    let f = vm.get_prop(&Value::Object(handler.clone()), name)?;
    if f.is_nullish() {
        return Ok(None);
    }
    if !f.is_callable() {
        return Err(vm.type_error(format!("{} is not a function", inspect_plain(&f))));
    }
    Ok(Some(f))
}

pub fn is_proxy(o: &Obj) -> bool {
    matches!(o.kind, ObjKind::Proxy(_))
}

impl Vm {
    pub fn op_get_prototype_of(&mut self, o: &Rc<Obj>) -> Result<Option<Rc<Obj>>, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "getPrototypeOf")?;
            return match trap(self, &h, "getPrototypeOf")? {
                Some(f) => {
                    let r = self.call_value(&f, Value::Object(h), &[Value::Object(t)])?;
                    match r {
                        Value::Object(p) => Ok(Some(p)),
                        Value::Null => Ok(None),
                        _ => Err(self.type_error(
                            "'getPrototypeOf' on proxy: trap returned neither object nor null",
                        )),
                    }
                }
                None => self.op_get_prototype_of(&t),
            };
        }
        Ok(o.proto.borrow().clone())
    }

    pub fn op_set_prototype_of(
        &mut self,
        o: &Rc<Obj>,
        proto: Option<Rc<Obj>>,
    ) -> Result<bool, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "setPrototypeOf")?;
            return match trap(self, &h, "setPrototypeOf")? {
                Some(f) => {
                    let p = proto.map(Value::Object).unwrap_or(Value::Null);
                    let r = self.call_value(&f, Value::Object(h), &[Value::Object(t), p])?;
                    Ok(r.truthy())
                }
                None => self.op_set_prototype_of(&t, proto),
            };
        }
        let current = o.proto.borrow().clone();
        let same = match (&current, &proto) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return Ok(true);
        }
        if !o.is_extensible() {
            return Ok(false);
        }
        let mut cur = proto.clone();
        while let Some(p) = cur {
            if Rc::ptr_eq(&p, o) {
                return Ok(false);
            }
            if is_proxy(&p) {
                break;
            }
            cur = p.proto.borrow().clone();
        }
        *o.proto.borrow_mut() = proto;
        Ok(true)
    }

    pub fn op_is_extensible(&mut self, o: &Rc<Obj>) -> Result<bool, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "isExtensible")?;
            return match trap(self, &h, "isExtensible")? {
                Some(f) => {
                    let r = self.call_value(&f, Value::Object(h), &[Value::Object(t)])?;
                    Ok(r.truthy())
                }
                None => self.op_is_extensible(&t),
            };
        }
        Ok(o.is_extensible())
    }

    pub fn op_prevent_extensions(&mut self, o: &Rc<Obj>) -> Result<bool, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "preventExtensions")?;
            return match trap(self, &h, "preventExtensions")? {
                Some(f) => {
                    let r = self.call_value(&f, Value::Object(h), &[Value::Object(t)])?;
                    Ok(r.truthy())
                }
                None => self.op_prevent_extensions(&t),
            };
        }
        self.prevent_extensions(o);
        Ok(true)
    }

    pub fn op_get_own_property(
        &mut self,
        o: &Rc<Obj>,
        key: &str,
    ) -> Result<Option<PropDesc>, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "getOwnPropertyDescriptor")?;
            return match trap(self, &h, "getOwnPropertyDescriptor")? {
                Some(f) => {
                    let r = self.call_value(
                        &f,
                        Value::Object(h),
                        &[Value::Object(t.clone()), key_value(key)],
                    )?;
                    if !r.is_object() && !r.is_undefined() {
                        return Err(self.type_error(format!(
                            "'getOwnPropertyDescriptor' on proxy: trap returned neither object nor undefined for property '{}'",
                            crate::props::key_text(key)
                        )));
                    }
                    let target_desc = self.op_get_own_property(&t, key)?;
                    if r.is_undefined() {
                        let Some(td) = target_desc else {
                            return Ok(None);
                        };
                        if td.configurable == Some(false) {
                            return Err(self.proxy_invariant("getOwnPropertyDescriptor", key));
                        }
                        if !self.op_is_extensible(&t)? {
                            return Err(self.proxy_invariant("getOwnPropertyDescriptor", key));
                        }
                        return Ok(None);
                    }
                    let extensible = self.op_is_extensible(&t)?;
                    let mut d = self.to_prop_desc(&r)?;
                    if !d.is_accessor() {
                        d.value.get_or_insert(Value::Undefined);
                        d.writable.get_or_insert(false);
                    }
                    d.enumerable.get_or_insert(false);
                    d.configurable.get_or_insert(false);
                    if !compatible(extensible, &d, &target_desc) {
                        return Err(self.proxy_invariant("getOwnPropertyDescriptor", key));
                    }
                    if d.configurable == Some(false) {
                        match &target_desc {
                            None => {
                                return Err(self.proxy_invariant("getOwnPropertyDescriptor", key));
                            }
                            Some(td) if td.configurable != Some(false) => {
                                return Err(self.proxy_invariant("getOwnPropertyDescriptor", key));
                            }
                            Some(td) => {
                                if d.writable == Some(false) && td.writable == Some(true) {
                                    return Err(
                                        self.proxy_invariant("getOwnPropertyDescriptor", key)
                                    );
                                }
                            }
                        }
                    }
                    Ok(Some(d))
                }
                None => self.op_get_own_property(&t, key),
            };
        }
        if let ObjKind::Global = o.kind {
            return Ok(self.global_descriptor(key));
        }
        Ok(self.own_descriptor(o, key))
    }

    pub fn op_define_own_property(
        &mut self,
        o: &Rc<Obj>,
        key: Rc<str>,
        d: PropDesc,
        throw: bool,
    ) -> Result<bool, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "defineProperty")?;
            return match trap(self, &h, "defineProperty")? {
                Some(f) => {
                    let desc = self.from_prop_desc(&d);
                    let r = self.call_value(
                        &f,
                        Value::Object(h),
                        &[Value::Object(t.clone()), key_value(&key), desc],
                    )?;
                    if !r.truthy() {
                        if throw {
                            return Err(self.type_error(format!(
                                "'defineProperty' on proxy: trap returned falsish for property '{}'",
                                crate::props::key_text(&key)
                            )));
                        }
                        return Ok(false);
                    }
                    let target_desc = self.op_get_own_property(&t, &key)?;
                    let extensible = self.op_is_extensible(&t)?;
                    let setting_config_false = d.configurable == Some(false);
                    match &target_desc {
                        None => {
                            if !extensible || setting_config_false {
                                return Err(self.proxy_invariant("defineProperty", &key));
                            }
                        }
                        Some(td) => {
                            if !compatible(extensible, &d, &target_desc) {
                                return Err(self.proxy_invariant("defineProperty", &key));
                            }
                            if setting_config_false && td.configurable == Some(true) {
                                return Err(self.proxy_invariant("defineProperty", &key));
                            }
                            if !td.is_accessor()
                                && td.configurable == Some(false)
                                && td.writable == Some(true)
                                && d.writable == Some(false)
                            {
                                return Err(self.proxy_invariant("defineProperty", &key));
                            }
                        }
                    }
                    Ok(true)
                }
                None => self.op_define_own_property(&t, key, d, throw),
            };
        }
        if let ObjKind::Global = o.kind {
            return self.define_global_prop(key, d, throw);
        }
        self.define_own(o, key, d, throw)
    }

    pub fn op_has_property(&mut self, o: &Rc<Obj>, key: &str) -> Result<bool, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "has")?;
            return match trap(self, &h, "has")? {
                Some(f) => {
                    let r =
                        self.call_value(&f, Value::Object(h), &[Value::Object(t), key_value(key)])?;
                    Ok(r.truthy())
                }
                None => self.op_has_property(&t, key),
            };
        }
        let mut cur = Some(o.clone());
        while let Some(c) = cur {
            if is_proxy(&c) {
                return self.op_has_property(&c, key);
            }
            if let ObjKind::Typed(t) = &c.kind {
                if let Some(n) = crate::builtins::typed::canonical_numeric(key) {
                    return Ok(crate::builtins::typed::valid_index(t, n).is_some());
                }
            }
            if self.has_own_any(&c, key) {
                return Ok(true);
            }
            cur = c.proto.borrow().clone();
        }
        Ok(false)
    }

    pub fn has_own_any(&self, o: &Rc<Obj>, key: &str) -> bool {
        if let ObjKind::Global = o.kind {
            return self.globals.get(key).is_some();
        }
        self.own_property(o, key).is_some()
    }

    pub fn op_get(&mut self, o: &Rc<Obj>, key: &str, receiver: &Value) -> R {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "get")?;
            return match trap(self, &h, "get")? {
                Some(f) => self.call_value(
                    &f,
                    Value::Object(h),
                    &[Value::Object(t), key_value(key), receiver.clone()],
                ),
                None => self.op_get(&t, key, receiver),
            };
        }
        let mut cur = Some(o.clone());
        while let Some(c) = cur {
            if is_proxy(&c) {
                return self.op_get(&c, key, receiver);
            }
            if let ObjKind::Global = c.kind {
                if let Some(v) = self.globals.get(key).cloned() {
                    return self.resolve_accessor(v, receiver);
                }
            } else if let Some((raw, _)) = self.own_property(&c, key) {
                return self.resolve_accessor(raw, receiver);
            }
            cur = c.proto.borrow().clone();
        }
        Ok(Value::Undefined)
    }

    pub fn op_set(
        &mut self,
        o: &Rc<Obj>,
        key: Rc<str>,
        val: Value,
        receiver: &Value,
    ) -> Result<bool, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "set")?;
            return match trap(self, &h, "set")? {
                Some(f) => {
                    let r = self.call_value(
                        &f,
                        Value::Object(h),
                        &[Value::Object(t), key_value(&key), val, receiver.clone()],
                    )?;
                    Ok(r.truthy())
                }
                None => self.op_set(&t, key, val, receiver),
            };
        }
        let mut cur = Some(o.clone());
        while let Some(c) = cur {
            if is_proxy(&c) && !Rc::ptr_eq(&c, o) {
                return self.op_set(&c, key, val, receiver);
            }
            if let ObjKind::Typed(t) = &c.kind {
                if let Some(n) = crate::builtins::typed::canonical_numeric(&key) {
                    let same = matches!(receiver, Value::Object(r) if Rc::ptr_eq(r, &c));
                    if same {
                        let x = crate::builtins::typed::coerce(self, t.kind, &val)?;
                        if let Some(i) = crate::builtins::typed::valid_index(t, n) {
                            t.set(i, &x);
                        }
                        return Ok(true);
                    }
                    if crate::builtins::typed::valid_index(t, n).is_none() {
                        return Ok(true);
                    }
                    break;
                }
            }
            if let Some((raw, attrs)) = self.own_property(&c, &key) {
                if let Some((_, setter)) = accessor_parts(&raw) {
                    if setter.is_callable() {
                        self.call_value(&setter, receiver.clone(), &[val])?;
                        return Ok(true);
                    }
                    return Ok(false);
                }
                if attrs & crate::object::READ_ONLY != 0 {
                    return Ok(false);
                }
                break;
            }
            cur = c.proto.borrow().clone();
        }
        let Value::Object(r) = receiver else {
            return Ok(false);
        };
        if let Some((raw, attrs)) = self.own_property(r, &key) {
            if accessor_parts(&raw).is_some() || attrs & crate::object::READ_ONLY != 0 {
                return Ok(false);
            }
            let d = PropDesc {
                value: Some(val),
                ..PropDesc::default()
            };
            return self.op_define_own_property(r, key, d, false);
        }
        let d = PropDesc {
            value: Some(val),
            writable: Some(true),
            enumerable: Some(true),
            configurable: Some(true),
            ..PropDesc::default()
        };
        self.op_define_own_property(r, key, d, false)
    }

    fn proxy_invariant(&mut self, trap: &str, key: &str) -> Value {
        self.type_error(format!(
            "'{trap}' on proxy: trap result violates an invariant for property '{}'",
            crate::props::key_text(key)
        ))
    }

    pub fn op_delete(&mut self, o: &Rc<Obj>, key: &str) -> Result<bool, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "deleteProperty")?;
            return match trap(self, &h, "deleteProperty")? {
                Some(f) => {
                    let r = self.call_value(
                        &f,
                        Value::Object(h),
                        &[Value::Object(t.clone()), key_value(key)],
                    )?;
                    if !r.truthy() {
                        return Ok(false);
                    }
                    if let Some(td) = self.op_get_own_property(&t, key)? {
                        if td.configurable == Some(false) || !self.op_is_extensible(&t)? {
                            return Err(self.proxy_invariant("deleteProperty", key));
                        }
                    }
                    Ok(true)
                }
                None => self.op_delete(&t, key),
            };
        }
        if let ObjKind::Global = o.kind {
            return Ok(self.delete_global(key));
        }
        Ok(self.try_delete(o, key))
    }

    pub fn op_own_keys(&mut self, o: &Rc<Obj>) -> Result<Vec<Rc<str>>, Value> {
        if is_proxy(o) {
            let (t, h) = proxy_parts(self, o, "ownKeys")?;
            return match trap(self, &h, "ownKeys")? {
                Some(f) => {
                    let r = self.call_value(&f, Value::Object(h), &[Value::Object(t)])?;
                    if !r.is_object() {
                        return Err(self.type_error("CreateListFromArrayLike called on non-object"));
                    }
                    let items = self.iterate(&r)?;
                    let mut out = Vec::with_capacity(items.len());
                    for v in items {
                        match &v {
                            Value::Str(s) => out.push(s.clone()),
                            Value::Symbol(s) => out.push(s.key.clone()),
                            _ => {
                                return Err(self.type_error(format!(
                                    "{} is not a valid property name",
                                    inspect_plain(&v)
                                )));
                            }
                        }
                    }
                    Ok(out)
                }
                None => self.op_own_keys(&t),
            };
        }
        let mut out: Vec<Rc<str>> = Vec::new();
        if let ObjKind::Global = o.kind {
            out.extend(self.globals.names());
            return Ok(out);
        }
        for k in self.own_property_names(&Value::Object(o.clone())) {
            if let Value::Str(s) = k {
                out.push(s);
            }
        }
        out.extend(
            o.props
                .borrow()
                .keys()
                .filter(|k| is_symbol_key(k) && !is_private_key(k))
                .cloned(),
        );
        Ok(out)
    }

    pub fn op_own_enumerable_keys(&mut self, o: &Rc<Obj>) -> Result<Vec<Rc<str>>, Value> {
        if !is_proxy(o) {
            return Ok(self
                .own_keys(&Value::Object(o.clone()))
                .into_iter()
                .filter_map(|v| match v {
                    Value::Str(s) => Some(s),
                    _ => None,
                })
                .collect());
        }
        let keys = self.op_own_keys(o)?;
        let mut out = Vec::new();
        for k in keys {
            if is_symbol_key(&k) {
                continue;
            }
            if let Some(d) = self.op_get_own_property(o, &k)? {
                if d.enumerable == Some(true) {
                    out.push(k);
                }
            }
        }
        Ok(out)
    }

    pub fn proxy_call(&mut self, o: &Rc<Obj>, this: Value, args: &[Value]) -> R {
        let (t, h) = proxy_parts(self, o, "apply")?;
        match trap(self, &h, "apply")? {
            Some(f) => {
                let list = self.new_array(args.to_vec());
                self.call_value(&f, Value::Object(h), &[Value::Object(t), this, list])
            }
            None => self.call_value(&Value::Object(t), this, args),
        }
    }

    pub fn proxy_construct(&mut self, o: &Rc<Obj>, args: &[Value], new_target: &Value) -> R {
        let (t, h) = proxy_parts(self, o, "construct")?;
        match trap(self, &h, "construct")? {
            Some(f) => {
                let list = self.new_array(args.to_vec());
                let r = self.call_value(
                    &f,
                    Value::Object(h),
                    &[Value::Object(t), list, new_target.clone()],
                )?;
                if !r.is_object() {
                    return Err(self.type_error("proxy [[Construct]] must return an object"));
                }
                Ok(r)
            }
            None => self.construct(&Value::Object(t), args),
        }
    }
}

pub fn install(vm: &mut Vm) {
    let reflect = vm.new_object();
    let fns: [(&str, u32, crate::object::NativeFn); 13] = [
        ("apply", 3, reflect_apply),
        ("construct", 2, reflect_construct),
        ("defineProperty", 3, reflect_define_property),
        ("deleteProperty", 2, reflect_delete_property),
        ("get", 2, reflect_get),
        (
            "getOwnPropertyDescriptor",
            2,
            reflect_get_own_property_descriptor,
        ),
        ("getPrototypeOf", 1, reflect_get_prototype_of),
        ("has", 2, reflect_has),
        ("isExtensible", 1, reflect_is_extensible),
        ("ownKeys", 1, reflect_own_keys),
        ("preventExtensions", 1, reflect_prevent_extensions),
        ("set", 3, reflect_set),
        ("setPrototypeOf", 2, reflect_set_prototype_of),
    ];
    for (name, arity, f) in fns {
        method(vm, &reflect, name, arity, f);
    }
    reflect.set_own(crate::object::TO_STRING_TAG_KEY, "Reflect".into());
    vm.define_global("Reflect", Value::Object(reflect));

    let pp = vm.new_object();
    let proxy = make_ctor(vm, "Proxy", 2, proxy_ctor, Some(|| ObjKind::Ordinary), &pp);
    proxy.props.borrow_mut().shift_remove("prototype");
    method(vm, &proxy, "revocable", 2, proxy_revocable);
    vm.define_global("Proxy", Value::Object(proxy));
}

fn target_obj(vm: &mut Vm, v: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    match v {
        Value::Object(o) => Ok(o.clone()),
        _ => Err(vm.type_error(format!("Reflect.{name} called on non-object"))),
    }
}

fn reflect_apply(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let f = arg(args, 0);
    if !f.is_callable() {
        return Err(vm.type_error(format!(
            "Function.prototype.apply was called on {}, which is {} and not a function",
            inspect_plain(&f),
            f.type_of()
        )));
    }
    let list = arg(args, 2);
    if !list.is_object() {
        return Err(vm.type_error("CreateListFromArrayLike called on non-object"));
    }
    let items = array_like(vm, &list)?;
    vm.call_value(&f, arg(args, 1), &items)
}

pub fn array_like(vm: &mut Vm, v: &Value) -> Result<Vec<Value>, Value> {
    if let Value::Object(o) = v {
        if let ObjKind::Array(a) = &o.kind {
            return Ok(a.borrow().clone());
        }
    }
    let len = vm.get_prop(v, "length")?;
    let len = super::to_integer(vm, &len)?.max(0.0) as usize;
    let mut out = Vec::with_capacity(len.min(1 << 20));
    for i in 0..len {
        out.push(vm.get_prop(v, &i.to_string())?);
    }
    Ok(out)
}

fn reflect_construct(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let f = arg(args, 0);
    if !vm.is_constructor(&f) {
        return Err(vm.type_error(format!("{} is not a constructor", inspect_plain(&f))));
    }
    let new_target = if args.len() > 2 {
        arg(args, 2)
    } else {
        f.clone()
    };
    if !vm.is_constructor(&new_target) {
        return Err(vm.type_error(format!(
            "{} is not a constructor",
            inspect_plain(&new_target)
        )));
    }
    let list = arg(args, 1);
    if !list.is_object() {
        return Err(vm.type_error("CreateListFromArrayLike called on non-object"));
    }
    let items = array_like(vm, &list)?;
    let r = vm.construct_with_target(&f, &items, &new_target)?;
    Ok(r)
}

fn reflect_define_property(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "defineProperty")?;
    let k = vm.to_property_key(&arg(args, 1))?;
    let d = vm.to_prop_desc(&arg(args, 2))?;
    Ok(Value::Bool(vm.op_define_own_property(&o, k, d, false)?))
}

fn reflect_delete_property(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "deleteProperty")?;
    let k = vm.to_property_key(&arg(args, 1))?;
    Ok(Value::Bool(vm.op_delete(&o, &k)?))
}

fn reflect_get(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    let o = target_obj(vm, &target, "get")?;
    let k = vm.to_property_key(&arg(args, 1))?;
    let receiver = if args.len() > 2 { arg(args, 2) } else { target };
    vm.op_get(&o, &k, &receiver)
}

fn reflect_get_own_property_descriptor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "getOwnPropertyDescriptor")?;
    let k = vm.to_property_key(&arg(args, 1))?;
    match vm.op_get_own_property(&o, &k)? {
        Some(d) => Ok(vm.from_prop_desc(&d)),
        None => Ok(Value::Undefined),
    }
}

fn reflect_get_prototype_of(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "getPrototypeOf")?;
    Ok(vm
        .op_get_prototype_of(&o)?
        .map(Value::Object)
        .unwrap_or(Value::Null))
}

fn reflect_has(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "has")?;
    let k = vm.to_property_key(&arg(args, 1))?;
    Ok(Value::Bool(vm.op_has_property(&o, &k)?))
}

fn reflect_is_extensible(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "isExtensible")?;
    Ok(Value::Bool(vm.op_is_extensible(&o)?))
}

fn reflect_own_keys(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "ownKeys")?;
    let keys = vm.op_own_keys(&o)?;
    let list = keys.iter().map(|k| key_value(k)).collect();
    Ok(vm.new_array(list))
}

fn reflect_prevent_extensions(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "preventExtensions")?;
    Ok(Value::Bool(vm.op_prevent_extensions(&o)?))
}

fn reflect_set(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    let o = target_obj(vm, &target, "set")?;
    let k = vm.to_property_key(&arg(args, 1))?;
    let receiver = if args.len() > 3 { arg(args, 3) } else { target };
    Ok(Value::Bool(vm.op_set(&o, k, arg(args, 2), &receiver)?))
}

fn reflect_set_prototype_of(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = target_obj(vm, &arg(args, 0), "setPrototypeOf")?;
    let proto = match arg(args, 1) {
        Value::Object(p) => Some(p),
        Value::Null => None,
        _ => return Err(vm.type_error("Object prototype may only be an Object or null")),
    };
    Ok(Value::Bool(vm.op_set_prototype_of(&o, proto)?))
}

fn make_proxy(vm: &mut Vm, target: &Value, handler: &Value) -> Result<Rc<Obj>, Value> {
    let (Value::Object(t), Value::Object(h)) = (target, handler) else {
        return Err(vm.type_error("Cannot create proxy with a non-object as target or handler"));
    };
    let callable = t.is_callable();
    let constructible = vm.is_constructor(target);
    Ok(Obj::new(
        ObjKind::Proxy(Box::new(ProxyData {
            target: RefCell::new(Some(t.clone())),
            handler: RefCell::new(Some(h.clone())),
            callable,
            constructible,
        })),
        None,
    ))
}

fn proxy_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !this.is_object() {
        return Err(vm.type_error("Constructor Proxy requires 'new'"));
    }
    let p = make_proxy(vm, &arg(args, 0), &arg(args, 1))?;
    Ok(Value::Object(p))
}

fn proxy_revocable(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = make_proxy(vm, &arg(args, 0), &arg(args, 1))?;
    let out = vm.new_object();
    out.set_own("proxy", Value::Object(p.clone()));
    let inner = native_fn(vm, "", 0, proxy_revoke);
    let revoke = Obj::new(
        ObjKind::Bound(Box::new(crate::object::Bound {
            target: inner,
            this: Value::Undefined,
            args: vec![Value::Object(p)],
        })),
        Some(vm.realm.function_proto.clone()),
    );
    out.set_own("revoke", Value::Object(revoke));
    Ok(Value::Object(out))
}

fn proxy_revoke(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if let Value::Object(p) = arg(args, 0) {
        if let ObjKind::Proxy(d) = &p.kind {
            *d.target.borrow_mut() = None;
            *d.handler.borrow_mut() = None;
        }
    }
    Ok(Value::Undefined)
}

fn compatible(extensible: bool, desc: &PropDesc, current: &Option<PropDesc>) -> bool {
    let Some(cur) = current else {
        return extensible;
    };
    if cur.configurable != Some(false) {
        return true;
    }
    if desc.configurable == Some(true) {
        return false;
    }
    if let Some(e) = desc.enumerable {
        if Some(e) != cur.enumerable {
            return false;
        }
    }
    let generic = !desc.is_accessor() && desc.value.is_none() && desc.writable.is_none();
    if !generic && desc.is_accessor() != cur.is_accessor() {
        return false;
    }
    if cur.is_accessor() {
        let same = |a: &Option<Value>, b: &Option<Value>| match (a, b) {
            (Some(x), Some(y)) => crate::props::same_value(x, y),
            (Some(x), None) => x.is_undefined(),
            _ => true,
        };
        if desc.get.is_some() && !same(&desc.get, &cur.get) {
            return false;
        }
        if desc.set.is_some() && !same(&desc.set, &cur.set) {
            return false;
        }
    } else if cur.writable == Some(false) {
        if desc.writable == Some(true) {
            return false;
        }
        if let (Some(v), Some(c)) = (&desc.value, &cur.value) {
            if !crate::props::same_value(v, c) {
                return false;
            }
        }
    }
    true
}
