use super::*;

#[repr(C)]
pub struct napi_property_descriptor {
    pub utf8name: *const c_char,
    pub name: napi_value,
    pub method: napi_callback,
    pub getter: napi_callback,
    pub setter: napi_callback,
    pub value: napi_value,
    pub attributes: i32,
    pub data: *mut c_void,
}

const WRITABLE: i32 = 1;
const ENUMERABLE: i32 = 2;
const CONFIGURABLE: i32 = 4;
const STATIC: i32 = 1024;

pub struct RefSlot {
    strong: Option<Value>,
    weak: Option<std::rc::Weak<Obj>>,
    count: u32,
}

fn key_of(env: &mut Env, h: napi_value) -> Result<JsStr, Value> {
    let k = env.get(h);
    env.vm().to_property_key(&k)
}

use crate::jsstr::JsStr;

fn object_arg(env: &mut Env, h: napi_value) -> Option<Value> {
    match env.get(h) {
        v @ Value::Object(_) => Some(v),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_object(env: napi_env, result: *mut napi_value) -> napi_status {
    let env = env!(env);
    let o = env.vm().new_object();
    out!(env, result, env.put(Value::Object(o)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_array(env: napi_env, result: *mut napi_value) -> napi_status {
    let env = env!(env);
    let a = env.vm().new_array(Vec::new());
    out!(env, result, env.put(a));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_array_with_length(
    env: napi_env,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let array = env.global("Array");
    let a = js!(
        env,
        env.vm().construct(
            &array,
            &[Value::Number(length.min(u32::MAX as usize) as f64)]
        )
    );
    out!(env, result, env.put(a));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_array(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = env.get(value).is_array();
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_array_length(
    env: napi_env,
    value: napi_value,
    result: *mut u32,
) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    if !v.is_array() {
        return env.status(ARRAY_EXPECTED);
    }
    let len = js!(env, env.vm().get_prop(&v, "length"));
    let n = match len {
        Value::Number(n) => n as u32,
        _ => 0,
    };
    out!(env, result, n);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_prototype(
    env: napi_env,
    object: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let v = env.get(object);
    let o = js!(env, env.vm().to_object(&v));
    let p = o
        .proto
        .borrow()
        .clone()
        .map(Value::Object)
        .unwrap_or(Value::Null);
    out!(env, result, env.put(p));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    value: napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let k = js!(env, key_of(env, key));
    let v = env.get(value);
    js!(env, env.vm().set_prop(&o, k, v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let o = env.get(object);
    let k = js!(env, key_of(env, key));
    let v = js!(env, env.vm().get_prop(&o, &k));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let k = env.get(key);
    let r = js!(env, env.vm().has_property(&o, &k));
    out!(env, result, r);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_own_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Some(Value::Object(o)) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let k = js!(env, key_of(env, key));
    let has = o.props.borrow().contains_key(&k) || {
        let own = env.vm().own_keys(&Value::Object(o.clone()));
        own.iter().any(|x| matches!(x, Value::Str(s) if *s == k))
    };
    out!(env, result, has);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let k = js!(env, key_of(env, key));
    let r = js!(env, env.vm().delete_prop(&o, &k));
    if !result.is_null() {
        unsafe { *result = r };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_named_property(
    env: napi_env,
    object: napi_value,
    name: *const c_char,
    value: napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let k = unsafe { c_str(name, usize::MAX) };
    let v = env.get(value);
    js!(env, env.vm().set_prop(&o, k.into(), v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_named_property(
    env: napi_env,
    object: napi_value,
    name: *const c_char,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let o = env.get(object);
    let k = unsafe { c_str(name, usize::MAX) };
    let v = js!(env, env.vm().get_prop(&o, &k));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_named_property(
    env: napi_env,
    object: napi_value,
    name: *const c_char,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let k = unsafe { c_str(name, usize::MAX) };
    let r = js!(env, env.vm().has_property(&o, &Value::Str(k.into())));
    out!(env, result, r);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_element(
    env: napi_env,
    object: napi_value,
    index: u32,
    value: napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let v = env.get(value);
    js!(env, env.vm().set_prop(&o, index.to_string().into(), v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_element(
    env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let o = env.get(object);
    let v = js!(env, env.vm().get_prop(&o, &index.to_string()));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_element(
    env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let r = js!(
        env,
        env.vm()
            .has_property(&o, &Value::Str(index.to_string().into()))
    );
    out!(env, result, r);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_element(
    env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let r = js!(env, env.vm().delete_prop(&o, &index.to_string()));
    if !result.is_null() {
        unsafe { *result = r };
    }
    env.status(OK)
}

fn keys_array(env: &mut Env, keys: Vec<Value>) -> napi_value {
    let a = env.vm().new_array(keys);
    env.put(a)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_property_names(
    env: napi_env,
    object: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let keys = env.vm().for_in_keys(&o);
    out!(env, result, keys_array(env, keys));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_all_property_names(
    env: napi_env,
    object: napi_value,
    key_mode: i32,
    key_filter: i32,
    key_conversion: i32,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let own = key_mode == 1;
    let keys = if own {
        let keys = env.vm().own_keys(&o);
        let reflect = env.global("Object");
        let mut out = Vec::new();
        for k in keys {
            let is_symbol = matches!(k, Value::Symbol(_));
            if is_symbol && key_filter & 16 != 0 {
                continue;
            }
            if !is_symbol && key_filter & 8 != 0 {
                continue;
            }
            if key_filter & 2 != 0 {
                let gopd = js!(env, env.vm().get_prop(&reflect, "getOwnPropertyDescriptor"));
                let d = js!(
                    env,
                    env.vm()
                        .call_value(&gopd, reflect.clone(), &[o.clone(), k.clone()])
                );
                let e = js!(env, env.vm().get_prop(&d, "enumerable"));
                if !e.truthy() {
                    continue;
                }
            }
            out.push(k);
        }
        out
    } else {
        env.vm().for_in_keys(&o)
    };
    let keys = if key_conversion == 1 {
        keys.into_iter()
            .map(|k| match &k {
                Value::Str(s) if s.parse::<u32>().is_ok() => k,
                _ => k,
            })
            .collect()
    } else {
        keys
    };
    out!(env, result, keys_array(env, keys));
    env.status(OK)
}

fn define_one(
    env: &mut Env,
    target: &Value,
    d: &napi_property_descriptor,
    holder: Option<&Rc<Obj>>,
) -> Result<(), Value> {
    let key = if d.utf8name.is_null() {
        let k = env.get(d.name);
        env.vm().to_property_key(&k)?
    } else {
        unsafe { c_str(d.utf8name, usize::MAX) }.into()
    };
    let display = key.to_string();
    let object = env.global("Object");
    let define = env.vm().get_prop(&object, "defineProperty")?;
    let desc = Value::Object(env.vm().new_object());
    let attrs = d.attributes;
    let set = |env: &mut Env, k: &str, v: Value| env.vm().set_prop(&desc, k.into(), v);
    set(env, "enumerable", Value::Bool(attrs & ENUMERABLE != 0))?;
    set(env, "configurable", Value::Bool(attrs & CONFIGURABLE != 0))?;
    if d.getter.is_some() || d.setter.is_some() {
        if d.getter.is_some() {
            let g = make_method(env, &display, d.getter, d.data, holder.cloned())?;
            set(env, "get", g)?;
        }
        if d.setter.is_some() {
            let s = make_method(env, &display, d.setter, d.data, holder.cloned())?;
            set(env, "set", s)?;
        }
    } else if d.method.is_some() {
        let f = make_method(env, &display, d.method, d.data, holder.cloned())?;
        set(env, "value", f)?;
        set(env, "writable", Value::Bool(attrs & WRITABLE != 0))?;
    } else {
        let v = env.get(d.value);
        set(env, "value", v)?;
        set(env, "writable", Value::Bool(attrs & WRITABLE != 0))?;
    }
    let key_value = Value::Str(key);
    env.vm()
        .call_value(&define, object, &[target.clone(), key_value, desc])?;
    Ok(())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_define_properties(
    env: napi_env,
    object: napi_value,
    property_count: usize,
    properties: *const napi_property_descriptor,
) -> napi_status {
    let env = env!(env);
    let Some(o) = object_arg(env, object) else {
        return env.status(OBJECT_EXPECTED);
    };
    for i in 0..property_count {
        let d = unsafe { &*properties.add(i) };
        js!(env, define_one(env, &o, d, None));
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_define_class(
    env: napi_env,
    utf8name: *const c_char,
    length: usize,
    constructor: napi_callback,
    data: *mut c_void,
    property_count: usize,
    properties: *const napi_property_descriptor,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let name = unsafe { c_str(utf8name, length) };
    let class = js!(env, make_function(env, &name, constructor, data));
    let proto = js!(env, env.vm().get_prop(&class, "prototype"));
    for i in 0..property_count {
        let d = unsafe { &*properties.add(i) };
        let (target, holder) = if d.attributes & STATIC != 0 {
            (class.clone(), None)
        } else {
            let holder = match &proto {
                Value::Object(p) => Some(p),
                _ => None,
            };
            (proto.clone(), holder)
        };
        js!(env, define_one(env, &target, d, holder));
    }
    out!(env, result, env.put(class));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external(
    env: napi_env,
    data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    let fin = finalize_cb.map(|cb| (Some(cb), envp, finalize_hint));
    let v = host_value(
        env.vm(),
        HostData {
            ptr: data,
            fin: RefCell::new(fin),
            tag: None,
        },
    );
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_external(
    env: napi_env,
    value: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    let Some(h) = host_of(&v) else {
        return env.status(INVALID_ARG);
    };
    out!(env, result, h.ptr);
    env.status(OK)
}

fn new_ref(v: Value, count: u32) -> *mut RefSlot {
    let slot = if count == 0 {
        match &v {
            Value::Object(o) => RefSlot {
                strong: None,
                weak: Some(Rc::downgrade(o)),
                count,
            },
            _ => RefSlot {
                strong: Some(v),
                weak: None,
                count,
            },
        }
    } else {
        RefSlot {
            strong: Some(v),
            weak: None,
            count,
        }
    };
    Box::into_raw(Box::new(slot))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_wrap(
    env: napi_env,
    js_object: napi_value,
    native_object: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    result: *mut *mut RefSlot,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    let Some(o) = object_arg(env, js_object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let (wrap_key, _) = hidden_keys(env.vm());
    let key = js!(env, env.vm().to_property_key(&wrap_key));
    let fin = finalize_cb.map(|cb| (Some(cb), envp, finalize_hint));
    let host = host_value(
        env.vm(),
        HostData {
            ptr: native_object,
            fin: RefCell::new(fin),
            tag: None,
        },
    );
    if let Value::Object(obj) = &o {
        obj.set_with_attrs(key, host, crate::object::NON_ENUM);
    }
    if !result.is_null() {
        unsafe { *result = new_ref(o, 0) };
    }
    env.status(OK)
}

fn wrapped(env: &mut Env, h: napi_value) -> Option<(Rc<Obj>, JsStr)> {
    let Value::Object(o) = env.get(h) else {
        return None;
    };
    let (wrap_key, _) = hidden_keys(env.vm());
    let key = env.vm().to_property_key(&wrap_key).ok()?;
    Some((o, key))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_unwrap(
    env: napi_env,
    js_object: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    let Some((o, key)) = wrapped(env, js_object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let Some(v) = o.get_own(&key) else {
        return env.status(INVALID_ARG);
    };
    let Some(h) = host_of(&v) else {
        return env.status(INVALID_ARG);
    };
    out!(env, result, h.ptr);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_remove_wrap(
    env: napi_env,
    js_object: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    let Some((o, key)) = wrapped(env, js_object) else {
        return env.status(OBJECT_EXPECTED);
    };
    let Some(v) = o.get_own(&key) else {
        return env.status(INVALID_ARG);
    };
    let ptr = match &v {
        Value::Object(h) => match &h.kind {
            ObjKind::Host(data) => data.ptr,
            _ => return env.status(INVALID_ARG),
        },
        _ => return env.status(INVALID_ARG),
    };
    if let Value::Object(h) = &v {
        if let ObjKind::Host(data) = &h.kind {
            data.fin.borrow_mut().take();
        }
    }
    o.props.borrow_mut().shift_remove(&key);
    if !result.is_null() {
        unsafe { *result = ptr };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_add_finalizer(
    env: napi_env,
    js_object: napi_value,
    data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    result: *mut *mut RefSlot,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    let Some(o) = object_arg(env, js_object) else {
        return env.status(OBJECT_EXPECTED);
    };
    env.finalizer_ids += 1;
    let key = format!("\u{0}@@napi.finalizer.{}", env.finalizer_ids);
    let fin = finalize_cb.map(|cb| (Some(cb), envp, finalize_hint));
    let host = host_value(
        env.vm(),
        HostData {
            ptr: data,
            fin: RefCell::new(fin),
            tag: None,
        },
    );
    if let Value::Object(obj) = &o {
        obj.set_with_attrs(key, host, crate::object::NON_ENUM);
    }
    if !result.is_null() {
        unsafe { *result = new_ref(o, 0) };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_post_finalizer(
    env: napi_env,
    finalize_cb: napi_finalize,
    finalize_data: *mut c_void,
    finalize_hint: *mut c_void,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    if let Some(cb) = finalize_cb {
        unsafe { cb(envp, finalize_data, finalize_hint) };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_reference(
    env: napi_env,
    value: napi_value,
    initial_refcount: u32,
    result: *mut *mut RefSlot,
) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    out!(env, result, new_ref(v, initial_refcount));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_reference(env: napi_env, r: *mut RefSlot) -> napi_status {
    let env = env!(env);
    if !r.is_null() {
        drop(unsafe { Box::from_raw(r) });
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reference_ref(
    env: napi_env,
    r: *mut RefSlot,
    result: *mut u32,
) -> napi_status {
    let env = env!(env);
    let Some(slot) = (unsafe { r.as_mut() }) else {
        return env.status(INVALID_ARG);
    };
    if slot.count == 0 {
        if let Some(o) = slot.weak.as_ref().and_then(|w| w.upgrade()) {
            slot.strong = Some(Value::Object(o));
        }
    }
    slot.count += 1;
    if !result.is_null() {
        unsafe { *result = slot.count };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reference_unref(
    env: napi_env,
    r: *mut RefSlot,
    result: *mut u32,
) -> napi_status {
    let env = env!(env);
    let Some(slot) = (unsafe { r.as_mut() }) else {
        return env.status(INVALID_ARG);
    };
    if slot.count == 0 {
        return env.status(GENERIC_FAILURE);
    }
    slot.count -= 1;
    if slot.count == 0 {
        if let Some(Value::Object(o)) = &slot.strong {
            slot.weak = Some(Rc::downgrade(o));
            slot.strong = None;
        }
    }
    if !result.is_null() {
        unsafe { *result = slot.count };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_reference_value(
    env: napi_env,
    r: *mut RefSlot,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(slot) = (unsafe { r.as_ref() }) else {
        return env.status(INVALID_ARG);
    };
    let v = match (&slot.strong, &slot.weak) {
        (Some(v), _) => Some(v.clone()),
        (None, Some(w)) => w.upgrade().map(Value::Object),
        _ => None,
    };
    let h = match v {
        Some(v) => env.put(v),
        None => std::ptr::null_mut(),
    };
    out!(env, result, h);
    env.status(OK)
}

#[repr(C)]
pub struct napi_type_tag {
    lower: u64,
    upper: u64,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_type_tag_object(
    env: napi_env,
    value: napi_value,
    tag: *const napi_type_tag,
) -> napi_status {
    let env = env!(env);
    let Some(Value::Object(o)) = object_arg(env, value) else {
        return env.status(OBJECT_EXPECTED);
    };
    let Some(t) = (unsafe { tag.as_ref() }) else {
        return env.status(INVALID_ARG);
    };
    let (_, tag_key) = hidden_keys(env.vm());
    let key = js!(env, env.vm().to_property_key(&tag_key));
    if o.get_own(&key).is_some() {
        return env.status(INVALID_ARG);
    }
    let host = host_value(
        env.vm(),
        HostData {
            ptr: std::ptr::null_mut(),
            fin: RefCell::new(None),
            tag: Some([t.lower, t.upper]),
        },
    );
    o.set_with_attrs(key, host, crate::object::NON_ENUM);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_check_object_type_tag(
    env: napi_env,
    value: napi_value,
    tag: *const napi_type_tag,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Some(Value::Object(o)) = object_arg(env, value) else {
        return env.status(OBJECT_EXPECTED);
    };
    let Some(t) = (unsafe { tag.as_ref() }) else {
        return env.status(INVALID_ARG);
    };
    let (_, tag_key) = hidden_keys(env.vm());
    let key = js!(env, env.vm().to_property_key(&tag_key));
    let matches = o
        .get_own(&key)
        .as_ref()
        .and_then(host_of)
        .and_then(|h| h.tag)
        .is_some_and(|x| x == [t.lower, t.upper]);
    out!(env, result, matches);
    env.status(OK)
}

unsafe fn object_call(env: napi_env, value: napi_value, method: &str) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    let object = env.global("Object");
    let f = js!(env, env.vm().get_prop(&object, method));
    js!(env, env.vm().call_value(&f, object, &[v]));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_object_freeze(env: napi_env, value: napi_value) -> napi_status {
    unsafe { object_call(env, value, "freeze") }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_object_seal(env: napi_env, value: napi_value) -> napi_status {
    unsafe { object_call(env, value, "seal") }
}
