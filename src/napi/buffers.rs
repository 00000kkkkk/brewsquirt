use super::*;
use crate::builtins::typed;
use crate::object::TypedKind;

fn buffer_ptr(o: &Obj) -> Option<(*mut u8, usize)> {
    match &o.kind {
        ObjKind::ArrayBuffer(b) => {
            let mut bytes = b.bytes.borrow_mut();
            Some((bytes.as_mut_ptr(), bytes.len()))
        }
        _ => None,
    }
}

fn attach_finalizer(
    env: &mut Env,
    envp: napi_env,
    target: &Rc<Obj>,
    data: *mut c_void,
    cb: napi_finalize,
    hint: *mut c_void,
) {
    if cb.is_none() {
        return;
    }
    env.finalizer_ids += 1;
    let key = format!("\u{0}@@napi.finalizer.{}", env.finalizer_ids);
    let host = host_value(
        env.vm(),
        HostData {
            ptr: data,
            fin: RefCell::new(Some((cb, envp, hint))),
            tag: None,
        },
    );
    target.set_with_attrs(key, host, crate::object::NON_ENUM);
}

fn to_node_buffer(env: &mut Env, ab: Rc<Obj>, offset: usize, len: usize) -> Result<Value, Value> {
    let buffer = env.global("Buffer");
    if buffer.is_undefined() {
        return Ok(typed::new_typed(
            env.vm(),
            TypedKind::Uint8,
            ab,
            offset,
            len,
        ));
    }
    let from = env.vm().get_prop(&buffer, "from")?;
    env.vm().call_value(
        &from,
        buffer,
        &[
            Value::Object(ab),
            Value::Number(offset as f64),
            Value::Number(len as f64),
        ],
    )
}

fn view_parts(v: &Value) -> Option<(TypedKind, Rc<Obj>, usize, usize)> {
    let Value::Object(o) = v else { return None };
    match &o.kind {
        ObjKind::Typed(view) => Some((view.kind, view.buffer.clone(), view.offset, view.length)),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_arraybuffer(
    env: napi_env,
    byte_length: usize,
    data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let ab = typed::new_buffer(env.vm(), vec![0u8; byte_length]);
    if !data.is_null() {
        let (p, _) = buffer_ptr(&ab).unwrap_or((std::ptr::null_mut(), 0));
        unsafe { *data = p as *mut c_void };
    }
    out!(env, result, env.put(Value::Object(ab)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external_arraybuffer(
    env: napi_env,
    external_data: *mut c_void,
    byte_length: usize,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    let ab = unsafe { typed::new_external_buffer(env.vm(), external_data as *mut u8, byte_length) };
    attach_finalizer(env, envp, &ab, external_data, finalize_cb, finalize_hint);
    out!(env, result, env.put(Value::Object(ab)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_arraybuffer_info(
    env: napi_env,
    arraybuffer: napi_value,
    data: *mut *mut c_void,
    byte_length: *mut usize,
) -> napi_status {
    let env = env!(env);
    let Value::Object(o) = env.get(arraybuffer) else {
        return env.status(ARRAYBUFFER_EXPECTED);
    };
    let Some((p, len)) = buffer_ptr(&o) else {
        return env.status(ARRAYBUFFER_EXPECTED);
    };
    if !data.is_null() {
        unsafe { *data = p as *mut c_void };
    }
    if !byte_length.is_null() {
        unsafe { *byte_length = len };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_arraybuffer(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is =
        matches!(env.get(value), Value::Object(o) if matches!(o.kind, ObjKind::ArrayBuffer(_)));
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_detach_arraybuffer(
    env: napi_env,
    arraybuffer: napi_value,
) -> napi_status {
    let env = env!(env);
    let Value::Object(o) = env.get(arraybuffer) else {
        return env.status(ARRAYBUFFER_EXPECTED);
    };
    if !matches!(o.kind, ObjKind::ArrayBuffer(_)) {
        return env.status(DETACHABLE_EXPECTED);
    }
    typed::detach(&o);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_detached_arraybuffer(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = matches!(env.get(value), Value::Object(o) if matches!(o.kind, ObjKind::ArrayBuffer(_)) && typed::is_detached(&o));
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_typedarray(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = view_parts(&env.get(value)).is_some();
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_typedarray(
    env: napi_env,
    kind: i32,
    length: usize,
    arraybuffer: napi_value,
    byte_offset: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(k) = TypedKind::ALL.get(kind as usize).copied() else {
        return env.status(INVALID_ARG);
    };
    let Value::Object(ab) = env.get(arraybuffer) else {
        return env.status(ARRAYBUFFER_EXPECTED);
    };
    if !matches!(ab.kind, ObjKind::ArrayBuffer(_)) {
        return env.status(ARRAYBUFFER_EXPECTED);
    }
    if byte_offset % k.size() != 0 {
        let e = env.vm().range_error(format!(
            "start offset of {} should be a multiple of {}",
            k.name(),
            k.size()
        ));
        return env.throw_pending(e);
    }
    let v = typed::new_typed(env.vm(), k, ab, byte_offset, length);
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_typedarray_info(
    env: napi_env,
    typedarray: napi_value,
    kind: *mut i32,
    length: *mut usize,
    data: *mut *mut c_void,
    arraybuffer: *mut napi_value,
    byte_offset: *mut usize,
) -> napi_status {
    let env = env!(env);
    let Some((k, ab, offset, len)) = view_parts(&env.get(typedarray)) else {
        return env.status(INVALID_ARG);
    };
    if !kind.is_null() {
        unsafe { *kind = k as i32 };
    }
    if !length.is_null() {
        unsafe { *length = len };
    }
    if !data.is_null() {
        let (p, _) = buffer_ptr(&ab).unwrap_or((std::ptr::null_mut(), 0));
        unsafe { *data = if p.is_null() { p } else { p.add(offset) } as *mut c_void };
    }
    if !byte_offset.is_null() {
        unsafe { *byte_offset = offset };
    }
    if !arraybuffer.is_null() {
        let h = env.put(Value::Object(ab));
        unsafe { *arraybuffer = h };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_dataview(
    env: napi_env,
    length: usize,
    arraybuffer: napi_value,
    byte_offset: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let ab = env.get(arraybuffer);
    let dv = env.global("DataView");
    let v = js!(
        env,
        env.vm().construct(
            &dv,
            &[
                ab,
                Value::Number(byte_offset as f64),
                Value::Number(length as f64)
            ]
        )
    );
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_dataview(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = matches!(env.get(value), Value::Object(o) if matches!(o.kind, ObjKind::DataView(_)));
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_dataview_info(
    env: napi_env,
    dataview: napi_value,
    byte_length: *mut usize,
    data: *mut *mut c_void,
    arraybuffer: *mut napi_value,
    byte_offset: *mut usize,
) -> napi_status {
    let env = env!(env);
    let Value::Object(o) = env.get(dataview) else {
        return env.status(INVALID_ARG);
    };
    let ObjKind::DataView(view) = &o.kind else {
        return env.status(INVALID_ARG);
    };
    let (ab, offset, len) = (view.buffer.clone(), view.offset, view.length);
    if !byte_length.is_null() {
        unsafe { *byte_length = len };
    }
    if !data.is_null() {
        let (p, _) = buffer_ptr(&ab).unwrap_or((std::ptr::null_mut(), 0));
        unsafe { *data = if p.is_null() { p } else { p.add(offset) } as *mut c_void };
    }
    if !byte_offset.is_null() {
        unsafe { *byte_offset = offset };
    }
    if !arraybuffer.is_null() {
        let h = env.put(Value::Object(ab));
        unsafe { *arraybuffer = h };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_buffer(
    env: napi_env,
    size: usize,
    data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let ab = typed::new_buffer(env.vm(), vec![0u8; size]);
    if !data.is_null() {
        let (p, _) = buffer_ptr(&ab).unwrap_or((std::ptr::null_mut(), 0));
        unsafe { *data = p as *mut c_void };
    }
    let v = js!(env, to_node_buffer(env, ab, 0, size));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_buffer_copy(
    env: napi_env,
    length: usize,
    data: *const c_void,
    result_data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let bytes = if data.is_null() || length == 0 {
        vec![0u8; length]
    } else {
        unsafe { std::slice::from_raw_parts(data as *const u8, length) }.to_vec()
    };
    let ab = typed::new_buffer(env.vm(), bytes);
    if !result_data.is_null() {
        let (p, _) = buffer_ptr(&ab).unwrap_or((std::ptr::null_mut(), 0));
        unsafe { *result_data = p as *mut c_void };
    }
    let v = js!(env, to_node_buffer(env, ab, 0, length));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external_buffer(
    env: napi_env,
    length: usize,
    data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    let ab = unsafe { typed::new_external_buffer(env.vm(), data as *mut u8, length) };
    attach_finalizer(env, envp, &ab, data, finalize_cb, finalize_hint);
    let v = js!(env, to_node_buffer(env, ab, 0, length));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_buffer_from_arraybuffer(
    env: napi_env,
    arraybuffer: napi_value,
    byte_offset: usize,
    byte_length: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let Value::Object(ab) = env.get(arraybuffer) else {
        return env.status(ARRAYBUFFER_EXPECTED);
    };
    let v = js!(env, to_node_buffer(env, ab, byte_offset, byte_length));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_buffer(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = matches!(view_parts(&env.get(value)), Some((TypedKind::Uint8, ..)));
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_buffer_info(
    env: napi_env,
    value: napi_value,
    data: *mut *mut c_void,
    length: *mut usize,
) -> napi_status {
    let env = env!(env);
    let Some((_, ab, offset, len)) = view_parts(&env.get(value)) else {
        return env.status(INVALID_ARG);
    };
    if !data.is_null() {
        let (p, _) = buffer_ptr(&ab).unwrap_or((std::ptr::null_mut(), 0));
        unsafe { *data = if p.is_null() { p } else { p.add(offset) } as *mut c_void };
    }
    if !length.is_null() {
        unsafe { *length = len };
    }
    env.status(OK)
}
