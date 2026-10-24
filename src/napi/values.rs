use super::*;
use crate::vm::ErrorKind;

fn units(s: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if crate::value::is_lone(c) {
            out.push(crate::value::lone_unit(c));
        } else {
            let mut buf = [0u16; 2];
            out.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    out
}

fn to_int32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    (n.trunc() as i64 as u64 & 0xFFFF_FFFF) as u32 as i32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_undefined(env: napi_env, result: *mut napi_value) -> napi_status {
    let env = env!(env);
    out!(env, result, env.put(Value::Undefined));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_null(env: napi_env, result: *mut napi_value) -> napi_status {
    let env = env!(env);
    out!(env, result, env.put(Value::Null));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_global(env: napi_env, result: *mut napi_value) -> napi_status {
    let env = env!(env);
    let g = env.vm().global_this();
    out!(env, result, env.put(g));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_boolean(
    env: napi_env,
    value: bool,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    out!(env, result, env.put(Value::Bool(value)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_double(
    env: napi_env,
    value: f64,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    out!(env, result, env.put(Value::Number(value)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_int32(
    env: napi_env,
    value: i32,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    out!(env, result, env.put(Value::Number(value as f64)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_uint32(
    env: napi_env,
    value: u32,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    out!(env, result, env.put(Value::Number(value as f64)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_int64(
    env: napi_env,
    value: i64,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    out!(env, result, env.put(Value::Number(value as f64)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_double(
    env: napi_env,
    value: napi_value,
    result: *mut f64,
) -> napi_status {
    let env = env!(env);
    let Value::Number(n) = env.get(value) else {
        return env.status(NUMBER_EXPECTED);
    };
    out!(env, result, n);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_int32(
    env: napi_env,
    value: napi_value,
    result: *mut i32,
) -> napi_status {
    let env = env!(env);
    let Value::Number(n) = env.get(value) else {
        return env.status(NUMBER_EXPECTED);
    };
    out!(env, result, to_int32(n));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_uint32(
    env: napi_env,
    value: napi_value,
    result: *mut u32,
) -> napi_status {
    let env = env!(env);
    let Value::Number(n) = env.get(value) else {
        return env.status(NUMBER_EXPECTED);
    };
    out!(env, result, to_int32(n) as u32);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_int64(
    env: napi_env,
    value: napi_value,
    result: *mut i64,
) -> napi_status {
    let env = env!(env);
    let Value::Number(n) = env.get(value) else {
        return env.status(NUMBER_EXPECTED);
    };
    let v = if n.is_finite() { n.trunc() as i64 } else { 0 };
    out!(env, result, v);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bool(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Value::Bool(b) = env.get(value) else {
        return env.status(BOOLEAN_EXPECTED);
    };
    out!(env, result, b);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_utf8(
    env: napi_env,
    s: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let text = unsafe { c_str(s, length) };
    out!(env, result, env.put(Value::Str(text.into())));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_property_key_utf8(
    env: napi_env,
    s: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_create_string_utf8(env, s, length, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_latin1(
    env: napi_env,
    s: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let text: String = if s.is_null() {
        String::new()
    } else {
        let bytes = if length == usize::MAX {
            unsafe { std::ffi::CStr::from_ptr(s) }.to_bytes()
        } else {
            unsafe { std::slice::from_raw_parts(s as *const u8, length) }
        };
        bytes.iter().map(|b| *b as char).collect()
    };
    out!(env, result, env.put(Value::Str(text.into())));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_property_key_latin1(
    env: napi_env,
    s: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_create_string_latin1(env, s, length, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_utf16(
    env: napi_env,
    s: *const u16,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let text = if s.is_null() {
        String::new()
    } else {
        let len = if length == usize::MAX {
            let mut n = 0;
            while unsafe { *s.add(n) } != 0 {
                n += 1;
            }
            n
        } else {
            length
        };
        crate::value::from16(unsafe { std::slice::from_raw_parts(s, len) })
    };
    out!(env, result, env.put(Value::Str(text.into())));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_property_key_utf16(
    env: napi_env,
    s: *const u16,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_create_string_utf16(env, s, length, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_external_string_latin1(
    env: napi_env,
    s: *mut c_char,
    length: usize,
    _finalize: napi_finalize,
    _hint: *mut c_void,
    result: *mut napi_value,
    copied: *mut bool,
) -> napi_status {
    if !copied.is_null() {
        unsafe { *copied = true };
    }
    unsafe { napi_create_string_latin1(env, s, length, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_external_string_utf16(
    env: napi_env,
    s: *mut u16,
    length: usize,
    _finalize: napi_finalize,
    _hint: *mut c_void,
    result: *mut napi_value,
    copied: *mut bool,
) -> napi_status {
    if !copied.is_null() {
        unsafe { *copied = true };
    }
    unsafe { napi_create_string_utf16(env, s, length, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_utf8(
    env: napi_env,
    value: napi_value,
    buf: *mut c_char,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    let env = env!(env);
    let Value::Str(s) = env.get(value) else {
        return env.status(STRING_EXPECTED);
    };
    let text = crate::value::utf8_text(&s);
    let bytes = text.as_bytes();
    if buf.is_null() {
        out!(env, result, bytes.len());
        return env.status(OK);
    }
    if bufsize == 0 {
        if !result.is_null() {
            unsafe { *result = 0 };
        }
        return env.status(OK);
    }
    let mut n = bytes.len().min(bufsize - 1);
    while n > 0 && !text.is_char_boundary(n) {
        n -= 1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    if !result.is_null() {
        unsafe { *result = n };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_latin1(
    env: napi_env,
    value: napi_value,
    buf: *mut c_char,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    let env = env!(env);
    let Value::Str(s) = env.get(value) else {
        return env.status(STRING_EXPECTED);
    };
    let bytes: Vec<u8> = units(&s).iter().map(|u| *u as u8).collect();
    if buf.is_null() {
        out!(env, result, bytes.len());
        return env.status(OK);
    }
    if bufsize == 0 {
        if !result.is_null() {
            unsafe { *result = 0 };
        }
        return env.status(OK);
    }
    let n = bytes.len().min(bufsize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    if !result.is_null() {
        unsafe { *result = n };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_utf16(
    env: napi_env,
    value: napi_value,
    buf: *mut u16,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    let env = env!(env);
    let Value::Str(s) = env.get(value) else {
        return env.status(STRING_EXPECTED);
    };
    let u = units(&s);
    if buf.is_null() {
        out!(env, result, u.len());
        return env.status(OK);
    }
    if bufsize == 0 {
        if !result.is_null() {
            unsafe { *result = 0 };
        }
        return env.status(OK);
    }
    let n = u.len().min(bufsize - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(u.as_ptr(), buf, n);
        *buf.add(n) = 0;
    }
    if !result.is_null() {
        unsafe { *result = n };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_symbol(
    env: napi_env,
    description: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let desc = env.get(description);
    let symbol = env.global("Symbol");
    let args = if desc.is_undefined() {
        Vec::new()
    } else {
        vec![desc]
    };
    let v = js!(env, env.vm().call_value(&symbol, Value::Undefined, &args));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_symbol_for(
    env: napi_env,
    s: *const c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let key = unsafe { c_str(s, length) };
    let symbol = env.global("Symbol");
    let f = js!(env, env.vm().get_prop(&symbol, "for"));
    let v = js!(
        env,
        env.vm().call_value(&f, symbol, &[Value::Str(key.into())])
    );
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_typeof(
    env: napi_env,
    value: napi_value,
    result: *mut i32,
) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    let t = match &v {
        Value::Undefined | Value::Uninit => 0,
        Value::Null => 1,
        Value::Bool(_) => 2,
        Value::Number(_) => 3,
        Value::Str(_) => 4,
        Value::Symbol(_) => 5,
        Value::BigInt(_) => 9,
        Value::Object(o) => {
            if matches!(o.kind, ObjKind::Host(_)) {
                8
            } else if v.is_callable() {
                7
            } else {
                6
            }
        }
    };
    out!(env, result, t);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_bool(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let b = env.get(value).truthy();
    out!(env, result, env.put(Value::Bool(b)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_number(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    let n = js!(env, env.vm().to_number(&v));
    out!(env, result, env.put(Value::Number(n)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_string(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    let s = js!(env, env.vm().to_string(&v));
    out!(env, result, env.put(Value::Str(s)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_object(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let v = env.get(value);
    let o = js!(env, env.vm().to_object(&v));
    out!(env, result, env.put(Value::Object(o)));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_strict_equals(
    env: napi_env,
    lhs: napi_value,
    rhs: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let eq = env.get(lhs).strict_eq(&env.get(rhs));
    out!(env, result, eq);
    env.status(OK)
}

fn make_error_value(
    env: &mut Env,
    kind: ErrorKind,
    code: Value,
    msg: Value,
) -> Result<Value, Value> {
    let text = env.vm().to_string(&msg)?;
    let err = env.vm().make_error(kind, text.to_string());
    if !code.is_nullish()
        && let Value::Object(o) = &err
    {
        o.set_own("code", code);
    }
    Ok(err)
}

unsafe fn create_error_kind(
    env: napi_env,
    kind: ErrorKind,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let (c, m) = (env.get(code), env.get(msg));
    if !matches!(m, Value::Str(_)) {
        return env.status(STRING_EXPECTED);
    }
    let e = js!(env, make_error_value(env, kind, c, m));
    out!(env, result, env.put(e));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_error(
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { create_error_kind(env, ErrorKind::Error, code, msg, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_type_error(
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { create_error_kind(env, ErrorKind::TypeError, code, msg, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_range_error(
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { create_error_kind(env, ErrorKind::RangeError, code, msg, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_syntax_error(
    env: napi_env,
    code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { create_error_kind(env, ErrorKind::SyntaxError, code, msg, result) }
}

unsafe fn throw_kind(
    env: napi_env,
    kind: ErrorKind,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    let env = env!(env);
    let text = unsafe { c_str(msg, usize::MAX) };
    let err = env.vm().make_error(kind, text);
    if !code.is_null() {
        let c = unsafe { c_str(code, usize::MAX) };
        if let Value::Object(o) = &err {
            o.set_own("code", Value::Str(c.into()));
        }
    }
    env.pending = Some(err);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw(env: napi_env, error: napi_value) -> napi_status {
    let env = env!(env);
    env.pending = Some(env.get(error));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_error(
    env: napi_env,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    unsafe { throw_kind(env, ErrorKind::Error, code, msg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_type_error(
    env: napi_env,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    unsafe { throw_kind(env, ErrorKind::TypeError, code, msg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_throw_range_error(
    env: napi_env,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    unsafe { throw_kind(env, ErrorKind::RangeError, code, msg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_throw_syntax_error(
    env: napi_env,
    code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    unsafe { throw_kind(env, ErrorKind::SyntaxError, code, msg) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_error(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = matches!(env.get(value), Value::Object(o) if matches!(o.kind, ObjKind::Error));
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_exception_pending(
    env: napi_env,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    out!(env, result, env.pending.is_some());
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_and_clear_last_exception(
    env: napi_env,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let e = env.pending.take().unwrap_or_default();
    out!(env, result, env.put(e));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_fatal_exception(env: napi_env, err: napi_value) -> napi_status {
    let env = env!(env);
    let e = env.get(err);
    let text = crate::inspect::inspect_plain(&e);
    eprintln!("Uncaught {text}");
    std::process::exit(1);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_function(
    env: napi_env,
    name: *const c_char,
    length: usize,
    cb: napi_callback,
    data: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let n = unsafe { c_str(name, length) };
    let f = js!(env, make_function(env, &n, cb, data));
    out!(env, result, env.put(f));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_cb_info(
    env: napi_env,
    cbinfo: napi_callback_info,
    argc: *mut usize,
    argv: *mut napi_value,
    this_arg: *mut napi_value,
    data: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    let Some(info) = (unsafe { cbinfo.as_ref() }) else {
        return env.status(INVALID_ARG);
    };
    if !argc.is_null() {
        let cap = unsafe { *argc };
        if !argv.is_null() {
            for i in 0..cap {
                let v = info.args.get(i).cloned().unwrap_or_default();
                let h = env.put(v);
                unsafe { *argv.add(i) = h };
            }
        }
        unsafe { *argc = info.args.len() };
    }
    if !this_arg.is_null() {
        let h = env.put(info.this.clone());
        unsafe { *this_arg = h };
    }
    if !data.is_null() {
        unsafe { *data = info.data };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_new_target(
    env: napi_env,
    cbinfo: napi_callback_info,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let Some(info) = (unsafe { cbinfo.as_ref() }) else {
        return env.status(INVALID_ARG);
    };
    let h = if info.new_target.is_undefined() {
        std::ptr::null_mut()
    } else {
        env.put(info.new_target.clone())
    };
    out!(env, result, h);
    env.status(OK)
}

fn arg_values(env: &Env, argc: usize, argv: *const napi_value) -> Vec<Value> {
    (0..argc)
        .map(|i| {
            if argv.is_null() {
                Value::Undefined
            } else {
                env.get(unsafe { *argv.add(i) })
            }
        })
        .collect()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_call_function(
    env: napi_env,
    recv: napi_value,
    func: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    if env.pending.is_some() {
        return env.status(PENDING_EXCEPTION);
    }
    let f = env.get(func);
    if !f.is_callable() {
        return env.status(FUNCTION_EXPECTED);
    }
    let this = env.get(recv);
    let args = arg_values(env, argc, argv);
    let v = js!(env, env.vm().call_value(&f, this, &args));
    if !result.is_null() {
        let h = env.put(v);
        unsafe { *result = h };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_make_callback(
    env: napi_env,
    _context: *mut c_void,
    recv: napi_value,
    func: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_call_function(env, recv, func, argc, argv, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_new_instance(
    env: napi_env,
    cons: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    if env.pending.is_some() {
        return env.status(PENDING_EXCEPTION);
    }
    let c = env.get(cons);
    let args = arg_values(env, argc, argv);
    let v = js!(env, env.vm().construct(&c, &args));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_instanceof(
    env: napi_env,
    object: napi_value,
    constructor: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let (o, c) = (env.get(object), env.get(constructor));
    if !c.is_callable() {
        return env.status(FUNCTION_EXPECTED);
    }
    let r = js!(env, env.vm().instance_of(&o, &c));
    out!(env, result, r);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_run_script(
    env: napi_env,
    script: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let Value::Str(src) = env.get(script) else {
        return env.status(STRING_EXPECTED);
    };
    let v = js!(env, env.vm().indirect_eval(&src));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_int64(
    env: napi_env,
    value: i64,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let v = Value::BigInt(Rc::new(num_bigint::BigInt::from(value)));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_uint64(
    env: napi_env,
    value: u64,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let v = Value::BigInt(Rc::new(num_bigint::BigInt::from(value)));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_bigint_words(
    env: napi_env,
    sign_bit: i32,
    word_count: usize,
    words: *const u64,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let list = if words.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(words, word_count) }
    };
    let mut digits = Vec::with_capacity(list.len() * 2);
    for w in list {
        digits.push(*w as u32);
        digits.push((*w >> 32) as u32);
    }
    let mag = num_bigint::BigUint::new(digits);
    let sign = if sign_bit != 0 {
        num_bigint::Sign::Minus
    } else {
        num_bigint::Sign::Plus
    };
    let v = Value::BigInt(Rc::new(num_bigint::BigInt::from_biguint(sign, mag)));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_int64(
    env: napi_env,
    value: napi_value,
    result: *mut i64,
    lossless: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Value::BigInt(b) = env.get(value) else {
        return env.status(BIGINT_EXPECTED);
    };
    let (sign, digits) = b.to_u64_digits();
    let low = digits.first().copied().unwrap_or(0);
    let v = if sign == num_bigint::Sign::Minus {
        (low as i64).wrapping_neg()
    } else {
        low as i64
    };
    out!(env, result, v);
    if !lossless.is_null() {
        unsafe { *lossless = num_bigint::BigInt::from(v) == *b };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_uint64(
    env: napi_env,
    value: napi_value,
    result: *mut u64,
    lossless: *mut bool,
) -> napi_status {
    let env = env!(env);
    let Value::BigInt(b) = env.get(value) else {
        return env.status(BIGINT_EXPECTED);
    };
    let (sign, digits) = b.to_u64_digits();
    let low = digits.first().copied().unwrap_or(0);
    let v = if sign == num_bigint::Sign::Minus {
        low.wrapping_neg()
    } else {
        low
    };
    out!(env, result, v);
    if !lossless.is_null() {
        unsafe { *lossless = sign != num_bigint::Sign::Minus && digits.len() <= 1 };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_bigint_words(
    env: napi_env,
    value: napi_value,
    sign_bit: *mut i32,
    word_count: *mut usize,
    words: *mut u64,
) -> napi_status {
    let env = env!(env);
    let Value::BigInt(b) = env.get(value) else {
        return env.status(BIGINT_EXPECTED);
    };
    let (sign, digits) = b.to_u64_digits();
    if word_count.is_null() {
        return env.status(INVALID_ARG);
    }
    if words.is_null() {
        unsafe { *word_count = digits.len() };
        return env.status(OK);
    }
    let cap = unsafe { *word_count };
    for (i, d) in digits.iter().take(cap).enumerate() {
        unsafe { *words.add(i) = *d };
    }
    unsafe { *word_count = digits.len().min(cap) };
    if !sign_bit.is_null() {
        unsafe { *sign_bit = i32::from(sign == num_bigint::Sign::Minus) };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_date(
    env: napi_env,
    time: f64,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let date = env.global("Date");
    let v = js!(env, env.vm().construct(&date, &[Value::Number(time)]));
    out!(env, result, env.put(v));
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_date(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = matches!(env.get(value), Value::Object(o) if matches!(o.kind, ObjKind::Date(_)));
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_date_value(
    env: napi_env,
    value: napi_value,
    result: *mut f64,
) -> napi_status {
    let env = env!(env);
    let Value::Object(o) = env.get(value) else {
        return env.status(DATE_EXPECTED);
    };
    let ObjKind::Date(t) = &o.kind else {
        return env.status(DATE_EXPECTED);
    };
    out!(env, result, t.get());
    env.status(OK)
}
