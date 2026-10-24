#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::object::{Bound, Obj, ObjKind};
use crate::value::Value;
use crate::vm::Vm;
use std::cell::RefCell;
use std::ffi::{CString, c_char, c_void};
use std::rc::Rc;

pub type napi_status = i32;
pub const OK: napi_status = 0;
pub const INVALID_ARG: napi_status = 1;
pub const OBJECT_EXPECTED: napi_status = 2;
pub const STRING_EXPECTED: napi_status = 3;
pub const NAME_EXPECTED: napi_status = 4;
pub const FUNCTION_EXPECTED: napi_status = 5;
pub const NUMBER_EXPECTED: napi_status = 6;
pub const BOOLEAN_EXPECTED: napi_status = 7;
pub const ARRAY_EXPECTED: napi_status = 8;
pub const GENERIC_FAILURE: napi_status = 9;
pub const PENDING_EXCEPTION: napi_status = 10;
pub const ESCAPE_CALLED_TWICE: napi_status = 12;
pub const BIGINT_EXPECTED: napi_status = 17;
pub const DATE_EXPECTED: napi_status = 18;
pub const ARRAYBUFFER_EXPECTED: napi_status = 19;
pub const DETACHABLE_EXPECTED: napi_status = 20;

pub type napi_env = *mut Env;
pub type napi_value = *mut c_void;
pub type napi_callback_info = *mut CallbackInfo;
pub type napi_callback = Option<unsafe extern "C" fn(napi_env, napi_callback_info) -> napi_value>;
pub type napi_finalize = Option<unsafe extern "C" fn(napi_env, *mut c_void, *mut c_void)>;

#[repr(C)]
pub struct napi_extended_error_info {
    pub error_message: *const c_char,
    pub engine_reserved: *mut c_void,
    pub engine_error_code: u32,
    pub error_code: napi_status,
}

#[repr(C)]
pub struct napi_module {
    pub nm_version: i32,
    pub nm_flags: u32,
    pub nm_filename: *const c_char,
    pub nm_register_func: Option<unsafe extern "C" fn(napi_env, napi_value) -> napi_value>,
    pub nm_modname: *const c_char,
    pub nm_priv: *mut c_void,
    pub reserved: [*mut c_void; 4],
}

pub struct CallbackInfo {
    pub(crate) this: Value,
    pub(crate) args: Vec<Value>,
    pub(crate) new_target: Value,
    pub(crate) data: *mut c_void,
}

pub struct Env {
    pub(crate) vm: *mut Vm,
    pub(crate) handles: Vec<Value>,
    pub(crate) scopes: Vec<usize>,
    pub(crate) pending: Option<Value>,
    pub(crate) error: napi_extended_error_info,
    pub(crate) messages: Vec<CString>,
    pub(crate) instance: (*mut c_void, napi_finalize, *mut c_void),
    pub(crate) filename: CString,
    pub(crate) tx: std::sync::mpsc::Sender<crate::io::IoEvent>,
    pub(crate) finalizer_ids: u64,
}

macro_rules! env {
    ($e:expr) => {
        match unsafe { $e.as_mut() } {
            Some(e) => e,
            None => return $crate::napi::INVALID_ARG,
        }
    };
}

macro_rules! out {
    ($env:expr, $p:expr, $v:expr) => {{
        if $p.is_null() {
            return $env.status($crate::napi::INVALID_ARG);
        }
        let value = $v;
        unsafe { *$p = value };
    }};
}

macro_rules! js {
    ($env:expr, $r:expr) => {
        match $r {
            Ok(v) => v,
            Err(e) => return $env.throw_pending(e),
        }
    };
}

mod buffers;
mod objects;
mod uv;
mod values;
mod work;

pub use work::{NapiEvent, handle_event, refed_tsfns};

const ERROR_MESSAGES: [&str; 24] = [
    "",
    "Invalid argument",
    "An object was expected",
    "A string was expected",
    "A string or symbol was expected",
    "A function was expected",
    "A number was expected",
    "A boolean was expected",
    "An array was expected",
    "Unknown failure",
    "An exception is pending",
    "The async work item was cancelled",
    "napi_escape_handle already called on scope",
    "Invalid handle scope usage",
    "Invalid callback scope usage",
    "Thread-safe function queue is full",
    "Thread-safe function handle is closing",
    "A bigint was expected",
    "A date was expected",
    "An arraybuffer was expected",
    "A detachable arraybuffer was expected",
    "Main thread would deadlock",
    "External buffers are not allowed",
    "Cannot run JavaScript",
];

impl Env {
    pub(crate) fn vm(&mut self) -> &mut Vm {
        unsafe { &mut *self.vm }
    }

    pub(crate) fn get(&self, h: napi_value) -> Value {
        let i = h as usize;
        if i == 0 {
            return Value::Undefined;
        }
        self.handles.get(i - 1).cloned().unwrap_or_default()
    }

    pub(crate) fn put(&mut self, v: Value) -> napi_value {
        self.handles.push(v);
        self.handles.len() as napi_value
    }

    pub(crate) fn status(&mut self, s: napi_status) -> napi_status {
        self.error.error_code = s;
        self.error.error_message = std::ptr::null();
        s
    }

    pub(crate) fn throw_pending(&mut self, e: Value) -> napi_status {
        self.pending = Some(e);
        self.status(PENDING_EXCEPTION)
    }

    pub(crate) fn global(&mut self, name: &str) -> Value {
        let vm = self.vm();
        if vm.globals.get(name).is_none() {
            let _ = vm.load_lazy_global(name);
        }
        vm.globals.get(name).cloned().unwrap_or_default()
    }
}

pub struct HostData {
    pub ptr: *mut c_void,
    pub fin: RefCell<Option<(napi_finalize, napi_env, *mut c_void)>>,
    pub tag: Option<[u64; 2]>,
}

thread_local! {
    static FINALIZERS: RefCell<Vec<(napi_finalize, napi_env, *mut c_void, *mut c_void)>> =
        const { RefCell::new(Vec::new()) };
    static PENDING_MODULE: RefCell<Option<*mut napi_module>> = const { RefCell::new(None) };
    static HIDDEN: RefCell<Option<(Value, Value)>> = const { RefCell::new(None) };
}

impl Drop for HostData {
    fn drop(&mut self) {
        if let Some((cb, env, hint)) = self.fin.get_mut().take() {
            let ptr = self.ptr;
            FINALIZERS.with(|f| f.borrow_mut().push((cb, env, ptr, hint)));
        }
    }
}

pub fn has_finalizers() -> bool {
    FINALIZERS.with(|f| !f.borrow().is_empty())
}

pub fn run_finalizers(vm: &mut Vm) {
    loop {
        let batch = FINALIZERS.with(|f| std::mem::take(&mut *f.borrow_mut()));
        if batch.is_empty() {
            return;
        }
        for (cb, env, ptr, hint) in batch {
            let Some(cb) = cb else { continue };
            let Some(e) = (unsafe { env.as_mut() }) else {
                continue;
            };
            e.vm = vm;
            let mark = e.handles.len();
            unsafe { cb(env, ptr, hint) };
            e.handles.truncate(mark);
            e.pending = None;
        }
    }
}

pub(crate) fn host_value(vm: &Vm, data: HostData) -> Value {
    Value::Object(Obj::new(
        ObjKind::Host(Box::new(data)),
        Some(vm.realm.object_proto.clone()),
    ))
}

pub(crate) fn host_of(v: &Value) -> Option<&HostData> {
    match v {
        Value::Object(o) => match &o.kind {
            ObjKind::Host(h) => Some(h),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn hidden_keys(vm: &mut Vm) -> (Value, Value) {
    if let Some(k) = HIDDEN.with(|h| h.borrow().clone()) {
        return k;
    }
    let symbol = vm.globals.get("Symbol").cloned().unwrap_or_default();
    let wrap = vm
        .call_value(&symbol, Value::Undefined, &[Value::Str("napi.wrap".into())])
        .unwrap_or_default();
    let tag = vm
        .call_value(&symbol, Value::Undefined, &[Value::Str("napi.tag".into())])
        .unwrap_or_default();
    let keys = (wrap, tag);
    HIDDEN.with(|h| *h.borrow_mut() = Some(keys.clone()));
    keys
}

struct FnRecord {
    env: napi_env,
    cb: napi_callback,
    data: *mut c_void,
    holder: Option<Rc<Obj>>,
}

fn holds(vm: &mut Vm, this: &Value, holder: &Rc<Obj>) -> Result<bool, Value> {
    let Value::Object(o) = this else {
        return Ok(false);
    };
    let mut cur = vm.op_get_prototype_of(o)?;
    while let Some(p) = cur {
        if Rc::ptr_eq(&p, holder) {
            return Ok(true);
        }
        cur = vm.op_get_prototype_of(&p)?;
    }
    Ok(false)
}

fn invoke(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    let ptr = match args.first() {
        Some(Value::Number(n)) => *n as usize as *const FnRecord,
        _ => return Ok(Value::Undefined),
    };
    let rec = unsafe { &*ptr };
    let new_target = args.get(1).cloned().unwrap_or_default();
    let this = args.get(2).cloned().unwrap_or_default();
    if let Some(holder) = &rec.holder {
        if !holds(vm, &this, holder)? {
            return Err(vm.type_error("Illegal invocation"));
        }
    }
    let list = match args.get(3) {
        Some(Value::Object(o)) => match &o.kind {
            ObjKind::Array(a) => a.borrow().clone(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    let Some(env) = (unsafe { rec.env.as_mut() }) else {
        return Ok(Value::Undefined);
    };
    env.vm = vm;
    let mark = env.handles.len();
    let mut info = CallbackInfo {
        this,
        args: list,
        new_target,
        data: rec.data,
    };
    let r = match rec.cb {
        Some(cb) => unsafe { cb(rec.env, &mut info) },
        None => std::ptr::null_mut(),
    };
    let result = env.get(r);
    env.handles.truncate(mark);
    if let Some(e) = env.pending.take() {
        return Err(e);
    }
    Ok(result)
}

pub(crate) fn make_function(
    env: &mut Env,
    name: &str,
    cb: napi_callback,
    data: *mut c_void,
) -> Result<Value, Value> {
    make_method(env, name, cb, data, None)
}

pub(crate) fn make_method(
    env: &mut Env,
    name: &str,
    cb: napi_callback,
    data: *mut c_void,
    holder: Option<Rc<Obj>>,
) -> Result<Value, Value> {
    let rec = Box::into_raw(Box::new(FnRecord {
        env: env as *mut Env,
        cb,
        data,
        holder,
    }));
    let maker = env.global("%napiFunction%");
    let vm = env.vm();
    let target = crate::builtins::native_fn(vm, "", 3, invoke);
    let bound = Value::Object(Obj::new(
        ObjKind::Bound(Box::new(Bound {
            target,
            this: Value::Undefined,
            args: vec![Value::Number(rec as usize as f64)],
        })),
        Some(vm.realm.function_proto.clone()),
    ));
    vm.call_value(&maker, Value::Undefined, &[Value::Str(name.into()), bound])
}

pub(crate) unsafe fn c_str(p: *const c_char, len: usize) -> String {
    if p.is_null() {
        return String::new();
    }
    let bytes = if len == usize::MAX {
        unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes()
    } else {
        unsafe { std::slice::from_raw_parts(p as *const u8, len) }
    };
    String::from_utf8_lossy(bytes).into_owned()
}

fn open_library(path: &std::path::Path) -> Result<libloading::Library, libloading::Error> {
    #[cfg(windows)]
    {
        const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;
        unsafe {
            libloading::os::windows::Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH)
        }
        .map(Into::into)
    }
    #[cfg(not(windows))]
    {
        unsafe { libloading::Library::new(path) }
    }
}

fn file_url(path: &std::path::Path) -> String {
    let s = path.display().to_string().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

pub fn load(vm: &mut Vm, path: &std::path::Path) -> Result<Value, Value> {
    load_with(vm, path, None)
}

fn uses_v8_api(path: &std::path::Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    [
        &b"@v8@@"[..],
        b"_ZN2v8",
        b"_ZNK2v8",
        b"node_module_register",
    ]
    .iter()
    .any(|p| memchr::memmem::find(&bytes, p).is_some())
}

fn napi_sibling(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let name = path.file_name()?.to_str()?;
    let mut candidates = Vec::new();
    if let Some(at) = name.find("abi") {
        let digits = name[at + 3..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits > 0 {
            candidates.push(format!("{}napi{}", &name[..at], &name[at + 3 + digits..]));
        }
    }
    candidates.push("node.napi.node".to_string());
    candidates
        .into_iter()
        .map(|c| path.with_file_name(c))
        .find(|p| p.is_file() && !uses_v8_api(p))
}

fn dlopen_error(vm: &mut Vm, message: String) -> Value {
    let err = vm.make_error(crate::vm::ErrorKind::Error, message);
    if let Value::Object(o) = &err {
        o.set_own("code", "ERR_DLOPEN_FAILED".into());
    }
    err
}

pub fn load_with(
    vm: &mut Vm,
    path: &std::path::Path,
    exports: Option<Value>,
) -> Result<Value, Value> {
    let fallback;
    let path = if uses_v8_api(path) {
        match napi_sibling(path) {
            Some(p) => {
                fallback = p;
                fallback.as_path()
            }
            None => {
                return Err(dlopen_error(
                    vm,
                    format!(
                        "{} is built against the V8 C++ API. Only Node-API addons can be loaded.",
                        path.display()
                    ),
                ));
            }
        }
    } else {
        path
    };
    let lib = match open_library(path) {
        Ok(l) => l,
        Err(e) => {
            let reason = std::error::Error::source(&e)
                .map(|s| s.to_string())
                .unwrap_or_else(|| e.to_string());
            let err = dlopen_error(vm, format!("{reason}\n{}", path.display()));
            PENDING_MODULE.with(|m| m.borrow_mut().take());
            return Err(err);
        }
    };
    let filename = CString::new(file_url(path)).unwrap_or_default();
    let env = Box::into_raw(Box::new(Env {
        vm,
        handles: Vec::new(),
        scopes: Vec::new(),
        pending: None,
        error: napi_extended_error_info {
            error_message: std::ptr::null(),
            engine_reserved: std::ptr::null_mut(),
            engine_error_code: 0,
            error_code: OK,
        },
        messages: Vec::new(),
        instance: (std::ptr::null_mut(), None, std::ptr::null_mut()),
        filename,
        tx: vm.io.tx.clone(),
        finalizer_ids: 0,
    }));
    type Init = unsafe extern "C" fn(napi_env, napi_value) -> napi_value;
    let init: Option<Init> = unsafe { lib.get::<Init>(b"napi_register_module_v1\0") }
        .ok()
        .map(|f| *f)
        .or_else(|| {
            PENDING_MODULE
                .with(|m| m.borrow_mut().take())
                .and_then(|m| unsafe { (*m).nm_register_func })
        });
    std::mem::forget(lib);
    let Some(init) = init else {
        return Err(vm.make_error(
            crate::vm::ErrorKind::Error,
            format!("Module did not self-register: '{}'.", path.display()),
        ));
    };
    let exports = exports.unwrap_or_else(|| Value::Object(vm.new_object()));
    let e = unsafe { &mut *env };
    let mark = e.handles.len();
    let h = e.put(exports.clone());
    let r = unsafe { init(env, h) };
    let result = if r.is_null() { exports } else { e.get(r) };
    e.handles.truncate(mark);
    if let Some(err) = e.pending.take() {
        return Err(err);
    }
    Ok(result)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_module_register(m: *mut napi_module) {
    PENDING_MODULE.with(|p| *p.borrow_mut() = Some(m));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_last_error_info(
    env: napi_env,
    result: *mut *const napi_extended_error_info,
) -> napi_status {
    let env = env!(env);
    let code = env.error.error_code as usize;
    if env.error.error_message.is_null() {
        if let Some(m) = ERROR_MESSAGES.get(code) {
            if let Ok(c) = CString::new(*m) {
                env.error.error_message = c.as_ptr();
                env.messages.push(c);
                if env.messages.len() > 32 {
                    env.messages.remove(0);
                }
            }
        }
    }
    if result.is_null() {
        return INVALID_ARG;
    }
    unsafe { *result = &env.error };
    OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_handle_scope(
    env: napi_env,
    result: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    env.scopes.push(env.handles.len());
    out!(env, result, env.scopes.len() as *mut c_void);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_handle_scope(
    env: napi_env,
    _scope: *mut c_void,
) -> napi_status {
    let env = env!(env);
    if let Some(mark) = env.scopes.pop() {
        env.handles.truncate(mark);
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_escapable_handle_scope(
    env: napi_env,
    result: *mut *mut c_void,
) -> napi_status {
    unsafe { napi_open_handle_scope(env, result) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_escapable_handle_scope(
    env: napi_env,
    scope: *mut c_void,
) -> napi_status {
    unsafe { napi_close_handle_scope(env, scope) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_escape_handle(
    env: napi_env,
    scope: *mut c_void,
    escapee: napi_value,
    result: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    let v = env.get(escapee);
    let idx = (scope as usize).wrapping_sub(1);
    let Some(mark) = env.scopes.get(idx).copied() else {
        return env.status(INVALID_ARG);
    };
    env.handles.insert(mark, v);
    for s in env.scopes.iter_mut().skip(idx) {
        *s += 1;
    }
    out!(env, result, (mark + 1) as napi_value);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_version(env: napi_env, result: *mut u32) -> napi_status {
    let env = env!(env);
    out!(env, result, 9);
    env.status(OK)
}

#[repr(C)]
pub struct napi_node_version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub release: *const c_char,
}

static NODE_VERSION: napi_node_version = napi_node_version {
    major: 22,
    minor: 12,
    patch: 0,
    release: c"node".as_ptr(),
};

unsafe impl Sync for napi_node_version {}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_node_version(
    env: napi_env,
    result: *mut *const napi_node_version,
) -> napi_status {
    let env = env!(env);
    out!(env, result, &NODE_VERSION as *const napi_node_version);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_get_module_file_name(
    env: napi_env,
    result: *mut *const c_char,
) -> napi_status {
    let env = env!(env);
    out!(env, result, env.filename.as_ptr());
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_instance_data(
    env: napi_env,
    data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
) -> napi_status {
    let env = env!(env);
    env.instance = (data, finalize_cb, finalize_hint);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_instance_data(
    env: napi_env,
    data: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    out!(env, data, env.instance.0);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_adjust_external_memory(
    env: napi_env,
    _change: i64,
    result: *mut i64,
) -> napi_status {
    let env = env!(env);
    if !result.is_null() {
        unsafe { *result = 0 };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_add_env_cleanup_hook(
    env: napi_env,
    _fun: Option<unsafe extern "C" fn(*mut c_void)>,
    _arg: *mut c_void,
) -> napi_status {
    let env = env!(env);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_remove_env_cleanup_hook(
    env: napi_env,
    _fun: Option<unsafe extern "C" fn(*mut c_void)>,
    _arg: *mut c_void,
) -> napi_status {
    let env = env!(env);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_add_async_cleanup_hook(
    env: napi_env,
    _hook: *mut c_void,
    _arg: *mut c_void,
    remove_handle: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    if !remove_handle.is_null() {
        unsafe { *remove_handle = std::ptr::dangling_mut::<c_void>() };
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_remove_async_cleanup_hook(_handle: *mut c_void) -> napi_status {
    OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_fatal_error(
    location: *const c_char,
    location_len: usize,
    message: *const c_char,
    message_len: usize,
) {
    let loc = unsafe { c_str(location, location_len) };
    let msg = unsafe { c_str(message, message_len) };
    eprintln!("FATAL ERROR: {loc} {msg}");
    std::process::abort();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_uv_event_loop(
    env: napi_env,
    _loop: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    env.status(GENERIC_FAILURE)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_callback_scope(
    env: napi_env,
    _resource: napi_value,
    _context: *mut c_void,
    result: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    out!(env, result, std::ptr::dangling_mut::<c_void>());
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_callback_scope(
    env: napi_env,
    _scope: *mut c_void,
) -> napi_status {
    let env = env!(env);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_async_init(
    env: napi_env,
    _resource: napi_value,
    _name: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    let env = env!(env);
    out!(env, result, std::ptr::dangling_mut::<c_void>());
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_async_destroy(env: napi_env, _context: *mut c_void) -> napi_status {
    let env = env!(env);
    env.status(OK)
}
