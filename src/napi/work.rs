use super::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub enum NapiEvent {
    Complete(usize),
    Call(usize, usize),
    Release(usize),
}

pub type napi_async_execute_callback = Option<unsafe extern "C" fn(napi_env, *mut c_void)>;
pub type napi_async_complete_callback =
    Option<unsafe extern "C" fn(napi_env, napi_status, *mut c_void)>;
pub type napi_threadsafe_function_call_js =
    Option<unsafe extern "C" fn(napi_env, napi_value, *mut c_void, *mut c_void)>;

pub struct AsyncWork {
    env: napi_env,
    execute: napi_async_execute_callback,
    complete: napi_async_complete_callback,
    data: *mut c_void,
    queued: bool,
}

struct SendPtr(usize);
unsafe impl Send for SendPtr {}

pub struct Tsfn {
    env: napi_env,
    id: usize,
    call_js: napi_threadsafe_function_call_js,
    context: *mut c_void,
    finalize: napi_finalize,
    finalize_data: *mut c_void,
    threads: AtomicUsize,
    closing: AtomicBool,
    refed: AtomicBool,
    tx: Mutex<std::sync::mpsc::Sender<crate::io::IoEvent>>,
}

unsafe impl Send for Tsfn {}
unsafe impl Sync for Tsfn {}

thread_local! {
    static TSFN_FUNCS: RefCell<rustc_hash::FxHashMap<usize, Value>> = RefCell::new(rustc_hash::FxHashMap::default());
    static TSFN_IDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_promise(
    env: napi_env,
    deferred: *mut *mut c_void,
    promise: *mut napi_value,
) -> napi_status {
    let env = env!(env);
    if deferred.is_null() || promise.is_null() {
        return env.status(INVALID_ARG);
    }
    let p = env.vm().new_promise();
    let boxed = Box::into_raw(Box::new(p.clone()));
    unsafe { *deferred = boxed as *mut c_void };
    let h = env.put(Value::Object(p));
    unsafe { *promise = h };
    env.status(OK)
}

unsafe fn settle(env: napi_env, deferred: *mut c_void, value: napi_value, ok: bool) -> napi_status {
    let env = env!(env);
    if deferred.is_null() {
        return env.status(INVALID_ARG);
    }
    let p = unsafe { Box::from_raw(deferred as *mut Rc<Obj>) };
    let v = env.get(value);
    if ok {
        env.vm().resolve_promise(&p, v);
    } else {
        env.vm().reject_promise(&p, v);
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_resolve_deferred(
    env: napi_env,
    deferred: *mut c_void,
    resolution: napi_value,
) -> napi_status {
    unsafe { settle(env, deferred, resolution, true) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reject_deferred(
    env: napi_env,
    deferred: *mut c_void,
    rejection: napi_value,
) -> napi_status {
    unsafe { settle(env, deferred, rejection, false) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_promise(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    let env = env!(env);
    let is = matches!(env.get(value), Value::Object(o) if matches!(o.kind, ObjKind::Promise(_)));
    out!(env, result, is);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_async_work(
    env: napi_env,
    _async_resource: napi_value,
    _async_resource_name: napi_value,
    execute: napi_async_execute_callback,
    complete: napi_async_complete_callback,
    data: *mut c_void,
    result: *mut *mut AsyncWork,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    let work = Box::into_raw(Box::new(AsyncWork {
        env: envp,
        execute,
        complete,
        data,
        queued: false,
    }));
    out!(env, result, work);
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_async_work(
    env: napi_env,
    work: *mut AsyncWork,
) -> napi_status {
    let env = env!(env);
    if !work.is_null() {
        drop(unsafe { Box::from_raw(work) });
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_queue_async_work(env: napi_env, work: *mut AsyncWork) -> napi_status {
    let env = env!(env);
    let Some(w) = (unsafe { work.as_mut() }) else {
        return env.status(INVALID_ARG);
    };
    if w.queued {
        return env.status(GENERIC_FAILURE);
    }
    w.queued = true;
    env.vm().io.pending += 1;
    let tx = env.tx.clone();
    let execute = w.execute;
    let wenv = SendPtr(w.env as usize);
    let data = SendPtr(w.data as usize);
    let ptr = SendPtr(work as usize);
    std::thread::spawn(move || {
        let (wenv, data, ptr) = (wenv, data, ptr);
        if let Some(f) = execute {
            unsafe { f(wenv.0 as napi_env, data.0 as *mut c_void) };
        }
        let _ = tx.send(crate::io::IoEvent::Napi(NapiEvent::Complete(ptr.0)));
    });
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_cancel_async_work(
    env: napi_env,
    _work: *mut AsyncWork,
) -> napi_status {
    let env = env!(env);
    env.status(GENERIC_FAILURE)
}

pub fn refed_tsfns() -> usize {
    REFED_TSFNS.load(Ordering::SeqCst)
}

static REFED_TSFNS: AtomicUsize = AtomicUsize::new(0);

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn napi_create_threadsafe_function(
    env: napi_env,
    func: napi_value,
    _async_resource: napi_value,
    _async_resource_name: napi_value,
    _max_queue_size: usize,
    initial_thread_count: usize,
    thread_finalize_data: *mut c_void,
    thread_finalize_cb: napi_finalize,
    context: *mut c_void,
    call_js_cb: napi_threadsafe_function_call_js,
    result: *mut *mut Tsfn,
) -> napi_status {
    let envp = env;
    let env = env!(env);
    let f = env.get(func);
    if f.is_undefined() && call_js_cb.is_none() {
        return env.status(INVALID_ARG);
    }
    let id = TSFN_IDS.with(|c| {
        let n = c.get() + 1;
        c.set(n);
        n
    });
    TSFN_FUNCS.with(|m| m.borrow_mut().insert(id, f));
    env.vm().io.pending += 1;
    REFED_TSFNS.fetch_add(1, Ordering::SeqCst);
    let tsfn = Box::into_raw(Box::new(Tsfn {
        env: envp,
        id,
        call_js: call_js_cb,
        context,
        finalize: thread_finalize_cb,
        finalize_data: thread_finalize_data,
        threads: AtomicUsize::new(initial_thread_count.max(1)),
        closing: AtomicBool::new(false),
        refed: AtomicBool::new(true),
        tx: Mutex::new(env.tx.clone()),
    }));
    out!(env, result, tsfn);
    env.status(OK)
}

fn send(t: &Tsfn, ev: NapiEvent) -> bool {
    match t.tx.lock() {
        Ok(tx) => tx.send(crate::io::IoEvent::Napi(ev)).is_ok(),
        Err(_) => false,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_threadsafe_function_context(
    func: *mut Tsfn,
    result: *mut *mut c_void,
) -> napi_status {
    let Some(t) = (unsafe { func.as_ref() }) else {
        return INVALID_ARG;
    };
    if result.is_null() {
        return INVALID_ARG;
    }
    unsafe { *result = t.context };
    OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_call_threadsafe_function(
    func: *mut Tsfn,
    data: *mut c_void,
    _mode: i32,
) -> napi_status {
    let Some(t) = (unsafe { func.as_ref() }) else {
        return INVALID_ARG;
    };
    if t.closing.load(Ordering::SeqCst) {
        return 16;
    }
    if send(t, NapiEvent::Call(func as usize, data as usize)) {
        OK
    } else {
        GENERIC_FAILURE
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_acquire_threadsafe_function(func: *mut Tsfn) -> napi_status {
    let Some(t) = (unsafe { func.as_ref() }) else {
        return INVALID_ARG;
    };
    if t.closing.load(Ordering::SeqCst) {
        return 16;
    }
    t.threads.fetch_add(1, Ordering::SeqCst);
    OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_release_threadsafe_function(
    func: *mut Tsfn,
    mode: i32,
) -> napi_status {
    let Some(t) = (unsafe { func.as_ref() }) else {
        return INVALID_ARG;
    };
    let abort = mode == 1;
    let left = if abort {
        t.threads.store(0, Ordering::SeqCst);
        0
    } else {
        t.threads.fetch_sub(1, Ordering::SeqCst).saturating_sub(1)
    };
    if left == 0 && !t.closing.swap(true, Ordering::SeqCst) {
        send(t, NapiEvent::Release(func as usize));
    }
    OK
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_unref_threadsafe_function(
    env: napi_env,
    func: *mut Tsfn,
) -> napi_status {
    let env = env!(env);
    let Some(t) = (unsafe { func.as_ref() }) else {
        return env.status(INVALID_ARG);
    };
    if t.refed.swap(false, Ordering::SeqCst) {
        env.vm().io.pending = env.vm().io.pending.saturating_sub(1);
        REFED_TSFNS.fetch_sub(1, Ordering::SeqCst);
    }
    env.status(OK)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_ref_threadsafe_function(
    env: napi_env,
    func: *mut Tsfn,
) -> napi_status {
    let env = env!(env);
    let Some(t) = (unsafe { func.as_ref() }) else {
        return env.status(INVALID_ARG);
    };
    if !t.refed.swap(true, Ordering::SeqCst) {
        env.vm().io.pending += 1;
        REFED_TSFNS.fetch_add(1, Ordering::SeqCst);
    }
    env.status(OK)
}

pub fn handle_event(vm: &mut Vm, ev: NapiEvent) -> Result<(), Value> {
    match ev {
        NapiEvent::Complete(ptr) => {
            vm.io.pending = vm.io.pending.saturating_sub(1);
            let w = ptr as *mut AsyncWork;
            let (envp, complete, data) = unsafe { ((*w).env, (*w).complete, (*w).data) };
            let Some(env) = (unsafe { envp.as_mut() }) else {
                return Ok(());
            };
            env.vm = vm;
            let mark = env.handles.len();
            if let Some(cb) = complete {
                unsafe { cb(envp, OK, data) };
            }
            env.handles.truncate(mark);
            if let Some(e) = env.pending.take() {
                return Err(e);
            }
        }
        NapiEvent::Call(ptr, data) => {
            let t = unsafe { &*(ptr as *const Tsfn) };
            let Some(env) = (unsafe { t.env.as_mut() }) else {
                return Ok(());
            };
            env.vm = vm;
            let mark = env.handles.len();
            let f = TSFN_FUNCS
                .with(|m| m.borrow().get(&t.id).cloned())
                .unwrap_or_default();
            match t.call_js {
                Some(cb) => {
                    let h = if f.is_undefined() {
                        std::ptr::null_mut()
                    } else {
                        env.put(f)
                    };
                    unsafe { cb(t.env, h, t.context, data as *mut c_void) };
                }
                None => {
                    if f.is_callable() {
                        if let Err(e) = vm.call_value(&f, Value::Undefined, &[]) {
                            env.pending = Some(e);
                        }
                    }
                }
            }
            env.handles.truncate(mark);
            if let Some(e) = env.pending.take() {
                return Err(e);
            }
        }
        NapiEvent::Release(ptr) => {
            let t = unsafe { Box::from_raw(ptr as *mut Tsfn) };
            if t.refed.load(Ordering::SeqCst) {
                vm.io.pending = vm.io.pending.saturating_sub(1);
                REFED_TSFNS.fetch_sub(1, Ordering::SeqCst);
            }
            TSFN_FUNCS.with(|m| m.borrow_mut().remove(&t.id));
            if let Some(env) = unsafe { t.env.as_mut() } {
                env.vm = vm;
                let mark = env.handles.len();
                if let Some(cb) = t.finalize {
                    unsafe { cb(t.env, t.finalize_data, t.context) };
                }
                env.handles.truncate(mark);
                env.pending = None;
            }
        }
    }
    Ok(())
}
