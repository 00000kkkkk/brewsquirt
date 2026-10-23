use std::ffi::{c_char, c_int, c_uint, c_void};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub type ThreadEntry = unsafe extern "C" fn(*mut c_void);

struct Sem {
    count: Mutex<u32>,
    cv: Condvar,
}

static SEMAPHORES: Mutex<Vec<Option<Arc<Sem>>>> = Mutex::new(Vec::new());

fn sem_of(s: *mut c_void) -> Option<Arc<Sem>> {
    let i = unsafe { *(s as *const u32) } as usize;
    SEMAPHORES.lock().ok()?.get(i.checked_sub(1)?)?.clone()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_sem_init(sem: *mut c_void, value: c_uint) -> c_int {
    let Ok(mut all) = SEMAPHORES.lock() else {
        return imp::EINVAL;
    };
    let sem_data = Some(Arc::new(Sem {
        count: Mutex::new(value),
        cv: Condvar::new(),
    }));
    let slot = match all.iter().position(Option::is_none) {
        Some(i) => {
            all[i] = sem_data;
            i
        }
        None => {
            all.push(sem_data);
            all.len() - 1
        }
    };
    unsafe { *(sem as *mut u32) = slot as u32 + 1 };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_sem_destroy(sem: *mut c_void) {
    let i = unsafe { *(sem as *const u32) } as usize;
    if let (Ok(mut all), Some(i)) = (SEMAPHORES.lock(), i.checked_sub(1)) {
        if let Some(slot) = all.get_mut(i) {
            *slot = None;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_sem_post(sem: *mut c_void) {
    if let Some(s) = sem_of(sem) {
        if let Ok(mut c) = s.count.lock() {
            *c += 1;
        }
        s.cv.notify_one();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_sem_wait(sem: *mut c_void) {
    if let Some(s) = sem_of(sem) {
        let Ok(mut c) = s.count.lock() else {
            return;
        };
        while *c == 0 {
            c = match s.cv.wait(c) {
                Ok(c) => c,
                Err(_) => return,
            };
        }
        *c -= 1;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_sem_trywait(sem: *mut c_void) -> c_int {
    let Some(s) = sem_of(sem) else {
        return imp::EINVAL;
    };
    let Ok(mut c) = s.count.lock() else {
        return imp::EINVAL;
    };
    if *c == 0 {
        return imp::EAGAIN;
    }
    *c -= 1;
    0
}

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_hrtime() -> u64 {
    start().elapsed().as_nanos() as u64 + 1_000_000_000
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_sleep(msec: c_uint) {
    std::thread::sleep(Duration::from_millis(msec as u64));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_version() -> c_uint {
    (1 << 16) | (49 << 8) | 1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_version_string() -> *const c_char {
    c"1.49.1".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_os_getpid() -> c_int {
    std::process::id() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_strerror(_err: c_int) -> *const c_char {
    c"unknown error".as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_err_name(_err: c_int) -> *const c_char {
    c"UNKNOWN".as_ptr()
}

#[repr(C)]
pub struct ThreadOptions {
    pub flags: c_uint,
    pub stack_size: usize,
}

struct Start {
    entry: ThreadEntry,
    arg: *mut c_void,
}

unsafe fn run_start(p: *mut c_void) {
    let s = unsafe { Box::from_raw(p as *mut Start) };
    unsafe { (s.entry)(s.arg) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn uv_thread_create_ex(
    tid: *mut c_void,
    _params: *const ThreadOptions,
    entry: ThreadEntry,
    arg: *mut c_void,
) -> c_int {
    unsafe { uv_thread_create(tid, entry, arg) }
}

#[cfg(windows)]
mod imp {
    use super::{Start, ThreadEntry, run_start};
    use std::cell::Cell;
    use std::ffi::{c_int, c_void};
    use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

    pub const EBUSY: c_int = -4082;
    pub const ETIMEDOUT: c_int = -4039;
    pub const EAGAIN: c_int = -4088;
    pub const EINVAL: c_int = -4071;
    const INFINITE: u32 = 0xFFFF_FFFF;
    const ERROR_TIMEOUT: u32 = 1460;
    const DUPLICATE_SAME_ACCESS: u32 = 2;

    type Handle = *mut c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn InitializeCriticalSection(cs: *mut c_void);
        fn DeleteCriticalSection(cs: *mut c_void);
        fn EnterCriticalSection(cs: *mut c_void);
        fn TryEnterCriticalSection(cs: *mut c_void) -> i32;
        fn LeaveCriticalSection(cs: *mut c_void);
        fn InitializeConditionVariable(cv: *mut c_void);
        fn WakeConditionVariable(cv: *mut c_void);
        fn WakeAllConditionVariable(cv: *mut c_void);
        fn SleepConditionVariableCS(cv: *mut c_void, cs: *mut c_void, ms: u32) -> i32;
        fn InitializeSRWLock(l: *mut c_void);
        fn AcquireSRWLockShared(l: *mut c_void);
        fn TryAcquireSRWLockShared(l: *mut c_void) -> u8;
        fn ReleaseSRWLockShared(l: *mut c_void);
        fn AcquireSRWLockExclusive(l: *mut c_void);
        fn TryAcquireSRWLockExclusive(l: *mut c_void) -> u8;
        fn ReleaseSRWLockExclusive(l: *mut c_void);
        fn TlsAlloc() -> u32;
        fn TlsFree(i: u32) -> i32;
        fn TlsGetValue(i: u32) -> *mut c_void;
        fn TlsSetValue(i: u32, v: *mut c_void) -> i32;
        fn GetLastError() -> u32;
        fn GetThreadId(h: Handle) -> u32;
        fn GetCurrentThread() -> Handle;
        fn GetCurrentProcess() -> Handle;
        fn DuplicateHandle(
            sp: Handle,
            sh: Handle,
            tp: Handle,
            th: *mut Handle,
            access: u32,
            inherit: i32,
            opts: u32,
        ) -> i32;
        fn CreateThread(
            attr: *mut c_void,
            stack: usize,
            start: unsafe extern "system" fn(*mut c_void) -> u32,
            arg: *mut c_void,
            flags: u32,
            id: *mut u32,
        ) -> Handle;
        fn WaitForSingleObject(h: Handle, ms: u32) -> u32;
        fn CloseHandle(h: Handle) -> i32;
    }

    thread_local! {
        static SELF: Cell<Handle> = const { Cell::new(std::ptr::null_mut()) };
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_init(m: *mut c_void) -> c_int {
        unsafe { InitializeCriticalSection(m) };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_init_recursive(m: *mut c_void) -> c_int {
        unsafe { InitializeCriticalSection(m) };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_destroy(m: *mut c_void) {
        unsafe { DeleteCriticalSection(m) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_lock(m: *mut c_void) {
        unsafe { EnterCriticalSection(m) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_trylock(m: *mut c_void) -> c_int {
        if unsafe { TryEnterCriticalSection(m) } != 0 {
            0
        } else {
            EBUSY
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_unlock(m: *mut c_void) {
        unsafe { LeaveCriticalSection(m) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_init(l: *mut c_void) -> c_int {
        unsafe { InitializeSRWLock(l) };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_destroy(_l: *mut c_void) {}

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_rdlock(l: *mut c_void) {
        unsafe { AcquireSRWLockShared(l) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_tryrdlock(l: *mut c_void) -> c_int {
        if unsafe { TryAcquireSRWLockShared(l) } != 0 {
            0
        } else {
            EBUSY
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_rdunlock(l: *mut c_void) {
        unsafe { ReleaseSRWLockShared(l) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_wrlock(l: *mut c_void) {
        unsafe { AcquireSRWLockExclusive(l) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_trywrlock(l: *mut c_void) -> c_int {
        if unsafe { TryAcquireSRWLockExclusive(l) } != 0 {
            0
        } else {
            EBUSY
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_wrunlock(l: *mut c_void) {
        unsafe { ReleaseSRWLockExclusive(l) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_init(c: *mut c_void) -> c_int {
        unsafe { InitializeConditionVariable(c) };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_destroy(_c: *mut c_void) {}

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_signal(c: *mut c_void) {
        unsafe { WakeConditionVariable(c) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_broadcast(c: *mut c_void) {
        unsafe { WakeAllConditionVariable(c) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_wait(c: *mut c_void, m: *mut c_void) {
        if unsafe { SleepConditionVariableCS(c, m, INFINITE) } == 0 {
            std::process::abort();
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_timedwait(
        c: *mut c_void,
        m: *mut c_void,
        timeout: u64,
    ) -> c_int {
        let ms = (timeout / 1_000_000).min(INFINITE as u64 - 1) as u32;
        if unsafe { SleepConditionVariableCS(c, m, ms) } != 0 {
            return 0;
        }
        if unsafe { GetLastError() } != ERROR_TIMEOUT {
            std::process::abort();
        }
        ETIMEDOUT
    }

    #[repr(C)]
    struct OnceGuard {
        ran: u8,
        event: *mut c_void,
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_once(guard: *mut c_void, cb: unsafe extern "C" fn()) {
        let ran = unsafe { &*(guard as *const AtomicU8) };
        if ran.load(Ordering::Acquire) == 1 {
            return;
        }
        let state = unsafe {
            &*((guard as *mut u8).add(std::mem::offset_of!(OnceGuard, event)) as *const AtomicUsize)
        };
        match state.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                unsafe { cb() };
                ran.store(1, Ordering::Release);
                state.store(2, Ordering::Release);
            }
            Err(_) => {
                while ran.load(Ordering::Acquire) != 1 {
                    std::thread::yield_now();
                }
            }
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_create(key: *mut c_void) -> c_int {
        let i = unsafe { TlsAlloc() };
        if i == 0xFFFF_FFFF {
            return -4057;
        }
        unsafe { *(key as *mut u32) = i };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_delete(key: *mut c_void) {
        unsafe { TlsFree(*(key as *const u32)) };
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_get(key: *mut c_void) -> *mut c_void {
        unsafe { TlsGetValue(*(key as *const u32)) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_set(key: *mut c_void, value: *mut c_void) {
        unsafe { TlsSetValue(*(key as *const u32), value) };
    }

    unsafe extern "system" fn trampoline(p: *mut c_void) -> u32 {
        unsafe { run_start(p) };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_create(
        tid: *mut c_void,
        entry: ThreadEntry,
        arg: *mut c_void,
    ) -> c_int {
        let start = Box::into_raw(Box::new(Start { entry, arg })) as *mut c_void;
        let h = unsafe {
            CreateThread(
                std::ptr::null_mut(),
                0,
                trampoline,
                start,
                0,
                std::ptr::null_mut(),
            )
        };
        if h.is_null() {
            drop(unsafe { Box::from_raw(start as *mut Start) });
            return EAGAIN;
        }
        unsafe { *(tid as *mut Handle) = h };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_join(tid: *mut c_void) -> c_int {
        let h = unsafe { *(tid as *const Handle) };
        if unsafe { WaitForSingleObject(h, INFINITE) } != 0 {
            return EINVAL;
        }
        unsafe { CloseHandle(h) };
        0
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_self() -> Handle {
        SELF.with(|s| {
            if s.get().is_null() {
                let mut h: Handle = std::ptr::null_mut();
                unsafe {
                    DuplicateHandle(
                        GetCurrentProcess(),
                        GetCurrentThread(),
                        GetCurrentProcess(),
                        &mut h,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    )
                };
                s.set(h);
            }
            s.get()
        })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_equal(a: *const c_void, b: *const c_void) -> c_int {
        let (a, b) = unsafe { (*(a as *const Handle), *(b as *const Handle)) };
        (a == b || unsafe { GetThreadId(a) == GetThreadId(b) }) as c_int
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: Handle,
            class: u32,
            info: *mut c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_os_getppid() -> c_int {
        let mut info = [0usize; 6];
        let r = unsafe {
            NtQueryInformationProcess(
                GetCurrentProcess(),
                0,
                info.as_mut_ptr().cast(),
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            )
        };
        if r < 0 { -1 } else { info[5] as c_int }
    }
}

#[cfg(unix)]
mod imp {
    use super::{Start, ThreadEntry, run_start};
    use std::ffi::{c_int, c_void};

    pub const EBUSY: c_int = -libc::EBUSY;
    pub const ETIMEDOUT: c_int = -libc::ETIMEDOUT;
    pub const EAGAIN: c_int = -libc::EAGAIN;
    pub const EINVAL: c_int = -libc::EINVAL;

    fn check(r: c_int) {
        if r != 0 {
            std::process::abort();
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_init(m: *mut c_void) -> c_int {
        -unsafe { libc::pthread_mutex_init(m.cast(), std::ptr::null()) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_init_recursive(m: *mut c_void) -> c_int {
        let mut attr: libc::pthread_mutexattr_t = unsafe { std::mem::zeroed() };
        unsafe {
            if libc::pthread_mutexattr_init(&mut attr) != 0 {
                std::process::abort();
            }
            if libc::pthread_mutexattr_settype(&mut attr, libc::PTHREAD_MUTEX_RECURSIVE) != 0 {
                std::process::abort();
            }
            let r = libc::pthread_mutex_init(m.cast(), &attr);
            libc::pthread_mutexattr_destroy(&mut attr);
            -r
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_destroy(m: *mut c_void) {
        check(unsafe { libc::pthread_mutex_destroy(m.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_lock(m: *mut c_void) {
        check(unsafe { libc::pthread_mutex_lock(m.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_trylock(m: *mut c_void) -> c_int {
        match unsafe { libc::pthread_mutex_trylock(m.cast()) } {
            0 => 0,
            libc::EBUSY | libc::EAGAIN => EBUSY,
            _ => std::process::abort(),
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_mutex_unlock(m: *mut c_void) {
        check(unsafe { libc::pthread_mutex_unlock(m.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_init(l: *mut c_void) -> c_int {
        -unsafe { libc::pthread_rwlock_init(l.cast(), std::ptr::null()) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_destroy(l: *mut c_void) {
        check(unsafe { libc::pthread_rwlock_destroy(l.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_rdlock(l: *mut c_void) {
        check(unsafe { libc::pthread_rwlock_rdlock(l.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_tryrdlock(l: *mut c_void) -> c_int {
        match unsafe { libc::pthread_rwlock_tryrdlock(l.cast()) } {
            0 => 0,
            libc::EBUSY | libc::EAGAIN => EBUSY,
            _ => std::process::abort(),
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_rdunlock(l: *mut c_void) {
        check(unsafe { libc::pthread_rwlock_unlock(l.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_wrlock(l: *mut c_void) {
        check(unsafe { libc::pthread_rwlock_wrlock(l.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_trywrlock(l: *mut c_void) -> c_int {
        match unsafe { libc::pthread_rwlock_trywrlock(l.cast()) } {
            0 => 0,
            libc::EBUSY | libc::EAGAIN => EBUSY,
            _ => std::process::abort(),
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_rwlock_wrunlock(l: *mut c_void) {
        check(unsafe { libc::pthread_rwlock_unlock(l.cast()) })
    }

    #[cfg(target_vendor = "apple")]
    const COND_CLOCK: libc::clockid_t = libc::CLOCK_REALTIME;
    #[cfg(not(target_vendor = "apple"))]
    const COND_CLOCK: libc::clockid_t = libc::CLOCK_MONOTONIC;

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_init(c: *mut c_void) -> c_int {
        #[cfg(target_vendor = "apple")]
        {
            -unsafe { libc::pthread_cond_init(c.cast(), std::ptr::null()) }
        }
        #[cfg(not(target_vendor = "apple"))]
        {
            let mut attr: libc::pthread_condattr_t = unsafe { std::mem::zeroed() };
            unsafe {
                let r = libc::pthread_condattr_init(&mut attr);
                if r != 0 {
                    return -r;
                }
                libc::pthread_condattr_setclock(&mut attr, COND_CLOCK);
                let r = libc::pthread_cond_init(c.cast(), &attr);
                libc::pthread_condattr_destroy(&mut attr);
                -r
            }
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_destroy(c: *mut c_void) {
        check(unsafe { libc::pthread_cond_destroy(c.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_signal(c: *mut c_void) {
        check(unsafe { libc::pthread_cond_signal(c.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_broadcast(c: *mut c_void) {
        check(unsafe { libc::pthread_cond_broadcast(c.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_wait(c: *mut c_void, m: *mut c_void) {
        check(unsafe { libc::pthread_cond_wait(c.cast(), m.cast()) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_cond_timedwait(
        c: *mut c_void,
        m: *mut c_void,
        timeout: u64,
    ) -> c_int {
        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(COND_CLOCK, &mut now) };
        let total = now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64 + timeout;
        let ts = libc::timespec {
            tv_sec: (total / 1_000_000_000) as libc::time_t,
            tv_nsec: (total % 1_000_000_000) as _,
        };
        match unsafe { libc::pthread_cond_timedwait(c.cast(), m.cast(), &ts) } {
            0 => 0,
            libc::ETIMEDOUT => ETIMEDOUT,
            _ => std::process::abort(),
        }
    }

    unsafe extern "C" {
        #[link_name = "pthread_once"]
        fn run_once(control: *mut c_void, init: unsafe extern "C" fn()) -> c_int;
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_once(guard: *mut c_void, cb: unsafe extern "C" fn()) {
        check(unsafe { run_once(guard, cb) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_create(key: *mut c_void) -> c_int {
        -unsafe { libc::pthread_key_create(key.cast(), None) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_delete(key: *mut c_void) {
        check(unsafe { libc::pthread_key_delete(*(key as *const libc::pthread_key_t)) })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_get(key: *mut c_void) -> *mut c_void {
        unsafe { libc::pthread_getspecific(*(key as *const libc::pthread_key_t)) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_key_set(key: *mut c_void, value: *mut c_void) {
        check(unsafe { libc::pthread_setspecific(*(key as *const libc::pthread_key_t), value) })
    }

    extern "C" fn trampoline(p: *mut c_void) -> *mut c_void {
        unsafe { run_start(p) };
        std::ptr::null_mut()
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_create(
        tid: *mut c_void,
        entry: ThreadEntry,
        arg: *mut c_void,
    ) -> c_int {
        let start = Box::into_raw(Box::new(Start { entry, arg })) as *mut c_void;
        let r = unsafe { libc::pthread_create(tid.cast(), std::ptr::null(), trampoline, start) };
        if r != 0 {
            drop(unsafe { Box::from_raw(start as *mut Start) });
        }
        -r
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_join(tid: *mut c_void) -> c_int {
        -unsafe { libc::pthread_join(*(tid as *const libc::pthread_t), std::ptr::null_mut()) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_self() -> libc::pthread_t {
        unsafe { libc::pthread_self() }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_thread_equal(a: *const c_void, b: *const c_void) -> c_int {
        unsafe {
            libc::pthread_equal(
                *(a as *const libc::pthread_t),
                *(b as *const libc::pthread_t),
            )
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn uv_os_getppid() -> c_int {
        unsafe { libc::getppid() }
    }
}

pub use imp::uv_thread_create;
