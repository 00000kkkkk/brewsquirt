use super::{arg, method};
use crate::io::IoEvent;
use crate::object::{BufferData, MapKey, Obj, ObjKind, TypedKind, View};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use rustc_hash::FxHashMap;
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::Duration;

type R = Result<Value, Value>;

pub struct SharedMem {
    ptr: *mut u8,
    cap: usize,
    pub len: AtomicUsize,
}

unsafe impl Send for SharedMem {}
unsafe impl Sync for SharedMem {}

impl SharedMem {
    pub fn new(len: usize, cap: usize) -> Option<Arc<SharedMem>> {
        let cap = cap.max(len).max(1);
        let layout = Layout::from_size_align(cap, 16).ok()?;
        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            return None;
        }
        Some(Arc::new(SharedMem {
            ptr,
            cap,
            len: AtomicUsize::new(len),
        }))
    }

    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }
}

impl Drop for SharedMem {
    fn drop(&mut self) {
        if let Ok(layout) = Layout::from_size_align(self.cap, 16) {
            unsafe { dealloc(self.ptr, layout) };
        }
    }
}

pub fn shared_buffer(vm: &Vm, mem: Arc<SharedMem>, max: Option<usize>) -> Rc<Obj> {
    let len = mem.len.load(Ordering::SeqCst);
    let bytes = unsafe { Vec::from_raw_parts(mem.ptr(), len, mem.capacity()) };
    Obj::new(
        ObjKind::ArrayBuffer(Box::new(BufferData {
            bytes: RefCell::new(bytes),
            max,
            shared: true,
            external: true,
            shared_mem: Some(mem),
        })),
        Some(vm.realm.shared_buffer_proto.clone()),
    )
}

pub enum Wire {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    BigInt(String),
    Str(String),
    Ref(u32),
    Array(Vec<Option<Wire>>),
    Object(Vec<(String, Wire)>),
    Date(f64),
    RegExp(String, String),
    Map(Vec<(Wire, Wire)>),
    Set(Vec<Wire>),
    Buffer(Vec<u8>, Option<usize>),
    Shared(Arc<SharedMem>, Option<usize>),
    View(u8, Box<Wire>, usize, usize, bool),
    Error {
        name: String,
        message: Option<String>,
        stack: Option<String>,
        cause: Option<Box<Wire>>,
    },
    Boxed(Box<Wire>),
    Port(u64),
}

const DATA_VIEW: u8 = 255;

fn clone_error(vm: &mut Vm, msg: &str) -> Value {
    super::binary::dom_error(vm, "DataCloneError", msg)
}

struct Encoder<'a> {
    memo: Vec<*const Obj>,
    transfer: &'a [Value],
    moved: Vec<Rc<Obj>>,
}

impl Encoder<'_> {
    fn transferred(&self, o: &Rc<Obj>) -> bool {
        self.transfer
            .iter()
            .any(|t| matches!(t, Value::Object(x) if Rc::ptr_eq(x, o)))
    }
}

fn encode(vm: &mut Vm, v: &Value, enc: &mut Encoder<'_>) -> Result<Wire, Value> {
    let o = match v {
        Value::Undefined | Value::Uninit => return Ok(Wire::Undefined),
        Value::Null => return Ok(Wire::Null),
        Value::Bool(b) => return Ok(Wire::Bool(*b)),
        Value::Number(n) => return Ok(Wire::Number(*n)),
        Value::BigInt(b) => return Ok(Wire::BigInt(b.to_string())),
        Value::Str(s) => return Ok(Wire::Str(s.to_string())),
        Value::Symbol(_) => {
            let text = crate::inspect::inspect_plain(v);
            return Err(clone_error(vm, &format!("{text} could not be cloned.")));
        }
        Value::Object(o) => o.clone(),
    };
    let ptr = Rc::as_ptr(&o);
    if let Some(i) = enc.memo.iter().position(|p| *p == ptr) {
        return Ok(Wire::Ref(i as u32));
    }
    if let Some(id) = vm.threads.port_id_of(&o) {
        if !enc.transferred(&o) {
            return Err(clone_error(
                vm,
                "Object that needs transfer was found in message but not listed in transferList",
            ));
        }
        enc.memo.push(ptr);
        return Ok(Wire::Port(id));
    }
    enc.memo.push(ptr);
    match &o.kind {
        ObjKind::Array(a) => {
            let items = a.borrow().clone();
            let mut out = Vec::with_capacity(items.len());
            for (i, x) in items.iter().enumerate() {
                if o.is_hole(i) {
                    out.push(None);
                } else {
                    out.push(Some(encode(vm, x, enc)?));
                }
            }
            Ok(Wire::Array(out))
        }
        ObjKind::Error => {
            let text = |v: Option<Value>| match v {
                Some(Value::Str(s)) => Some(s.to_string()),
                _ => None,
            };
            let name = text(o.get_chain("name")).unwrap_or_else(|| "Error".into());
            let name = match name.as_str() {
                "EvalError" | "RangeError" | "ReferenceError" | "SyntaxError" | "TypeError"
                | "URIError" => name,
                _ => "Error".to_string(),
            };
            let message = text(o.get_own("message"));
            let stack = text(o.get_own("stack"));
            let cause = match o.get_own("cause") {
                Some(c) => Some(Box::new(encode(vm, &c, enc)?)),
                None => None,
            };
            Ok(Wire::Error {
                name,
                message,
                stack,
                cause,
            })
        }
        ObjKind::Date(t) => Ok(Wire::Date(t.get())),
        ObjKind::RegExp(r) => {
            let d = r.data.borrow();
            Ok(Wire::RegExp(d.source.to_string(), d.flags.to_string()))
        }
        ObjKind::Map(m) => {
            let entries: Vec<(Value, Value)> = m
                .borrow()
                .iter()
                .map(|(k, v)| (k.to_value(), v.clone()))
                .collect();
            let mut out = Vec::with_capacity(entries.len());
            for (k, x) in entries {
                out.push((encode(vm, &k, enc)?, encode(vm, &x, enc)?));
            }
            Ok(Wire::Map(out))
        }
        ObjKind::Set(s) => {
            let items: Vec<Value> = s.borrow().iter().map(|k| k.to_value()).collect();
            let mut out = Vec::with_capacity(items.len());
            for x in items {
                out.push(encode(vm, &x, enc)?);
            }
            Ok(Wire::Set(out))
        }
        ObjKind::ArrayBuffer(b) => {
            if super::typed::is_detached(&o) {
                return Err(clone_error(
                    vm,
                    "An ArrayBuffer is detached and could not be cloned.",
                ));
            }
            if b.shared {
                if let Some(mem) = &b.shared_mem {
                    return Ok(Wire::Shared(mem.clone(), b.max));
                }
                let bytes = b.bytes.borrow().clone();
                return Ok(Wire::Buffer(bytes, b.max));
            }
            if enc.transferred(&o) {
                if b.external {
                    return Err(clone_error(vm, "An ArrayBuffer could not be transferred."));
                }
                let bytes = b.bytes.borrow().clone();
                enc.moved.push(o.clone());
                return Ok(Wire::Buffer(bytes, b.max));
            }
            Ok(Wire::Buffer(b.bytes.borrow().clone(), b.max))
        }
        ObjKind::Typed(t) => {
            let buf = encode(vm, &Value::Object(t.buffer.clone()), enc)?;
            Ok(Wire::View(
                t.kind as u8,
                Box::new(buf),
                t.offset,
                t.length,
                t.tracking,
            ))
        }
        ObjKind::DataView(t) => {
            let buf = encode(vm, &Value::Object(t.buffer.clone()), enc)?;
            Ok(Wire::View(
                DATA_VIEW,
                Box::new(buf),
                t.offset,
                t.length,
                t.tracking,
            ))
        }
        ObjKind::Primitive(p) => {
            if matches!(p, Value::Symbol(_)) {
                return Err(clone_error(vm, "Symbol object could not be cloned."));
            }
            Ok(Wire::Boxed(Box::new(encode(vm, &p.clone(), enc)?)))
        }
        ObjKind::Ordinary => {
            let entries = vm.own_entries_resolved(v, false)?;
            let mut out = Vec::with_capacity(entries.len());
            for (k, x) in entries {
                if x.is_callable() {
                    let text = crate::inspect::inspect_plain(&x);
                    return Err(clone_error(vm, &format!("{text} could not be cloned.")));
                }
                out.push((k.to_string(), encode(vm, &x, enc)?));
            }
            Ok(Wire::Object(out))
        }
        _ => {
            let text = crate::inspect::inspect_plain(v);
            Err(clone_error(vm, &format!("{text} could not be cloned.")))
        }
    }
}

pub fn serialize(vm: &mut Vm, v: &Value, transfer: &[Value]) -> Result<Wire, Value> {
    for t in transfer {
        let ok = match t {
            Value::Object(o) => match &o.kind {
                ObjKind::ArrayBuffer(b) => !b.shared,
                _ => vm.threads.port_id_of(o).is_some(),
            },
            _ => false,
        };
        if !ok {
            return Err(clone_error(
                vm,
                "Value in transfer list is not transferable.",
            ));
        }
    }
    let mut enc = Encoder {
        memo: Vec::new(),
        transfer,
        moved: Vec::new(),
    };
    let wire = encode(vm, v, &mut enc)?;
    for b in enc.moved {
        super::typed::detach(&b);
    }
    for t in transfer {
        if let Value::Object(o) = t {
            if let Some(id) = vm.threads.port_id_of(o) {
                detach_port(vm, id);
            }
        }
    }
    Ok(wire)
}

fn decode(vm: &mut Vm, w: &Wire, memo: &mut Vec<Value>) -> R {
    Ok(match w {
        Wire::Undefined => Value::Undefined,
        Wire::Null => Value::Null,
        Wire::Bool(b) => Value::Bool(*b),
        Wire::Number(n) => Value::Number(*n),
        Wire::BigInt(s) => match s.parse::<num_bigint::BigInt>() {
            Ok(b) => Value::BigInt(Rc::new(b)),
            Err(_) => Value::Undefined,
        },
        Wire::Str(s) => Value::Str(s.as_str().into()),
        Wire::Ref(i) => memo.get(*i as usize).cloned().unwrap_or_default(),
        Wire::Array(items) => {
            let out = vm.new_array(Vec::new());
            memo.push(out.clone());
            let mut values = Vec::with_capacity(items.len());
            let mut holes = Vec::new();
            for (i, x) in items.iter().enumerate() {
                match x {
                    Some(x) => values.push(decode(vm, x, memo)?),
                    None => {
                        values.push(Value::Undefined);
                        holes.push(i);
                    }
                }
            }
            if let Value::Object(n) = &out {
                if let ObjKind::Array(a) = &n.kind {
                    *a.borrow_mut() = values;
                }
                for i in holes {
                    n.mark_holes(i, i + 1);
                }
            }
            out
        }
        Wire::Object(entries) => {
            let o = vm.new_object();
            let out = Value::Object(o.clone());
            memo.push(out.clone());
            for (k, x) in entries {
                let v = decode(vm, x, memo)?;
                o.set_own(k.as_str(), v);
            }
            out
        }
        Wire::Date(t) => {
            let o = Obj::new(
                ObjKind::Date(std::cell::Cell::new(*t)),
                Some(vm.realm.date_proto.clone()),
            );
            let out = Value::Object(o);
            memo.push(out.clone());
            out
        }
        Wire::RegExp(source, flags) => {
            let out = super::make_regexp(vm, source, flags)?;
            memo.push(out.clone());
            out
        }
        Wire::Map(entries) => {
            let o = Obj::new(
                ObjKind::Map(Box::default()),
                Some(vm.realm.map_proto.clone()),
            );
            let out = Value::Object(o.clone());
            memo.push(out.clone());
            for (k, x) in entries {
                let k = decode(vm, k, memo)?;
                let x = decode(vm, x, memo)?;
                if let ObjKind::Map(m) = &o.kind {
                    m.borrow_mut().insert(MapKey::from_value(&k), x);
                }
            }
            out
        }
        Wire::Set(items) => {
            let o = Obj::new(
                ObjKind::Set(Box::default()),
                Some(vm.realm.set_proto.clone()),
            );
            let out = Value::Object(o.clone());
            memo.push(out.clone());
            for x in items {
                let x = decode(vm, x, memo)?;
                if let ObjKind::Set(s) = &o.kind {
                    s.borrow_mut().insert(MapKey::from_value(&x));
                }
            }
            out
        }
        Wire::Buffer(bytes, max) => {
            let o = super::typed::new_buffer(vm, bytes.clone());
            if let ObjKind::ArrayBuffer(b) = &o.kind {
                if max.is_some() {
                    let data = BufferData {
                        bytes: RefCell::new(std::mem::take(&mut *b.bytes.borrow_mut())),
                        max: *max,
                        shared: false,
                        external: false,
                        shared_mem: None,
                    };
                    let rebuilt = Obj::new(
                        ObjKind::ArrayBuffer(Box::new(data)),
                        o.proto.borrow().clone(),
                    );
                    let out = Value::Object(rebuilt);
                    memo.push(out.clone());
                    return Ok(out);
                }
            }
            let out = Value::Object(o);
            memo.push(out.clone());
            out
        }
        Wire::Shared(mem, max) => {
            let out = Value::Object(shared_buffer(vm, mem.clone(), *max));
            memo.push(out.clone());
            out
        }
        Wire::View(kind, buf, offset, length, tracking) => {
            let slot = memo.len();
            memo.push(Value::Undefined);
            let Value::Object(buffer) = decode(vm, buf, memo)? else {
                return Ok(Value::Undefined);
            };
            let view = View {
                kind: if *kind == DATA_VIEW {
                    TypedKind::Uint8
                } else {
                    TypedKind::ALL[*kind as usize]
                },
                buffer,
                offset: *offset,
                length: *length,
                tracking: *tracking,
            };
            let out = if *kind == DATA_VIEW {
                Value::Object(Obj::new(
                    ObjKind::DataView(Box::new(view)),
                    Some(vm.realm.dataview_proto.clone()),
                ))
            } else {
                let proto = vm.realm.typed_protos[*kind as usize].clone();
                Value::Object(Obj::new(ObjKind::Typed(Box::new(view)), Some(proto)))
            };
            memo[slot] = out.clone();
            out
        }
        Wire::Error {
            name,
            message,
            stack,
            cause,
        } => {
            let kind = match name.as_str() {
                "EvalError" => ErrorKind::EvalError,
                "RangeError" => ErrorKind::RangeError,
                "ReferenceError" => ErrorKind::ReferenceError,
                "SyntaxError" => ErrorKind::SyntaxError,
                "TypeError" => ErrorKind::TypeError,
                "URIError" => ErrorKind::URIError,
                _ => ErrorKind::Error,
            };
            let e = vm.make_error(kind, message.clone().unwrap_or_default());
            memo.push(e.clone());
            if let Value::Object(o) = &e {
                if message.is_none() {
                    o.props.borrow_mut().shift_remove("message");
                }
                if let Some(s) = stack {
                    o.set_hidden("stack", Value::Str(s.as_str().into()));
                }
                if let Some(c) = cause {
                    let c = decode(vm, c, memo)?;
                    o.set_hidden("cause", c);
                }
            }
            e
        }
        Wire::Boxed(inner) => {
            let p = decode(vm, inner, memo)?;
            let o = Value::Object(vm.to_object(&p)?);
            memo.push(o.clone());
            o
        }
        Wire::Port(id) => {
            let out = attach_port(vm, *id)?;
            memo.push(out.clone());
            out
        }
    })
}

pub fn deserialize(vm: &mut Vm, w: &Wire) -> R {
    let mut memo = Vec::new();
    decode(vm, w, &mut memo)
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

struct PortState {
    peer: u64,
    owner: Option<(u64, Sender<IoEvent>)>,
    queue: Vec<Wire>,
    closed: bool,
}

static PORTS: LazyLock<Mutex<HashMap<u64, PortState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub enum PortEvent {
    Message(Wire),
    Flush,
    Close,
}

pub enum WorkerEvent {
    Online,
    Error(Wire),
    Exit(i32),
}

struct LocalPort {
    object: Value,
    deliver: Value,
    closed: Value,
    started: bool,
    refed: bool,
    counted: bool,
    queue: VecDeque<Wire>,
}

struct LocalWorker {
    object: Value,
    refed: bool,
    counted: bool,
}

pub struct WorkerInit {
    pub thread_id: u64,
    pub port: u64,
    pub data: Option<Wire>,
    pub env: Option<Wire>,
    pub name: String,
    pub process_env: Option<Vec<(String, String)>>,
}

pub struct ThreadState {
    pub uid: u64,
    pub interrupt: Arc<AtomicBool>,
    pub sample: Arc<AtomicBool>,
    pub terminating: bool,
    pub exit_code: Option<i32>,
    pub worker: Option<WorkerInit>,
    pub deferred: VecDeque<IoEvent>,
    ports: FxHashMap<u64, LocalPort>,
    workers: FxHashMap<u64, LocalWorker>,
    broadcasts: FxHashMap<u64, Value>,
    port_factory: Option<Value>,
    async_waits: FxHashMap<u64, Rc<Obj>>,
}

impl Default for ThreadState {
    fn default() -> ThreadState {
        ThreadState {
            uid: next_id(),
            interrupt: Arc::new(AtomicBool::new(false)),
            sample: Arc::new(AtomicBool::new(false)),
            terminating: false,
            exit_code: None,
            worker: None,
            deferred: VecDeque::new(),
            ports: FxHashMap::default(),
            workers: FxHashMap::default(),
            broadcasts: FxHashMap::default(),
            port_factory: None,
            async_waits: FxHashMap::default(),
        }
    }
}

impl ThreadState {
    pub fn port_id_of(&self, o: &Rc<Obj>) -> Option<u64> {
        self.ports
            .iter()
            .find(|(_, p)| matches!(&p.object, Value::Object(x) if Rc::ptr_eq(x, o)))
            .map(|(id, _)| *id)
    }

    pub fn active_names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        out.extend(
            self.ports
                .values()
                .filter(|p| p.counted)
                .map(|_| "MessagePort"),
        );
        out.extend(
            self.workers
                .values()
                .filter(|w| w.counted)
                .map(|_| "Worker"),
        );
        out.extend(self.async_waits.keys().map(|_| "AtomicsWaitAsync"));
        out
    }

    #[inline]
    pub fn interrupted(&self) -> bool {
        self.interrupt.load(Ordering::Relaxed)
    }

    pub fn take_sample(&self) -> bool {
        if self.sample.swap(false, Ordering::SeqCst) {
            self.interrupt.store(false, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub fn start_sampler(&self, every: std::time::Duration) {
        let sample = self.sample.clone();
        let interrupt = self.interrupt.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(every);
                sample.store(true, Ordering::SeqCst);
                interrupt.store(true, Ordering::SeqCst);
            }
        });
    }
}

fn recount(vm: &mut Vm, id: u64) {
    let Some(p) = vm.threads.ports.get_mut(&id) else {
        return;
    };
    let want = p.started && p.refed;
    if want != p.counted {
        p.counted = want;
        if want {
            vm.io.pending += 1;
        } else {
            vm.io.pending = vm.io.pending.saturating_sub(1);
        }
    }
}

fn send_to(
    owner: &Option<(u64, Sender<IoEvent>)>,
    port: u64,
    event: PortEvent,
) -> Result<(), PortEvent> {
    match owner {
        Some((_, tx)) => tx
            .send(IoEvent::Port { port, event })
            .map_err(|e| match e.0 {
                IoEvent::Port { event, .. } => event,
                _ => PortEvent::Close,
            }),
        None => Err(event),
    }
}

fn post(port: u64, wire: Wire) {
    let mut ports = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(peer) = ports.get(&port).map(|p| p.peer) else {
        return;
    };
    let Some(target) = ports.get_mut(&peer) else {
        return;
    };
    if target.closed {
        return;
    }
    if let Err(PortEvent::Message(w)) = send_to(&target.owner, peer, PortEvent::Message(wire)) {
        target.owner = None;
        target.queue.push(w);
    }
}

fn create_pair(
    owner_a: Option<(u64, Sender<IoEvent>)>,
    owner_b: Option<(u64, Sender<IoEvent>)>,
) -> (u64, u64) {
    let a = next_id();
    let b = next_id();
    let mut ports = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    ports.insert(
        a,
        PortState {
            peer: b,
            owner: owner_a,
            queue: Vec::new(),
            closed: false,
        },
    );
    ports.insert(
        b,
        PortState {
            peer: a,
            owner: owner_b,
            queue: Vec::new(),
            closed: false,
        },
    );
    (a, b)
}

fn port_object(vm: &mut Vm, id: u64) -> R {
    let factory = match &vm.threads.port_factory {
        Some(f) => f.clone(),
        None => {
            super::web::load(vm)?;
            vm.threads.port_factory.clone().unwrap_or_default()
        }
    };
    let made = vm.call_value(&factory, Value::Undefined, &[Value::Number(id as f64)])?;
    let obj = vm.get_prop(&made, "0")?;
    let deliver = vm.get_prop(&made, "1")?;
    let closed = vm.get_prop(&made, "2")?;
    vm.threads.ports.insert(
        id,
        LocalPort {
            object: obj.clone(),
            deliver,
            closed,
            started: false,
            refed: true,
            counted: false,
            queue: VecDeque::new(),
        },
    );
    Ok(obj)
}

fn detach_port(vm: &mut Vm, id: u64) {
    if let Some(p) = vm.threads.ports.remove(&id) {
        if p.counted {
            vm.io.pending = vm.io.pending.saturating_sub(1);
        }
        let mut ports = PORTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(st) = ports.get_mut(&id) {
            st.owner = None;
            for w in p.queue.into_iter().rev() {
                st.queue.insert(0, w);
            }
        }
    }
}

fn attach_port(vm: &mut Vm, id: u64) -> R {
    if let Some(p) = vm.threads.ports.get(&id) {
        return Ok(p.object.clone());
    }
    let obj = port_object(vm, id)?;
    let queued = {
        let mut ports = PORTS.lock().unwrap_or_else(|e| e.into_inner());
        match ports.get_mut(&id) {
            Some(st) => {
                st.owner = Some((vm.threads.uid, vm.io.tx.clone()));
                std::mem::take(&mut st.queue)
            }
            None => Vec::new(),
        }
    };
    if let Some(p) = vm.threads.ports.get_mut(&id) {
        p.queue.extend(queued);
    }
    Ok(obj)
}

fn deliver(vm: &mut Vm, id: u64) -> Result<(), Value> {
    loop {
        let (obj, f, wire) = {
            let Some(p) = vm.threads.ports.get_mut(&id) else {
                return Ok(());
            };
            if !p.started {
                return Ok(());
            }
            let Some(w) = p.queue.pop_front() else {
                return Ok(());
            };
            (p.object.clone(), p.deliver.clone(), w)
        };
        let data = deserialize(vm, &wire)?;
        vm.call_value(&f, obj, &[data])?;
        vm.run_microtasks()?;
    }
}

pub fn handle_port(vm: &mut Vm, port: u64, event: PortEvent) -> Result<(), Value> {
    if !vm.threads.ports.contains_key(&port) {
        let mut ports = PORTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(st) = ports.get_mut(&port) {
            let mine = matches!(&st.owner, Some((uid, _)) if *uid == vm.threads.uid);
            if mine {
                st.owner = None;
            }
            if let PortEvent::Message(w) = event {
                if let Err(PortEvent::Message(w)) = send_to(&st.owner, port, PortEvent::Message(w))
                {
                    st.queue.push(w);
                }
            }
        }
        return Ok(());
    }
    match event {
        PortEvent::Message(w) => {
            if let Some(p) = vm.threads.ports.get_mut(&port) {
                p.queue.push_back(w);
            }
            deliver(vm, port)
        }
        PortEvent::Flush => deliver(vm, port),
        PortEvent::Close => close_local(vm, port, false),
    }
}

fn close_local(vm: &mut Vm, id: u64, notify: bool) -> Result<(), Value> {
    let Some(p) = vm.threads.ports.remove(&id) else {
        return Ok(());
    };
    if p.counted {
        vm.io.pending = vm.io.pending.saturating_sub(1);
    }
    {
        let mut ports = PORTS.lock().unwrap_or_else(|e| e.into_inner());
        let peer = ports.get(&id).map(|s| s.peer);
        if let Some(st) = ports.get_mut(&id) {
            st.closed = true;
        }
        if notify {
            if let Some(peer) = peer {
                if let Some(st) = ports.get(&peer) {
                    if !st.closed {
                        let _ = send_to(&st.owner, peer, PortEvent::Close);
                    }
                }
            }
        }
        if let Some(peer) = peer {
            if ports.get(&peer).is_none_or(|s| s.closed) {
                ports.remove(&peer);
                ports.remove(&id);
            }
        }
    }
    if p.closed.is_callable() {
        vm.call_value(&p.closed, p.object.clone(), &[])?;
    }
    Ok(())
}

fn close_owned_ports(vm: &mut Vm) {
    let local: Vec<u64> = vm.threads.ports.keys().copied().collect();
    for id in local {
        let _ = close_local(vm, id, true);
    }
    let uid = vm.threads.uid;
    let mut ports = PORTS.lock().unwrap_or_else(|e| e.into_inner());
    let owned: Vec<u64> = ports
        .iter()
        .filter(|(_, st)| !st.closed && st.owner.as_ref().is_some_and(|(u, _)| *u == uid))
        .map(|(id, _)| *id)
        .collect();
    for id in owned {
        let peer = ports.get(&id).map(|s| s.peer);
        if let Some(st) = ports.get_mut(&id) {
            st.closed = true;
            st.owner = None;
        }
        if let Some(peer) = peer {
            if let Some(st) = ports.get(&peer) {
                if !st.closed {
                    let _ = send_to(&st.owner, peer, PortEvent::Close);
                }
            }
        }
    }
}

fn port_arg(vm: &mut Vm, args: &[Value]) -> Result<u64, Value> {
    Ok(vm.to_number(&arg(args, 0))? as u64)
}

fn native_port_pair(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let me = Some((vm.threads.uid, vm.io.tx.clone()));
    let (a, b) = create_pair(me.clone(), me);
    let pa = port_object(vm, a)?;
    let pb = port_object(vm, b)?;
    Ok(vm.new_array(vec![pa, pb]))
}

fn native_port_post(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = port_arg(vm, args)?;
    let transfer = match arg(args, 2) {
        Value::Undefined | Value::Null => Vec::new(),
        v => vm.iterate(&v)?,
    };
    if transfer
        .iter()
        .any(|t| matches!(t, Value::Object(o) if vm.threads.port_id_of(o) == Some(id)))
    {
        return Err(clone_error(vm, "Transfer list contains source port"));
    }
    let wire = serialize(vm, &arg(args, 1), &transfer)?;
    if vm.threads.ports.contains_key(&id) {
        post(id, wire);
    }
    Ok(Value::Undefined)
}

fn native_port_start(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = port_arg(vm, args)?;
    let pending = match vm.threads.ports.get_mut(&id) {
        Some(p) if !p.started => {
            p.started = true;
            !p.queue.is_empty()
        }
        _ => return Ok(Value::Undefined),
    };
    recount(vm, id);
    if pending {
        let _ = vm.io.tx.send(IoEvent::Port {
            port: id,
            event: PortEvent::Flush,
        });
    }
    Ok(Value::Undefined)
}

fn native_port_ref(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = port_arg(vm, args)?;
    let on = arg(args, 1).truthy();
    if let Some(p) = vm.threads.ports.get_mut(&id) {
        p.refed = on;
    }
    recount(vm, id);
    Ok(Value::Undefined)
}

fn native_port_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = port_arg(vm, args)?;
    close_local(vm, id, true)?;
    Ok(Value::Undefined)
}

fn native_port_receive(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = port_arg(vm, args)?;
    if !vm.threads.ports.contains_key(&id) {
        return Ok(Value::Undefined);
    }
    loop {
        let next = vm
            .threads
            .ports
            .get_mut(&id)
            .and_then(|p| p.queue.pop_front());
        if let Some(w) = next {
            let data = deserialize(vm, &w)?;
            let o = vm.new_object();
            o.set_own("message", data);
            return Ok(Value::Object(o));
        }
        match vm.io.rx.try_recv() {
            Ok(IoEvent::Port {
                port,
                event: PortEvent::Message(w),
            }) if vm.threads.ports.contains_key(&port) => {
                let started = match vm.threads.ports.get_mut(&port) {
                    Some(p) => {
                        p.queue.push_back(w);
                        p.started
                    }
                    None => false,
                };
                if port != id && started {
                    vm.threads.deferred.push_back(IoEvent::Port {
                        port,
                        event: PortEvent::Flush,
                    });
                }
            }
            Ok(other) => vm.threads.deferred.push_back(other),
            Err(_) => return Ok(Value::Undefined),
        }
    }
}

fn native_port_factory(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    vm.threads.port_factory = Some(arg(args, 0));
    Ok(Value::Undefined)
}

struct Subscriber {
    id: u64,
    owner: (u64, Sender<IoEvent>),
}

static BROADCAST: LazyLock<Mutex<HashMap<String, Vec<Subscriber>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn native_broadcast_open(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = vm.to_string(&arg(args, 0))?.to_string();
    let id = next_id();
    vm.threads.broadcasts.insert(id, arg(args, 1));
    let mut b = BROADCAST.lock().unwrap_or_else(|e| e.into_inner());
    b.entry(name).or_default().push(Subscriber {
        id,
        owner: (vm.threads.uid, vm.io.tx.clone()),
    });
    Ok(Value::Number(id as f64))
}

fn native_broadcast_post(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = vm.to_string(&arg(args, 0))?.to_string();
    let from = vm.to_number(&arg(args, 1))? as u64;
    let message = arg(args, 2);
    let targets: Vec<(u64, Sender<IoEvent>)> = {
        let b = BROADCAST.lock().unwrap_or_else(|e| e.into_inner());
        b.get(&name)
            .map(|subs| {
                subs.iter()
                    .filter(|s| s.id != from)
                    .map(|s| (s.id, s.owner.1.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    for (id, tx) in targets {
        let wire = serialize(vm, &message, &[])?;
        let _ = tx.send(IoEvent::Broadcast { id, data: wire });
    }
    Ok(Value::Undefined)
}

fn native_broadcast_close(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = vm.to_string(&arg(args, 0))?.to_string();
    let id = vm.to_number(&arg(args, 1))? as u64;
    vm.threads.broadcasts.remove(&id);
    let mut b = BROADCAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(subs) = b.get_mut(&name) {
        subs.retain(|s| s.id != id);
        if subs.is_empty() {
            b.remove(&name);
        }
    }
    Ok(Value::Undefined)
}

pub fn start_async_wait(vm: &mut Vm, handle: WaitHandle, timeout: Option<Duration>) -> Rc<Obj> {
    let p = vm.new_promise();
    let id = next_id();
    vm.threads.async_waits.insert(id, p.clone());
    vm.io.pending += 1;
    let tx = vm.io.tx.clone();
    let spawned = std::thread::Builder::new()
        .name("jsq-wait-async".into())
        .spawn(move || {
            let r = block_on(&handle, timeout);
            let _ = tx.send(IoEvent::WaitDone { id, result: r });
        });
    if spawned.is_err() {
        vm.threads.async_waits.remove(&id);
        vm.io.pending = vm.io.pending.saturating_sub(1);
        vm.resolve_promise(&p, "timed-out".into());
    }
    p
}

pub fn handle_wait_done(vm: &mut Vm, id: u64, result: &'static str) {
    if let Some(p) = vm.threads.async_waits.remove(&id) {
        vm.io.pending = vm.io.pending.saturating_sub(1);
        vm.resolve_promise(&p, result.into());
    }
}

pub fn handle_broadcast(vm: &mut Vm, id: u64, data: Wire) -> Result<(), Value> {
    let Some(f) = vm.threads.broadcasts.get(&id).cloned() else {
        return Ok(());
    };
    let v = deserialize(vm, &data)?;
    vm.call_value(&f, Value::Undefined, &[v])?;
    Ok(())
}

struct WorkerControl {
    interrupt: Arc<AtomicBool>,
    tx: Sender<IoEvent>,
}

static WORKERS: LazyLock<Mutex<HashMap<u64, WorkerControl>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static THREAD_IDS: AtomicU64 = AtomicU64::new(1);

fn native_worker_spawn(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = vm.to_string(&arg(args, 0))?.to_string();
    let is_eval = arg(args, 1).truthy();
    let mut argv = vec!["jsq".to_string(), target.clone()];
    if let Value::Object(_) = arg(args, 2) {
        for v in vm.iterate(&arg(args, 2))? {
            argv.push(vm.to_string(&v)?.to_string());
        }
    }
    let transfer = match arg(args, 4) {
        Value::Undefined | Value::Null => Vec::new(),
        v => vm.iterate(&v)?,
    };
    let data = serialize(vm, &arg(args, 3), &transfer)?;
    let env = serialize(vm, &arg(args, 5), &[])?;
    let name = match arg(args, 6) {
        Value::Str(s) => s.to_string(),
        _ => String::new(),
    };
    let handler = arg(args, 7);
    let process_env = match arg(args, 8) {
        source @ Value::Object(_) => {
            let mut pairs = Vec::new();
            for k in vm.own_keys(&source) {
                let key = vm.to_string(&k)?.to_string();
                let v = vm.get_prop(&source, &key)?;
                pairs.push((key, vm.to_string(&v)?.to_string()));
            }
            Some(pairs)
        }
        _ => None,
    };
    let thread_id = THREAD_IDS.fetch_add(1, Ordering::SeqCst);
    let io = crate::io::Io::default();
    let worker_tx = io.tx.clone();
    let me = Some((vm.threads.uid, vm.io.tx.clone()));
    let (a, b) = create_pair(me, None);
    let port = port_object(vm, a)?;
    let interrupt = Arc::new(AtomicBool::new(false));
    WORKERS.lock().unwrap_or_else(|e| e.into_inner()).insert(
        thread_id,
        WorkerControl {
            interrupt: interrupt.clone(),
            tx: worker_tx,
        },
    );
    vm.threads.workers.insert(
        thread_id,
        LocalWorker {
            object: handler,
            refed: true,
            counted: true,
        },
    );
    vm.io.pending += 1;
    let parent_tx = vm.io.tx.clone();
    let init = WorkerInit {
        thread_id,
        port: b,
        data: Some(data),
        env: Some(env),
        name,
        process_env,
    };
    let spawned = std::thread::Builder::new()
        .name(format!("jsq-worker-{thread_id}"))
        .stack_size(128 * 1024 * 1024)
        .spawn(move || run_worker(io, init, interrupt, argv, target, is_eval, parent_tx));
    if spawned.is_err() {
        return Err(vm.make_error(ErrorKind::Error, "Failed to start worker thread"));
    }
    let out = vm.new_object();
    out.set_own("threadId", Value::Number(thread_id as f64));
    out.set_own("port", port);
    Ok(Value::Object(out))
}

fn run_worker(
    io: crate::io::Io,
    init: WorkerInit,
    interrupt: Arc<AtomicBool>,
    argv: Vec<String>,
    target: String,
    is_eval: bool,
    parent: Sender<IoEvent>,
) {
    let id = init.thread_id;
    let send = |event: WorkerEvent| {
        let _ = parent.send(IoEvent::Worker { id, event });
    };
    let mut vm = Vm::for_worker(argv, io, init, interrupt);
    send(WorkerEvent::Online);
    let r = if is_eval {
        vm.eval("[worker eval]", &target, crate::compiler::Mode::Repl)
            .map(|_| ())
    } else {
        vm.run_file(std::path::Path::new(&target)).map(|_| ())
    };
    let r = vm.finish_main(r);
    let code = match r {
        Ok(()) => vm
            .threads
            .exit_code
            .unwrap_or_else(|| super::exit_code(&mut vm)),
        Err(e) => {
            if vm.threads.terminating {
                vm.threads.exit_code.unwrap_or(1)
            } else {
                let wire = serialize(&mut vm, &e, &[]).unwrap_or_else(|_| Wire::Error {
                    name: "Error".into(),
                    message: Some(crate::inspect::inspect_plain(&e)),
                    stack: None,
                    cause: None,
                });
                send(WorkerEvent::Error(wire));
                1
            }
        }
    };
    vm.flush();
    super::netio::flush_sockets(&mut vm);
    let port = vm.threads.worker.as_ref().map(|w| w.port);
    if let Some(port) = port {
        let _ = close_local(&mut vm, port, true);
    }
    close_owned_ports(&mut vm);
    drop(vm);
    WORKERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    send(WorkerEvent::Exit(code));
}

pub fn handle_worker(vm: &mut Vm, id: u64, event: WorkerEvent) -> Result<(), Value> {
    let Some(w) = vm.threads.workers.get(&id) else {
        return Ok(());
    };
    let handler = w.object.clone();
    match event {
        WorkerEvent::Online => {
            vm.call_value(&handler, Value::Undefined, &["online".into()])?;
        }
        WorkerEvent::Error(wire) => {
            let e = deserialize(vm, &wire)?;
            vm.call_value(&handler, Value::Undefined, &["error".into(), e])?;
        }
        WorkerEvent::Exit(code) => {
            if let Some(w) = vm.threads.workers.remove(&id) {
                if w.counted {
                    vm.io.pending = vm.io.pending.saturating_sub(1);
                }
            }
            vm.call_value(
                &handler,
                Value::Undefined,
                &["exit".into(), Value::Number(code as f64)],
            )?;
        }
    }
    Ok(())
}

fn native_worker_terminate(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    if let Some(w) = vm.threads.workers.get_mut(&id) {
        if !w.counted {
            w.counted = true;
            vm.io.pending += 1;
        }
    }
    let workers = WORKERS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = workers.get(&id) {
        w.interrupt.store(true, Ordering::SeqCst);
        let _ = w.tx.send(IoEvent::Wake);
        wake_interrupted(&w.interrupt);
    }
    Ok(Value::Undefined)
}

fn native_worker_ref(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = vm.to_number(&arg(args, 0))? as u64;
    let on = arg(args, 1).truthy();
    if let Some(w) = vm.threads.workers.get_mut(&id) {
        w.refed = on;
        if w.counted != on {
            w.counted = on;
            if on {
                vm.io.pending += 1;
            } else {
                vm.io.pending = vm.io.pending.saturating_sub(1);
            }
        }
    }
    Ok(Value::Undefined)
}

fn native_worker_info(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let Some(init) = vm.threads.worker.as_mut() else {
        return Ok(Value::Null);
    };
    let thread_id = init.thread_id;
    let port = init.port;
    let name = init.name.clone();
    let data = init.data.take();
    let env = init.env.take();
    let out = vm.new_object();
    out.set_own("threadId", Value::Number(thread_id as f64));
    out.set_own("name", name.as_str().into());
    let port = attach_port(vm, port)?;
    out.set_own("port", port);
    let data = match data {
        Some(w) => deserialize(vm, &w)?,
        None => Value::Undefined,
    };
    out.set_own("workerData", data);
    let env = match env {
        Some(w) => deserialize(vm, &w)?,
        None => Value::Undefined,
    };
    out.set_own("environmentData", env);
    Ok(Value::Object(out))
}

fn native_structured_clone(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let transfer = match arg(args, 1) {
        Value::Undefined | Value::Null => Vec::new(),
        v => vm.iterate(&v)?,
    };
    let wire = serialize(vm, &arg(args, 0), &transfer)?;
    deserialize(vm, &wire)
}

const WIRE_MAGIC: &[u8; 4] = b"JSQ\x01";

fn put_u32(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len());
    out.extend_from_slice(s.as_bytes());
}

fn put_opt_str(out: &mut Vec<u8>, s: &Option<String>) {
    match s {
        Some(s) => {
            out.push(1);
            put_str(out, s);
        }
        None => out.push(0),
    }
}

fn wire_bytes(w: &Wire, out: &mut Vec<u8>) -> Result<(), &'static str> {
    match w {
        Wire::Undefined => out.push(0),
        Wire::Null => out.push(1),
        Wire::Bool(b) => out.extend_from_slice(&[2, *b as u8]),
        Wire::Number(n) => {
            out.push(3);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Wire::BigInt(s) => {
            out.push(4);
            put_str(out, s);
        }
        Wire::Str(s) => {
            out.push(5);
            put_str(out, s);
        }
        Wire::Ref(r) => {
            out.push(6);
            put_u32(out, *r as usize);
        }
        Wire::Array(items) => {
            out.push(7);
            put_u32(out, items.len());
            for item in items {
                match item {
                    Some(v) => {
                        out.push(1);
                        wire_bytes(v, out)?;
                    }
                    None => out.push(0),
                }
            }
        }
        Wire::Object(props) => {
            out.push(8);
            put_u32(out, props.len());
            for (k, v) in props {
                put_str(out, k);
                wire_bytes(v, out)?;
            }
        }
        Wire::Date(t) => {
            out.push(9);
            out.extend_from_slice(&t.to_le_bytes());
        }
        Wire::RegExp(src, flags) => {
            out.push(10);
            put_str(out, src);
            put_str(out, flags);
        }
        Wire::Map(entries) => {
            out.push(11);
            put_u32(out, entries.len());
            for (k, v) in entries {
                wire_bytes(k, out)?;
                wire_bytes(v, out)?;
            }
        }
        Wire::Set(items) => {
            out.push(12);
            put_u32(out, items.len());
            for v in items {
                wire_bytes(v, out)?;
            }
        }
        Wire::Buffer(bytes, max) => {
            out.push(13);
            put_u32(out, bytes.len());
            out.extend_from_slice(bytes);
            match max {
                Some(m) => {
                    out.push(1);
                    put_u32(out, *m);
                }
                None => out.push(0),
            }
        }
        Wire::View(kind, buf, offset, length, tracking) => {
            out.push(14);
            out.push(*kind);
            wire_bytes(buf, out)?;
            put_u32(out, *offset);
            put_u32(out, *length);
            out.push(*tracking as u8);
        }
        Wire::Error {
            name,
            message,
            stack,
            cause,
        } => {
            out.push(15);
            put_str(out, name);
            put_opt_str(out, message);
            put_opt_str(out, stack);
            match cause {
                Some(c) => {
                    out.push(1);
                    wire_bytes(c, out)?;
                }
                None => out.push(0),
            }
        }
        Wire::Boxed(v) => {
            out.push(16);
            wire_bytes(v, out)?;
        }
        Wire::Shared(..) => return Err("SharedArrayBuffer cannot be serialized"),
        Wire::Port(_) => return Err("MessagePort cannot be serialized"),
    }
    Ok(())
}

struct WireReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl WireReader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let end = self.at.checked_add(n)?;
        let s = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(s)
    }
    fn byte(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u32(&mut self) -> Option<usize> {
        let b = self.take(4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }
    fn f64(&mut self) -> Option<f64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Some(f64::from_le_bytes(a))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.u32()?;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    fn opt_string(&mut self) -> Option<Option<String>> {
        match self.byte()? {
            0 => Some(None),
            _ => Some(Some(self.string()?)),
        }
    }
    fn wire(&mut self, depth: usize) -> Option<Wire> {
        if depth > 10_000 {
            return None;
        }
        Some(match self.byte()? {
            0 => Wire::Undefined,
            1 => Wire::Null,
            2 => Wire::Bool(self.byte()? != 0),
            3 => Wire::Number(self.f64()?),
            4 => Wire::BigInt(self.string()?),
            5 => Wire::Str(self.string()?),
            6 => Wire::Ref(self.u32()? as u32),
            7 => {
                let n = self.u32()?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    items.push(match self.byte()? {
                        0 => None,
                        _ => Some(self.wire(depth + 1)?),
                    });
                }
                Wire::Array(items)
            }
            8 => {
                let n = self.u32()?;
                let mut props = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    let k = self.string()?;
                    props.push((k, self.wire(depth + 1)?));
                }
                Wire::Object(props)
            }
            9 => Wire::Date(self.f64()?),
            10 => {
                let src = self.string()?;
                Wire::RegExp(src, self.string()?)
            }
            11 => {
                let n = self.u32()?;
                let mut entries = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    let k = self.wire(depth + 1)?;
                    entries.push((k, self.wire(depth + 1)?));
                }
                Wire::Map(entries)
            }
            12 => {
                let n = self.u32()?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for _ in 0..n {
                    items.push(self.wire(depth + 1)?);
                }
                Wire::Set(items)
            }
            13 => {
                let n = self.u32()?;
                let bytes = self.take(n)?.to_vec();
                let max = match self.byte()? {
                    0 => None,
                    _ => Some(self.u32()?),
                };
                Wire::Buffer(bytes, max)
            }
            14 => {
                let kind = self.byte()?;
                let buf = self.wire(depth + 1)?;
                let offset = self.u32()?;
                let length = self.u32()?;
                Wire::View(kind, Box::new(buf), offset, length, self.byte()? != 0)
            }
            15 => {
                let name = self.string()?;
                let message = self.opt_string()?;
                let stack = self.opt_string()?;
                let cause = match self.byte()? {
                    0 => None,
                    _ => Some(Box::new(self.wire(depth + 1)?)),
                };
                Wire::Error {
                    name,
                    message,
                    stack,
                    cause,
                }
            }
            16 => Wire::Boxed(Box::new(self.wire(depth + 1)?)),
            _ => return None,
        })
    }
}

fn native_wire_serialize(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let wire = serialize(vm, &arg(args, 0), &[])?;
    let mut out = WIRE_MAGIC.to_vec();
    if let Err(msg) = wire_bytes(&wire, &mut out) {
        return Err(clone_error(vm, msg));
    }
    Ok(super::typed::new_uint8(vm, out))
}

fn native_wire_deserialize(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bytes = super::binary::buffer_source_bytes(&arg(args, 0)).unwrap_or_default();
    let parsed = bytes.strip_prefix(WIRE_MAGIC).and_then(|body| {
        let mut r = WireReader { bytes: body, at: 0 };
        let w = r.wire(0)?;
        (r.at == body.len()).then_some(w)
    });
    match parsed {
        Some(w) => deserialize(vm, &w),
        None => Err(vm.make_error(
            crate::vm::ErrorKind::Error,
            "Unable to deserialize cloned data.",
        )),
    }
}

pub fn install_natives(vm: &mut Vm, natives: &Rc<Obj>) {
    let list: [(&str, u32, crate::object::NativeFn); 18] = [
        ("portPair", 0, native_port_pair),
        ("portPost", 3, native_port_post),
        ("portStart", 1, native_port_start),
        ("portRef", 2, native_port_ref),
        ("portClose", 1, native_port_close),
        ("portReceive", 1, native_port_receive),
        ("portFactory", 1, native_port_factory),
        ("broadcastOpen", 2, native_broadcast_open),
        ("broadcastPost", 3, native_broadcast_post),
        ("broadcastClose", 2, native_broadcast_close),
        ("workerSpawn", 8, native_worker_spawn),
        ("workerTerminate", 1, native_worker_terminate),
        ("workerRef", 2, native_worker_ref),
        ("workerInfo", 0, native_worker_info),
        ("structuredClone", 2, native_structured_clone),
        ("isMainThread", 0, native_is_main_thread),
        ("wireSerialize", 1, native_wire_serialize),
        ("wireDeserialize", 1, native_wire_deserialize),
    ];
    for (name, arity, f) in list {
        method(vm, natives, name, arity, f);
    }
}

fn native_is_main_thread(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Bool(vm.threads.worker.is_none()))
}

struct Waiter {
    addr: usize,
    woken: Mutex<bool>,
    cv: Condvar,
    interrupt: Arc<AtomicBool>,
}

static WAITERS: LazyLock<Mutex<Vec<Arc<Waiter>>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub struct WaitHandle(Arc<Waiter>);

pub fn register_waiter(
    addr: usize,
    check: impl FnOnce() -> bool,
    interrupt: Arc<AtomicBool>,
) -> Option<WaitHandle> {
    let waiter = Arc::new(Waiter {
        addr,
        woken: Mutex::new(false),
        cv: Condvar::new(),
        interrupt,
    });
    let mut list = WAITERS.lock().unwrap_or_else(|e| e.into_inner());
    if !check() {
        return None;
    }
    list.push(waiter.clone());
    Some(WaitHandle(waiter))
}

pub fn wait_on(
    addr: usize,
    check: impl FnOnce() -> bool,
    timeout: Option<Duration>,
    interrupt: Arc<AtomicBool>,
) -> &'static str {
    match register_waiter(addr, check, interrupt) {
        Some(h) => block_on(&h, timeout),
        None => "not-equal",
    }
}

pub fn block_on(handle: &WaitHandle, timeout: Option<Duration>) -> &'static str {
    let waiter = handle.0.clone();
    let mut woken = waiter.woken.lock().unwrap_or_else(|e| e.into_inner());
    let deadline = timeout.map(|t| std::time::Instant::now() + t);
    while !*woken && !waiter.interrupt.load(Ordering::SeqCst) {
        match deadline {
            Some(d) => {
                let now = std::time::Instant::now();
                if now >= d {
                    break;
                }
                woken = match waiter.cv.wait_timeout(woken, d - now) {
                    Ok((g, _)) => g,
                    Err(e) => e.into_inner().0,
                };
            }
            None => {
                woken = match waiter.cv.wait(woken) {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
            }
        }
    }
    let ok = *woken;
    drop(woken);
    if !ok {
        let mut list = WAITERS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = list.iter().position(|w| Arc::ptr_eq(w, &waiter)) {
            list.remove(i);
            return "timed-out";
        }
        return "ok";
    }
    "ok"
}

fn wake_interrupted(interrupt: &Arc<AtomicBool>) {
    let list = WAITERS.lock().unwrap_or_else(|e| e.into_inner());
    for w in list.iter() {
        if Arc::ptr_eq(&w.interrupt, interrupt) {
            let _guard = w.woken.lock().unwrap_or_else(|e| e.into_inner());
            w.cv.notify_all();
        }
    }
}

pub fn notify(addr: usize, count: usize) -> usize {
    let mut list = WAITERS.lock().unwrap_or_else(|e| e.into_inner());
    let mut woken = 0;
    let mut i = 0;
    while i < list.len() && woken < count {
        if list[i].addr == addr {
            let w = list.remove(i);
            *w.woken.lock().unwrap_or_else(|e| e.into_inner()) = true;
            w.cv.notify_one();
            woken += 1;
        } else {
            i += 1;
        }
    }
    woken
}
