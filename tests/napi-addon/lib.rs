#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr::null_mut;
use std::sync::OnceLock;

type Env = *mut c_void;
type Val = *mut c_void;
type Info = *mut c_void;
type Callback = unsafe extern "C" fn(Env, Info) -> Val;
type Finalize = unsafe extern "C" fn(Env, *mut c_void, *mut c_void);
type Execute = unsafe extern "C" fn(Env, *mut c_void);
type Complete = unsafe extern "C" fn(Env, i32, *mut c_void);
type CallJs = unsafe extern "C" fn(Env, Val, *mut c_void, *mut c_void);

#[repr(C)]
struct Property {
    utf8name: *const c_char,
    name: Val,
    method: Option<Callback>,
    getter: Option<Callback>,
    setter: Option<Callback>,
    value: Val,
    attributes: i32,
    data: *mut c_void,
}

#[cfg(windows)]
fn symbol(name: &CStr) -> usize {
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }
    unsafe { GetProcAddress(GetModuleHandleW(std::ptr::null()), name.as_ptr()) as usize }
}

#[cfg(unix)]
fn symbol(name: &CStr) -> usize {
    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    }
    #[cfg(target_vendor = "apple")]
    let handle = -2isize as *mut c_void;
    #[cfg(not(target_vendor = "apple"))]
    let handle = null_mut();
    unsafe { dlsym(handle, name.as_ptr()) as usize }
}

macro_rules! napi {
    ($($name:ident($($arg:ident: $ty:ty),*);)*) => {
        $(
            #[allow(clippy::too_many_arguments)]
            unsafe fn $name($($arg: $ty),*) -> i32 {
                static F: OnceLock<usize> = OnceLock::new();
                let f = *F.get_or_init(|| {
                    symbol(&CString::new(stringify!($name)).unwrap())
                });
                assert!(f != 0, "missing {}", stringify!($name));
                let f: unsafe extern "C" fn($($ty),*) -> i32 = unsafe { std::mem::transmute(f) };
                unsafe { f($($arg),*) }
            }
        )*
    };
}

napi! {
    napi_create_function(env: Env, name: *const c_char, len: usize, cb: Callback, data: *mut c_void, result: *mut Val);
    napi_get_cb_info(env: Env, info: Info, argc: *mut usize, argv: *mut Val, this: *mut Val, data: *mut *mut c_void);
    napi_get_value_string_utf8(env: Env, v: Val, buf: *mut c_char, size: usize, result: *mut usize);
    napi_create_string_utf8(env: Env, s: *const c_char, len: usize, result: *mut Val);
    napi_get_value_double(env: Env, v: Val, result: *mut f64);
    napi_create_double(env: Env, v: f64, result: *mut Val);
    napi_create_object(env: Env, result: *mut Val);
    napi_set_named_property(env: Env, obj: Val, name: *const c_char, v: Val);
    napi_get_named_property(env: Env, obj: Val, name: *const c_char, result: *mut Val);
    napi_create_array_with_length(env: Env, len: usize, result: *mut Val);
    napi_set_element(env: Env, arr: Val, index: u32, v: Val);
    napi_define_class(env: Env, name: *const c_char, len: usize, ctor: Callback, data: *mut c_void, count: usize, props: *const Property, result: *mut Val);
    napi_wrap(env: Env, obj: Val, native: *mut c_void, fin: Option<Finalize>, hint: *mut c_void, result: *mut Val);
    napi_unwrap(env: Env, obj: Val, result: *mut *mut c_void);
    napi_throw_error(env: Env, code: *const c_char, msg: *const c_char);
    napi_create_promise(env: Env, deferred: *mut *mut c_void, result: *mut Val);
    napi_resolve_deferred(env: Env, deferred: *mut c_void, v: Val);
    napi_create_async_work(env: Env, resource: Val, name: Val, exec: Execute, done: Complete, data: *mut c_void, result: *mut *mut c_void);
    napi_queue_async_work(env: Env, work: *mut c_void);
    napi_delete_async_work(env: Env, work: *mut c_void);
    napi_create_threadsafe_function(env: Env, func: Val, resource: Val, name: Val, queue: usize, threads: usize, fin_data: *mut c_void, fin: Option<Finalize>, ctx: *mut c_void, call: Option<CallJs>, result: *mut *mut c_void);
    napi_call_threadsafe_function(f: *mut c_void, data: *mut c_void, mode: i32);
    napi_release_threadsafe_function(f: *mut c_void, mode: i32);
    napi_call_function(env: Env, recv: Val, func: Val, argc: usize, argv: *const Val, result: *mut Val);
    napi_get_undefined(env: Env, result: *mut Val);
    napi_get_buffer_info(env: Env, v: Val, data: *mut *mut c_void, len: *mut usize);
    napi_create_buffer_copy(env: Env, len: usize, data: *const c_void, out: *mut *mut c_void, result: *mut Val);
    napi_typeof(env: Env, v: Val, result: *mut i32);
    napi_create_reference(env: Env, v: Val, count: u32, result: *mut *mut c_void);
    napi_get_reference_value(env: Env, r: *mut c_void, result: *mut Val);
    napi_delete_reference(env: Env, r: *mut c_void);
    napi_get_version(env: Env, result: *mut u32);
}

unsafe fn args<const N: usize>(env: Env, info: Info) -> ([Val; N], Val) {
    let mut argv = [null_mut(); N];
    let mut argc = N;
    let mut this = null_mut();
    unsafe {
        napi_get_cb_info(
            env,
            info,
            &mut argc,
            argv.as_mut_ptr(),
            &mut this,
            null_mut(),
        )
    };
    (argv, this)
}

unsafe fn string(env: Env, v: Val) -> String {
    let mut len = 0;
    unsafe { napi_get_value_string_utf8(env, v, null_mut(), 0, &mut len) };
    let mut buf = vec![0u8; len + 1];
    unsafe { napi_get_value_string_utf8(env, v, buf.as_mut_ptr().cast(), len + 1, &mut len) };
    buf.truncate(len);
    String::from_utf8(buf).unwrap()
}

unsafe fn js_string(env: Env, s: &str) -> Val {
    let mut v = null_mut();
    unsafe { napi_create_string_utf8(env, s.as_ptr().cast(), s.len(), &mut v) };
    v
}

unsafe fn number(env: Env, v: Val) -> f64 {
    let mut n = 0.0;
    unsafe { napi_get_value_double(env, v, &mut n) };
    n
}

unsafe fn js_number(env: Env, n: f64) -> Val {
    let mut v = null_mut();
    unsafe { napi_create_double(env, n, &mut v) };
    v
}

unsafe fn undefined(env: Env) -> Val {
    let mut v = null_mut();
    unsafe { napi_get_undefined(env, &mut v) };
    v
}

unsafe extern "C" fn hello(env: Env, info: Info) -> Val {
    let ([name], _) = unsafe { args::<1>(env, info) };
    let name = unsafe { string(env, name) };
    unsafe { js_string(env, &format!("hello {name}")) }
}

unsafe extern "C" fn add(env: Env, info: Info) -> Val {
    let ([a, b], _) = unsafe { args::<2>(env, info) };
    unsafe { js_number(env, number(env, a) + number(env, b)) }
}

unsafe extern "C" fn make_object(env: Env, _info: Info) -> Val {
    let mut obj = null_mut();
    let mut arr = null_mut();
    unsafe {
        napi_create_object(env, &mut obj);
        napi_set_named_property(env, obj, c"n".as_ptr(), js_number(env, 1.0));
        napi_set_named_property(env, obj, c"s".as_ptr(), js_string(env, "x"));
        napi_create_array_with_length(env, 3, &mut arr);
        for i in 0..3 {
            napi_set_element(env, arr, i, js_number(env, (i + 1) as f64));
        }
        napi_set_named_property(env, obj, c"arr".as_ptr(), arr);
    }
    obj
}

unsafe extern "C" fn fail(env: Env, _info: Info) -> Val {
    unsafe { napi_throw_error(env, c"E_TEST".as_ptr(), c"native failure".as_ptr()) };
    null_mut()
}

unsafe extern "C" fn type_names(env: Env, info: Info) -> Val {
    let ([a, b, c, d], _) = unsafe { args::<4>(env, info) };
    let mut out = String::new();
    for v in [a, b, c, d] {
        let mut t = -1;
        unsafe { napi_typeof(env, v, &mut t) };
        out.push_str(&t.to_string());
    }
    unsafe { js_string(env, &out) }
}

unsafe extern "C" fn buffer_sum(env: Env, info: Info) -> Val {
    let ([buf], _) = unsafe { args::<1>(env, info) };
    let mut data = null_mut();
    let mut len = 0;
    unsafe { napi_get_buffer_info(env, buf, &mut data, &mut len) };
    let bytes = unsafe { std::slice::from_raw_parts(data as *const u8, len) };
    unsafe { js_number(env, bytes.iter().map(|b| *b as f64).sum()) }
}

unsafe extern "C" fn reverse_buffer(env: Env, info: Info) -> Val {
    let ([buf], _) = unsafe { args::<1>(env, info) };
    let mut data = null_mut();
    let mut len = 0;
    unsafe { napi_get_buffer_info(env, buf, &mut data, &mut len) };
    let mut bytes = unsafe { std::slice::from_raw_parts(data as *const u8, len) }.to_vec();
    bytes.reverse();
    let mut out = null_mut();
    let mut result = null_mut();
    unsafe { napi_create_buffer_copy(env, len, bytes.as_ptr().cast(), &mut out, &mut result) };
    result
}

struct Counter {
    value: f64,
}

unsafe extern "C" fn counter_finalize(_env: Env, data: *mut c_void, _hint: *mut c_void) {
    drop(unsafe { Box::from_raw(data as *mut Counter) });
}

unsafe extern "C" fn counter_new(env: Env, info: Info) -> Val {
    let ([start], this) = unsafe { args::<1>(env, info) };
    let counter = Box::into_raw(Box::new(Counter {
        value: unsafe { number(env, start) },
    }));
    unsafe {
        napi_wrap(
            env,
            this,
            counter.cast(),
            Some(counter_finalize),
            null_mut(),
            null_mut(),
        )
    };
    this
}

unsafe fn counter_of(env: Env, this: Val) -> Option<&'static mut Counter> {
    let mut p = null_mut();
    let status = unsafe { napi_unwrap(env, this, &mut p) };
    if status != 0 || p.is_null() {
        unsafe {
            napi_throw_error(
                env,
                c"E_NOT_COUNTER".as_ptr(),
                c"receiver is not a Counter".as_ptr(),
            )
        };
        return None;
    }
    Some(unsafe { &mut *(p as *mut Counter) })
}

unsafe extern "C" fn counter_inc(env: Env, info: Info) -> Val {
    let ([by], this) = unsafe { args::<1>(env, info) };
    let Some(c) = (unsafe { counter_of(env, this) }) else {
        return null_mut();
    };
    let mut t = 0;
    unsafe { napi_typeof(env, by, &mut t) };
    c.value += if t == 3 {
        unsafe { number(env, by) }
    } else {
        1.0
    };
    unsafe { js_number(env, c.value) }
}

unsafe extern "C" fn counter_value(env: Env, info: Info) -> Val {
    let (_, this) = unsafe { args::<0>(env, info) };
    match unsafe { counter_of(env, this) } {
        Some(c) => unsafe { js_number(env, c.value) },
        None => null_mut(),
    }
}

struct SumJob {
    n: u64,
    total: u64,
    deferred: *mut c_void,
    work: *mut c_void,
}

unsafe extern "C" fn sum_execute(_env: Env, data: *mut c_void) {
    let job = unsafe { &mut *(data as *mut SumJob) };
    job.total = (0..=job.n).sum();
}

unsafe extern "C" fn sum_complete(env: Env, _status: i32, data: *mut c_void) {
    let job = unsafe { Box::from_raw(data as *mut SumJob) };
    unsafe {
        napi_resolve_deferred(env, job.deferred, js_number(env, job.total as f64));
        napi_delete_async_work(env, job.work);
    }
}

unsafe extern "C" fn async_sum(env: Env, info: Info) -> Val {
    let ([n], _) = unsafe { args::<1>(env, info) };
    let mut promise = null_mut();
    let job = Box::into_raw(Box::new(SumJob {
        n: unsafe { number(env, n) } as u64,
        total: 0,
        deferred: null_mut(),
        work: null_mut(),
    }));
    unsafe {
        napi_create_promise(env, &mut (*job).deferred, &mut promise);
        napi_create_async_work(
            env,
            null_mut(),
            js_string(env, "sum"),
            sum_execute,
            sum_complete,
            job.cast(),
            &mut (*job).work,
        );
        napi_queue_async_work(env, (*job).work);
    }
    promise
}

unsafe extern "C" fn tick_call(env: Env, cb: Val, _ctx: *mut c_void, data: *mut c_void) {
    let n = unsafe { Box::from_raw(data as *mut u32) };
    let arg = unsafe { js_number(env, *n as f64) };
    let mut out = null_mut();
    unsafe { napi_call_function(env, undefined(env), cb, 1, &arg, &mut out) };
}

struct Tsfn(*mut c_void);

unsafe impl Send for Tsfn {}

unsafe extern "C" fn ticks(env: Env, info: Info) -> Val {
    let ([cb, count], _) = unsafe { args::<2>(env, info) };
    let count = unsafe { number(env, count) } as u32;
    let mut f = null_mut();
    unsafe {
        napi_create_threadsafe_function(
            env,
            cb,
            null_mut(),
            js_string(env, "ticks"),
            0,
            1,
            null_mut(),
            None,
            null_mut(),
            Some(tick_call),
            &mut f,
        )
    };
    let f = Tsfn(f);
    std::thread::spawn(move || {
        let f = f;
        for i in 0..count {
            let data = Box::into_raw(Box::new(i));
            unsafe { napi_call_threadsafe_function(f.0, data.cast(), 1) };
        }
        unsafe { napi_release_threadsafe_function(f.0, 0) };
    });
    unsafe { undefined(env) }
}

unsafe extern "C" fn keep(env: Env, info: Info) -> Val {
    let ([v], _) = unsafe { args::<1>(env, info) };
    let mut r = null_mut();
    let mut out = null_mut();
    unsafe {
        napi_create_reference(env, v, 1, &mut r);
        napi_get_reference_value(env, r, &mut out);
        napi_delete_reference(env, r);
    }
    out
}

unsafe extern "C" fn call_back(env: Env, info: Info) -> Val {
    let ([f, x], _) = unsafe { args::<2>(env, info) };
    let mut out = null_mut();
    let status = unsafe { napi_call_function(env, undefined(env), f, 1, &x, &mut out) };
    if status != 0 {
        return null_mut();
    }
    let mut field = null_mut();
    unsafe { napi_get_named_property(env, out, c"value".as_ptr(), &mut field) };
    field
}

unsafe fn export(env: Env, exports: Val, name: &CStr, cb: Callback) {
    let mut f = null_mut();
    unsafe {
        napi_create_function(env, name.as_ptr(), usize::MAX, cb, null_mut(), &mut f);
        napi_set_named_property(env, exports, name.as_ptr(), f);
    }
}

fn method(name: &'static CStr, cb: Callback) -> Property {
    Property {
        utf8name: name.as_ptr(),
        name: null_mut(),
        method: Some(cb),
        getter: None,
        setter: None,
        value: null_mut(),
        attributes: 0,
        data: null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_register_module_v1(env: Env, exports: Val) -> Val {
    unsafe {
        export(env, exports, c"hello", hello);
        export(env, exports, c"add", add);
        export(env, exports, c"makeObject", make_object);
        export(env, exports, c"fail", fail);
        export(env, exports, c"typeNames", type_names);
        export(env, exports, c"bufferSum", buffer_sum);
        export(env, exports, c"reverseBuffer", reverse_buffer);
        export(env, exports, c"asyncSum", async_sum);
        export(env, exports, c"ticks", ticks);
        export(env, exports, c"keep", keep);
        export(env, exports, c"callBack", call_back);
        let mut getter = method(c"value", counter_value);
        getter.method = None;
        getter.getter = Some(counter_value);
        let props = [method(c"inc", counter_inc), getter];
        let mut class = null_mut();
        napi_define_class(
            env,
            c"Counter".as_ptr(),
            usize::MAX,
            counter_new,
            null_mut(),
            props.len(),
            props.as_ptr(),
            &mut class,
        );
        napi_set_named_property(env, exports, c"Counter".as_ptr(), class);
        let mut version = 0;
        napi_get_version(env, &mut version);
        napi_set_named_property(
            env,
            exports,
            c"napiVersion".as_ptr(),
            js_number(env, version as f64),
        );
    }
    exports
}
