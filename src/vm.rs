use crate::bigint::BigOp;
use crate::builtins;
use crate::bytecode::{Cmp, FunctionProto, ModuleInfo, NO_NAME, Op, ProtoKind};
use crate::compiler::{Mode, compile_eval, compile_indirect_eval, compile_module};
use crate::event_loop::{Job, ResumeMode, Signal, Step, Timer};
use crate::inspect::{inspect_plain, inspect_top};
use crate::jsstr::JsStr;
use crate::module;
use crate::number::{js_pow, to_int32, to_uint32};
use crate::object::{
    Closure, ITER_KEY, IterState, MapKey, Obj, ObjKind, UpCell, Upvalue, array_index, is_symbol_key,
};
use crate::parser;
use crate::token::SyntaxError;
use crate::value::{Value, char_at16, char_at16_rc, compare_strings, len16, len16_rc};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

const MAX_FRAMES: usize = 9000;
const MAX_NATIVE_DEPTH: usize = 2500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Error,
    TypeError,
    RangeError,
    ReferenceError,
    SyntaxError,
    URIError,
    EvalError,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 7] = [
        ErrorKind::Error,
        ErrorKind::TypeError,
        ErrorKind::RangeError,
        ErrorKind::ReferenceError,
        ErrorKind::SyntaxError,
        ErrorKind::URIError,
        ErrorKind::EvalError,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::Error => "Error",
            ErrorKind::TypeError => "TypeError",
            ErrorKind::RangeError => "RangeError",
            ErrorKind::ReferenceError => "ReferenceError",
            ErrorKind::SyntaxError => "SyntaxError",
            ErrorKind::URIError => "URIError",
            ErrorKind::EvalError => "EvalError",
        }
    }
}

pub struct Pristine {
    array_iter: Value,
    pub(crate) regexp_exec: Value,
    pub(crate) regexp_symbols: [Value; 5],
    pub(crate) regexp_flags: Value,
    map_iter: Value,
    set_iter: Value,
    string_iter: Value,
    iter_nexts: [Value; 4],
    gen_next: Value,
    pub(crate) set_add: Value,
    pub(crate) map_set: Value,
    pub(crate) array_join: Value,
}

pub struct Realm {
    pub object_proto: Rc<Obj>,
    pub function_proto: Rc<Obj>,
    pub array_proto: Rc<Obj>,
    pub string_proto: Rc<Obj>,
    pub number_proto: Rc<Obj>,
    pub boolean_proto: Rc<Obj>,
    pub map_proto: Rc<Obj>,
    pub set_proto: Rc<Obj>,
    pub iter_proto: Rc<Obj>,
    pub map_iter_proto: Rc<Obj>,
    pub set_iter_proto: Rc<Obj>,
    pub string_iter_proto: Rc<Obj>,
    pub generator_proto: Rc<Obj>,
    pub async_generator_proto: Rc<Obj>,
    pub generator_function_proto: Rc<Obj>,
    pub async_generator_function_proto: Rc<Obj>,
    pub async_function_proto: Rc<Obj>,
    pub promise_proto: Rc<Obj>,
    pub symbol_proto: Rc<Obj>,
    pub regexp_proto: Rc<Obj>,
    pub date_proto: Rc<Obj>,
    pub weakmap_proto: Rc<Obj>,
    pub weakset_proto: Rc<Obj>,
    pub weakref_proto: Rc<Obj>,
    pub error_protos: Vec<Rc<Obj>>,
    pub array_buffer_proto: Rc<Obj>,
    pub shared_buffer_proto: Rc<Obj>,
    pub typed_proto: Rc<Obj>,
    pub typed_protos: Vec<Rc<Obj>>,
    pub dataview_proto: Rc<Obj>,
    pub bigint_proto: Rc<Obj>,
    pub iterator_base: Rc<Obj>,
    pub async_iterator_base: Rc<Obj>,
}

impl Realm {
    fn new() -> Realm {
        let object_proto = Obj::new(ObjKind::Ordinary, None);
        let mk = || Obj::new(ObjKind::Ordinary, Some(object_proto.clone()));
        let error_proto = mk();
        let mut error_protos = vec![error_proto.clone()];
        for _ in 1..ErrorKind::ALL.len() {
            error_protos.push(Obj::new(ObjKind::Ordinary, Some(error_proto.clone())));
        }
        let iterator_base = mk();
        let async_iterator_base = mk();
        let function_proto = Obj::new(
            ObjKind::Native(Box::new(crate::object::Native {
                name: "".into(),
                arity: 0,
                func: function_proto_call,
                ctor: None,
            })),
            Some(object_proto.clone()),
        );
        let typed_proto = mk();
        let typed_protos = crate::object::TypedKind::ALL
            .iter()
            .map(|_| Obj::new(ObjKind::Ordinary, Some(typed_proto.clone())))
            .collect();
        Realm {
            array_buffer_proto: mk(),
            shared_buffer_proto: mk(),
            typed_proto,
            typed_protos,
            dataview_proto: mk(),
            bigint_proto: mk(),
            iter_proto: Obj::new(ObjKind::Ordinary, Some(iterator_base.clone())),
            map_iter_proto: Obj::new(ObjKind::Ordinary, Some(iterator_base.clone())),
            set_iter_proto: Obj::new(ObjKind::Ordinary, Some(iterator_base.clone())),
            string_iter_proto: Obj::new(ObjKind::Ordinary, Some(iterator_base.clone())),
            generator_proto: Obj::new(ObjKind::Ordinary, Some(iterator_base.clone())),
            async_generator_proto: Obj::new(ObjKind::Ordinary, Some(async_iterator_base.clone())),
            generator_function_proto: Obj::new(ObjKind::Ordinary, Some(function_proto.clone())),
            async_generator_function_proto: Obj::new(
                ObjKind::Ordinary,
                Some(function_proto.clone()),
            ),
            async_function_proto: Obj::new(ObjKind::Ordinary, Some(function_proto.clone())),
            iterator_base,
            async_iterator_base,
            function_proto,
            array_proto: Obj::new(
                ObjKind::Array(RefCell::new(Vec::new())),
                Some(object_proto.clone()),
            ),
            string_proto: Obj::new(
                ObjKind::Primitive(Value::Str("".into())),
                Some(object_proto.clone()),
            ),
            number_proto: Obj::new(
                ObjKind::Primitive(Value::Number(0.0)),
                Some(object_proto.clone()),
            ),
            boolean_proto: Obj::new(
                ObjKind::Primitive(Value::Bool(false)),
                Some(object_proto.clone()),
            ),
            map_proto: mk(),
            set_proto: mk(),
            promise_proto: mk(),
            symbol_proto: mk(),
            regexp_proto: mk(),
            date_proto: mk(),
            weakmap_proto: mk(),
            weakset_proto: mk(),
            weakref_proto: mk(),
            error_protos,
            object_proto,
        }
    }
}

#[derive(Default)]
pub struct Globals {
    index: FxHashMap<JsStr, u32>,
    lex: FxHashMap<JsStr, u32>,
    values: Vec<Value>,
    kinds: Vec<u8>,
    pub(crate) attrs: FxHashMap<JsStr, u8>,
    pub(crate) readonly: bool,
}

const SLOT_LEX: u8 = 1;
const SLOT_CONST: u8 = 2;

impl Globals {
    pub fn resolve(&self, name: &str) -> Option<u32> {
        if !self.lex.is_empty() {
            if let Some(&i) = self.lex.get(name) {
                return Some(i);
            }
        }
        self.index.get(name).copied()
    }

    pub fn lookup(&self, name: &str) -> Option<&Value> {
        self.resolve(name).map(|i| &self.values[i as usize])
    }

    pub fn has_lex(&self, name: &str) -> bool {
        self.lex.contains_key(name)
    }

    pub fn is_lex_slot(&self, slot: u32) -> bool {
        self.kinds[slot as usize] & SLOT_LEX != 0
    }

    pub fn is_const_slot(&self, slot: u32) -> bool {
        self.kinds[slot as usize] & SLOT_CONST != 0
    }

    pub fn declare_lex(&mut self, name: JsStr, v: Value, is_const: bool) -> u32 {
        let kind = SLOT_LEX | if is_const { SLOT_CONST } else { 0 };
        if let Some(&i) = self.lex.get(&name) {
            self.values[i as usize] = v;
            self.kinds[i as usize] = kind;
            return i;
        }
        let i = self.values.len() as u32;
        self.values.push(v);
        self.kinds.push(kind);
        self.lex.insert(name, i);
        i
    }

    pub fn names(&self) -> Vec<JsStr> {
        let mut v: Vec<(u32, JsStr)> = self.index.iter().map(|(k, &i)| (i, k.clone())).collect();
        v.sort_unstable_by_key(|(i, _)| *i);
        v.into_iter().map(|(_, k)| k).collect()
    }

    pub fn attrs_of(&self, name: &str) -> u8 {
        self.attrs.get(name).copied().unwrap_or(0)
    }

    pub fn remove(&mut self, name: &str) -> bool {
        if let Some(i) = self.index.remove(name) {
            self.values[i as usize] = Value::Undefined;
            self.attrs.remove(name);
            return true;
        }
        false
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.index.get(name).map(|&i| &self.values[i as usize])
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        match self.index.get(name) {
            Some(&i) => Some(&mut self.values[i as usize]),
            None => None,
        }
    }

    pub fn slot(&self, name: &str) -> Option<u32> {
        self.index.get(name).copied()
    }

    pub fn insert(&mut self, name: JsStr, v: Value) -> u32 {
        if let Some(&i) = self.index.get(&name) {
            self.values[i as usize] = v;
            return i;
        }
        let i = self.values.len() as u32;
        self.values.push(v);
        self.kinds.push(0);
        self.index.insert(name, i);
        i
    }

    pub fn at(&self, slot: u32) -> &Value {
        &self.values[slot as usize]
    }

    pub fn at_mut(&mut self, slot: u32) -> &mut Value {
        &mut self.values[slot as usize]
    }
}

pub(crate) struct Frame {
    pub(crate) closure: Rc<Closure>,
    pub(crate) ip: usize,
    pub(crate) base: usize,
    pub(crate) construct: bool,
    pub(crate) co: Option<Rc<Obj>>,
    pub(crate) new_target: Option<Rc<Obj>>,
}

pub(crate) struct Handler {
    pub(crate) frame: usize,
    pub(crate) stack_len: usize,
    pub(crate) catch_ip: usize,
    pub(crate) guard: bool,
}

pub struct Vm {
    pub stack: Vec<Value>,
    pub(crate) frames: Vec<Frame>,
    pub(crate) handlers: Vec<Handler>,
    pub(crate) open_upvalues: BTreeMap<usize, Rc<UpCell>>,
    pub(crate) console_indent: usize,
    pub globals: Globals,
    arg_buf: Vec<Value>,
    builders: Vec<(usize, String)>,
    pub realm: Realm,
    modules: FxHashMap<PathBuf, Value>,
    pub(crate) require_cache: Option<Rc<Obj>>,
    proto_cache: FxHashMap<PathBuf, Rc<FunctionProto>>,
    ns_cache: FxHashMap<PathBuf, Value>,
    ns_get_fn: Option<Value>,
    loading: Vec<PathBuf>,
    builtin_modules: FxHashMap<String, Value>,
    pub out: Box<dyn Write>,
    pub err: Box<dyn Write>,
    pub argv: Vec<String>,
    pub start: Instant,
    pub rng: u64,
    pub(crate) native_depth: usize,
    pub(crate) microtasks: VecDeque<Job>,
    pub(crate) ticks: VecDeque<(Value, Vec<Value>)>,
    pub(crate) timers: BTreeMap<(u128, u64), Timer>,
    pub(crate) timer_keys: FxHashMap<u64, (u128, u64)>,
    pub(crate) timer_seq: u64,
    pub(crate) unref_timers: rustc_hash::FxHashSet<u64>,
    pub(crate) pending_rejections: Vec<Rc<Obj>>,
    pub(crate) co_signal: Option<Signal>,
    pub accessors_used: bool,
    pub(crate) readonly_used: bool,
    pub(crate) proxies_used: bool,
    pub(crate) global_accessors: bool,
    pub(crate) next_new_target: Option<Rc<Obj>>,
    pub(crate) eval_fn: Option<Rc<Obj>>,
    pub(crate) gen_return_pending: bool,
    pub(crate) silent_fail: bool,
    pub(crate) pending_ns: Option<Value>,
    pub(crate) import_metas: FxHashMap<JsStr, Value>,
    pub rejection: bool,
    pub exiting: bool,
    pub(crate) main_cjs_pending: bool,
    pub(crate) promise_intrinsic: Option<Rc<Obj>>,
    pub(crate) wasm: Option<Box<builtins::wasm::WasmState>>,
    pub(crate) kept_alive: FxHashMap<usize, Rc<Obj>>,
    pub symbol_seq: u64,
    pub symbol_registry: FxHashMap<JsStr, Value>,
    pub symbol_iterator: Value,
    pub symbol_async_iterator: Value,
    pub promise_resolve_fn: Value,
    pub promise_reject_fn: Value,
    pub async_from_sync_next: Value,
    pub iter_value_fn: Value,
    pub default_has_instance: Value,
    pub proto_accessor: Value,
    pub(crate) thrower: Value,
    pub(crate) array_species: Value,
    pub(crate) proto_indices: bool,
    pub(crate) binary_ctors: Vec<Value>,
    pub(crate) pristine: Option<Box<Pristine>>,
    pub(crate) array_ctor: Option<Rc<Obj>>,
    pub(crate) io: crate::io::Io,
    pub(crate) io_fetches: FxHashMap<u64, Rc<Obj>>,
    pub(crate) io_sockets: FxHashMap<u64, builtins::netio::SocketEntry>,
    pub(crate) io_net_servers: FxHashMap<u64, builtins::netio::NetServerEntry>,
    pub(crate) io_children: FxHashMap<u64, builtins::netio::ChildEntry>,
    pub(crate) io_watchers: FxHashMap<u64, builtins::netio::WatchEntry>,
    pub(crate) io_files: FxHashMap<u32, std::fs::File>,
    pub(crate) next_fd: u32,
    pub(crate) io_servers: FxHashMap<u64, (Value, Value, crate::io::ServerHandle)>,
    pub(crate) io_replies: FxHashMap<u64, std::sync::mpsc::Sender<crate::io::HttpReply>>,
    pub(crate) web: Option<Value>,
    pub(crate) node: Option<Value>,
    pub(crate) stdin_handler: Option<Value>,
    pub(crate) stdin_stopped: bool,
}

impl Vm {
    pub fn new(argv: Vec<String>) -> Vm {
        Self::with_output(
            argv,
            Box::new(std::io::BufWriter::new(std::io::stdout())),
            Box::new(std::io::stderr()),
        )
    }

    pub fn with_output(argv: Vec<String>, out: Box<dyn Write>, err: Box<dyn Write>) -> Vm {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        let mut vm = Vm {
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(64),
            handlers: Vec::new(),
            open_upvalues: BTreeMap::new(),
            console_indent: 0,
            globals: Globals::default(),
            arg_buf: Vec::new(),
            builders: Vec::new(),
            realm: Realm::new(),
            modules: FxHashMap::default(),
            require_cache: None,
            proto_cache: FxHashMap::default(),
            ns_cache: FxHashMap::default(),
            ns_get_fn: None,
            loading: Vec::new(),
            builtin_modules: FxHashMap::default(),
            out,
            err,
            argv,
            start: Instant::now(),
            rng: seed | 1,
            native_depth: 0,
            microtasks: VecDeque::new(),
            ticks: VecDeque::new(),
            timers: BTreeMap::new(),
            timer_keys: FxHashMap::default(),
            timer_seq: 0,
            unref_timers: rustc_hash::FxHashSet::default(),
            pending_rejections: Vec::new(),
            co_signal: None,
            accessors_used: false,
            readonly_used: false,
            proxies_used: false,
            global_accessors: false,
            next_new_target: None,
            eval_fn: None,
            gen_return_pending: false,
            silent_fail: false,
            pending_ns: None,
            import_metas: FxHashMap::default(),
            rejection: false,
            symbol_seq: 0,
            symbol_registry: FxHashMap::default(),
            symbol_iterator: Value::Undefined,
            symbol_async_iterator: Value::Undefined,
            promise_resolve_fn: Value::Undefined,
            promise_reject_fn: Value::Undefined,
            async_from_sync_next: Value::Undefined,
            iter_value_fn: Value::Undefined,
            default_has_instance: Value::Undefined,
            proto_accessor: Value::Undefined,
            thrower: Value::Undefined,
            array_species: Value::Undefined,
            proto_indices: false,
            binary_ctors: Vec::new(),
            pristine: None,
            array_ctor: None,
            io: crate::io::Io::default(),
            io_fetches: FxHashMap::default(),
            io_sockets: FxHashMap::default(),
            io_net_servers: FxHashMap::default(),
            io_children: FxHashMap::default(),
            io_watchers: FxHashMap::default(),
            io_files: FxHashMap::default(),
            next_fd: 3,
            io_servers: FxHashMap::default(),
            io_replies: FxHashMap::default(),
            web: None,
            node: None,
            stdin_handler: None,
            stdin_stopped: false,
            exiting: false,
            main_cjs_pending: false,
            promise_intrinsic: None,
            wasm: None,
            kept_alive: FxHashMap::default(),
        };
        crate::gc::configure_from_env();
        builtins::install(&mut vm);
        builtins::run_prelude(&mut vm);
        if std::env::var_os("JSQ_IPC").is_some() {
            let _ = builtins::node::setup_ipc(&mut vm);
        }
        builtins::hide_builtin_members(&mut vm);
        builtins::array::capture_species(&mut vm);
        vm.capture_pristine();
        vm.accessors_used = false;
        vm.readonly_used = false;
        vm.proxies_used = false;
        vm
    }

    pub fn flush(&mut self) {
        let _ = self.out.flush();
        let _ = self.err.flush();
    }

    pub fn define_global(&mut self, name: &str, v: Value) {
        self.globals.insert(name.into(), v);
    }

    pub fn compile(
        &self,
        src: &str,
        name: &str,
        dir: PathBuf,
        mode: Mode,
    ) -> Result<Rc<FunctionProto>, SyntaxError> {
        self.compile_unit(src, name, dir, mode, false)
    }

    pub fn compile_unit(
        &self,
        src: &str,
        name: &str,
        dir: PathBuf,
        mode: Mode,
        sloppy: bool,
    ) -> Result<Rc<FunctionProto>, SyntaxError> {
        let ast = parser::parse(src)?;
        compile_module(
            &ast,
            Rc::new(ModuleInfo {
                name: name.into(),
                dir,
                source: src.into(),
                sloppy,
            }),
            mode,
        )
    }

    pub fn compile_script(
        &self,
        src: &str,
        name: &str,
        dir: PathBuf,
        sloppy: bool,
    ) -> Result<Rc<FunctionProto>, SyntaxError> {
        let ast = parser::parse_script(src)?;
        compile_module(
            &ast,
            Rc::new(ModuleInfo {
                name: name.into(),
                dir,
                source: src.into(),
                sloppy,
            }),
            Mode::Repl,
        )
    }

    pub fn syntax_error_value(&mut self, e: &SyntaxError, name: &str) -> Value {
        let v = self.make_error(ErrorKind::SyntaxError, e.message.clone());
        if let Value::Object(o) = &v {
            o.set_own(
                "stack",
                format!(
                    "SyntaxError: {}\n    at {}:{}:{}",
                    e.message, name, e.line, e.col
                )
                .into(),
            );
        }
        v
    }

    pub fn wrap_primitive(&self, v: Value) -> Rc<Obj> {
        let proto = match &v {
            Value::Str(_) => &self.realm.string_proto,
            Value::Number(_) => &self.realm.number_proto,
            Value::Bool(_) => &self.realm.boolean_proto,
            Value::Symbol(_) => &self.realm.symbol_proto,
            Value::BigInt(_) => &self.realm.bigint_proto,
            _ => &self.realm.object_proto,
        };
        Obj::new(ObjKind::Primitive(v), Some(proto.clone()))
    }

    pub fn to_object(&mut self, v: &Value) -> Result<Rc<Obj>, Value> {
        match v {
            Value::Object(o) => Ok(o.clone()),
            Value::Undefined | Value::Null => {
                Err(self.type_error("Cannot convert undefined or null to object"))
            }
            _ => Ok(self.wrap_primitive(v.clone())),
        }
    }

    fn tdz_error(&mut self, proto: &FunctionProto, k: u32) -> Value {
        let name = match proto.consts.get(k as usize) {
            Some(Value::Str(s)) => s.to_string(),
            _ => String::new(),
        };
        self.make_error(
            ErrorKind::ReferenceError,
            format!("Cannot access '{name}' before initialization"),
        )
    }

    pub fn new_arguments(&mut self, items: Vec<Value>) -> Value {
        self.new_arguments_kind(items, ObjKind::Ordinary)
    }

    fn new_mapped_arguments(&mut self, items: Vec<Value>, mapped: &[bool], first: usize) -> Value {
        let mut cells = Vec::with_capacity(items.len().min(mapped.len()));
        for (i, m) in mapped.iter().enumerate().take(items.len()) {
            cells.push(m.then(|| self.capture_upvalue(first + i)));
        }
        let map = crate::object::ArgMap {
            cells: RefCell::new(cells),
        };
        self.new_arguments_kind(items, ObjKind::Arguments(Box::new(map)))
    }

    fn mapped_arg(&self, m: &crate::object::ArgMap, key: &str) -> Option<Value> {
        let c = m.cell(key)?;
        Some(match &*c.borrow() {
            Upvalue::Open(idx) => self.stack[*idx].clone(),
            Upvalue::Closed(v) => v.clone(),
        })
    }

    pub(crate) fn set_mapped_arg(&mut self, m: &crate::object::ArgMap, key: &str, val: &Value) {
        if let Some(c) = m.cell(key) {
            match &mut *c.borrow_mut() {
                Upvalue::Open(idx) => self.stack[*idx] = val.clone(),
                Upvalue::Closed(v) => *v = val.clone(),
            }
        }
    }

    pub(crate) fn sync_arguments(&self, o: &Rc<Obj>) {
        if let ObjKind::Arguments(m) = &o.kind {
            let cells = m.cells.borrow();
            for (i, c) in cells.iter().enumerate() {
                if let Some(c) = c {
                    let v = match &*c.borrow() {
                        Upvalue::Open(idx) => self.stack[*idx].clone(),
                        Upvalue::Closed(v) => v.clone(),
                    };
                    let k: JsStr = i.to_string().into();
                    o.props.borrow_mut().insert(k, v);
                }
            }
        }
    }

    fn new_arguments_kind(&mut self, items: Vec<Value>, kind: ObjKind) -> Value {
        let o = Obj::new(kind, Some(self.realm.object_proto.clone()));
        let len = items.len();
        for (i, v) in items.into_iter().enumerate() {
            o.set_own(i.to_string(), v);
        }
        o.set_with_attrs("length", Value::Number(len as f64), crate::object::NON_ENUM);
        if let Some(f) = self.realm.array_proto.get_own(ITER_KEY) {
            o.set_with_attrs(ITER_KEY, f, crate::object::NON_ENUM);
        }
        let t = self.thrower.clone();
        o.set_with_attrs(
            "callee",
            crate::builtins::intrinsics::raw_accessor(t.clone(), t),
            crate::object::NON_ENUM | crate::object::NON_CONFIG,
        );
        o.flags.set(crate::object::ARGUMENTS);
        Value::Object(o)
    }

    fn get_private(&mut self, obj: &Value, key: &str) -> Result<Value, Value> {
        let Value::Object(o) = obj else {
            return Err(self.type_error(format!(
                "Cannot read private member {} from an object whose class did not declare it",
                private_label(key)
            )));
        };
        let found = o.props.borrow().get(key).cloned();
        match found {
            Some(v) => {
                if let Some((getter, _)) = crate::props::accessor_parts(&v) {
                    if !getter.is_callable() {
                        return Err(self.type_error(format!(
                            "'{}' was defined without a getter",
                            private_label(key)
                        )));
                    }
                    return self.call_value(&getter, obj.clone(), &[]);
                }
                Ok(v)
            }
            None => Err(self.type_error(format!(
                "Cannot read private member {} from an object whose class did not declare it",
                private_label(key)
            ))),
        }
    }

    fn set_private(&mut self, obj: &Value, key: &str, val: Value) -> Result<(), Value> {
        let Value::Object(o) = obj else {
            return Err(self.type_error(format!(
                "Cannot write private member {} to an object whose class did not declare it",
                private_label(key)
            )));
        };
        let found = o.props.borrow().get(key).cloned();
        let Some(v) = found else {
            return Err(self.type_error(format!(
                "Cannot write private member {} to an object whose class did not declare it",
                private_label(key)
            )));
        };
        if let Some((_, setter)) = crate::props::accessor_parts(&v) {
            if !setter.is_callable() {
                return Err(self.type_error(format!(
                    "'{}' was defined without a setter",
                    private_label(key)
                )));
            }
            self.call_value(&setter, obj.clone(), &[val])?;
            return Ok(());
        }
        if o.props.borrow().attrs(key) & crate::object::READ_ONLY != 0 {
            return Err(self.type_error(format!(
                "Private method '{}' is not writable",
                private_label(key)
            )));
        }
        o.props.borrow_mut().insert(key.into(), val);
        Ok(())
    }

    pub fn get_method(&mut self, v: &Value, key: &str) -> Result<Value, Value> {
        let m = self.get_prop(v, key)?;
        if m.is_nullish() {
            return Ok(Value::Undefined);
        }
        if !m.is_callable() {
            return Err(self.type_error(format!("{} is not a function", inspect_plain(&m))));
        }
        Ok(m)
    }

    pub fn global_this(&self) -> Value {
        self.globals.get("globalThis").cloned().unwrap_or_default()
    }

    pub fn run_script_proto(&mut self, proto: Rc<FunctionProto>) -> Result<Value, Value> {
        let closure = Rc::new(Closure {
            gc_slot: std::cell::Cell::new(u32::MAX),
            proto,
            upvalues: Box::new([]),
            home: RefCell::new(None),
            new_target: RefCell::new(None),
        });
        let f = Value::Object(Obj::new(
            ObjKind::Function(closure),
            Some(self.realm.function_proto.clone()),
        ));
        let this = self.global_this();
        self.call_value(&f, this, &[])
    }

    pub fn run_script(&mut self, src: &str, name: &str) -> Result<Value, Value> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let dir = Path::new(name)
            .parent()
            .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
            .map(|p| cwd.join(p))
            .unwrap_or(cwd);
        let proto = match self.compile_script(src, name, dir, false) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, name)),
        };
        self.run_script_proto(proto)
    }

    pub fn indirect_eval(&mut self, src: &str) -> Result<Value, Value> {
        let ast = match parser::parse_script(src) {
            Ok(a) => a,
            Err(e) => return Err(self.syntax_error_value(&e, "<eval>")),
        };
        let t = src.trim_start();
        let strict = t.starts_with("\"use strict\"") || t.starts_with("'use strict'");
        let module = Rc::new(ModuleInfo {
            name: "<eval>".into(),
            dir: std::env::current_dir().unwrap_or_default(),
            source: src.into(),
            sloppy: false,
        });
        let proto = match compile_indirect_eval(&ast, module, strict) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, "<eval>")),
        };
        self.run_script_proto(proto)
    }

    fn direct_eval(
        &mut self,
        src: &str,
        scope: &Rc<crate::bytecode::EvalScope>,
        cl: &Rc<Closure>,
        base: usize,
        nt: Value,
    ) -> Result<Value, Value> {
        let ast = match parser::parse_eval(src, &scope.ctx) {
            Ok(a) => a,
            Err(e) => return Err(self.syntax_error_value(&e, "<eval>")),
        };
        let module = Rc::new(ModuleInfo {
            name: "<eval>".into(),
            dir: cl.proto.module.dir.clone(),
            source: src.into(),
            sloppy: false,
        });
        let proto = match compile_eval(&ast, module, scope) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, "<eval>")),
        };
        let mut ups = Vec::with_capacity(proto.upvalues.len());
        for u in &proto.upvalues {
            if u.is_local {
                self.flush_builder(base + u.index as usize);
                ups.push(self.capture_upvalue(base + u.index as usize));
            } else {
                ups.push(cl.upvalues[u.index as usize].clone());
            }
        }
        let c = Rc::new(Closure {
            gc_slot: std::cell::Cell::new(u32::MAX),
            proto,
            upvalues: ups.into_boxed_slice(),
            home: RefCell::new(cl.home.borrow().clone()),
            new_target: RefCell::new(Some(nt)),
        });
        let f = Value::Object(Obj::new(
            ObjKind::Function(c),
            Some(self.realm.function_proto.clone()),
        ));
        self.call_value(&f, Value::Undefined, &[])
    }

    fn frame_new_target(&self, fi: usize, base: usize, cl: &Closure) -> Value {
        if self.frames[fi].construct {
            return match &self.frames[fi].new_target {
                Some(t) => Value::Object(t.clone()),
                None => self.stack[base - 1].clone(),
            };
        }
        if cl.proto.kind == ProtoKind::Arrow {
            return cl.new_target.borrow().clone().unwrap_or_default();
        }
        Value::Undefined
    }

    pub fn run_proto(&mut self, proto: Rc<FunctionProto>) -> Result<Value, Value> {
        let closure = Rc::new(Closure {
            gc_slot: std::cell::Cell::new(u32::MAX),
            proto,
            upvalues: Box::new([]),
            home: RefCell::new(None),
            new_target: RefCell::new(None),
        });
        let f = Value::Object(Obj::new(
            ObjKind::Function(closure),
            Some(self.realm.function_proto.clone()),
        ));
        self.call_value(&f, Value::Undefined, &[])
    }

    pub fn eval(&mut self, name: &str, src: &str, mode: Mode) -> Result<Value, Value> {
        let dir = std::env::current_dir().unwrap_or_default();
        let proto = match self.compile(src, name, dir, mode) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, name)),
        };
        let is_async = proto.is_async;
        let v = if mode == Mode::Repl {
            self.run_script_proto(proto)?
        } else {
            self.run_proto(proto)?
        };
        if is_async {
            return self.await_value(&v);
        }
        Ok(v)
    }

    pub fn run_file(&mut self, path: &Path) -> Result<Value, Value> {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return Err(self.make_error(
                    ErrorKind::Error,
                    format!("Cannot read '{}': {e}", path.display()),
                ));
            }
        };
        let name = path.display().to_string();
        let abs = module::canonical(path);
        if module::is_commonjs(&abs) {
            self.main_cjs_pending = true;
            let key = abs.display().to_string();
            let r = self.require_path(&abs, &key);
            self.main_cjs_pending = false;
            return r;
        }
        let dir = abs.parent().map(Path::to_path_buf).unwrap_or_default();
        let proto = match self.compile(&src, &name, dir, Mode::Module) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, &name)),
        };
        self.proto_cache.insert(abs.clone(), proto.clone());
        self.link_module(&abs, &proto)?;
        let ns = self.namespace_for(&abs);
        self.install_reexports(&abs, &ns)?;
        self.modules.insert(abs.clone(), ns.clone());
        self.pending_ns = Some(ns.clone());
        self.loading.push(abs.clone());
        let r = self.run_proto(proto);
        self.loading.pop();
        match r {
            Ok(v) if Vm::is_promise(&v) => Ok(v),
            Ok(_) => Ok(ns),
            Err(e) => {
                self.modules.remove(&abs);
                Err(e)
            }
        }
    }

    pub fn format_uncaught(&self, e: &Value) -> String {
        let prefix = if self.rejection {
            "Uncaught (in promise)"
        } else {
            "Uncaught"
        };
        format!("{prefix} {}", inspect_top(e))
    }

    pub fn write_out(&mut self, s: &str) {
        let _ = self.out.write_all(crate::value::utf8_text(s).as_bytes());
    }

    pub fn write_err(&mut self, s: &str) {
        let _ = self.out.flush();
        let _ = self.err.write_all(crate::value::utf8_text(s).as_bytes());
        let _ = self.err.flush();
    }

    pub fn load_lazy_global(&mut self, name: &str) -> Result<bool, Value> {
        if self.web.is_none() && builtins::web::LAZY_GLOBALS.contains(&name) {
            builtins::web::load(self)?;
            return Ok(true);
        }
        if self.node.is_none() && builtins::node::LAZY_GLOBALS.contains(&name) {
            builtins::node::load(self)?;
            return Ok(true);
        }
        if builtins::intl::LAZY_GLOBALS.contains(&name) && self.globals.get(name).is_none() {
            builtins::intl::load(self)?;
            return Ok(true);
        }
        if builtins::wasm::LAZY_GLOBALS.contains(&name) && self.globals.get(name).is_none() {
            builtins::wasm::load(self)?;
            return Ok(true);
        }
        Ok(false)
    }

    pub fn import_node(&mut self, name: &str) -> Result<Option<Value>, Value> {
        let key = format!("node:{name}");
        if let Some(v) = self.builtin_modules.get(&key) {
            return Ok(Some(v.clone()));
        }
        match builtins::node::module(self, name) {
            Some(Ok(v)) => {
                self.builtin_modules.insert(key, v.clone());
                Ok(Some(v))
            }
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn new_namespace(&mut self) -> Value {
        let ns = Obj::new(ObjKind::Namespace, None);
        ns.flags.set(ns.flags.get() | crate::object::NOT_EXTENSIBLE);
        ns.set_with_attrs(
            crate::object::TO_STRING_TAG_KEY,
            "Module".into(),
            crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
        );
        Value::Object(ns)
    }

    fn module_file_url(path: &Path) -> String {
        let s = module::display_path_abs(path).replace('\\', "/");
        let encoded: String = s
            .chars()
            .map(|c| match c {
                ' ' => "%20".to_string(),
                '%' => "%25".to_string(),
                '#' => "%23".to_string(),
                '?' => "%3F".to_string(),
                c => c.to_string(),
            })
            .collect();
        if encoded.starts_with('/') {
            format!("file://{encoded}")
        } else {
            format!("file:///{encoded}")
        }
    }

    pub fn import_meta(&mut self, module: &Rc<ModuleInfo>) -> Result<Value, Value> {
        if let Some(m) = self.import_metas.get(&module.name) {
            return Ok(m.clone());
        }
        let raw = Path::new(&*module.name);
        let file = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            match raw.file_name() {
                Some(f) => module.dir.join(f),
                None => module.dir.clone(),
            }
        };
        let meta = Obj::new(ObjKind::Ordinary, None);
        let filename = module::display_path_abs(&file);
        let dirname = module::display_path_abs(&module.dir);
        meta.set_own("dirname", dirname.into());
        meta.set_own("filename", filename.into());
        meta.set_own("url", Self::module_file_url(&file).into());
        meta.set_own("main", Value::Bool(self.loading.len() <= 1));
        let resolver = Obj::new(
            ObjKind::Bound(Box::new(crate::object::Bound {
                target: crate::builtins::native_fn(self, "resolve", 1, meta_resolve),
                this: Value::Undefined,
                args: vec![Value::Str(dirname_of(module).into())],
            })),
            Some(self.realm.function_proto.clone()),
        );
        meta.set_own("resolve", Value::Object(resolver));
        let v = Value::Object(meta);
        self.import_metas.insert(module.name.clone(), v.clone());
        Ok(v)
    }

    pub fn import_builtin(&mut self, name: &str) -> Result<Value, Value> {
        if let Some(v) = self.builtin_modules.get(name) {
            return Ok(v.clone());
        }
        let v = match builtins::builtin_module(self, name) {
            Some(Ok(v)) => v,
            Some(Err(e)) => return Err(e),
            None => {
                return Err(self.make_error(
                    ErrorKind::Error,
                    format!("Cannot find module 'squirt:{name}'"),
                ));
            }
        };
        self.builtin_modules.insert(name.to_string(), v.clone());
        Ok(v)
    }

    fn load_proto(&mut self, path: &PathBuf, spec: &str) -> Result<Rc<FunctionProto>, Value> {
        if let Some(p) = self.proto_cache.get(path) {
            return Ok(p.clone());
        }
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return Err(self.make_error(
                    ErrorKind::Error,
                    format!("Cannot read module '{spec}': {e}"),
                ));
            }
        };
        let name = module::display_path(path);
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let proto = match self.compile(&src, &name, dir, Mode::Module) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, &name)),
        };
        self.proto_cache.insert(path.clone(), proto.clone());
        Ok(proto)
    }

    fn link_target(&mut self, from: &Path, spec: &str) -> Result<Option<PathBuf>, Value> {
        if spec.starts_with("squirt:") || spec.starts_with("node:") {
            return Ok(None);
        }
        if Self::bare_specifier(spec) && Self::is_node_builtin(spec) {
            return Ok(None);
        }
        let dir = from.parent().map(Path::to_path_buf).unwrap_or_default();
        match module::resolve(spec, &dir) {
            Ok(p) => Ok(Some(p)),
            Err(msg) => Err(self.make_error(ErrorKind::Error, msg)),
        }
    }

    fn bare_specifier(spec: &str) -> bool {
        !spec.starts_with('.') && !spec.starts_with('/') && !spec.contains(':')
    }

    fn is_node_builtin(spec: &str) -> bool {
        builtins::node::BUILTIN_MODULES.contains(&spec)
            && !builtins::node::PREFIX_ONLY.contains(&spec)
    }

    fn cjs_namespace(&mut self, path: &PathBuf) -> Result<Value, Value> {
        if let Some(v) = self.modules.get(path) {
            return Ok(v.clone());
        }
        let spec = path.display().to_string();
        let exports = self.require_path(path, &spec)?;
        if !module::is_commonjs(path) {
            if let Some(v) = self.modules.get(path) {
                return Ok(v.clone());
            }
        }
        let ns = self.new_object();
        ns.set_own("default", exports.clone());
        if exports.is_object() {
            for (k, v) in self.own_entries(&exports) {
                if &*k != "default" {
                    ns.set_own(k, v);
                }
            }
        }
        let ns = Value::Object(ns);
        self.modules.insert(path.clone(), ns.clone());
        Ok(ns)
    }

    fn resolve_export(
        &mut self,
        path: &PathBuf,
        name: &str,
        seen: &mut Vec<(PathBuf, JsStr)>,
    ) -> Result<Resolution, Value> {
        let key = (path.clone(), JsStr::from(name));
        if seen.contains(&key) {
            return Ok(Resolution::NotFound);
        }
        seen.push(key);
        if module::is_commonjs(path) {
            return Ok(Resolution::Found(path.clone(), name.into()));
        }
        let spec = path.display().to_string();
        let proto = self.load_proto(path, &spec)?;
        let Some(link) = proto.link.clone() else {
            return Ok(Resolution::NotFound);
        };
        if !link.esm || link.local_exports.iter().any(|e| &**e == name) {
            return Ok(Resolution::Found(path.clone(), name.into()));
        }
        for (exported, source, imported) in &link.indirect {
            if &**exported == name {
                let Some(dep) = self.link_target(path, source)? else {
                    let bare = source
                        .strip_prefix("node:")
                        .or_else(|| source.strip_prefix("squirt:"))
                        .unwrap_or(source);
                    return Ok(Resolution::Builtin(JsStr::from(bare), imported.clone()));
                };
                if &**imported == "*" {
                    return Ok(Resolution::Namespace(dep));
                }
                return self.resolve_export(&dep, imported, seen);
            }
        }
        if name == "default" {
            return Ok(Resolution::NotFound);
        }
        let mut star: Option<Resolution> = None;
        for source in &link.stars {
            let Some(dep) = self.link_target(path, source)? else {
                continue;
            };
            match self.resolve_export(&dep, name, seen)? {
                Resolution::Ambiguous => return Ok(Resolution::Ambiguous),
                Resolution::NotFound => {}
                found => match &star {
                    None => star = Some(found),
                    Some(prev) => {
                        if *prev != found {
                            return Ok(Resolution::Ambiguous);
                        }
                    }
                },
            }
        }
        Ok(star.unwrap_or(Resolution::NotFound))
    }

    fn link_error(&mut self, source: &str, name: &str, r: &Resolution) -> Option<Value> {
        match r {
            Resolution::NotFound => Some(self.make_error(
                ErrorKind::SyntaxError,
                format!("The requested module '{source}' does not provide an export named '{name}'"),
            )),
            Resolution::Ambiguous => Some(self.make_error(
                ErrorKind::SyntaxError,
                format!("The requested module '{source}' contains conflicting star exports for name '{name}'"),
            )),
            _ => None,
        }
    }

    fn namespace_for(&mut self, path: &Path) -> Value {
        if let Some(ns) = self.ns_cache.get(path) {
            return ns.clone();
        }
        let ns = self.new_namespace();
        self.ns_cache.insert(path.to_path_buf(), ns.clone());
        ns
    }

    fn exported_names(
        &mut self,
        path: &PathBuf,
        visited: &mut Vec<PathBuf>,
    ) -> Result<Vec<JsStr>, Value> {
        if visited.contains(path) {
            return Ok(Vec::new());
        }
        visited.push(path.clone());
        if module::is_commonjs(path) {
            let ns = self.cjs_namespace(path)?;
            return Ok(self
                .own_entries(&ns)
                .into_iter()
                .filter(|(k, _)| &**k != "default")
                .map(|(k, _)| JsStr::from(&*k))
                .collect());
        }
        let spec = path.display().to_string();
        let proto = self.load_proto(path, &spec)?;
        let Some(link) = proto.link.clone() else {
            return Ok(Vec::new());
        };
        let mut names: Vec<JsStr> = Vec::new();
        for n in &link.local_exports {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
        for (exported, _, _) in &link.indirect {
            if !names.contains(exported) {
                names.push(exported.clone());
            }
        }
        for source in &link.stars {
            let Some(dep) = self.link_target(path, source)? else {
                continue;
            };
            for n in self.exported_names(&dep, visited)? {
                if &*n != "default" && !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        Ok(names)
    }

    fn install_reexports(&mut self, path: &PathBuf, ns: &Value) -> Result<(), Value> {
        let spec = path.display().to_string();
        let proto = self.load_proto(path, &spec)?;
        let Some(link) = proto.link.clone() else {
            return Ok(());
        };
        if !link.esm {
            return Ok(());
        }
        let mut visited = Vec::new();
        let names = self.exported_names(path, &mut visited)?;
        let Value::Object(nso) = ns else {
            return Ok(());
        };
        for name in names {
            if link.local_exports.contains(&name) {
                continue;
            }
            let mut seen = Vec::new();
            match self.resolve_export(path, &name, &mut seen)? {
                Resolution::Found(target, local) => {
                    if target == *path && local == name {
                        continue;
                    }
                    let target_ns = if module::is_commonjs(&target) {
                        self.cjs_namespace(&target)?
                    } else {
                        self.namespace_for(&target)
                    };
                    let getter = self.ns_getter(target_ns, &local);
                    nso.set_with_attrs(
                        name.clone(),
                        crate::builtins::intrinsics::raw_accessor(getter, Value::Undefined),
                        0,
                    );
                }
                Resolution::Namespace(dep) => {
                    let dep_ns = if module::is_commonjs(&dep) {
                        self.cjs_namespace(&dep)?
                    } else {
                        self.namespace_for(&dep)
                    };
                    nso.set_own(name.clone(), dep_ns);
                }
                Resolution::Builtin(source, imported) => {
                    let ns = match self.import_node(&source)? {
                        Some(v) => v,
                        None => self.import_builtin(&source)?,
                    };
                    if &*imported == "*" {
                        nso.set_own(name.clone(), ns);
                    } else {
                        let getter = self.ns_getter(ns, &imported);
                        nso.set_with_attrs(
                            name.clone(),
                            crate::builtins::intrinsics::raw_accessor(getter, Value::Undefined),
                            0,
                        );
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn ns_getter(&mut self, target_ns: Value, local: &str) -> Value {
        let inner = match &self.ns_get_fn {
            Some(f) => f.clone(),
            None => {
                let f = builtins::native_fn(self, "", 0, ns_get);
                self.ns_get_fn = Some(f.clone());
                f
            }
        };
        Value::Object(Obj::new(
            ObjKind::Bound(Box::new(crate::object::Bound {
                target: inner,
                this: target_ns,
                args: vec![Value::Str(local.into())],
            })),
            Some(self.realm.function_proto.clone()),
        ))
    }

    fn link_module(&mut self, path: &Path, proto: &Rc<FunctionProto>) -> Result<(), Value> {
        let Some(link) = proto.link.clone() else {
            return Ok(());
        };
        for source in &link.stars {
            if let Some(dep) = self.link_target(path, source)? {
                if !module::is_commonjs(&dep) {
                    self.load_proto(&dep, source)?;
                }
            }
        }
        for (source, names) in &link.imports {
            let Some(dep) = self.link_target(path, source)? else {
                continue;
            };
            if module::is_commonjs(&dep) {
                continue;
            }
            let dep_proto = self.load_proto(&dep, source)?;
            if !dep_proto.link.as_ref().is_some_and(|l| l.esm) {
                continue;
            }
            for name in names {
                let mut seen = Vec::new();
                let r = self.resolve_export(&dep, name, &mut seen)?;
                if let Some(e) = self.link_error(source, name, &r) {
                    return Err(e);
                }
            }
        }
        for (_, source, imported) in &link.indirect {
            if &**imported == "*" {
                continue;
            }
            let Some(dep) = self.link_target(path, source)? else {
                continue;
            };
            if module::is_commonjs(&dep) {
                continue;
            }
            let dep_proto = self.load_proto(&dep, source)?;
            if !dep_proto.link.as_ref().is_some_and(|l| l.esm) {
                continue;
            }
            let mut seen = Vec::new();
            let r = self.resolve_export(&dep, imported, &mut seen)?;
            if let Some(e) = self.link_error(source, imported, &r) {
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn import_module(&mut self, spec: &str, from: &Rc<ModuleInfo>) -> Result<Value, Value> {
        if let Some(name) = spec.strip_prefix("squirt:") {
            return self.import_builtin(name);
        }
        if let Some(name) = spec.strip_prefix("node:") {
            return match self.import_node(name)? {
                Some(v) => Ok(v),
                None => {
                    Err(self
                        .make_error(ErrorKind::Error, format!("No such built-in module: {spec}")))
                }
            };
        }
        if Self::bare_specifier(spec) && !builtins::node::PREFIX_ONLY.contains(&spec) {
            if let Some(v) = self.import_node(spec)? {
                return Ok(v);
            }
        }
        let path = match module::resolve(spec, &from.dir) {
            Ok(p) => p,
            Err(msg) => return Err(self.make_error(ErrorKind::Error, msg)),
        };
        if let Some(v) = self.modules.get(&path) {
            return Ok(v.clone());
        }
        if module::is_commonjs(&path) {
            return self.cjs_namespace(&path);
        }
        let proto = self.load_proto(&path, spec)?;
        self.link_module(&path, &proto)?;
        let is_async = proto.is_async;
        let ns = self.namespace_for(&path);
        self.install_reexports(&path, &ns)?;
        self.modules.insert(path.clone(), ns.clone());
        self.pending_ns = Some(ns.clone());
        self.loading.push(path.clone());
        let r = self.run_proto(proto);
        let r = match r {
            Ok(v) if is_async => self.await_value(&v),
            other => other,
        };
        self.loading.pop();
        if let Err(e) = r {
            self.modules.remove(&path);
            return Err(e);
        }
        Ok(ns)
    }

    fn require_esm_result(&mut self, ns: &Value) -> Result<Value, Value> {
        let Value::Object(nso) = ns else {
            return Ok(ns.clone());
        };
        if nso.get_own("module.exports").is_some() {
            return self.get_prop(ns, "module.exports");
        }
        let out = self.new_object();
        let mut has_default = false;
        for key in self.op_own_keys(nso)? {
            if key.starts_with('\u{0}') {
                continue;
            }
            let v = self.get_prop(ns, &key)?;
            if &*key == "default" {
                has_default = true;
            }
            out.set_own(key, v);
        }
        if has_default {
            out.set_hidden("__esModule", Value::Bool(true));
        }
        Ok(Value::Object(out))
    }

    pub fn require_cache_object_public(&mut self) -> Rc<Obj> {
        self.require_cache_object()
    }

    fn require_cache_object(&mut self) -> Rc<Obj> {
        if let Some(c) = &self.require_cache {
            return c.clone();
        }
        let c = self.new_object();
        self.require_cache = Some(c.clone());
        c
    }

    pub fn require_module(&mut self, spec: &str, from: &Rc<ModuleInfo>) -> Result<Value, Value> {
        let unwrap = |v: Value| match &v {
            Value::Object(o) => o.get_own("default").unwrap_or(v),
            _ => v,
        };
        if let Some(name) = spec.strip_prefix("squirt:") {
            return self.import_builtin(name).map(unwrap);
        }
        if let Some(name) = spec.strip_prefix("node:") {
            return match self.import_node(name)? {
                Some(v) => Ok(unwrap(v)),
                None => {
                    Err(self
                        .make_error(ErrorKind::Error, format!("No such built-in module: {spec}")))
                }
            };
        }
        if Self::bare_specifier(spec) && !builtins::node::PREFIX_ONLY.contains(&spec) {
            if let Some(v) = self.import_node(spec)? {
                return Ok(unwrap(v));
            }
        }
        let path = match module::resolve_with(spec, &from.dir, true) {
            Ok(p) => p,
            Err(msg) => return Err(self.make_error(ErrorKind::Error, msg)),
        };
        self.require_path(&path, spec)
    }

    fn require_path(&mut self, path: &PathBuf, spec: &str) -> Result<Value, Value> {
        let key: JsStr = path.display().to_string().into();
        let cache = self.require_cache_object();
        if let Some(m @ Value::Object(_)) = cache.get_own(&key) {
            return self.get_prop(&m, "exports");
        }
        if let Some(ns) = self.modules.get(path) {
            return Ok(ns.clone());
        }
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return Err(self.make_error(
                    ErrorKind::Error,
                    format!("Cannot read module '{spec}': {e}"),
                ));
            }
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "json" {
            let value = builtins::json::json_parse(self, &src)?;
            let module = self.new_object();
            module.set_own("exports", value.clone());
            module.set_own("id", Value::Str(key.clone()));
            module.set_own("filename", Value::Str(key.clone()));
            module.set_own("loaded", Value::Bool(true));
            cache.set_own(key, Value::Object(module));
            return Ok(value);
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        if !module::is_commonjs(path) {
            let from = Rc::new(ModuleInfo {
                name: key.clone(),
                dir: dir.clone(),
                source: "".into(),
                sloppy: false,
            });
            let ns = self.import_module(&path.display().to_string(), &from)?;
            return self.require_esm_result(&ns);
        }
        let body = if src.starts_with("#!") {
            match src.find('\n') {
                Some(i) => &src[i..],
                None => "",
            }
        } else {
            &src
        };
        let wrapped =
            format!("(function (exports, require, module, __filename, __dirname) {{{body}\n}})");
        let proto = match self.compile_unit(&wrapped, &key, dir.clone(), Mode::Repl, true) {
            Ok(p) => p,
            Err(e) => {
                let detect = module::esm_only_error(&e.message)
                    || (module::redeclares_wrapper(&e.message) && !module::explicit_commonjs(&dir));
                if ext == "js" && detect {
                    module::mark_esm(path);
                    let from = Rc::new(ModuleInfo {
                        name: key.clone(),
                        dir: dir.clone(),
                        source: "".into(),
                        sloppy: false,
                    });
                    let ns = self.import_module(&path.display().to_string(), &from)?;
                    return self.require_esm_result(&ns);
                }
                return Err(self.syntax_error_value(&e, &key));
            }
        };
        let wrapper = self.run_script_proto(proto)?;
        let module = self.new_object();
        let exports = self.new_object();
        module.set_own("exports", Value::Object(exports.clone()));
        module.set_own("id", Value::Str(key.clone()));
        module.set_own("filename", Value::Str(key.clone()));
        module.set_own("path", Value::Str(dir.display().to_string().into()));
        module.set_own("loaded", Value::Bool(false));
        let children = self.new_array(Vec::new());
        module.set_own("children", children);
        let paths = self.new_array(Vec::new());
        module.set_own("paths", paths);
        cache.set_own(key.clone(), Value::Object(module.clone()));
        let require = self.globals.get("require").cloned().unwrap_or_default();
        if std::mem::take(&mut self.main_cjs_pending) {
            module.set_own("id", ".".into());
            if let Value::Object(r) = &require {
                r.set_own("main", Value::Object(module.clone()));
            }
            if let Some(Value::Object(p)) = self.globals.get("process").cloned() {
                p.set_hidden("mainModule", Value::Object(module.clone()));
            }
        }
        let r = self.call_value(
            &wrapper,
            Value::Object(exports.clone()),
            &[
                Value::Object(exports),
                require,
                Value::Object(module.clone()),
                Value::Str(key.clone()),
                Value::Str(dir.display().to_string().into()),
            ],
        );
        if let Err(e) = r {
            cache.props.borrow_mut().shift_remove(&key);
            return Err(e);
        }
        module.set_own("loaded", Value::Bool(true));
        self.get_prop(&Value::Object(module), "exports")
    }

    pub fn stack_trace(&self) -> String {
        let mut s = String::new();
        for f in self
            .frames
            .iter()
            .rev()
            .filter(|f| {
                let m = &f.closure.proto.module.name;
                !m.starts_with("<squirt:") && &**m != "<prelude>"
            })
            .take(12)
        {
            let p = &f.closure.proto;
            let name = if p.name.is_empty() {
                "<anonymous>"
            } else {
                &p.name
            };
            s.push_str(&format!(
                "\n    at {} ({}:{})",
                name,
                p.module.name,
                p.line_at(f.ip)
            ));
        }
        s
    }

    pub fn make_error(&mut self, kind: ErrorKind, msg: impl Into<String>) -> Value {
        let proto = self.realm.error_protos[kind as usize].clone();
        let o = Obj::new(ObjKind::Error, Some(proto));
        let msg: String = msg.into();
        let stack = format!("{}: {}{}", kind.name(), msg, self.stack_trace());
        o.set_hidden("stack", stack.into());
        o.set_hidden("message", msg.into());
        Value::Object(o)
    }

    pub fn type_error(&mut self, msg: impl Into<String>) -> Value {
        self.make_error(ErrorKind::TypeError, msg)
    }

    pub fn assign_error(&mut self, msg: impl Into<String>) -> Value {
        self.silent_fail = true;
        self.make_error(ErrorKind::TypeError, msg)
    }

    pub fn range_error(&mut self, msg: impl Into<String>) -> Value {
        self.make_error(ErrorKind::RangeError, msg)
    }

    pub fn new_object(&self) -> Rc<Obj> {
        Obj::new(ObjKind::Ordinary, Some(self.realm.object_proto.clone()))
    }

    pub fn new_array(&self, items: Vec<Value>) -> Value {
        Value::Object(Obj::new(
            ObjKind::Array(RefCell::new(items)),
            Some(self.realm.array_proto.clone()),
        ))
    }

    pub fn new_iter(&self, state: IterState) -> Value {
        let proto = self.iter_family_proto(state.family()).clone();
        Value::Object(Obj::new(
            ObjKind::Iter(Box::new(RefCell::new(state))),
            Some(proto),
        ))
    }

    pub fn iter_family_proto(&self, family: usize) -> &Rc<Obj> {
        match family {
            1 => &self.realm.map_iter_proto,
            2 => &self.realm.set_iter_proto,
            3 => &self.realm.string_iter_proto,
            _ => &self.realm.iter_proto,
        }
    }

    fn family_next_pristine(&self, family: usize) -> bool {
        let Some(p) = &self.pristine else {
            return true;
        };
        self.iter_family_proto(family)
            .props
            .borrow()
            .get("next")
            .is_some_and(|v| v.strict_eq(&p.iter_nexts[family]))
    }

    pub(crate) fn capture_upvalue(&mut self, idx: usize) -> Rc<UpCell> {
        if let Some(u) = self.open_upvalues.get(&idx) {
            return u.clone();
        }
        let u = UpCell::new(Upvalue::Open(idx));
        self.open_upvalues.insert(idx, u.clone());
        u
    }

    #[inline(always)]
    fn flush_builder(&mut self, abs: usize) {
        if !self.builders.is_empty() {
            self.flush_builder_slow(abs);
        }
    }

    #[cold]
    #[inline(never)]
    fn flush_builder_slow(&mut self, abs: usize) {
        if let Some(i) = self.builders.iter().position(|(a, _)| *a == abs) {
            let (_, buf) = self.builders.swap_remove(i);
            self.stack[abs] = Value::Str(buf.into());
        }
    }

    #[inline(always)]
    fn drop_builder(&mut self, abs: usize) {
        if !self.builders.is_empty() {
            self.drop_builder_slow(abs);
        }
    }

    #[cold]
    #[inline(never)]
    fn drop_builder_slow(&mut self, abs: usize) {
        self.builders.retain(|(a, _)| *a != abs);
    }

    #[inline(always)]
    pub(crate) fn prune_builders(&mut self, len: usize) {
        if !self.builders.is_empty() {
            self.prune_builders_slow(len);
        }
    }

    #[cold]
    #[inline(never)]
    fn prune_builders_slow(&mut self, len: usize) {
        self.builders.retain(|(a, _)| *a < len);
    }

    pub(crate) fn flush_builders_from(&mut self, from: usize) {
        let mut i = 0;
        while i < self.builders.len() {
            if self.builders[i].0 >= from {
                let (abs, buf) = self.builders.swap_remove(i);
                self.stack[abs] = Value::Str(buf.into());
            } else {
                i += 1;
            }
        }
    }

    fn append_local(&mut self, abs: usize, rhs: Value) -> Result<(), Value> {
        if let Some(i) = self.builders.iter().position(|(a, _)| *a == abs) {
            let piece = match &rhs {
                Value::Str(s) => s.clone(),
                other => {
                    let p = self.default_primitive(other)?;
                    self.to_string(&p)?
                }
            };
            crate::value::push16(&mut self.builders[i].1, &piece);
            return Ok(());
        }
        let current = self.stack[abs].clone();
        if matches!(current, Value::BigInt(_)) || matches!(rhs, Value::BigInt(_)) {
            let r = self.add_slow(&current, &rhs)?;
            self.stack[abs] = r;
            return Ok(());
        }
        if let Value::Str(cur) = &current {
            let piece = match &rhs {
                Value::Str(s) => s.clone(),
                other => {
                    let p = self.default_primitive(other)?;
                    self.to_string(&p)?
                }
            };
            let total = cur.len() + piece.len();
            let captured = self.open_upvalues.contains_key(&abs);
            if total >= 256 && !captured {
                let mut buf = String::with_capacity(total * 2);
                buf.push_str(cur);
                crate::value::push16(&mut buf, &piece);
                self.stack[abs] = Value::Undefined;
                self.builders.push((abs, buf));
            } else {
                let mut s = String::with_capacity(total);
                s.push_str(cur);
                crate::value::push16(&mut s, &piece);
                self.stack[abs] = Value::Str(s.into());
            }
            return Ok(());
        }
        let r = match (&current, &rhs) {
            (Value::Number(x), Value::Number(y)) => Value::Number(x + y),
            _ => self.add_slow(&current, &rhs)?,
        };
        self.stack[abs] = r;
        Ok(())
    }

    fn close_upvalues(&mut self, from: usize) {
        if self
            .open_upvalues
            .last_key_value()
            .is_none_or(|(k, _)| *k < from)
        {
            return;
        }
        let tail = self.open_upvalues.split_off(&from);
        for (idx, u) in tail {
            let v = self.stack[idx].clone();
            *u.borrow_mut() = Upvalue::Closed(v);
        }
    }

    pub(crate) fn push_frame(
        &mut self,
        c: Rc<Closure>,
        argc: usize,
        construct: bool,
    ) -> Result<(), Value> {
        if self.frames.len() >= MAX_FRAMES {
            return Err(self.range_error("Maximum call stack size exceeded"));
        }
        let p = &c.proto;
        let np = p.num_params as usize;
        let ci = self.stack.len() - argc - 2;
        if p.sloppy && p.kind != ProtoKind::Arrow && !construct {
            match &self.stack[ci + 1] {
                Value::Undefined | Value::Null => self.stack[ci + 1] = self.global_this(),
                Value::Object(_) => {}
                v => {
                    let w = self.wrap_primitive(v.clone());
                    self.stack[ci + 1] = Value::Object(w);
                }
            }
        }
        let all_args = p
            .uses_arguments
            .then(|| self.stack[ci + 2..ci + 2 + argc].to_vec());
        if argc < np {
            for _ in argc..np {
                self.stack.push(Value::Undefined);
            }
        }
        if p.has_rest {
            let rest = if argc > np {
                self.stack.split_off(ci + 2 + np)
            } else {
                Vec::new()
            };
            let arr = self.new_array(rest);
            self.stack.push(arr);
        } else if argc > np {
            self.stack.truncate(ci + 2 + np);
        }
        if let Some(all) = all_args {
            let arr = if p.mapped.is_empty() {
                self.new_arguments(all)
            } else {
                let mapped = p.mapped.clone();
                self.new_mapped_arguments(all, &mapped, ci + 2)
            };
            if p.sloppy {
                if let Value::Object(a) = &arr {
                    a.set_with_attrs("callee", self.stack[ci].clone(), crate::object::NON_ENUM);
                }
            }
            self.stack.push(arr);
        }
        self.frames.push(Frame {
            closure: c,
            ip: 0,
            base: ci + 1,
            construct,
            co: None,
            new_target: if construct {
                self.next_new_target.take()
            } else {
                None
            },
        });
        Ok(())
    }

    pub(crate) fn unwind(&mut self, err: Value, stop: usize) -> Result<(), Value> {
        if let Some(h) = self.handlers.last() {
            if h.frame >= stop {
                let h = self.handlers.pop().expect("handler");
                self.frames.truncate(h.frame + 1);
                self.close_upvalues(h.stack_len);
                self.prune_builders(h.stack_len);
                self.stack.truncate(h.stack_len);
                self.stack.push(err);
                self.frames[h.frame].ip = h.catch_ip;
                return Ok(());
            }
        }
        let base = self.frames[stop].base;
        self.close_upvalues(base);
        self.prune_builders(base - 1);
        self.stack.truncate(base - 1);
        self.frames.truncate(stop);
        Err(err)
    }

    pub fn call_value(&mut self, f: &Value, this: Value, args: &[Value]) -> Result<Value, Value> {
        let Value::Object(o) = f else {
            return Err(self.type_error(format!("{} is not a function", inspect_plain(f))));
        };
        if let ObjKind::Proxy(p) = &o.kind {
            if p.callable {
                return self.proxy_call(o, this, args);
            }
        }
        match &o.kind {
            ObjKind::Function(c) => {
                if c.proto.is_class() {
                    return Err(self.type_error(format!(
                        "Class constructor {} cannot be invoked without 'new'",
                        c.proto.name
                    )));
                }
                if self.native_depth >= MAX_NATIVE_DEPTH {
                    return Err(self.range_error("Maximum call stack size exceeded"));
                }
                if c.proto.is_coroutine() {
                    return self.start_coroutine(f, c.clone(), this, args);
                }
                let mark = self.stack.len();
                self.stack.push(f.clone());
                self.stack.push(this);
                self.stack.extend_from_slice(args);
                if let Err(e) = self.push_frame(c.clone(), args.len(), false) {
                    self.stack.truncate(mark);
                    return Err(e);
                }
                self.native_depth += 1;
                let stop = self.frames.len() - 1;
                let r = self.run(stop);
                self.native_depth -= 1;
                r
            }
            ObjKind::Native(n) => {
                if self.native_depth >= MAX_NATIVE_DEPTH {
                    return Err(self.range_error("Maximum call stack size exceeded"));
                }
                self.native_depth += 1;
                let r = (n.func)(self, this, args);
                self.native_depth -= 1;
                r
            }
            ObjKind::Bound(b) => {
                let mut all = b.args.clone();
                all.extend_from_slice(args);
                let (t, th) = (b.target.clone(), b.this.clone());
                self.call_value(&t, th, &all)
            }
            _ => Err(self.type_error(format!("{} is not a function", inspect_plain(f)))),
        }
    }

    fn root_ctor_kind(&self, f: &Rc<Obj>) -> Option<fn() -> ObjKind> {
        let mut cur = f.proto.borrow().clone();
        while let Some(o) = cur {
            if let ObjKind::Native(n) = &o.kind {
                return n.ctor;
            }
            cur = o.proto.borrow().clone();
        }
        None
    }

    fn alloc_this(&mut self, f: &Rc<Obj>) -> Value {
        let proto = match self.get_obj(f, "prototype") {
            Value::Object(p) => p,
            _ => self.realm.object_proto.clone(),
        };
        let kind = self
            .root_ctor_kind(f)
            .map(|k| k())
            .unwrap_or(ObjKind::Ordinary);
        Value::Object(Obj::new(kind, Some(proto)))
    }

    fn not_constructor(&mut self, callee: &Value, name: u32, consts: Option<&[Value]>) -> Value {
        let desc = match (consts, name) {
            (Some(c), n) if n != NO_NAME => inspect_top(&c[n as usize]),
            _ => inspect_plain(callee),
        };
        self.type_error(format!("{desc} is not a constructor"))
    }

    pub fn is_constructor(&self, f: &Value) -> bool {
        let Value::Object(o) = f else {
            return false;
        };
        match &o.kind {
            ObjKind::Function(c) => c.proto.is_constructor(),
            ObjKind::Native(n) => n.ctor.is_some() || o.get_own("prototype").is_some(),
            ObjKind::Bound(b) => self.is_constructor(&b.target),
            ObjKind::Proxy(p) => p.constructible,
            _ => false,
        }
    }

    fn check_function_realm(&mut self, f: &Value) -> Result<(), Value> {
        let mut cur = f.clone();
        loop {
            let next = match &cur {
                Value::Object(o) => match &o.kind {
                    ObjKind::Proxy(p) => match p.target.borrow().clone() {
                        Some(t) => Value::Object(t),
                        None => {
                            return Err(self.type_error(
                                "Cannot perform 'construct' on a proxy that has been revoked",
                            ));
                        }
                    },
                    ObjKind::Bound(b) => b.target.clone(),
                    _ => return Ok(()),
                },
                _ => return Ok(()),
            };
            cur = next;
        }
    }

    pub fn construct_with_target(
        &mut self,
        callee: &Value,
        args: &[Value],
        new_target: &Value,
    ) -> Result<Value, Value> {
        if let Value::Object(o) = callee {
            if let ObjKind::Proxy(_) = &o.kind {
                return self.proxy_construct(o, args, new_target);
            }
        }
        if !new_target.strict_eq(callee) {
            if let Value::Object(o) = callee {
                if let ObjKind::Native(n) = &o.kind {
                    if let Some(k) = n.ctor {
                        let f = n.func;
                        let proto = match self.get_prop(new_target, "prototype")? {
                            Value::Object(p) => p,
                            _ => {
                                self.check_function_realm(new_target)?;
                                match o.get_own("prototype") {
                                    Some(Value::Object(p)) => p,
                                    _ => self.realm.object_proto.clone(),
                                }
                            }
                        };
                        let this = Obj::new(k(), Some(proto.clone()));
                        let r = f(self, Value::Object(this.clone()), args)?;
                        if let Value::Object(ro) = &r {
                            let reused = Rc::ptr_eq(ro, &this)
                                || args
                                    .iter()
                                    .any(|a| matches!(a, Value::Object(x) if Rc::ptr_eq(x, ro)));
                            if !reused {
                                *ro.proto.borrow_mut() = Some(proto);
                            }
                            return Ok(r);
                        }
                        return Ok(Value::Object(this));
                    }
                }
            }
        }
        if let (Value::Object(o), Value::Object(nt)) = (callee, new_target) {
            if let ObjKind::Function(c) = &o.kind {
                if c.proto.is_constructor() && !Rc::ptr_eq(o, nt) {
                    let proto = match self.get_prop(new_target, "prototype")? {
                        Value::Object(p) => p,
                        _ => {
                            self.check_function_realm(new_target)?;
                            if &*c.proto.module.name == "<prelude>" {
                                self.function_prototype(o)
                                    .unwrap_or_else(|| self.realm.object_proto.clone())
                            } else {
                                self.realm.object_proto.clone()
                            }
                        }
                    };
                    let this = if c.proto.kind == ProtoKind::DerivedConstructor {
                        Value::Uninit
                    } else {
                        let kind = self
                            .root_ctor_kind(o)
                            .map(|k| k())
                            .unwrap_or(ObjKind::Ordinary);
                        Value::Object(Obj::new(kind, Some(proto)))
                    };
                    let c = c.clone();
                    let mark = self.stack.len();
                    self.stack.push(callee.clone());
                    self.stack.push(this);
                    self.stack.extend_from_slice(args);
                    self.next_new_target = Some(nt.clone());
                    if let Err(e) = self.push_frame(c, args.len(), true) {
                        self.next_new_target = None;
                        self.stack.truncate(mark);
                        return Err(e);
                    }
                    self.native_depth += 1;
                    let stop = self.frames.len() - 1;
                    let r = self.run(stop);
                    self.native_depth -= 1;
                    return r;
                }
            }
        }
        let r = self.construct(callee, args)?;
        if !new_target.strict_eq(callee) {
            if let (Value::Object(obj), Value::Object(nt)) = (&r, new_target) {
                if let Value::Object(p) = self.get_prop(&Value::Object(nt.clone()), "prototype")? {
                    *obj.proto.borrow_mut() = Some(p);
                }
            }
        }
        Ok(r)
    }

    pub fn set_global_prop(&mut self, key: JsStr, val: Value) -> Result<(), Value> {
        if let Some(raw) = self.globals.get(&key).cloned() {
            if let Some((_, setter)) = crate::props::accessor_parts(&raw) {
                if setter.is_callable() {
                    self.call_value(&setter, Value::Undefined, &[val])?;
                    return Ok(());
                }
                return Err(self.assign_error(format!(
                    "Cannot set property {key} of #<Object> which has only a getter"
                )));
            }
            if self.globals.attrs_of(&key) & crate::object::READ_ONLY != 0 {
                return Err(self.assign_error(format!(
                    "Cannot assign to read only property '{key}' of object '#<Object>'"
                )));
            }
        }
        self.globals.insert(key, val);
        Ok(())
    }

    pub fn global_descriptor(&self, key: &str) -> Option<crate::props::PropDesc> {
        let raw = self.globals.get(key)?.clone();
        let attrs = self.globals.attrs_of(key);
        Some(match crate::props::accessor_parts(&raw) {
            Some((g, s)) => crate::props::PropDesc {
                get: Some(g),
                set: Some(s),
                enumerable: Some(attrs & crate::object::NON_ENUM == 0),
                configurable: Some(attrs & crate::object::NON_CONFIG == 0),
                ..Default::default()
            },
            None => crate::props::PropDesc::data(raw, attrs),
        })
    }

    pub fn define_global_prop(
        &mut self,
        key: JsStr,
        d: crate::props::PropDesc,
        throw: bool,
    ) -> Result<bool, Value> {
        use crate::object::{NON_CONFIG, NON_ENUM, READ_ONLY};
        let scratch = self.new_object();
        if let Some(cur) = self.global_descriptor(&key) {
            let attrs = self.globals.attrs_of(&key);
            let raw = self.globals.get(&key).cloned().unwrap_or_default();
            scratch.set_with_attrs(key.clone(), raw, attrs);
            let _ = cur;
        }
        let ok = self.define_own(&scratch, key.clone(), d, throw)?;
        if !ok {
            return Ok(false);
        }
        let raw = scratch.get_own(&key).unwrap_or_default();
        let attrs = scratch.props.borrow().attrs(&key) & (NON_ENUM | READ_ONLY | NON_CONFIG);
        if crate::props::accessor_parts(&raw).is_some() {
            self.global_accessors = true;
        }
        if attrs & READ_ONLY != 0 {
            self.globals.readonly = true;
        }
        self.globals.insert(key.clone(), raw);
        if attrs == 0 {
            self.globals.attrs.remove(&key);
        } else {
            self.globals.attrs.insert(key, attrs);
        }
        Ok(true)
    }

    pub fn delete_global(&mut self, key: &str) -> bool {
        if self.globals.attrs_of(key) & crate::object::NON_CONFIG != 0 {
            return false;
        }
        self.globals.remove(key);
        true
    }

    pub fn construct(&mut self, callee: &Value, args: &[Value]) -> Result<Value, Value> {
        let Value::Object(o) = callee else {
            return Err(self.not_constructor(callee, NO_NAME, None));
        };
        if let ObjKind::Proxy(_) = &o.kind {
            return self.proxy_construct(o, args, callee);
        }
        match &o.kind {
            ObjKind::Function(c) => {
                if !c.proto.is_constructor() {
                    return Err(self.not_constructor(callee, NO_NAME, None));
                }
                let this = if c.proto.kind == ProtoKind::DerivedConstructor {
                    Value::Uninit
                } else {
                    self.alloc_this(o)
                };
                let mark = self.stack.len();
                self.stack.push(callee.clone());
                self.stack.push(this);
                self.stack.extend_from_slice(args);
                if let Err(e) = self.push_frame(c.clone(), args.len(), true) {
                    self.stack.truncate(mark);
                    return Err(e);
                }
                self.native_depth += 1;
                let stop = self.frames.len() - 1;
                let r = self.run(stop);
                self.native_depth -= 1;
                r
            }
            ObjKind::Native(n) => {
                let this = match n.ctor {
                    Some(k) => {
                        let proto = match o.get_own("prototype") {
                            Some(Value::Object(p)) => p,
                            _ => self.realm.object_proto.clone(),
                        };
                        Value::Object(Obj::new(k(), Some(proto)))
                    }
                    None => Value::Undefined,
                };
                let r = (n.func)(self, this.clone(), args)?;
                Ok(if r.is_object() || this.is_undefined() {
                    r
                } else {
                    this
                })
            }
            ObjKind::Bound(b) => {
                let mut all = b.args.clone();
                all.extend_from_slice(args);
                let t = b.target.clone();
                self.construct(&t, &all)
            }
            _ => Err(self.not_constructor(callee, NO_NAME, None)),
        }
    }

    pub fn function_prototype(&self, f: &Rc<Obj>) -> Option<Rc<Obj>> {
        match self.get_obj(f, "prototype") {
            Value::Object(p) => Some(p),
            _ => None,
        }
    }

    pub fn get_obj(&self, o: &Rc<Obj>, key: &str) -> Value {
        match &o.kind {
            ObjKind::Array(a) => {
                if key == "length" {
                    return Value::Number(a.borrow().len() as f64);
                }
                if let Some(i) = array_index(key) {
                    if let Some(v) = a.borrow().get(i) {
                        if !o.is_hole(i) {
                            return v.clone();
                        }
                    }
                    if !self.proto_indices {
                        return Value::Undefined;
                    }
                    let proto = o.proto.borrow().clone();
                    return match proto {
                        Some(p) => self.get_inherited(&p, key),
                        None => Value::Undefined,
                    };
                }
            }
            ObjKind::Function(c) => {
                if let Some(v) = o.get_own(key) {
                    return v;
                }
                {
                    match key {
                        "prototype" if c.proto.is_generator => {
                            let base = if c.proto.is_async {
                                self.realm.async_generator_proto.clone()
                            } else {
                                self.realm.generator_proto.clone()
                            };
                            let p = Obj::new(ObjKind::Ordinary, Some(base));
                            o.set_with_attrs(
                                "prototype",
                                Value::Object(p.clone()),
                                crate::object::NON_ENUM | crate::object::NON_CONFIG,
                            );
                            return Value::Object(p);
                        }
                        "prototype" if c.proto.is_constructor() => {
                            let p = self.new_object();
                            p.set_hidden("constructor", Value::Object(o.clone()));
                            o.set_with_attrs(
                                "prototype",
                                Value::Object(p.clone()),
                                crate::object::NON_ENUM | crate::object::NON_CONFIG,
                            );
                            return Value::Object(p);
                        }
                        "name" if o.flags.get() & crate::object::NAME_DELETED == 0 => {
                            return Value::Str(c.proto.name.clone());
                        }
                        "length" if o.flags.get() & crate::object::LENGTH_DELETED == 0 => {
                            return Value::Number(c.proto.arity as f64);
                        }
                        _ => {}
                    }
                }
            }
            ObjKind::Native(n) => {
                if let Some(v) = o.get_own(key) {
                    return v;
                }
                {
                    match key {
                        "name" if o.flags.get() & crate::object::NAME_DELETED == 0 => {
                            return Value::Str(n.name.clone());
                        }
                        "length" if o.flags.get() & crate::object::LENGTH_DELETED == 0 => {
                            return Value::Number(n.arity as f64);
                        }
                        _ => {}
                    }
                }
            }
            ObjKind::Bound(_)
                if key == "name"
                    && !o.has_own("name")
                    && o.flags.get() & crate::object::NAME_DELETED == 0 =>
            {
                return o.function_name().map(Value::Str).unwrap_or_default();
            }
            ObjKind::Map(m) if key == "size" => {
                return Value::Number(m.borrow().len() as f64);
            }
            ObjKind::Global => {
                if let Some(v) = self.globals.get(key) {
                    return v.clone();
                }
            }
            ObjKind::Arguments(m) => {
                if let Some(v) = self.mapped_arg(m, key) {
                    return v;
                }
            }
            ObjKind::Proxy(p) => {
                if let Some(t) = p.target.borrow().clone() {
                    return self.get_obj(&t, key);
                }
                return Value::Undefined;
            }
            ObjKind::Typed(t) => {
                if let Some(i) = array_index(key) {
                    return if i < t.len() {
                        t.get(i).unwrap_or_default()
                    } else {
                        Value::Undefined
                    };
                }
                if crate::builtins::typed::canonical_numeric(key).is_some() {
                    return Value::Undefined;
                }
                if !o.props.borrow().contains_key(key) {
                    match key {
                        "length" => return Value::Number(t.len() as f64),
                        "byteLength" => return Value::Number(t.byte_len() as f64),
                        "byteOffset" => {
                            return Value::Number(if t.out_of_bounds() {
                                0.0
                            } else {
                                t.offset as f64
                            });
                        }
                        "buffer" => return Value::Object(t.buffer.clone()),
                        _ => {}
                    }
                }
            }
            ObjKind::Primitive(Value::Str(s)) => {
                if key == "length" {
                    return Value::Number(len16_rc(s) as f64);
                }
                if let Some(i) = array_index(key) {
                    if let Some(c) = char_at16_rc(s, i) {
                        return Value::from(c);
                    }
                }
            }
            ObjKind::Set(s) if key == "size" => {
                return Value::Number(s.borrow().len() as f64);
            }
            ObjKind::RegExp(r) if key == "lastIndex" => {
                return r.last_index.borrow().clone();
            }
            _ => {}
        }
        if let Some(v) = o.props.borrow().get(key) {
            return v.clone();
        }
        let proto = o.proto.borrow().clone();
        match proto {
            Some(p) => self.get_inherited(&p, key),
            None => Value::Undefined,
        }
    }

    fn get_inherited(&self, o: &Rc<Obj>, key: &str) -> Value {
        match &o.kind {
            ObjKind::Map(_) | ObjKind::Set(_) if key == "size" => {}
            ObjKind::Typed(_)
                if matches!(key, "length" | "byteLength" | "byteOffset" | "buffer") => {}
            _ => return self.get_obj(o, key),
        }
        if let Some(v) = o.props.borrow().get(key) {
            return v.clone();
        }
        let proto = o.proto.borrow().clone();
        match proto {
            Some(p) => self.get_inherited(&p, key),
            None => Value::Undefined,
        }
    }

    pub fn resolve_accessor(&mut self, v: Value, receiver: &Value) -> Result<Value, Value> {
        if matches!(v, Value::Uninit) {
            return Err(self.make_error(
                ErrorKind::ReferenceError,
                "Cannot access binding before initialization",
            ));
        }
        if let Value::Object(a) = &v {
            if let ObjKind::Accessor(acc) = &a.kind {
                let getter = acc.get.borrow().clone();
                if getter.is_undefined() {
                    return Ok(Value::Undefined);
                }
                return self.call_value(&getter, receiver.clone(), &[]);
            }
        }
        Ok(v)
    }

    fn accessor_setter(&self, o: &Rc<Obj>, key: &str) -> Option<Value> {
        let mut cur = Some(o.clone());
        while let Some(c) = cur {
            if matches!(c.kind, ObjKind::Typed(_))
                && crate::builtins::typed::canonical_numeric(key).is_some()
            {
                return None;
            }
            if let Some(v) = c.props.borrow().get(key) {
                if let Value::Object(a) = v {
                    if let ObjKind::Accessor(acc) = &a.kind {
                        return Some(acc.set.borrow().clone());
                    }
                }
                return None;
            }
            cur = c.proto.borrow().clone();
        }
        None
    }

    pub fn define_accessor(&mut self, target: &Rc<Obj>, key: JsStr, f: Value, setter: bool) {
        self.accessors_used = true;
        let existing = target.props.borrow().get(&key).cloned();
        let acc_obj = match existing {
            Some(Value::Object(a)) if a.is_accessor() => a,
            _ => {
                let a = Obj::new(ObjKind::Accessor(Box::default()), None);
                target.set_own(key, Value::Object(a.clone()));
                a
            }
        };
        if let ObjKind::Accessor(acc) = &acc_obj.kind {
            if setter {
                *acc.set.borrow_mut() = f;
            } else {
                *acc.get.borrow_mut() = f;
            }
        }
    }

    pub fn get_prop(&mut self, v: &Value, key: &str) -> Result<Value, Value> {
        match v {
            Value::Object(o) => {
                match &o.kind {
                    ObjKind::Proxy(_) => return self.op_get(o, key, v),
                    ObjKind::Global if self.globals.get(key).is_none() => {
                        self.load_lazy_global(key)?;
                    }
                    _ => {}
                }
                if self.proxies_used && proxy_in_chain(o) {
                    return self.op_get(o, key, v);
                }
                let raw = self.get_obj(o, key);
                if key == "caller" {
                    if let Some(c) = self.legacy_caller(o, &raw) {
                        return Ok(c);
                    }
                }
                self.resolve_accessor(raw, v)
            }
            Value::Symbol(s) => {
                if key == "description" {
                    return Ok(s.description.clone().map(Value::Str).unwrap_or_default());
                }
                let raw = self.realm.symbol_proto.get_chain(key).unwrap_or_default();
                self.resolve_accessor(raw, v)
            }
            Value::Str(s) => {
                if key == "length" {
                    return Ok(Value::Number(len16_rc(s) as f64));
                }
                if let Some(i) = array_index(key) {
                    return Ok(char_at16_rc(s, i).map(Value::from).unwrap_or_default());
                }
                let raw = self.realm.string_proto.get_chain(key).unwrap_or_default();
                self.resolve_accessor(raw, v)
            }
            Value::Number(_) => {
                let raw = self.realm.number_proto.get_chain(key).unwrap_or_default();
                self.resolve_accessor(raw, v)
            }
            Value::BigInt(_) => {
                let raw = self.realm.bigint_proto.get_chain(key).unwrap_or_default();
                self.resolve_accessor(raw, v)
            }
            Value::Bool(_) => {
                let raw = self.realm.boolean_proto.get_chain(key).unwrap_or_default();
                self.resolve_accessor(raw, v)
            }
            Value::Undefined | Value::Uninit | Value::Null => Err(self.type_error(format!(
                "Cannot read properties of {} (reading '{}')",
                inspect_plain(v),
                key
            ))),
        }
    }

    #[inline(never)]
    fn legacy_caller(&self, o: &Rc<Obj>, raw: &Value) -> Option<Value> {
        let Value::Object(a) = raw else {
            return None;
        };
        let ObjKind::Accessor(acc) = &a.kind else {
            return None;
        };
        if !acc.get.borrow().strict_eq(&self.thrower) {
            return None;
        }
        let ObjKind::Function(c) = &o.kind else {
            return None;
        };
        if !crate::builtins::intrinsics::plain_sloppy_function(&Value::Object(o.clone())) {
            return None;
        }
        let mut i = self.frames.len();
        while i > 0 {
            i -= 1;
            if Rc::ptr_eq(&self.frames[i].closure, c) {
                if i == 0 {
                    return Some(Value::Null);
                }
                let base = self.frames[i - 1].base;
                let f = self.stack[base - 1].clone();
                return Some(if crate::builtins::intrinsics::plain_sloppy_function(&f) {
                    f
                } else {
                    Value::Null
                });
            }
        }
        Some(Value::Null)
    }

    pub(crate) fn note_proto_index(&mut self, o: &Rc<Obj>, key: &str) {
        if !self.proto_indices
            && array_index(key).is_some()
            && (Rc::ptr_eq(o, &self.realm.array_proto) || Rc::ptr_eq(o, &self.realm.object_proto))
        {
            self.proto_indices = true;
        }
    }

    pub fn set_prop(&mut self, target: &Value, key: JsStr, val: Value) -> Result<(), Value> {
        if let Value::Object(o) = target {
            if matches!(o.kind, ObjKind::Error) && matches!(&*key, "name" | "message") {
                let old = crate::inspect::error_header(o);
                self.set_prop_inner(target, key, val)?;
                let new = crate::inspect::error_header(o);
                if old != new {
                    if let Some(Value::Str(stack)) = o.get_own("stack") {
                        if let Some(rest) = stack.strip_prefix(&*old) {
                            o.set_hidden("stack", format!("{new}{rest}").into());
                        }
                    }
                }
                return Ok(());
            }
        }
        self.set_prop_inner(target, key, val)
    }

    fn set_prop_inner(&mut self, target: &Value, key: JsStr, val: Value) -> Result<(), Value> {
        match target {
            Value::Object(o) => {
                self.note_proto_index(o, &key);
                match &o.kind {
                    ObjKind::Proxy(_) => {
                        if !self.op_set(o, key.clone(), val, target)? {
                            return Err(self.assign_error(format!(
                                "'set' on proxy: trap returned falsish for property '{}'",
                                crate::props::key_text(&key)
                            )));
                        }
                        return Ok(());
                    }
                    ObjKind::Global => return self.set_global_prop(key, val),
                    ObjKind::Namespace => {
                        return Err(self.assign_error(format!(
                            "Cannot assign to read only property '{}' of object '[object Module]'",
                            crate::props::key_text(&key)
                        )));
                    }
                    _ => {}
                }
                if self.proxies_used && proxy_in_chain(o) {
                    if !self.op_set(o, key.clone(), val, target)? {
                        return Err(self.assign_error(format!(
                            "Cannot assign to property '{}'",
                            crate::props::key_text(&key)
                        )));
                    }
                    return Ok(());
                }
                if self.accessors_used
                    || o.is_callable()
                    || !matches!(o.kind, ObjKind::Ordinary | ObjKind::Array(_))
                {
                    if let Some(setter) = self.accessor_setter(o, &key) {
                        if setter.is_callable() {
                            self.call_value(&setter, target.clone(), &[val])?;
                            return Ok(());
                        }
                        return Err(self.assign_error(format!(
                            "Cannot set property {key} of {} which has only a getter",
                            inspect_plain(target)
                        )));
                    }
                }
                if let ObjKind::RegExp(r) = &o.kind {
                    if &*key == "lastIndex" {
                        if o.flags.get() & crate::object::ELEMENTS_FROZEN != 0 {
                            return Err(self.read_only_error(o, &key));
                        }
                        *r.last_index.borrow_mut() = val;
                        return Ok(());
                    }
                }
                if let ObjKind::Primitive(Value::Str(s)) = &o.kind {
                    if &*key == "length" || array_index(&key).is_some_and(|i| i < len16(s)) {
                        return Err(self.read_only_error(o, &key));
                    }
                }
                if let ObjKind::Arguments(m) = &o.kind {
                    self.set_mapped_arg(m, &key, &val);
                }
                if let ObjKind::Typed(t) = &o.kind {
                    if let Some(i) = array_index(&key) {
                        let x = crate::builtins::typed::coerce(self, t.kind, &val)?;
                        t.set(i, &x);
                        return Ok(());
                    }
                    if crate::builtins::typed::canonical_numeric(&key).is_some() {
                        crate::builtins::typed::coerce(self, t.kind, &val)?;
                        return Ok(());
                    }
                    if matches!(&*key, "length" | "byteLength" | "byteOffset" | "buffer")
                        && !o.props.borrow().contains_key(&key)
                    {
                        return Err(self.assign_error(format!(
                            "Cannot set property {key} of {} which has only a getter",
                            inspect_plain(target)
                        )));
                    }
                }
                if let ObjKind::Array(a) = &o.kind {
                    if let Some(i) = array_index(&key) {
                        let flags = o.flags.get();
                        if flags & crate::object::SLOW_ELEMENTS != 0 && !o.is_hole(i) {
                            let raw = a.borrow().get(i).cloned().unwrap_or_default();
                            if let Some((_, setter)) = crate::props::accessor_parts(&raw) {
                                if setter.is_callable() {
                                    self.call_value(&setter, target.clone(), &[val])?;
                                    return Ok(());
                                }
                                return Err(self.assign_error(format!(
                                    "Cannot set property {key} of {} which has only a getter",
                                    inspect_plain(target)
                                )));
                            }
                            if o.props.borrow().attrs(&key) & crate::object::READ_ONLY != 0 {
                                return Err(self.read_only_error(o, &key));
                            }
                        }
                        if flags != 0 {
                            let len = a.borrow().len();
                            if flags & crate::object::ELEMENTS_FROZEN != 0 && i < len {
                                return Err(self.read_only_error(o, &key));
                            }
                            if !o.is_extensible() && i >= len {
                                return Err(self.not_extensible_error(&key));
                            }
                        }
                        return self.set_array_index(o, a, i, val);
                    }
                    if &*key == "length" {
                        let n = self.to_number(&val)?;
                        if n < 0.0 || n.fract() != 0.0 || n > 4294967295.0 {
                            return Err(self.range_error("Invalid array length"));
                        }
                        let flags = o.flags.get();
                        if flags & crate::object::ELEMENTS_FROZEN != 0 {
                            return Err(self.read_only_error(o, &key));
                        }
                        let len = a.borrow().len();
                        if n as usize > len + 50_000_000 {
                            return Err(self.range_error("Array length too large"));
                        }
                        let limit = self.shrink_limit(o, n as usize);
                        a.borrow_mut().resize(limit, Value::Undefined);
                        if limit > n as usize {
                            if limit < len {
                                o.truncate_holes(limit);
                            }
                            return Err(self.assign_error(format!(
                                "Cannot delete property '{}' of [object Array]",
                                limit - 1
                            )));
                        }
                        if (n as usize) < len {
                            o.truncate_holes(n as usize);
                        } else {
                            o.mark_holes(len, n as usize);
                        }
                        return Ok(());
                    }
                }
                if matches!(
                    o.kind,
                    ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_)
                ) && matches!(&*key, "name" | "length")
                    && !o.has_own(&key)
                {
                    return Err(self.read_only_error(o, &key));
                }
                if &*key == "__proto__" && !o.has_own("__proto__") {
                    if let Some(found) = o.get_chain("__proto__") {
                        if found.strict_eq(&self.proto_accessor) {
                            let proto = match val {
                                Value::Object(p) => Some(p),
                                Value::Null => None,
                                _ => return Ok(()),
                            };
                            if !self.op_set_prototype_of(o, proto)? {
                                return Err(self.type_error("Cyclic __proto__ value"));
                            }
                            return Ok(());
                        }
                    }
                }
                if &*key == "prototype" {
                    if let ObjKind::Function(f) = &o.kind {
                        if !o.has_own("prototype")
                            && (f.proto.is_constructor() || f.proto.is_generator)
                        {
                            let _ = self.get_obj(o, "prototype");
                        }
                    }
                }
                let mut props = o.props.borrow_mut();
                let hit = props
                    .get(&key)
                    .map(|raw| crate::props::accessor_parts(raw).map(|(_, s)| s));
                match hit {
                    Some(Some(setter)) => {
                        drop(props);
                        if setter.is_callable() {
                            self.call_value(&setter, target.clone(), &[val])?;
                            return Ok(());
                        }
                        return Err(self.assign_error(format!(
                            "Cannot set property {key} of {} which has only a getter",
                            inspect_plain(target)
                        )));
                    }
                    Some(None) => {
                        if props.attrs(&key) & crate::object::READ_ONLY != 0 {
                            drop(props);
                            return Err(self.read_only_error(o, &key));
                        }
                        props.insert(key, val);
                        return Ok(());
                    }
                    None => {}
                }
                drop(props);
                if self.readonly_used && self.inherited_read_only(o, &key) {
                    return Err(self.read_only_error(o, &key));
                }
                if !o.is_extensible() {
                    return Err(self.not_extensible_error(&key));
                }
                o.props.borrow_mut().insert(key, val);
                Ok(())
            }
            Value::Undefined | Value::Null => Err(self.type_error(format!(
                "Cannot set properties of {} (setting '{}')",
                inspect_plain(target),
                key
            ))),
            _ => self.set_on_primitive(target, key, val),
        }
    }

    #[inline(never)]
    fn set_on_primitive(&mut self, target: &Value, key: JsStr, val: Value) -> Result<(), Value> {
        let proto = match target {
            Value::Number(_) => self.realm.number_proto.clone(),
            Value::Str(s) => {
                if &*key == "length" || array_index(&key).is_some_and(|i| i < len16(s)) {
                    return Err(self.assign_error(format!(
                        "Cannot assign to read only property '{}' of string",
                        crate::props::key_text(&key)
                    )));
                }
                self.realm.string_proto.clone()
            }
            Value::Bool(_) => self.realm.boolean_proto.clone(),
            Value::Symbol(_) => self.realm.symbol_proto.clone(),
            Value::BigInt(_) => self.realm.bigint_proto.clone(),
            _ => return Ok(()),
        };
        if self.proxies_used && proxy_in_chain(&proto) {
            if self.op_set(&proto, key.clone(), val, target)? {
                return Ok(());
            }
        } else if let Some(setter) = self.accessor_setter(&proto, &key) {
            if setter.is_callable() {
                self.call_value(&setter, target.clone(), &[val])?;
                return Ok(());
            }
        }
        Err(self.assign_error(format!(
            "Cannot create property '{}' on {} '{}'",
            crate::props::key_text(&key),
            target.type_of(),
            inspect_plain(target)
        )))
    }

    fn set_array_index(
        &mut self,
        o: &Rc<Obj>,
        a: &RefCell<Vec<Value>>,
        i: usize,
        val: Value,
    ) -> Result<(), Value> {
        let mut arr = a.borrow_mut();
        if i < arr.len() {
            arr[i] = val;
            drop(arr);
            o.fill_hole(i);
        } else if i - arr.len() > 50_000_000 {
            drop(arr);
            return Err(self.range_error("Array index too far beyond the end of the array"));
        } else {
            let old = arr.len();
            arr.resize(i, Value::Undefined);
            arr.push(val);
            drop(arr);
            o.mark_holes(old, i);
        }
        Ok(())
    }

    fn get_index(&mut self, obj: &Value, key: &Value) -> Result<Value, Value> {
        if let Value::Number(n) = key {
            let i = *n as usize;
            if i as f64 == *n {
                match obj {
                    Value::Object(o) => {
                        if let ObjKind::Array(a) = &o.kind {
                            if o.flags.get() & crate::object::SLOW_ELEMENTS == 0 && i < 4294967295 {
                                if let Some(v) = a.borrow().get(i) {
                                    if !o.is_hole(i) {
                                        return Ok(v.clone());
                                    }
                                }
                                if !self.proto_indices {
                                    return Ok(Value::Undefined);
                                }
                            }
                        }
                        if let ObjKind::Typed(t) = &o.kind {
                            return Ok(t.get(i).unwrap_or_default());
                        }
                    }
                    Value::Str(s) => {
                        return Ok(char_at16_rc(s, i).map(Value::from).unwrap_or_default());
                    }
                    _ => {}
                }
            }
        }
        if obj.is_nullish() {
            return Err(self.type_error(format!(
                "Cannot read properties of {} (reading '{}')",
                inspect_plain(obj),
                inspect_plain(key)
            )));
        }
        let k = self.to_property_key(key)?;
        self.get_prop(obj, &k)
    }

    fn set_index(&mut self, obj: &Value, key: &Value, val: Value) -> Result<(), Value> {
        if let (Value::Object(o), Value::Number(n)) = (obj, key) {
            if let ObjKind::Array(a) = &o.kind {
                let i = *n as usize;
                if i as f64 == *n
                    && i < 4294967295
                    && o.flags.get() & !crate::object::HAS_HOLES == 0
                    && !(self.proxies_used && proxy_in_chain(o))
                    && !Rc::ptr_eq(o, &self.realm.array_proto)
                {
                    return self.set_array_index(o, a, i, val);
                }
            }
            if let ObjKind::Typed(t) = &o.kind {
                let i = *n as usize;
                if i as f64 == *n {
                    let x = crate::builtins::typed::coerce(self, t.kind, &val)?;
                    t.set(i, &x);
                    return Ok(());
                }
            }
        }
        if obj.is_nullish() {
            return Err(self.type_error(format!(
                "Cannot set properties of {} (setting '{}')",
                inspect_plain(obj),
                inspect_plain(key)
            )));
        }
        let k = self.to_property_key(key)?;
        self.set_prop(obj, k, val)
    }

    pub fn to_primitive(&mut self, v: &Value, prefer_number: bool) -> Result<Value, Value> {
        self.to_primitive_hint(v, if prefer_number { "number" } else { "string" })
    }

    pub fn to_primitive_hint(&mut self, v: &Value, hint: &str) -> Result<Value, Value> {
        let Value::Object(_) = v else {
            return Ok(v.clone());
        };
        let exotic = self.get_prop(v, crate::object::TO_PRIMITIVE_KEY)?;
        if !exotic.is_nullish() {
            if !exotic.is_callable() {
                return Err(
                    self.type_error(format!("{} is not a function", inspect_plain(&exotic)))
                );
            }
            let r = self.call_value(&exotic, v.clone(), &[Value::Str(hint.into())])?;
            if r.is_object() {
                return Err(self.type_error("Cannot convert object to primitive value"));
            }
            return Ok(r);
        }
        self.ordinary_to_primitive(v, hint)
    }

    pub fn ordinary_to_primitive(&mut self, v: &Value, hint: &str) -> Result<Value, Value> {
        let order = if hint == "string" {
            ["toString", "valueOf"]
        } else {
            ["valueOf", "toString"]
        };
        for name in order {
            let f = self.get_prop(v, name)?;
            if f.is_callable() {
                let r = self.call_value(&f, v.clone(), &[])?;
                if !r.is_object() {
                    return Ok(r);
                }
            }
        }
        Err(self.type_error("Cannot convert object to primitive value"))
    }

    pub fn to_string(&mut self, v: &Value) -> Result<JsStr, Value> {
        if let Some(s) = v.to_primitive_string() {
            return Ok(s);
        }
        let p = self.to_primitive(v, false)?;
        if matches!(p, Value::Symbol(_)) {
            return Err(self.type_error("Cannot convert a Symbol value to a string"));
        }
        Ok(p.to_primitive_string().unwrap_or_else(|| "".into()))
    }

    pub fn to_number(&mut self, v: &Value) -> Result<f64, Value> {
        if let Some(n) = v.to_primitive_number() {
            return Ok(n);
        }
        let p = self.to_primitive(v, true)?;
        if matches!(p, Value::Symbol(_)) {
            return Err(self.type_error("Cannot convert a Symbol value to a number"));
        }
        if matches!(p, Value::BigInt(_)) {
            return Err(self.type_error("Cannot convert a BigInt value to a number"));
        }
        Ok(p.to_primitive_number().unwrap_or(f64::NAN))
    }

    pub fn to_property_key(&mut self, v: &Value) -> Result<JsStr, Value> {
        match v {
            Value::Str(s) => Ok(s.clone()),
            Value::Symbol(s) => Ok(s.key.clone()),
            Value::Object(_) => match self.to_primitive(v, false)? {
                Value::Symbol(s) => Ok(s.key.clone()),
                other => self.to_string(&other),
            },
            _ => self.to_string(v),
        }
    }

    pub fn loose_eq(&mut self, a: &Value, b: &Value) -> Result<bool, Value> {
        Ok(match (a, b) {
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            (Value::Symbol(_), Value::Object(_)) | (Value::Object(_), Value::Symbol(_)) => {
                let (obj, sym) = if matches!(a, Value::Object(_)) {
                    (a, b)
                } else {
                    (b, a)
                };
                let p = self.default_primitive(obj)?;
                return Ok(p.strict_eq(sym));
            }
            (Value::Symbol(_), _) | (_, Value::Symbol(_)) => a.strict_eq(b),
            (Value::BigInt(x), Value::BigInt(y)) => x == y,
            (Value::BigInt(x), Value::Number(n)) | (Value::Number(n), Value::BigInt(x)) => {
                crate::bigint::compare_f64(x, *n) == Some(std::cmp::Ordering::Equal)
            }
            (Value::BigInt(x), Value::Str(s)) | (Value::Str(s), Value::BigInt(x)) => {
                crate::bigint::from_string(s).is_some_and(|y| **x == y)
            }
            (Value::BigInt(x), Value::Bool(bv)) | (Value::Bool(bv), Value::BigInt(x)) => {
                **x == num_bigint::BigInt::from(*bv as i32)
            }
            (Value::Number(_), Value::Number(_))
            | (Value::Str(_), Value::Str(_))
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Object(_), Value::Object(_)) => a.strict_eq(b),
            (Value::Object(_), _) => {
                let p = self.default_primitive(a)?;
                return self.loose_eq(&p, b);
            }
            (_, Value::Object(_)) => {
                let p = self.default_primitive(b)?;
                return self.loose_eq(a, &p);
            }
            _ => {
                let x = a.to_primitive_number().unwrap_or(f64::NAN);
                let y = b.to_primitive_number().unwrap_or(f64::NAN);
                x == y
            }
        })
    }

    fn default_primitive(&mut self, v: &Value) -> Result<Value, Value> {
        self.to_primitive_hint(v, "default")
    }

    fn add_slow(&mut self, a: &Value, b: &Value) -> Result<Value, Value> {
        let pa = self.default_primitive(a)?;
        let pb = self.default_primitive(b)?;
        if matches!(pa, Value::Str(_)) || matches!(pb, Value::Str(_)) {
            let sa = self.to_string(&pa)?;
            let sb = self.to_string(&pb)?;
            let mut s = String::with_capacity(sa.len() + sb.len());
            s.push_str(&sa);
            crate::value::push16(&mut s, &sb);
            return Ok(Value::Str(s.into()));
        }
        if matches!(pa, Value::BigInt(_)) || matches!(pb, Value::BigInt(_)) {
            let (x, y) = self.numeric_pair(&pa, &pb)?;
            return self.bigint_op(crate::bigint::BigOp::Add, &x, &y);
        }
        let x = self.to_number(&pa)?;
        let y = self.to_number(&pb)?;
        Ok(Value::Number(x + y))
    }

    fn compare(&mut self, a: &Value, b: &Value) -> Result<Option<std::cmp::Ordering>, Value> {
        let pa = self.to_primitive(a, true)?;
        let pb = self.to_primitive(b, true)?;
        if let (Value::Str(x), Value::Str(y)) = (&pa, &pb) {
            return Ok(Some(compare_strings(x, y)));
        }
        match (&pa, &pb) {
            (Value::BigInt(x), Value::BigInt(y)) => return Ok(Some(x.cmp(y))),
            (Value::BigInt(x), Value::Str(s)) => {
                return Ok(crate::bigint::from_string(s).map(|y| (**x).cmp(&y)));
            }
            (Value::Str(s), Value::BigInt(y)) => {
                return Ok(crate::bigint::from_string(s).map(|x| x.cmp(y)));
            }
            (Value::BigInt(x), other) => {
                let n = self.to_number(other)?;
                return Ok(crate::bigint::compare_f64(x, n));
            }
            (other, Value::BigInt(y)) => {
                let n = self.to_number(other)?;
                return Ok(crate::bigint::compare_f64(y, n).map(|o| o.reverse()));
            }
            _ => {}
        }
        let x = self.to_number(&pa)?;
        let y = self.to_number(&pb)?;
        Ok(x.partial_cmp(&y))
    }

    pub fn to_numeric(&mut self, v: &Value) -> Result<Value, Value> {
        if let Value::BigInt(_) = v {
            return Ok(v.clone());
        }
        let p = self.to_primitive(v, true)?;
        if let Value::BigInt(_) = p {
            return Ok(p);
        }
        Ok(Value::Number(self.to_number(&p)?))
    }

    fn numeric_pair(&mut self, a: &Value, b: &Value) -> Result<(Value, Value), Value> {
        let x = self.to_numeric(a)?;
        let y = self.to_numeric(b)?;
        match (&x, &y) {
            (Value::BigInt(_), Value::Number(_)) | (Value::Number(_), Value::BigInt(_)) => {
                Err(self.type_error("Cannot mix BigInt and other types, use explicit conversions"))
            }
            _ => Ok((x, y)),
        }
    }

    pub fn bigint_op(
        &mut self,
        op: crate::bigint::BigOp,
        a: &Value,
        b: &Value,
    ) -> Result<Value, Value> {
        let (Value::BigInt(x), Value::BigInt(y)) = (a, b) else {
            return Err(
                self.type_error("Cannot mix BigInt and other types, use explicit conversions")
            );
        };
        match crate::bigint::binop(op, x, y) {
            Ok(r) => Ok(Value::BigInt(Rc::new(r))),
            Err(crate::bigint::BigError::Range(m)) => Err(self.range_error(m)),
            Err(crate::bigint::BigError::Type(m)) => Err(self.type_error(m)),
        }
    }

    pub fn instance_of(&mut self, v: &Value, ctor: &Value) -> Result<bool, Value> {
        let Value::Object(c) = ctor else {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        };
        let h = self.get_prop(ctor, crate::object::HAS_INSTANCE_KEY)?;
        if !h.is_nullish() && !h.strict_eq(&self.default_has_instance) {
            if !h.is_callable() {
                return Err(self.type_error(format!("{} is not a function", inspect_plain(&h))));
            }
            let r = self.call_value(&h, ctor.clone(), std::slice::from_ref(v))?;
            return Ok(r.truthy());
        }
        if !c.is_callable() {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        }
        self.ordinary_has_instance(v, c)
    }

    pub fn ordinary_has_instance(&mut self, v: &Value, c: &Rc<Obj>) -> Result<bool, Value> {
        if !c.is_callable() {
            return Ok(false);
        }
        if let ObjKind::Bound(b) = &c.kind {
            let t = b.target.clone();
            return self.instance_of(v, &t);
        }
        let Value::Object(o) = v else {
            return Ok(false);
        };
        let proto = match self.get_prop(&Value::Object(c.clone()), "prototype")? {
            Value::Object(p) => p,
            _ => {
                return Err(self.type_error(
                    "Function has non-object prototype 'undefined' in instanceof check",
                ));
            }
        };
        let mut cur = self.op_get_prototype_of(o)?;
        while let Some(p) = cur {
            if Rc::ptr_eq(&p, &proto) {
                return Ok(true);
            }
            cur = self.op_get_prototype_of(&p)?;
        }
        Ok(false)
    }

    pub fn has_property(&mut self, obj: &Value, key: &Value) -> Result<bool, Value> {
        let Value::Object(o) = obj else {
            let k = self.to_string(key)?;
            return Err(self.type_error(format!(
                "Cannot use 'in' operator to search for '{k}' in {}",
                inspect_plain(obj)
            )));
        };
        let k = self.to_property_key(key)?;
        if crate::object::is_private_key(&k) {
            return Ok(o.props.borrow().contains_key(&k));
        }
        match &o.kind {
            ObjKind::Proxy(_) | ObjKind::Global => return self.op_has_property(o, &k),
            ObjKind::Array(a) => {
                if &*k == "length" {
                    return Ok(true);
                }
                if let Some(i) = array_index(&k) {
                    if i < a.borrow().len() && !o.is_hole(i) {
                        return Ok(true);
                    }
                }
            }
            ObjKind::Primitive(Value::Str(s)) => {
                if &*k == "length" || array_index(&k).is_some_and(|i| i < len16(s)) {
                    return Ok(true);
                }
            }
            ObjKind::Typed(t) => {
                if let Some(n) = crate::builtins::typed::canonical_numeric(&k) {
                    return Ok(crate::builtins::typed::valid_index(t, n).is_some());
                }
            }
            ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_)
                if self.own_property(o, &k).is_some() =>
            {
                return Ok(true);
            }
            _ => {}
        }
        if o.props.borrow().contains_key(&k) {
            return Ok(true);
        }
        let proto = o.proto.borrow().clone();
        match proto {
            Some(p) => self.has_property(&Value::Object(p), &Value::Str(k)),
            None => Ok(false),
        }
    }

    fn capture_pristine(&mut self) {
        let own = |o: &Rc<Obj>, k: &str| o.props.borrow().get(k).cloned().unwrap_or_default();
        let gen_next = self
            .realm
            .generator_proto
            .get_chain("next")
            .unwrap_or_default();
        let rp = self.realm.regexp_proto.clone();
        self.pristine = Some(Box::new(Pristine {
            regexp_exec: own(&rp, "exec"),
            regexp_flags: own(&rp, "flags"),
            regexp_symbols: [
                own(&rp, crate::object::MATCH_KEY),
                own(&rp, crate::object::MATCH_ALL_KEY),
                own(&rp, crate::object::REPLACE_KEY),
                own(&rp, crate::object::SEARCH_KEY),
                own(&rp, crate::object::SPLIT_KEY),
            ],
            array_iter: own(&self.realm.array_proto, ITER_KEY),
            map_iter: own(&self.realm.map_proto, ITER_KEY),
            set_iter: own(&self.realm.set_proto, ITER_KEY),
            string_iter: own(&self.realm.string_proto, ITER_KEY),
            iter_nexts: [
                own(&self.realm.iter_proto, "next"),
                own(&self.realm.map_iter_proto, "next"),
                own(&self.realm.set_iter_proto, "next"),
                own(&self.realm.string_iter_proto, "next"),
            ],
            gen_next,
            set_add: own(&self.realm.set_proto, "add"),
            map_set: own(&self.realm.map_proto, "set"),
            array_join: own(&self.realm.array_proto, "join"),
        }));
    }

    fn builtin_iterable(&self, o: &Rc<Obj>) -> bool {
        let Some(p) = &self.pristine else {
            return true;
        };
        let (proto, orig, family) = match &o.kind {
            ObjKind::Array(_) => (&self.realm.array_proto, &p.array_iter, 0),
            ObjKind::Map(_) => (&self.realm.map_proto, &p.map_iter, 1),
            ObjKind::Set(_) => (&self.realm.set_proto, &p.set_iter, 2),
            ObjKind::Iter(st) => {
                let family = st.borrow().family();
                let plain = o
                    .proto
                    .borrow()
                    .as_ref()
                    .is_some_and(|x| Rc::ptr_eq(x, self.iter_family_proto(family)));
                return plain
                    && !o.props.borrow().contains_key("next")
                    && self.family_next_pristine(family);
            }
            ObjKind::Coroutine(_) => {
                return o
                    .get_chain("next")
                    .is_some_and(|v| v.strict_eq(&p.gen_next));
            }
            _ => return false,
        };
        let plain = o
            .proto
            .borrow()
            .as_ref()
            .is_some_and(|x| Rc::ptr_eq(x, proto));
        plain
            && !o.props.borrow().contains_key(ITER_KEY)
            && proto
                .props
                .borrow()
                .get(ITER_KEY)
                .is_some_and(|v| v.strict_eq(orig))
            && self.family_next_pristine(family)
    }

    fn string_iterable(&self) -> bool {
        let Some(p) = &self.pristine else {
            return true;
        };
        self.realm
            .string_proto
            .props
            .borrow()
            .get(ITER_KEY)
            .is_some_and(|v| v.strict_eq(&p.string_iter))
            && self.family_next_pristine(3)
    }

    pub fn get_iterator(&mut self, v: &Value) -> Result<Value, Value> {
        match v {
            Value::Str(s) if self.string_iterable() => {
                Ok(self.new_iter(IterState::Str(s.clone(), 0)))
            }
            Value::Object(o) if self.builtin_iterable(o) => match &o.kind {
                ObjKind::Array(_) => Ok(self.new_iter(IterState::Array(o.clone(), 0, 1))),
                ObjKind::Map(_) => Ok(self.new_iter(IterState::MapEntries(o.clone(), 0))),
                ObjKind::Set(_) => Ok(self.new_iter(IterState::SetValues(o.clone(), 0))),
                ObjKind::Iter(_) => Ok(v.clone()),
                ObjKind::Coroutine(c) if c.is_generator && !c.is_async => Ok(v.clone()),
                _ => self.protocol_iterator(v),
            },
            Value::Object(_) | Value::Str(_) => self.protocol_iterator(v),
            _ if v.is_nullish() => {
                Err(self.type_error(format!("{} is not iterable", inspect_plain(v))))
            }
            _ => self.protocol_iterator(v),
        }
    }

    pub fn get_async_iterator(&mut self, v: &Value) -> Result<Value, Value> {
        if !v.is_nullish() {
            let method = self.get_prop(v, crate::object::ASYNC_ITER_KEY)?;
            if !method.is_nullish() && !method.is_callable() {
                return Err(
                    self.type_error(format!("{} is not a function", inspect_plain(&method)))
                );
            }
            if method.is_callable() {
                let it = self.call_value(&method, v.clone(), &[])?;
                if !it.is_object() {
                    return Err(self
                        .type_error("Result of the Symbol.asyncIterator method is not an object"));
                }
                return Ok(it);
            }
        }
        let method = if v.is_nullish() {
            Value::Undefined
        } else {
            self.get_prop(v, ITER_KEY)?
        };
        if !method.is_callable() {
            return Err(self.type_error(format!("{} is not async iterable", inspect_plain(v))));
        }
        let it = self.call_value(&method, v.clone(), &[])?;
        if !it.is_object() {
            return Err(self.type_error("Result of the Symbol.iterator method is not an object"));
        }
        let next = self.get_prop(&it, "next")?;
        let make = self
            .globals
            .get("%CreateAsyncFromSyncIterator%")
            .cloned()
            .unwrap_or_default();
        self.call_value(&make, Value::Undefined, &[it, next])
    }

    fn protocol_iterator(&mut self, v: &Value) -> Result<Value, Value> {
        let method = self.get_prop(v, ITER_KEY)?;
        if !method.is_callable() {
            return Err(self.type_error(format!("{} is not iterable", inspect_plain(v))));
        }
        self.iterator_from_method(v, &method)
    }

    pub fn iterator_from_method(&mut self, v: &Value, method: &Value) -> Result<Value, Value> {
        let it = self.call_value(method, v.clone(), &[])?;
        let Value::Object(o) = &it else {
            return Err(self.type_error("Result of the Symbol.iterator method is not an object"));
        };
        match &o.kind {
            ObjKind::Iter(_) if self.builtin_iterable(o) => Ok(it),
            ObjKind::Coroutine(c) if c.is_generator && !c.is_async && self.builtin_iterable(o) => {
                Ok(it)
            }
            _ => {
                let next = self.get_prop(&it, "next")?;
                Ok(self.new_iter(IterState::Protocol(it, next)))
            }
        }
    }

    pub fn iter_next(&mut self, it: &Value) -> Result<Option<Value>, Value> {
        let Value::Object(o) = it else {
            return Ok(None);
        };
        let state = match &o.kind {
            ObjKind::Iter(state) => state,
            ObjKind::Coroutine(_) => {
                let o = o.clone();
                return match self.resume(&o, Value::Undefined, ResumeMode::Next)? {
                    Step::Yield(v) => Ok(Some(v)),
                    Step::YieldRaw(r) => {
                        if self.get_prop(&r, "done")?.truthy() {
                            Ok(None)
                        } else {
                            Ok(Some(self.get_prop(&r, "value")?))
                        }
                    }
                    _ => Ok(None),
                };
            }
            _ => return Ok(None),
        };
        fn map_at(m: &Obj, i: usize) -> Option<(Value, Value)> {
            let ObjKind::Map(map) = &m.kind else {
                return None;
            };
            let map = map.borrow();
            let (k, v) = map.get_index(i)?;
            Some((k.to_value(), v.clone()))
        }
        let protocol = {
            let mut st = state.borrow_mut();
            match &mut *st {
                IterState::Typed(ta, i, kind) => {
                    let ObjKind::Typed(t) = &ta.kind else {
                        return Ok(None);
                    };
                    if *i == usize::MAX {
                        return Ok(None);
                    }
                    if t.out_of_bounds() {
                        return Err(self.type_error(
                            "Cannot perform %ArrayIteratorPrototype%.next on a detached ArrayBuffer",
                        ));
                    }
                    if *i >= t.len() {
                        *i = usize::MAX;
                        return Ok(None);
                    }
                    let k = *i;
                    *i += 1;
                    let v = match kind {
                        0 => Value::Number(k as f64),
                        1 => t.get(k).unwrap_or_default(),
                        _ => {
                            let x = t.get(k).unwrap_or_default();
                            self.new_array(vec![Value::Number(k as f64), x])
                        }
                    };
                    return Ok(Some(v));
                }
                IterState::Array(arr, i, kind) => {
                    if *i == usize::MAX {
                        return Ok(None);
                    }
                    let kind = *kind;
                    let idx = *i;
                    let arr = arr.clone();
                    let len = match &arr.kind {
                        ObjKind::Array(a) => Some(a.borrow().len()),
                        ObjKind::Typed(t) => {
                            if t.out_of_bounds() {
                                return Err(self.type_error(
                                    "Cannot perform %ArrayIteratorPrototype%.next on a detached ArrayBuffer",
                                ));
                            }
                            Some(t.len())
                        }
                        _ => None,
                    };
                    let fast = matches!(arr.kind, ObjKind::Array(_))
                        && arr.flags.get() & crate::object::SLOW_ELEMENTS == 0
                        && !(self.proto_indices && arr.is_hole(idx));
                    drop(st);
                    let len = match len {
                        Some(n) => n,
                        None => {
                            let v = self.get_prop(&Value::Object(arr.clone()), "length")?;
                            let n = crate::builtins::to_integer(self, &v)?;
                            n.clamp(0.0, 9007199254740991.0) as usize
                        }
                    };
                    let mut st = state.borrow_mut();
                    let IterState::Array(_, i, _) = &mut *st else {
                        return Ok(None);
                    };
                    if idx >= len {
                        *i = usize::MAX;
                        return Ok(None);
                    }
                    *i = idx + 1;
                    drop(st);
                    if kind == 0 {
                        return Ok(Some(Value::Number(idx as f64)));
                    }
                    let v = if fast {
                        arr.array()
                            .and_then(|a| a.borrow().get(idx).cloned())
                            .unwrap_or_default()
                    } else {
                        self.get_prop(&Value::Object(arr.clone()), &idx.to_string())?
                    };
                    if kind == 1 {
                        return Ok(Some(v));
                    }
                    return Ok(Some(self.new_array(vec![Value::Number(idx as f64), v])));
                }
                IterState::Str(s, pos) => {
                    let Some(c) = s[*pos..].chars().next() else {
                        return Ok(None);
                    };
                    *pos += c.len_utf8();
                    return Ok(Some(Value::Str(c.to_string().into())));
                }
                IterState::MapKeys(m, i) => {
                    let Some((k, _)) = map_at(m, *i) else {
                        *i = usize::MAX;
                        return Ok(None);
                    };
                    *i += 1;
                    return Ok(Some(k));
                }
                IterState::MapValues(m, i) => {
                    let Some((_, v)) = map_at(m, *i) else {
                        *i = usize::MAX;
                        return Ok(None);
                    };
                    *i += 1;
                    return Ok(Some(v));
                }
                IterState::MapEntries(m, i) => {
                    let Some((k, v)) = map_at(m, *i) else {
                        *i = usize::MAX;
                        return Ok(None);
                    };
                    *i += 1;
                    drop(st);
                    return Ok(Some(self.new_array(vec![k, v])));
                }
                IterState::SetValues(s, i) => {
                    let ObjKind::Set(set) = &s.kind else {
                        return Ok(None);
                    };
                    let v = set.borrow().get_index(*i).map(|k| k.to_value());
                    match v {
                        Some(_) => *i += 1,
                        None => *i = usize::MAX,
                    }
                    return Ok(v);
                }
                IterState::SetEntries(s, i) => {
                    let ObjKind::Set(set) = &s.kind else {
                        return Ok(None);
                    };
                    let v = set.borrow().get_index(*i).map(|k| k.to_value());
                    let Some(v) = v else {
                        *i = usize::MAX;
                        return Ok(None);
                    };
                    *i += 1;
                    drop(st);
                    return Ok(Some(self.new_array(vec![v.clone(), v])));
                }
                IterState::List(items, i) => {
                    let v = items.get(*i).cloned();
                    if v.is_some() {
                        *i += 1;
                    }
                    return Ok(v);
                }
                IterState::ForIn(obj, keys, i) => {
                    let obj = obj.clone();
                    let k = keys.get(*i).cloned();
                    if k.is_some() {
                        *i += 1;
                    }
                    drop(st);
                    return match k {
                        None => Ok(None),
                        Some(k) => {
                            if self.has_property(&obj, &k)? {
                                Ok(Some(k))
                            } else {
                                self.iter_next(it)
                            }
                        }
                    };
                }
                IterState::Protocol(iterator, next) => (iterator.clone(), next.clone()),
            }
        };
        let (iterator, next) = protocol;
        let step = self.protocol_step(&iterator, &next);
        if !matches!(step, Ok(Some(_))) {
            if let ObjKind::Iter(state) = &o.kind {
                *state.borrow_mut() = IterState::List(Vec::new(), 0);
            }
        }
        step
    }

    fn protocol_step(&mut self, iterator: &Value, next: &Value) -> Result<Option<Value>, Value> {
        let r = self.call_value(next, iterator.clone(), &[])?;
        if !r.is_object() {
            return Err(self.type_error(format!(
                "Iterator result {} is not an object",
                inspect_plain(&r)
            )));
        }
        if self.get_prop(&r, "done")?.truthy() {
            return Ok(None);
        }
        Ok(Some(self.get_prop(&r, "value")?))
    }

    pub fn iter_close(&mut self, it: &Value) -> Result<(), Value> {
        let Value::Object(o) = it else {
            return Ok(());
        };
        match &o.kind {
            ObjKind::Coroutine(c) => {
                if c.state.get() != crate::object::CoState::Done && !c.is_async {
                    let o = o.clone();
                    self.resume(&o, Value::Undefined, ResumeMode::Return)?;
                }
                Ok(())
            }
            ObjKind::Iter(state) => {
                let target = match &*state.borrow() {
                    IterState::Protocol(iterator, _) => Some(iterator.clone()),
                    _ => None,
                };
                let Some(iterator) = target else {
                    return Ok(());
                };
                *state.borrow_mut() = IterState::List(Vec::new(), 0);
                let ret = self.get_prop(&iterator, "return")?;
                if ret.is_nullish() {
                    return Ok(());
                }
                let r = self.call_value(&ret, iterator, &[])?;
                if !r.is_object() {
                    return Err(self.type_error(format!(
                        "Iterator result {} is not an object",
                        inspect_plain(&r)
                    )));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn drain_iterator(&mut self, it: &Value) -> Result<Vec<Value>, Value> {
        let mut out = Vec::new();
        while let Some(x) = self.iter_next(it)? {
            out.push(x);
        }
        Ok(out)
    }

    pub fn iterate(&mut self, v: &Value) -> Result<Vec<Value>, Value> {
        if let Value::Object(o) = v {
            if let ObjKind::Array(a) = &o.kind {
                if self.builtin_iterable(o)
                    && o.flags.get() & crate::object::SLOW_ELEMENTS == 0
                    && !(self.proto_indices && o.has_holes())
                {
                    return Ok(a.borrow().clone());
                }
            }
        }
        let it = self.get_iterator(v)?;
        let mut out = Vec::new();
        while let Some(x) = self.iter_next(&it)? {
            out.push(x);
        }
        Ok(out)
    }

    pub fn for_in_keys(&self, v: &Value) -> Vec<Value> {
        let mut keys = self.own_keys(v);
        let Value::Object(o) = v else {
            return keys;
        };
        let mut seen: rustc_hash::FxHashSet<JsStr> = o.props.borrow().keys().cloned().collect();
        let mut cur = o.proto.borrow().clone();
        while let Some(p) = cur {
            for (k, _) in p.props.borrow().iter() {
                if is_symbol_key(k) || !seen.insert(k.clone()) {
                    continue;
                }
                if p.props.borrow().is_enumerable(k) {
                    keys.push(Value::Str(k.clone()));
                }
            }
            cur = p.proto.borrow().clone();
        }
        keys
    }

    pub fn own_property_names(&self, v: &Value) -> Vec<Value> {
        let mut keys = Vec::new();
        if let Value::Object(o) = v {
            if let ObjKind::Array(a) = &o.kind {
                for i in 0..a.borrow().len() {
                    if !o.is_hole(i) {
                        keys.push(Value::Str(i.to_string().into()));
                    }
                }
                keys.push(Value::Str("length".into()));
            }
            if let ObjKind::Typed(t) = &o.kind {
                for i in 0..t.len() {
                    keys.push(Value::Str(i.to_string().into()));
                }
            }
            if let ObjKind::Primitive(Value::Str(s)) = &o.kind {
                for i in 0..len16(s) {
                    keys.push(Value::Str(i.to_string().into()));
                }
                keys.push(Value::Str("length".into()));
            }
            if o.is_callable() {
                if let ObjKind::Function(c) = &o.kind {
                    if c.proto.is_constructor() {
                        let _ = self.get_obj(o, "prototype");
                    }
                }
                let mut indices: Vec<(usize, JsStr)> = o
                    .props
                    .borrow()
                    .keys()
                    .filter_map(|k| array_index(k).map(|i| (i, k.clone())))
                    .collect();
                indices.sort_by_key(|(i, _)| *i);
                for (_, k) in indices {
                    keys.push(Value::Str(k));
                }
                for k in ["length", "name", "prototype"] {
                    if self.own_property(o, k).is_some() {
                        keys.push(Value::Str(k.into()));
                    }
                }
                for k in o.props.borrow().keys() {
                    if !is_symbol_key(k)
                        && !matches!(&**k, "length" | "name" | "prototype")
                        && array_index(k).is_none()
                    {
                        keys.push(Value::Str(k.clone()));
                    }
                }
                return keys;
            }
            for k in o.props.borrow().keys() {
                if !is_symbol_key(k) {
                    keys.push(Value::Str(k.clone()));
                }
            }
            if matches!(o.kind, ObjKind::Namespace) {
                keys.sort_by(|a, b| match (a, b) {
                    (Value::Str(x), Value::Str(y)) => compare_strings(x, y),
                    _ => std::cmp::Ordering::Equal,
                });
            }
            return keys;
        }
        self.own_keys(v)
    }

    pub fn own_keys(&self, v: &Value) -> Vec<Value> {
        let mut keys = Vec::new();
        if let Value::Object(o) = v {
            if let ObjKind::Array(a) = &o.kind {
                let slow = o.flags.get() & crate::object::SLOW_ELEMENTS != 0;
                if o.has_holes() || slow {
                    for i in 0..a.borrow().len() {
                        if o.is_hole(i) {
                            continue;
                        }
                        let k: JsStr = i.to_string().into();
                        if slow && o.props.borrow().attrs(&k) & crate::object::NON_ENUM != 0 {
                            continue;
                        }
                        keys.push(Value::Str(k));
                    }
                    for (k, _) in o.props.borrow().enumerable() {
                        if !is_symbol_key(k) {
                            keys.push(Value::Str(k.clone()));
                        }
                    }
                    return keys;
                }
            }
        }
        match v {
            Value::Object(o) if matches!(o.kind, ObjKind::Global) => {
                for k in self.globals.names() {
                    if self.globals.attrs_of(&k) & crate::object::NON_ENUM == 0 {
                        keys.push(Value::Str(k));
                    }
                }
            }
            Value::Object(o) if matches!(o.kind, ObjKind::Proxy(_)) => {
                if let ObjKind::Proxy(p) = &o.kind {
                    if let Some(t) = p.target.borrow().clone() {
                        return self.own_keys(&Value::Object(t));
                    }
                }
            }
            Value::Object(o) => {
                if let ObjKind::Array(a) = &o.kind {
                    for i in 0..a.borrow().len() {
                        keys.push(Value::Str(i.to_string().into()));
                    }
                }
                if let ObjKind::Typed(t) = &o.kind {
                    for i in 0..t.len() {
                        keys.push(Value::Str(i.to_string().into()));
                    }
                }
                if let ObjKind::Primitive(Value::Str(s)) = &o.kind {
                    for i in 0..len16(s) {
                        keys.push(Value::Str(i.to_string().into()));
                    }
                }
                for (k, _) in o.props.borrow().enumerable() {
                    if !is_symbol_key(k) {
                        keys.push(Value::Str(k.clone()));
                    }
                }
            }
            Value::Str(s) => {
                for i in 0..len16(s) {
                    keys.push(Value::Str(i.to_string().into()));
                }
            }
            _ => {}
        }
        keys
    }

    fn super_home(&mut self, cl: &Closure) -> Result<Rc<Obj>, Value> {
        match cl.home.borrow().clone() {
            Some(h) => Ok(h),
            None => Err(self.make_error(ErrorKind::SyntaxError, "'super' keyword unexpected here")),
        }
    }

    fn super_base(&mut self, cl: &Closure, is_static: bool) -> Result<Option<Rc<Obj>>, Value> {
        let home = self.super_home(cl)?;
        Ok(if is_static {
            home.proto.borrow().clone()
        } else if home.is_callable() {
            self.function_prototype(&home)
                .and_then(|p| p.proto.borrow().clone())
        } else {
            home.proto.borrow().clone()
        })
    }

    fn super_get_from(
        &mut self,
        base: Option<Rc<Obj>>,
        key: &str,
        this: &Value,
    ) -> Result<Value, Value> {
        match base {
            Some(s) => {
                let raw = self.get_obj(&s, key);
                self.resolve_accessor(raw, this)
            }
            None => Err(self.type_error(format!(
                "Cannot read properties of null (reading '{}')",
                crate::props::key_text(key)
            ))),
        }
    }

    fn super_lookup(
        &mut self,
        cl: &Closure,
        key: &str,
        is_static: bool,
        this: &Value,
    ) -> Result<Value, Value> {
        let base = self.super_base(cl, is_static)?;
        self.super_get_from(base, key, this)
    }

    fn super_set_to(
        &mut self,
        base: Option<Rc<Obj>>,
        key: JsStr,
        this: &Value,
        val: Value,
    ) -> Result<(), Value> {
        let ok = match base {
            Some(s) => self.op_set(&s, key.clone(), val, this)?,
            None => {
                return Err(self.type_error(format!(
                    "Cannot set properties of null (setting '{}')",
                    crate::props::key_text(&key)
                )));
            }
        };
        if !ok {
            return Err(self.assign_error(format!(
                "Cannot assign to read only property '{}' of object",
                crate::props::key_text(&key)
            )));
        }
        Ok(())
    }

    fn super_store(
        &mut self,
        cl: &Closure,
        key: JsStr,
        is_static: bool,
        this: &Value,
        val: Value,
    ) -> Result<(), Value> {
        let base = self.super_base(cl, is_static)?;
        self.super_set_to(base, key, this, val)
    }

    pub fn run(&mut self, stop: usize) -> Result<Value, Value> {
        let mut fi = self.frames.len() - 1;
        let mut cl = self.frames[fi].closure.clone();
        let mut proto = cl.proto.clone();
        let mut ip = self.frames[fi].ip;
        let mut base = self.frames[fi].base;

        macro_rules! reload {
            () => {{
                fi = self.frames.len() - 1;
                let f = &self.frames[fi];
                cl = f.closure.clone();
                proto = cl.proto.clone();
                ip = f.ip;
                base = f.base;
            }};
        }
        macro_rules! throw {
            ($e:expr) => {{
                let e = $e;
                self.frames[fi].ip = ip;
                match self.unwind(e, stop) {
                    Ok(()) => {
                        reload!();
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }};
        }
        macro_rules! tri {
            ($e:expr) => {{
                self.frames[fi].ip = ip;
                match $e {
                    Ok(v) => v,
                    Err(e) => throw!(e),
                }
            }};
        }
        macro_rules! pop {
            () => {
                self.stack.pop().unwrap_or_default()
            };
        }
        macro_rules! peek {
            () => {
                self.stack.last().cloned().unwrap_or_default()
            };
        }
        macro_rules! name {
            ($k:expr) => {
                match &proto.consts[$k as usize] {
                    Value::Str(s) => s.clone(),
                    _ => JsStr::from(""),
                }
            };
        }
        macro_rules! num_op {
            ($f:expr, $big:expr) => {{
                let b = pop!();
                if let (Some(Value::Number(x)), Value::Number(y)) = (self.stack.last(), &b) {
                    let r = $f(*x, *y);
                    if let Some(top) = self.stack.last_mut() {
                        *top = Value::Number(r);
                    }
                    continue;
                }
                let a = pop!();
                let (a, b) = tri!(self.numeric_pair(&a, &b));
                if let (Value::Number(x), Value::Number(y)) = (&a, &b) {
                    self.stack.push(Value::Number($f(*x, *y)));
                } else {
                    let r = tri!(self.bigint_op($big, &a, &b));
                    self.stack.push(r);
                }
            }};
        }
        macro_rules! int_op {
            ($f:expr, $big:expr) => {{
                let b = pop!();
                let a = pop!();
                let (a, b) = match (&a, &b) {
                    (Value::Number(_), Value::Number(_)) => (a, b),
                    _ => tri!(self.numeric_pair(&a, &b)),
                };
                if let (Value::Number(x), Value::Number(y)) = (&a, &b) {
                    self.stack.push(Value::Number($f(*x, *y)));
                } else {
                    let r = tri!(self.bigint_op($big, &a, &b));
                    self.stack.push(r);
                }
            }};
        }
        macro_rules! cmp_op {
            ($num:expr, $ord:expr) => {{
                let b = pop!();
                if let (Some(Value::Number(x)), Value::Number(y)) = (self.stack.last(), &b) {
                    let r = $num(*x, *y);
                    if let Some(top) = self.stack.last_mut() {
                        *top = Value::Bool(r);
                    }
                    continue;
                }
                let a = pop!();
                let r = match tri!(self.compare(&a, &b)) {
                    Some(o) => $ord(o),
                    None => false,
                };
                self.stack.push(Value::Bool(r));
            }};
        }

        loop {
            let op = proto.code[ip];
            ip += 1;
            match op {
                Op::Const(i) => self.stack.push(proto.consts[i as usize].clone()),
                Op::Undefined => self.stack.push(Value::Undefined),
                Op::Null => self.stack.push(Value::Null),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => {
                    self.stack.pop();
                }
                Op::Dup => {
                    let v = peek!();
                    self.stack.push(v);
                }
                Op::Dup2 => {
                    let n = self.stack.len();
                    let a = self.stack[n - 2].clone();
                    let b = self.stack[n - 1].clone();
                    self.stack.push(a);
                    self.stack.push(b);
                }
                Op::Swap => {
                    let n = self.stack.len();
                    self.stack.swap(n - 1, n - 2);
                }
                Op::Rot3 => {
                    let v = pop!();
                    let n = self.stack.len();
                    self.stack.insert(n - 2, v);
                }
                Op::Rot4 => {
                    let v = pop!();
                    let n = self.stack.len();
                    self.stack.insert(n - 3, v);
                }
                Op::GetLocal(s) => {
                    self.flush_builder(base + s as usize);
                    let v = self.stack[base + s as usize].clone();
                    self.stack.push(v);
                }
                Op::Uninit => self.stack.push(Value::Uninit),
                Op::GetLocalChecked(s, k) => {
                    self.flush_builder(base + s as usize);
                    let v = self.stack[base + s as usize].clone();
                    if matches!(v, Value::Uninit) {
                        throw!(self.tdz_error(&proto, k));
                    }
                    self.stack.push(v);
                }
                Op::SetLocalChecked(s, k) => {
                    if matches!(self.stack[base + s as usize], Value::Uninit) {
                        throw!(self.tdz_error(&proto, k));
                    }
                    self.drop_builder(base + s as usize);
                    let v = peek!();
                    self.stack[base + s as usize] = v;
                }
                Op::GetUpvalChecked(i, k) => {
                    let v = match &*cl.upvalues[i as usize].borrow() {
                        Upvalue::Open(idx) => self.stack[*idx].clone(),
                        Upvalue::Closed(v) => v.clone(),
                    };
                    if matches!(v, Value::Uninit) {
                        throw!(self.tdz_error(&proto, k));
                    }
                    self.stack.push(v);
                }
                Op::SetUpvalChecked(i, k) => {
                    let uninit = match &*cl.upvalues[i as usize].borrow() {
                        Upvalue::Open(idx) => matches!(self.stack[*idx], Value::Uninit),
                        Upvalue::Closed(v) => matches!(v, Value::Uninit),
                    };
                    if uninit {
                        throw!(self.tdz_error(&proto, k));
                    }
                    let v = peek!();
                    let mut u = cl.upvalues[i as usize].borrow_mut();
                    match &mut *u {
                        Upvalue::Open(idx) => self.stack[*idx] = v,
                        Upvalue::Closed(c) => *c = v,
                    }
                }
                Op::SetLocal(s) => {
                    self.drop_builder(base + s as usize);
                    let v = peek!();
                    self.stack[base + s as usize] = v;
                }
                Op::AppendLocal(s) => {
                    let rhs = pop!();
                    tri!(self.append_local(base + s as usize, rhs));
                }
                Op::GetLocalProp(s, k) => {
                    let abs = base + s as usize;
                    if !self.builders.is_empty() {
                        self.flush_builder(abs);
                    }
                    let fast = match &self.stack[abs] {
                        Value::Object(o) if matches!(o.kind, ObjKind::Ordinary) => {
                            let Value::Str(key) = &proto.consts[k as usize] else {
                                unreachable!()
                            };
                            match o.get_chain_plain(key) {
                                Some(v)
                                    if !matches!(&v, Value::Object(a) if a.is_accessor())
                                        && !matches!(v, Value::Uninit) =>
                                {
                                    Some(v)
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    let v = match fast {
                        Some(v) => v,
                        None => {
                            let obj = self.stack[abs].clone();
                            let n = name!(k);
                            tri!(self.get_prop(&obj, &n))
                        }
                    };
                    self.stack.push(v);
                }
                Op::GetLocalMethod(s, k) => {
                    let abs = base + s as usize;
                    self.flush_builder(abs);
                    let fast = match &self.stack[abs] {
                        Value::Object(o) if matches!(o.kind, ObjKind::Ordinary) => {
                            let Value::Str(key) = &proto.consts[k as usize] else {
                                unreachable!()
                            };
                            match o.get_chain_plain(key) {
                                Some(v)
                                    if !matches!(&v, Value::Object(a) if a.is_accessor())
                                        && !matches!(v, Value::Uninit) =>
                                {
                                    Some(v)
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    let obj = self.stack[abs].clone();
                    let f = match fast {
                        Some(v) => v,
                        None => {
                            let n = name!(k);
                            tri!(self.get_prop(&obj, &n))
                        }
                    };
                    self.stack.push(f);
                    self.stack.push(obj);
                }
                Op::StoreLocalProp(s, k) => {
                    let val = pop!();
                    let abs = base + s as usize;
                    let done = match &self.stack[abs] {
                        Value::Object(o)
                            if matches!(o.kind, ObjKind::Ordinary)
                                && o.flags.get() == 0
                                && !self.accessors_used =>
                        {
                            o.props.borrow_mut().insert_fast(
                                name!(k),
                                val.clone(),
                                !self.readonly_used,
                            )
                        }
                        _ => false,
                    };
                    if !done {
                        if !self.builders.is_empty() {
                            self.flush_builder(abs);
                        }
                        let obj = self.stack[abs].clone();
                        self.silent_fail = false;
                        if let Err(e) = self.set_prop(&obj, name!(k), val) {
                            if !(proto.sloppy && self.silent_fail) {
                                throw!(e);
                            }
                        }
                    }
                }
                Op::GetUpval(i) => {
                    let v = match &*cl.upvalues[i as usize].borrow() {
                        Upvalue::Open(idx) => self.stack[*idx].clone(),
                        Upvalue::Closed(v) => v.clone(),
                    };
                    self.stack.push(v);
                }
                Op::SetUpval(i) => {
                    let v = peek!();
                    let mut u = cl.upvalues[i as usize].borrow_mut();
                    match &mut *u {
                        Upvalue::Open(idx) => self.stack[*idx] = v,
                        Upvalue::Closed(c) => *c = v,
                    }
                }
                Op::GetGlobal(k) => {
                    let cached = proto.global_cache[k as usize].get();
                    if cached != u32::MAX && !self.global_accessors {
                        let v = self.globals.at(cached).clone();
                        if matches!(v, Value::Uninit) {
                            throw!(self.tdz_error(&proto, k));
                        }
                        self.stack.push(v);
                        continue;
                    }
                    let n = name!(k);
                    let mut slot = self.globals.resolve(&n);
                    if slot.is_none() && tri!(self.load_lazy_global(&n)) {
                        slot = self.globals.resolve(&n);
                    }
                    match slot {
                        Some(slot) => {
                            proto.global_cache[k as usize].set(slot);
                            let mut v = self.globals.at(slot).clone();
                            if matches!(v, Value::Uninit) {
                                throw!(self.tdz_error(&proto, k));
                            }
                            if self.global_accessors {
                                if let Value::Object(a) = &v {
                                    if a.is_accessor() {
                                        v = tri!(self.resolve_accessor(v, &Value::Undefined));
                                    }
                                }
                            }
                            self.stack.push(v);
                        }
                        None => match self.realm.object_proto.get_own(&n) {
                            Some(v) if crate::props::accessor_parts(&v).is_none() => {
                                self.stack.push(v)
                            }
                            _ => throw!(self.make_error_at(
                                ErrorKind::ReferenceError,
                                format!("{n} is not defined"),
                                fi,
                                ip
                            )),
                        },
                    }
                }
                Op::GlobalRef(k) => {
                    let n = name!(k);
                    let found = self.globals.resolve(&n).is_some()
                        || tri!(self.load_lazy_global(&n))
                        || self.realm.object_proto.get_own(&n).is_some();
                    self.stack.push(Value::Bool(found));
                }
                Op::SetGlobal(k) | Op::SetGlobalStrict(k) => {
                    let n = name!(k);
                    if matches!(op, Op::SetGlobalStrict(_)) {
                        let v = pop!();
                        let flag = pop!();
                        self.stack.push(v);
                        if !matches!(flag, Value::Bool(true)) {
                            throw!(self.make_error_at(
                                ErrorKind::ReferenceError,
                                format!("{n} is not defined"),
                                fi,
                                ip
                            ));
                        }
                    }
                    let v = peek!();
                    match self.globals.resolve(&n) {
                        Some(slot) => {
                            if matches!(self.globals.at(slot), Value::Uninit) {
                                throw!(self.tdz_error(&proto, k));
                            }
                            if self.globals.is_lex_slot(slot) {
                                if self.globals.is_const_slot(slot) {
                                    throw!(self.make_error_at(
                                        ErrorKind::TypeError,
                                        "Assignment to constant variable.".to_string(),
                                        fi,
                                        ip
                                    ));
                                }
                                proto.global_cache[k as usize].set(slot);
                                *self.globals.at_mut(slot) = v;
                                continue;
                            }
                            if self.global_accessors || self.globals.readonly {
                                self.silent_fail = false;
                                if let Err(e) = self.set_global_prop(n.clone(), v) {
                                    if !(proto.sloppy && self.silent_fail) {
                                        throw!(e);
                                    }
                                }
                                continue;
                            }
                            proto.global_cache[k as usize].set(slot);
                            *self.globals.at_mut(slot) = v;
                        }
                        None if proto.sloppy => {
                            if tri!(self.load_lazy_global(&n)) {
                                tri!(self.set_global_prop(n, v));
                            } else {
                                self.globals.insert(n, v);
                            }
                        }
                        None => throw!(self.make_error_at(
                            ErrorKind::ReferenceError,
                            format!("{n} is not defined"),
                            fi,
                            ip
                        )),
                    }
                }
                Op::DefineGlobal(k, flags) => {
                    let n = name!(k);
                    let v = peek!();
                    tri!(self.define_global_binding(n, v, flags));
                }
                Op::CanDeclareGlobalFn(k) => {
                    let n = name!(k);
                    tri!(self.can_declare_global_fn(&n));
                }
                Op::CheckGlobalLex(k) => {
                    let n = name!(k);
                    tri!(self.check_global_lex(&n));
                }
                Op::CheckGlobalVar(k) => {
                    let n = name!(k);
                    tri!(self.check_global_var(&n));
                }
                Op::EvalVar(k) => {
                    let obj = pop!();
                    let n = name!(k);
                    if let Value::Object(o) = &obj {
                        if !o.has_own(&n) {
                            o.set_own(n, Value::Undefined);
                        }
                    }
                }
                Op::DeleteGlobal(k) => {
                    let n = name!(k);
                    let r = self.delete_global_binding(&n);
                    self.stack.push(Value::Bool(r));
                }
                Op::TypeofGlobal(k) => {
                    let n = name!(k);
                    if self.globals.resolve(&n).is_none() {
                        tri!(self.load_lazy_global(&n));
                    }
                    if matches!(self.globals.lookup(&n), Some(Value::Uninit)) {
                        throw!(self.tdz_error(&proto, k));
                    }
                    let mut found = self.globals.lookup(&n).cloned();
                    if found.is_none() {
                        found = self
                            .realm
                            .object_proto
                            .get_own(&n)
                            .filter(|v| crate::props::accessor_parts(v).is_none());
                    }
                    if let Some(Value::Object(a)) = &found {
                        if a.is_accessor() {
                            let raw = found.take().unwrap_or_default();
                            found = Some(tri!(self.resolve_accessor(raw, &Value::Undefined)));
                        }
                    }
                    let t = found.as_ref().map(|v| v.type_of()).unwrap_or("undefined");
                    self.stack.push(Value::Str(t.into()));
                }
                Op::GetProp(k) => {
                    let obj = pop!();
                    let v = match &obj {
                        Value::Object(o) if matches!(o.kind, ObjKind::Ordinary) => {
                            let key = &proto.consts[k as usize];
                            let Value::Str(key) = key else { unreachable!() };
                            match o.get_chain_plain(key) {
                                Some(v)
                                    if matches!(&v, Value::Object(a) if a.is_accessor())
                                        || matches!(v, Value::Uninit) =>
                                {
                                    tri!(self.resolve_accessor(v, &obj))
                                }
                                Some(v) => v,
                                None => tri!(self.get_prop(&obj, key)),
                            }
                        }
                        _ => {
                            let n = name!(k);
                            tri!(self.get_prop(&obj, &n))
                        }
                    };
                    self.stack.push(v);
                }
                Op::SetProp(k) => {
                    let val = pop!();
                    let obj = pop!();
                    let done = match &obj {
                        Value::Object(o)
                            if matches!(o.kind, ObjKind::Ordinary)
                                && o.flags.get() == 0
                                && !self.accessors_used =>
                        {
                            o.props.borrow_mut().insert_fast(
                                name!(k),
                                val.clone(),
                                !self.readonly_used,
                            )
                        }
                        _ => false,
                    };
                    if !done {
                        self.silent_fail = false;
                        if let Err(e) = self.set_prop(&obj, name!(k), val.clone()) {
                            if !(proto.sloppy && self.silent_fail) {
                                throw!(e);
                            }
                        }
                    }
                    self.stack.push(val);
                }
                Op::GetIndex => {
                    let key = pop!();
                    let obj = pop!();
                    let v = tri!(self.get_index(&obj, &key));
                    self.stack.push(v);
                }
                Op::SetIndex => {
                    let val = pop!();
                    let key = pop!();
                    let obj = pop!();
                    self.silent_fail = false;
                    if let Err(e) = self.set_index(&obj, &key, val.clone()) {
                        if !(proto.sloppy && self.silent_fail) {
                            throw!(e);
                        }
                    }
                    self.stack.push(val);
                }
                Op::DeleteProp(k) => {
                    let obj = pop!();
                    let n = name!(k);
                    self.silent_fail = false;
                    let r = match self.delete_prop(&obj, &n) {
                        Ok(r) => r,
                        Err(_) if proto.sloppy && self.silent_fail => false,
                        Err(e) => throw!(e),
                    };
                    self.stack.push(Value::Bool(r));
                }
                Op::DeleteIndex => {
                    let key = pop!();
                    let obj = pop!();
                    let n = tri!(self.to_property_key(&key));
                    self.silent_fail = false;
                    let r = match self.delete_prop(&obj, &n) {
                        Ok(r) => r,
                        Err(_) if proto.sloppy && self.silent_fail => false,
                        Err(e) => throw!(e),
                    };
                    self.stack.push(Value::Bool(r));
                }
                Op::GetMethod(k) => {
                    let obj = pop!();
                    let n = name!(k);
                    let f = tri!(self.get_prop(&obj, &n));
                    self.stack.push(f);
                    self.stack.push(obj);
                }
                Op::GetIndexMethod => {
                    let key = pop!();
                    let obj = pop!();
                    let f = tri!(self.get_index(&obj, &key));
                    self.stack.push(f);
                    self.stack.push(obj);
                }
                Op::SuperGet(k, is_static) => {
                    let this = pop!();
                    let n = name!(k);
                    let v = tri!(self.super_lookup(&cl, &n, is_static, &this));
                    self.stack.push(v);
                }
                Op::SuperGetMethod(k, is_static) => {
                    let this = pop!();
                    let n = name!(k);
                    let v = tri!(self.super_lookup(&cl, &n, is_static, &this));
                    self.stack.push(v);
                    self.stack.push(this);
                }
                Op::DelegateInit(is_async) => {
                    let v = pop!();
                    let it = if is_async {
                        tri!(self.get_async_iterator(&v))
                    } else {
                        let method = if v.is_nullish() {
                            Value::Undefined
                        } else {
                            tri!(self.get_prop(&v, ITER_KEY))
                        };
                        if !method.is_callable() {
                            throw!(
                                self.type_error(format!("{} is not iterable", inspect_plain(&v)))
                            );
                        }
                        let it = tri!(self.call_value(&method, v.clone(), &[]));
                        if !it.is_object() {
                            throw!(self.type_error(
                                "Result of the Symbol.iterator method is not an object"
                            ));
                        }
                        it
                    };
                    let next = tri!(self.get_prop(&it, "next"));
                    self.stack.push(it);
                    self.stack.push(next);
                    self.stack.push(Value::Number(0.0));
                    self.stack.push(Value::Undefined);
                }
                Op::DelegateCall => {
                    let n = self.stack.len();
                    let it = self.stack[n - 4].clone();
                    let next = self.stack[n - 3].clone();
                    let mode = match self.stack[n - 2] {
                        Value::Number(m) => m as u8,
                        _ => 0,
                    };
                    let val = self.stack[n - 1].clone();
                    let r = match mode {
                        0 => tri!(self.call_value(&next, it, &[val])),
                        1 => {
                            let m = tri!(self.get_method(&it, "throw"));
                            if m.is_undefined() {
                                let ret = tri!(self.get_method(&it, "return"));
                                if !ret.is_undefined() {
                                    let r = tri!(self.call_value(&ret, it.clone(), &[]));
                                    if !r.is_object() {
                                        throw!(self.type_error("Iterator result is not an object"));
                                    }
                                }
                                throw!(
                                    self.type_error(
                                        "The iterator does not provide a 'throw' method"
                                    )
                                );
                            }
                            tri!(self.call_value(&m, it, &[val]))
                        }
                        _ => {
                            let m = tri!(self.get_method(&it, "return"));
                            if m.is_undefined() {
                                self.stack[n - 2] = Value::Number(3.0);
                                self.stack.push(val);
                                continue;
                            }
                            tri!(self.call_value(&m, it, &[val]))
                        }
                    };
                    self.stack.push(r);
                }
                Op::DelegateDone(exit, ret) => {
                    let n = self.stack.len();
                    let mode = match self.stack[n - 3] {
                        Value::Number(m) => m as u8,
                        _ => 0,
                    };
                    if mode == 3 {
                        ip = ret as usize;
                        continue;
                    }
                    let r = self.stack[n - 1].clone();
                    if !r.is_object() {
                        throw!(self.type_error("Iterator result is not an object"));
                    }
                    let done = tri!(self.get_prop(&r, "done")).truthy();
                    if done {
                        let v = tri!(self.get_prop(&r, "value"));
                        self.stack[n - 1] = v;
                        ip = if mode == 2 { ret } else { exit } as usize;
                        continue;
                    }
                    if proto.is_async {
                        let v = tri!(self.get_prop(&r, "value"));
                        self.stack[n - 1] = v;
                    }
                }
                Op::YieldDelegate => {
                    let v = pop!();
                    self.frames[fi].ip = ip;
                    if !self.suspend_frame(fi) {
                        throw!(self.type_error("Cannot suspend outside of a coroutine"));
                    }
                    self.co_signal = Some(if proto.is_async {
                        Signal::Yield
                    } else {
                        Signal::YieldRaw
                    });
                    return Ok(v);
                }
                Op::GetPrivate(k) => {
                    let obj = pop!();
                    let key = name!(k);
                    let v = tri!(self.get_private(&obj, &key));
                    self.stack.push(v);
                }
                Op::SetPrivate(k) => {
                    let val = pop!();
                    let obj = pop!();
                    let key = name!(k);
                    tri!(self.set_private(&obj, &key, val.clone()));
                    self.stack.push(val);
                }
                Op::InitPrivate(k) => {
                    let val = pop!();
                    let key = name!(k);
                    name_anonymous(&val, &key, "");
                    if let Some(Value::Object(o)) = self.stack.last().cloned() {
                        if o.props.borrow().contains_key(&key) {
                            throw!(self.type_error(format!(
                                "Cannot initialize {} twice on the same object",
                                private_label(&key)
                            )));
                        }
                        o.set_hidden(key, val);
                    }
                }
                Op::DefineIndex => {
                    let val = pop!();
                    let key = pop!();
                    let key = tri!(self.to_property_key(&key));
                    if let Some(Value::Object(o)) = self.stack.last().cloned() {
                        let d = crate::props::PropDesc {
                            value: Some(val),
                            writable: Some(true),
                            enumerable: Some(true),
                            configurable: Some(true),
                            ..Default::default()
                        };
                        tri!(self.op_define_own_property(&o, key, d, true));
                    }
                }
                Op::DefineField(k) => {
                    let val = pop!();
                    let key = name!(k);
                    name_anonymous(&val, &key, "");
                    if let Some(Value::Object(o)) = self.stack.last().cloned() {
                        let d = crate::props::PropDesc {
                            value: Some(val),
                            writable: Some(true),
                            enumerable: Some(true),
                            configurable: Some(true),
                            ..Default::default()
                        };
                        tri!(self.op_define_own_property(&o, key, d, true));
                    }
                }
                Op::CopyPrivates => {
                    let callee = pop!();
                    let this = peek!();
                    if let (Value::Object(c), Value::Object(t)) = (&callee, &this) {
                        let proto = self.get_obj(c, "prototype");
                        if let Value::Object(p) = proto {
                            let entries: Vec<(JsStr, Value)> = p
                                .props
                                .borrow()
                                .iter()
                                .filter(|(k, _)| crate::object::is_private_key(k))
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            for (k, v) in entries {
                                if t.props.borrow().contains_key(&k) {
                                    throw!(self.type_error(format!(
                                        "Cannot initialize private methods of class {} twice on the same object",
                                        private_label(&k)
                                    )));
                                }
                                let attrs = if v.as_object().is_some_and(|a| a.is_accessor()) {
                                    crate::object::NON_ENUM
                                } else {
                                    crate::object::NON_ENUM | crate::object::READ_ONLY
                                };
                                t.set_with_attrs(k, v, attrs);
                            }
                        }
                    }
                }
                Op::DelegateEnd => {
                    let v = pop!();
                    let n = self.stack.len();
                    self.stack.truncate(n - 4);
                    self.stack.push(v);
                }
                Op::ToPropertyKey => {
                    let v = pop!();
                    if self.stack.last().is_some_and(|o| o.is_nullish()) {
                        let base = self.stack.last().cloned().unwrap_or_default();
                        throw!(self.type_error(format!(
                            "Cannot read properties of {} (reading '{}')",
                            inspect_plain(&base),
                            inspect_plain(&v)
                        )));
                    }
                    let k = tri!(self.to_property_key(&v));
                    self.stack.push(Value::Str(k));
                }
                Op::SuperGetIndex(is_static) => {
                    let key = pop!();
                    let this = pop!();
                    let base = tri!(self.super_base(&cl, is_static));
                    let n = tri!(self.to_property_key(&key));
                    let v = tri!(self.super_get_from(base, &n, &this));
                    self.stack.push(v);
                }
                Op::SuperGetIndexMethod(is_static) => {
                    let key = pop!();
                    let this = pop!();
                    let base = tri!(self.super_base(&cl, is_static));
                    let n = tri!(self.to_property_key(&key));
                    let v = tri!(self.super_get_from(base, &n, &this));
                    self.stack.push(v);
                    self.stack.push(this);
                }
                Op::SuperSet(k, is_static) => {
                    let val = pop!();
                    let this = pop!();
                    let n = name!(k);
                    self.silent_fail = false;
                    if let Err(e) = self.super_store(&cl, n, is_static, &this, val.clone()) {
                        if !(proto.sloppy && self.silent_fail) {
                            throw!(e);
                        }
                    }
                    self.stack.push(val);
                }
                Op::SuperSetIndex(is_static) => {
                    let val = pop!();
                    let key = pop!();
                    let this = pop!();
                    let base = tri!(self.super_base(&cl, is_static));
                    let n = tri!(self.to_property_key(&key));
                    self.silent_fail = false;
                    if let Err(e) = self.super_set_to(base, n, &this, val.clone()) {
                        if !(proto.sloppy && self.silent_fail) {
                            throw!(e);
                        }
                    }
                    self.stack.push(val);
                }
                Op::SuperCall(argc) => {
                    let argc = if argc == u32::MAX {
                        let arr = pop!();
                        let items = tri!(self.iterate(&arr));
                        let n = items.len();
                        self.stack.extend(items);
                        n
                    } else if argc == u32::MAX - 1 {
                        let arr = pop!();
                        let items = match &arr {
                            Value::Object(o) => {
                                o.array().map(|a| a.borrow().clone()).unwrap_or_default()
                            }
                            _ => Vec::new(),
                        };
                        let n = items.len();
                        self.stack.extend(items);
                        n
                    } else {
                        argc as usize
                    };
                    let home = tri!(self.super_home(&cl));
                    let sup = home.proto.borrow().clone();
                    let Some(sup) = sup else {
                        throw!(self.type_error("Super constructor is not a constructor"));
                    };
                    if !self.is_constructor(&Value::Object(sup.clone())) {
                        throw!(self.type_error("Super constructor is not a constructor"));
                    }
                    self.next_new_target = match &self.frames[fi].new_target {
                        Some(t) => Some(t.clone()),
                        None => self.stack[base - 1].as_object().cloned(),
                    };
                    let ti = self.stack.len() - argc - 1;
                    self.stack[ti] = Value::Uninit;
                    {
                        let derived_sup = matches!(&sup.kind, ObjKind::Function(c) if c.proto.kind == ProtoKind::DerivedConstructor);
                        if !derived_sup {
                            let proto = match self.next_new_target.clone() {
                                Some(t) => {
                                    match tri!(self.get_prop(&Value::Object(t), "prototype")) {
                                        Value::Object(p) => p,
                                        _ => self.realm.object_proto.clone(),
                                    }
                                }
                                None => self.realm.object_proto.clone(),
                            };
                            let kind = match &sup.kind {
                                ObjKind::Native(n) => n.ctor.map(|k| k()),
                                _ => self.root_ctor_kind(&sup).map(|k| k()),
                            }
                            .unwrap_or(ObjKind::Ordinary);
                            self.stack[ti] = Value::Object(Obj::new(kind, Some(proto)));
                        }
                    }
                    match &sup.kind {
                        ObjKind::Function(c) => {
                            let c = c.clone();
                            self.stack.insert(ti, Value::Object(sup.clone()));
                            self.frames[fi].ip = ip;
                            tri!(self.push_frame(c, argc, true));
                            reload!();
                        }
                        ObjKind::Native(n) => {
                            self.next_new_target = None;
                            let f = n.func;
                            let args = self.stack.split_off(ti + 1);
                            let this = pop!();
                            let r = tri!(f(self, this.clone(), &args));
                            if let (Value::Object(ro), Value::Object(t)) = (&r, &this) {
                                let reused = Rc::ptr_eq(ro, t)
                                    || args.iter().any(
                                        |a| matches!(a, Value::Object(x) if Rc::ptr_eq(x, ro)),
                                    );
                                if !reused {
                                    let p = t.proto.borrow().clone();
                                    *ro.proto.borrow_mut() = p;
                                }
                            }
                            self.stack.push(if r.is_object() { r } else { this });
                        }
                        ObjKind::Bound(_) | ObjKind::Proxy(_) => {
                            let nt = self
                                .next_new_target
                                .take()
                                .map(Value::Object)
                                .unwrap_or(Value::Undefined);
                            let args = self.stack.split_off(ti + 1);
                            let this = pop!();
                            let r = tri!(self.construct_with_target(
                                &Value::Object(sup.clone()),
                                &args,
                                &nt
                            ));
                            self.stack.push(if r.is_object() { r } else { this });
                        }
                        _ => throw!(self.type_error("Super constructor is not a constructor")),
                    }
                }
                Op::Add => {
                    let b = pop!();
                    if let (Some(Value::Number(x)), Value::Number(y)) = (self.stack.last(), &b) {
                        let r = x + y;
                        if let Some(top) = self.stack.last_mut() {
                            *top = Value::Number(r);
                        }
                        continue;
                    }
                    let a = pop!();
                    let r = match (&a, &b) {
                        (Value::Number(x), Value::Number(y)) => Value::Number(x + y),
                        (Value::Str(x), Value::Str(y)) => {
                            Value::Str(crate::value::concat16(x, y).into())
                        }
                        _ => tri!(self.add_slow(&a, &b)),
                    };
                    self.stack.push(r);
                }
                Op::Sub => num_op!(|x: f64, y: f64| x - y, BigOp::Sub),
                Op::Mul => num_op!(|x: f64, y: f64| x * y, BigOp::Mul),
                Op::Div => num_op!(|x: f64, y: f64| x / y, BigOp::Div),
                Op::Mod => num_op!(|x: f64, y: f64| x % y, BigOp::Mod),
                Op::Pow => num_op!(js_pow, BigOp::Pow),
                Op::BitAnd => int_op!(|x, y| (to_int32(x) & to_int32(y)) as f64, BigOp::BitAnd),
                Op::BitOr => int_op!(|x, y| (to_int32(x) | to_int32(y)) as f64, BigOp::BitOr),
                Op::BitXor => int_op!(|x, y| (to_int32(x) ^ to_int32(y)) as f64, BigOp::BitXor),
                Op::Shl => int_op!(
                    |x, y| to_int32(x).wrapping_shl(to_uint32(y) & 31) as f64,
                    BigOp::Shl
                ),
                Op::Shr => int_op!(
                    |x, y| (to_int32(x) >> (to_uint32(y) & 31)) as f64,
                    BigOp::Shr
                ),
                Op::UShr => int_op!(
                    |x, y| (to_uint32(x) >> (to_uint32(y) & 31)) as f64,
                    BigOp::UShr
                ),
                Op::Neg => {
                    let a = pop!();
                    let v = match a {
                        Value::Number(n) => Value::Number(-n),
                        _ => match tri!(self.to_numeric(&a)) {
                            Value::BigInt(b) => Value::BigInt(Rc::new(-(*b).clone())),
                            Value::Number(n) => Value::Number(-n),
                            _ => Value::Number(f64::NAN),
                        },
                    };
                    self.stack.push(v);
                }
                Op::RequireObject(name) => {
                    if let Some(v) = self.stack.last() {
                        if v.is_nullish() {
                            let t = if v.is_undefined() {
                                "undefined"
                            } else {
                                "null"
                            };
                            let msg = if name != NO_NAME {
                                let key = cl.proto.consts[name as usize].clone();
                                format!(
                                    "Cannot destructure property '{}' of '{t}' as it is {t}.",
                                    inspect_top(&key)
                                )
                            } else {
                                format!("Cannot destructure '{t}' as it is {t}.")
                            };
                            throw!(self.type_error(msg));
                        }
                    }
                }
                Op::ToNumeric => {
                    let a = pop!();
                    let v = match a {
                        Value::Number(_) => a,
                        _ => tri!(self.to_numeric(&a)),
                    };
                    self.stack.push(v);
                }
                Op::ToNumber => {
                    let a = pop!();
                    let n = match a {
                        Value::Number(n) => n,
                        _ => tri!(self.to_number(&a)),
                    };
                    self.stack.push(Value::Number(n));
                }
                Op::Inc | Op::Dec => {
                    let a = pop!();
                    let v = match a {
                        Value::Number(n) => n,
                        _ => match tri!(self.to_numeric(&a)) {
                            Value::BigInt(b) => {
                                let one = num_bigint::BigInt::from(1);
                                let r = if op == Op::Inc { &*b + one } else { &*b - one };
                                self.stack.push(Value::BigInt(Rc::new(r)));
                                continue;
                            }
                            Value::Number(n) => n,
                            _ => f64::NAN,
                        },
                    };
                    self.stack
                        .push(Value::Number(if op == Op::Inc { v + 1.0 } else { v - 1.0 }));
                }
                Op::Not => {
                    let a = pop!();
                    self.stack.push(Value::Bool(!a.truthy()));
                }
                Op::BitNot => {
                    let a = pop!();
                    let n = match a {
                        Value::Number(n) => n,
                        _ => match tri!(self.to_numeric(&a)) {
                            Value::BigInt(b) => {
                                let r = -(*b).clone() - num_bigint::BigInt::from(1);
                                self.stack.push(Value::BigInt(Rc::new(r)));
                                continue;
                            }
                            Value::Number(n) => n,
                            _ => f64::NAN,
                        },
                    };
                    self.stack.push(Value::Number(!to_int32(n) as f64));
                }
                Op::Eq | Op::Ne => {
                    let b = pop!();
                    let a = pop!();
                    let r = tri!(self.loose_eq(&a, &b));
                    self.stack
                        .push(Value::Bool(if op == Op::Eq { r } else { !r }));
                }
                Op::StrictEq => {
                    let b = pop!();
                    let a = pop!();
                    self.stack.push(Value::Bool(a.strict_eq(&b)));
                }
                Op::StrictNe => {
                    let b = pop!();
                    let a = pop!();
                    self.stack.push(Value::Bool(!a.strict_eq(&b)));
                }
                Op::Lt => cmp_op!(|x, y| x < y, |o: std::cmp::Ordering| o.is_lt()),
                Op::Gt => cmp_op!(|x, y| x > y, |o: std::cmp::Ordering| o.is_gt()),
                Op::Le => cmp_op!(|x, y| x <= y, |o: std::cmp::Ordering| o.is_le()),
                Op::Ge => cmp_op!(|x, y| x >= y, |o: std::cmp::Ordering| o.is_ge()),
                Op::In => {
                    let obj = pop!();
                    let key = pop!();
                    let r = tri!(self.has_property(&obj, &key));
                    self.stack.push(Value::Bool(r));
                }
                Op::InstanceOf => {
                    let c = pop!();
                    let v = pop!();
                    let r = tri!(self.instance_of(&v, &c));
                    self.stack.push(Value::Bool(r));
                }
                Op::Typeof => {
                    let v = pop!();
                    self.stack.push(Value::Str(v.type_of().into()));
                }
                Op::Jump(t) => ip = t as usize,
                Op::JumpIfFalse(t) => {
                    if !pop!().truthy() {
                        ip = t as usize;
                    }
                }
                Op::JumpIfTrue(t) => {
                    if pop!().truthy() {
                        ip = t as usize;
                    }
                }
                Op::JumpIfFalseKeep(t) => {
                    if self.stack.last().is_some_and(|v| !v.truthy()) {
                        ip = t as usize;
                    } else {
                        self.stack.pop();
                    }
                }
                Op::JumpIfTrueKeep(t) => {
                    if self.stack.last().is_some_and(|v| v.truthy()) {
                        ip = t as usize;
                    } else {
                        self.stack.pop();
                    }
                }
                Op::JumpIfNotNullishKeep(t) => {
                    if self.stack.last().is_some_and(|v| !v.is_nullish()) {
                        ip = t as usize;
                    } else {
                        self.stack.pop();
                    }
                }
                Op::JumpIfNullish(t) => {
                    if self.stack.last().is_some_and(|v| v.is_nullish()) {
                        self.stack.pop();
                        self.stack.push(Value::Undefined);
                        ip = t as usize;
                    }
                }
                Op::JumpIfNullishCall(t) => {
                    let n = self.stack.len();
                    if self.stack[n - 2].is_nullish() {
                        self.stack.truncate(n - 2);
                        self.stack.push(Value::Undefined);
                        ip = t as usize;
                    }
                }
                Op::JumpIfNotUndef(t) => {
                    if self.stack.last().is_some_and(|v| !v.is_undefined()) {
                        ip = t as usize;
                    } else {
                        self.stack.pop();
                    }
                }
                Op::Call(argc, name) => {
                    if crate::gc::due() {
                        self.frames[fi].ip = ip;
                        crate::gc::collect();
                    }
                    let argc = argc as usize;
                    let ci = self.stack.len() - argc - 2;
                    self.frames[fi].ip = ip;
                    let callee = match &self.stack[ci] {
                        Value::Object(o) => match &o.kind {
                            ObjKind::Function(c) => Some(c.clone()),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(c) = callee {
                        if c.proto.is_class() {
                            throw!(self.type_error(format!(
                                "Class constructor {} cannot be invoked without 'new'",
                                c.proto.name
                            )));
                        }
                        if !c.proto.is_coroutine() {
                            tri!(self.push_frame(c, argc, false));
                            reload!();
                            continue;
                        }
                    }
                    let mut args = std::mem::take(&mut self.arg_buf);
                    args.extend(self.stack.drain(ci + 2..));
                    let this = pop!();
                    let f = pop!();
                    if !f.is_callable() {
                        let desc = if name != NO_NAME {
                            inspect_top(&cl.proto.consts[name as usize])
                        } else {
                            inspect_plain(&f)
                        };
                        throw!(self.type_error(format!("{desc} is not a function")));
                    }
                    let r = self.call_value(&f, this, &args);
                    args.clear();
                    if self.arg_buf.capacity() < args.capacity() {
                        self.arg_buf = args;
                    }
                    let r = tri!(r);
                    self.stack.push(r);
                }
                Op::CallSpread(name) => {
                    let arr = pop!();
                    let args = tri!(self.iterate(&arr));
                    let this = pop!();
                    let f = pop!();
                    if !f.is_callable() {
                        let desc = if name != NO_NAME {
                            inspect_top(&cl.proto.consts[name as usize])
                        } else {
                            inspect_plain(&f)
                        };
                        throw!(self.type_error(format!("{desc} is not a function")));
                    }
                    let r = tri!(self.call_value(&f, this, &args));
                    self.stack.push(r);
                }
                Op::New(argc, name) => {
                    let argc = argc as usize;
                    let ci = self.stack.len() - argc - 1;
                    self.frames[fi].ip = ip;
                    let callee = self.stack[ci].clone();
                    let fast = match &callee {
                        Value::Object(o) => match &o.kind {
                            ObjKind::Function(c) if c.proto.is_constructor() => {
                                Some((o.clone(), c.clone()))
                            }
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some((o, c)) = fast {
                        let this = if c.proto.kind == ProtoKind::DerivedConstructor {
                            Value::Uninit
                        } else {
                            self.alloc_this(&o)
                        };
                        self.stack.insert(ci + 1, this);
                        tri!(self.push_frame(c, argc, true));
                        reload!();
                        continue;
                    }
                    let args = self.stack.split_off(ci + 1);
                    let f = pop!();
                    if !self.is_constructor(&f) {
                        throw!(self.not_constructor(&f, name, Some(cl.proto.consts.as_slice())));
                    }
                    let r = tri!(self.construct(&f, &args));
                    self.stack.push(r);
                }
                Op::NewSpread(name) => {
                    let arr = pop!();
                    let args = tri!(self.iterate(&arr));
                    let f = pop!();
                    if !f.is_callable() {
                        throw!(self.not_constructor(&f, name, Some(cl.proto.consts.as_slice())));
                    }
                    let r = tri!(self.construct(&f, &args));
                    self.stack.push(r);
                }
                Op::Return => {
                    let result = pop!();
                    let result = if self.frames[fi].construct && !result.is_object() {
                        let derived = proto.kind == ProtoKind::DerivedConstructor;
                        if derived && !result.is_undefined() {
                            throw!(self.type_error(
                                "Derived constructors may only return object or undefined"
                            ));
                        }
                        let this = self.stack[base].clone();
                        if derived && matches!(this, Value::Uninit) {
                            throw!(self.make_error(
                                ErrorKind::ReferenceError,
                                "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                            ));
                        }
                        this
                    } else {
                        result
                    };
                    let frame = self.frames.pop().expect("frame");
                    self.close_upvalues(frame.base);
                    while self
                        .handlers
                        .last()
                        .is_some_and(|h| h.frame >= self.frames.len())
                    {
                        self.handlers.pop();
                    }
                    self.prune_builders(frame.base - 1);
                    self.stack.truncate(frame.base - 1);
                    if self.frames.len() == stop {
                        return Ok(result);
                    }
                    self.stack.push(result);
                    reload!();
                }
                Op::Closure(i) => {
                    if crate::gc::due() {
                        self.frames[fi].ip = ip;
                        crate::gc::collect();
                    }
                    let p = cl.proto.functions[i as usize].clone();
                    let mut ups = Vec::with_capacity(p.upvalues.len());
                    for u in &p.upvalues {
                        if u.is_local {
                            self.flush_builder(base + u.index as usize);
                            ups.push(self.capture_upvalue(base + u.index as usize));
                        } else {
                            ups.push(cl.upvalues[u.index as usize].clone());
                        }
                    }
                    let (home, nt) = if p.kind == ProtoKind::Arrow {
                        (
                            cl.home.borrow().clone(),
                            Some(self.frame_new_target(fi, base, &cl)),
                        )
                    } else {
                        (None, None)
                    };
                    let c = Rc::new(Closure {
                        gc_slot: std::cell::Cell::new(u32::MAX),
                        proto: p,
                        upvalues: ups.into_boxed_slice(),
                        home: RefCell::new(home),
                        new_target: RefCell::new(nt),
                    });
                    let fproto = match (c.proto.is_generator, c.proto.is_async) {
                        (true, true) => &self.realm.async_generator_function_proto,
                        (true, false) => &self.realm.generator_function_proto,
                        (false, true) => &self.realm.async_function_proto,
                        _ => &self.realm.function_proto,
                    };
                    let o = Obj::new(ObjKind::Function(c), Some(fproto.clone()));
                    self.stack.push(Value::Object(o));
                }
                Op::PopTo(n) => {
                    let to = base + n as usize;
                    self.close_upvalues(to);
                    self.prune_builders(to);
                    self.stack.truncate(to);
                }
                Op::CloseUpvalues(n) => {
                    self.close_upvalues(base + n as usize);
                }
                Op::Collapse(n) => {
                    let top = pop!();
                    let to = base + n as usize;
                    self.close_upvalues(to);
                    self.prune_builders(to);
                    self.stack.truncate(to);
                    self.stack.push(top);
                }
                Op::NewArray(n) => {
                    let items = self.stack.split_off(self.stack.len() - n as usize);
                    let a = self.new_array(items);
                    self.stack.push(a);
                }
                Op::ArrayPush => {
                    let v = pop!();
                    if let Some(Value::Object(o)) = self.stack.last() {
                        if let ObjKind::Array(a) = &o.kind {
                            a.borrow_mut().push(v);
                        }
                    }
                }
                Op::ArrayHole => {
                    if let Some(Value::Object(o)) = self.stack.last() {
                        if let ObjKind::Array(a) = &o.kind {
                            let i = a.borrow().len();
                            a.borrow_mut().push(Value::Undefined);
                            o.mark_holes(i, i + 1);
                        }
                    }
                }
                Op::ArraySpread => {
                    let src = pop!();
                    let items = tri!(self.iterate(&src));
                    if let Some(Value::Object(o)) = self.stack.last() {
                        if let ObjKind::Array(a) = &o.kind {
                            a.borrow_mut().extend(items);
                        }
                    }
                }
                Op::NewObject => {
                    if crate::gc::due() {
                        self.frames[fi].ip = ip;
                        crate::gc::collect();
                    }
                    let o = self.new_object();
                    self.stack.push(Value::Object(o));
                }
                Op::InitProp(k) => {
                    let v = pop!();
                    if let Some(Value::Object(o)) = self.stack.last() {
                        o.props.borrow_mut().insert(name!(k), v);
                    }
                }
                Op::InitMethod(k) => {
                    let v = pop!();
                    if let Some(Value::Object(o)) = self.stack.last() {
                        if let Value::Object(f) = &v {
                            if let ObjKind::Function(c) = &f.kind {
                                *c.home.borrow_mut() = Some(o.clone());
                            }
                        }
                        o.props.borrow_mut().insert(name!(k), v);
                    }
                }
                Op::InitMethodComputed => {
                    let v = pop!();
                    let key = pop!();
                    let k = tri!(self.to_property_key(&key));
                    name_anonymous(&v, &k, "");
                    if let Some(Value::Object(o)) = self.stack.last() {
                        if let Value::Object(f) = &v {
                            if let ObjKind::Function(c) = &f.kind {
                                *c.home.borrow_mut() = Some(o.clone());
                            }
                        }
                        o.props.borrow_mut().insert(k, v);
                    }
                }
                Op::InitProto => {
                    let v = pop!();
                    if let Some(Value::Object(o)) = self.stack.last() {
                        match v {
                            Value::Object(p) => *o.proto.borrow_mut() = Some(p),
                            Value::Null => *o.proto.borrow_mut() = None,
                            _ => {}
                        }
                    }
                }
                Op::NewTarget => {
                    let v = self.frame_new_target(fi, base, &cl);
                    self.stack.push(v);
                }
                Op::DirectEval(argc, idx) => {
                    let args = if argc == u32::MAX {
                        let arr = pop!();
                        tri!(self.iterate(&arr))
                    } else {
                        self.stack.split_off(self.stack.len() - argc as usize)
                    };
                    pop!();
                    let callee = pop!();
                    let intrinsic = matches!((&callee, &self.eval_fn), (Value::Object(a), Some(b)) if Rc::ptr_eq(a, b));
                    let v = if !intrinsic {
                        tri!(self.call_value(&callee, Value::Undefined, &args))
                    } else {
                        match args.first() {
                            Some(Value::Str(src)) => {
                                let scope = proto.evals[idx as usize].clone();
                                let nt = if scope.ctx.field_init {
                                    Value::Undefined
                                } else {
                                    self.frame_new_target(fi, base, &cl)
                                };
                                let c = cl.clone();
                                tri!(self.direct_eval(src, &scope, &c, base, nt))
                            }
                            Some(v) => v.clone(),
                            None => Value::Undefined,
                        }
                    };
                    self.stack.push(v);
                }
                Op::TemplateCache(site, t) => {
                    let cached = proto.templates[site as usize].borrow().clone();
                    if let Some(v) = cached {
                        self.stack.push(v);
                        ip = t as usize;
                    }
                }
                Op::TemplateStore(site) => {
                    let v = peek!();
                    if let Value::Object(o) = &v {
                        if let Some(Value::Object(raw)) = o.get_own("raw") {
                            let _ = self.freeze(&raw);
                        }
                        let _ = self.freeze(o);
                    }
                    *proto.templates[site as usize].borrow_mut() = Some(v);
                }
                Op::ModuleNamespace => {
                    let ns = match self.pending_ns.take() {
                        Some(ns) => ns,
                        None => self.new_namespace(),
                    };
                    self.stack.push(ns);
                }
                Op::ImportMeta => {
                    let module = cl.proto.module.clone();
                    let meta = tri!(self.import_meta(&module));
                    self.stack.push(meta);
                }
                Op::DynImport => {
                    let spec = pop!();
                    let module = cl.proto.module.clone();
                    let p = self.new_promise();
                    let r = self
                        .to_string(&spec)
                        .and_then(|s| self.import_module(&s, &module));
                    match r {
                        Ok(ns) => self.resolve_promise(&p, ns),
                        Err(e) => self.reject_promise(&p, e),
                    }
                    self.stack.push(Value::Object(p));
                }
                Op::IterClose(slot) => {
                    let it = self.stack[base + slot as usize].clone();
                    tri!(self.iter_close(&it));
                }
                Op::IterCloseTop => {
                    let it = pop!();
                    tri!(self.iter_close(&it));
                }
                Op::IterCloseQuiet(slot) => {
                    let it = self.stack[base + slot as usize].clone();
                    let _ = self.iter_close(&it);
                }
                Op::IterCloseUnder => {
                    let exc = pop!();
                    let it = pop!();
                    let _ = self.iter_close(&it);
                    self.stack.push(exc);
                }
                Op::TryBeginTop(t) => {
                    self.handlers.push(Handler {
                        frame: fi,
                        stack_len: self.stack.len(),
                        catch_ip: t as usize,
                        guard: true,
                    });
                }
                Op::Pick(d) => {
                    let n = self.stack.len();
                    let v = self.stack[n - 1 - d as usize].clone();
                    self.stack.push(v);
                }
                Op::WithHas(k) => {
                    let obj = pop!();
                    let n = name!(k);
                    let has = tri!(self.with_has(&obj, &n));
                    self.stack.push(Value::Bool(has));
                }
                Op::WithGet(k) => {
                    let obj = pop!();
                    let n = name!(k);
                    let v = tri!(self.with_get(&obj, &n, proto.sloppy));
                    self.stack.push(v);
                }
                Op::WithSet(k) => {
                    let obj = pop!();
                    let v = peek!();
                    let n = name!(k);
                    tri!(self.with_set(&obj, n, v, proto.sloppy));
                }
                Op::ToObject => {
                    let v = pop!();
                    let o = tri!(self.to_object(&v));
                    self.stack.push(Value::Object(o));
                }
                Op::RequireUninit => {
                    let raw = pop!();
                    if !matches!(raw, Value::Uninit) {
                        throw!(self.make_error(
                            ErrorKind::ReferenceError,
                            "Super constructor may only be called once",
                        ));
                    }
                }
                Op::YieldReturn(t) => {
                    if self.gen_return_pending {
                        self.gen_return_pending = false;
                        tri!(self.close_guards(fi));
                    } else {
                        ip = t as usize;
                    }
                }
                Op::SetHome => {
                    let n = self.stack.len();
                    if let (Value::Object(h), Value::Object(f)) =
                        (&self.stack[n - 2], &self.stack[n - 1])
                    {
                        if let ObjKind::Function(c) = &f.kind {
                            *c.home.borrow_mut() = Some(h.clone());
                        }
                    }
                }
                Op::InitHidden(k) => {
                    let v = pop!();
                    if let Some(Value::Object(o)) = self.stack.last() {
                        o.set_hidden(name!(k), v);
                    }
                }
                Op::InitIndex => {
                    let v = pop!();
                    let key = pop!();
                    let k = tri!(self.to_property_key(&key));
                    name_anonymous(&v, &k, "");
                    if let Some(Value::Object(o)) = self.stack.last() {
                        o.props.borrow_mut().insert(k, v);
                    }
                }
                Op::ObjSpread => {
                    let src = pop!();
                    let pairs = tri!(self.copy_data_entries(&src, &[]));
                    if let Some(Value::Object(o)) = self.stack.last() {
                        let mut props = o.props.borrow_mut();
                        for (k, v) in pairs {
                            props.insert(k, v);
                        }
                    }
                }
                Op::ObjRest(n, saved) => {
                    let mut keys = self.stack.split_off(self.stack.len() - n as usize);
                    let src = pop!();
                    if saved > 0 {
                        let orig = pop!();
                        let extra = self.stack.split_off(self.stack.len() - saved as usize);
                        self.stack.push(orig);
                        keys.extend(extra);
                    }
                    let o = tri!(self.obj_rest(&src, &keys));
                    self.stack.push(o);
                }
                Op::MakeClass(k, has_super) => {
                    let ctor = pop!();
                    let sup = if has_super { Some(pop!()) } else { None };
                    let r = self.make_class(&ctor, sup, name!(k));
                    tri!(r);
                    self.stack.push(ctor);
                }
                Op::DefineMethod(k, is_static) => {
                    let m = pop!();
                    let class = peek!();
                    tri!(self.define_method(&class, name!(k), m, is_static));
                }
                Op::DefineMethodComputed(is_static) => {
                    let m = pop!();
                    let key = pop!();
                    let class = peek!();
                    let k = tri!(self.to_property_key(&key));
                    name_anonymous(&m, &k, "");
                    tri!(self.define_method(&class, k, m, is_static));
                }
                Op::GetIter => {
                    let v = pop!();
                    let it = tri!(self.get_iterator(&v));
                    self.stack.push(it);
                }
                Op::ForInIter => {
                    let v = pop!();
                    let keys = self.for_in_keys(&v);
                    let it = if v.is_object() {
                        self.new_iter(IterState::ForIn(v, keys, 0))
                    } else {
                        self.new_iter(IterState::List(keys, 0))
                    };
                    self.stack.push(it);
                }
                Op::IterNext(t) => {
                    let it = pop!();
                    match tri!(self.iter_next(&it)) {
                        Some(v) => self.stack.push(v),
                        None => ip = t as usize,
                    }
                }
                Op::IterNextOrUndef => {
                    let it = pop!();
                    let v = tri!(self.iter_next(&it)).unwrap_or_default();
                    self.stack.push(v);
                }
                Op::IterRest => {
                    let it = pop!();
                    let items = tri!(self.drain_iterator(&it));
                    let a = self.new_array(items);
                    self.stack.push(a);
                }
                Op::Template(n) => {
                    let parts = self.stack.split_off(self.stack.len() - n as usize);
                    let mut s = String::new();
                    for p in &parts {
                        match p {
                            Value::Str(x) => crate::value::push16(&mut s, x),
                            _ => {
                                let x = tri!(self.to_string(p));
                                crate::value::push16(&mut s, &x);
                            }
                        }
                    }
                    self.stack.push(Value::Str(s.into()));
                }
                Op::Throw => {
                    let e = pop!();
                    throw!(e);
                }
                Op::TryBegin(t, slot) => {
                    self.handlers.push(Handler {
                        frame: fi,
                        stack_len: base + slot as usize,
                        catch_ip: t as usize,
                        guard: false,
                    });
                }
                Op::TryEnd => {
                    self.handlers.pop();
                }
                Op::AsyncIterReturn(slot) | Op::AsyncIterReturnQuiet(slot) => {
                    let quiet = matches!(op, Op::AsyncIterReturnQuiet(_));
                    let it = self.stack[base + slot as usize].clone();
                    let (r, called) = tri!(self.async_iter_return(it, quiet));
                    self.stack.push(r);
                    self.stack.push(Value::Bool(called));
                }
                Op::CheckIterResult => {
                    if !self.stack.last().is_some_and(|v| v.is_object()) {
                        throw!(self.type_error("Iterator result is not an object"));
                    }
                }
                Op::ThrowRef(k) => {
                    throw!(self.make_error_at(
                        ErrorKind::ReferenceError,
                        name!(k).to_string(),
                        fi,
                        ip
                    ));
                }
                Op::ThrowConstAssign(_) => {
                    throw!(self.make_error_at(
                        ErrorKind::TypeError,
                        "Assignment to constant variable.".to_string(),
                        fi,
                        ip
                    ));
                }
                Op::Import(k) => {
                    let spec = name!(k);
                    let module = cl.proto.module.clone();
                    let ns = tri!(self.import_module(&spec, &module));
                    self.stack.push(ns);
                }
                Op::GetCallee => {
                    let f = self.stack[base - 1].clone();
                    self.stack.push(f);
                }
                Op::Await | Op::Yield => {
                    let v = pop!();
                    self.frames[fi].ip = ip;
                    if !self.suspend_frame(fi) {
                        throw!(self.type_error("Cannot suspend outside of a coroutine"));
                    }
                    self.co_signal = Some(if op == Op::Await {
                        Signal::Await
                    } else {
                        Signal::Yield
                    });
                    return Ok(v);
                }
                Op::CmpJump(cmp, t) => {
                    let b = pop!();
                    let a = pop!();
                    let ok = match (&a, &b) {
                        (Value::Number(x), Value::Number(y)) => match cmp {
                            Cmp::Lt => x < y,
                            Cmp::Le => x <= y,
                            Cmp::Gt => x > y,
                            Cmp::Ge => x >= y,
                            Cmp::StrictEq | Cmp::Eq => x == y,
                            Cmp::StrictNe | Cmp::Ne => x != y,
                        },
                        _ => match cmp {
                            Cmp::StrictEq => a.strict_eq(&b),
                            Cmp::StrictNe => !a.strict_eq(&b),
                            Cmp::Eq => tri!(self.loose_eq(&a, &b)),
                            Cmp::Ne => !tri!(self.loose_eq(&a, &b)),
                            _ => match tri!(self.compare(&a, &b)) {
                                Some(o) => match cmp {
                                    Cmp::Lt => o.is_lt(),
                                    Cmp::Le => o.is_le(),
                                    Cmp::Gt => o.is_gt(),
                                    _ => o.is_ge(),
                                },
                                None => false,
                            },
                        },
                    };
                    if !ok {
                        ip = t as usize;
                    }
                }
                Op::IncLocal(s) | Op::DecLocal(s) => {
                    let slot = base + s as usize;
                    self.flush_builder(slot);
                    let delta = if matches!(op, Op::IncLocal(_)) {
                        1.0
                    } else {
                        -1.0
                    };
                    if let Value::Number(n) = &mut self.stack[slot] {
                        *n += delta;
                    } else {
                        let v = self.stack[slot].clone();
                        match tri!(self.to_numeric(&v)) {
                            Value::BigInt(b) => {
                                let r = &*b + num_bigint::BigInt::from(delta as i64);
                                self.stack[slot] = Value::BigInt(Rc::new(r));
                            }
                            other => {
                                let n = other.to_primitive_number().unwrap_or(f64::NAN);
                                self.stack[slot] = Value::Number(n + delta);
                            }
                        }
                    }
                }
                Op::StoreLocal(s) => {
                    let v = pop!();
                    self.drop_builder(base + s as usize);
                    self.stack[base + s as usize] = v;
                }
                Op::StoreUpval(i) => {
                    let v = pop!();
                    let mut u = cl.upvalues[i as usize].borrow_mut();
                    match &mut *u {
                        Upvalue::Open(idx) => self.stack[*idx] = v,
                        Upvalue::Closed(c) => *c = v,
                    }
                }
                Op::StoreProp(k) => {
                    let val = pop!();
                    let obj = pop!();
                    let done = match &obj {
                        Value::Object(o)
                            if matches!(o.kind, ObjKind::Ordinary)
                                && o.flags.get() == 0
                                && !self.accessors_used =>
                        {
                            o.props.borrow_mut().insert_fast(
                                name!(k),
                                val.clone(),
                                !self.readonly_used,
                            )
                        }
                        _ => false,
                    };
                    if !done {
                        self.silent_fail = false;
                        if let Err(e) = self.set_prop(&obj, name!(k), val) {
                            if !(proto.sloppy && self.silent_fail) {
                                throw!(e);
                            }
                        }
                    }
                }
                Op::GetAsyncIter => {
                    let v = pop!();
                    let it = tri!(self.get_async_iterator(&v));
                    self.stack.push(it);
                }
                Op::NewRegex(src, flags) => {
                    let source = name!(src);
                    let flags = name!(flags);
                    let re = tri!(builtins::make_regexp(self, &source, &flags));
                    self.stack.push(re);
                }
                Op::InitAccessor(k, setter) => {
                    let f = pop!();
                    if let Some(Value::Object(o)) = self.stack.last() {
                        let o = o.clone();
                        if let Value::Object(func) = &f {
                            if let ObjKind::Function(c) = &func.kind {
                                *c.home.borrow_mut() = Some(o.clone());
                            }
                        }
                        self.define_accessor(&o, name!(k), f, setter);
                    }
                }
                Op::InitAccessorComputed(setter) => {
                    let f = pop!();
                    let key = pop!();
                    let k = tri!(self.to_property_key(&key));
                    name_anonymous(&f, &k, if setter { "set " } else { "get " });
                    if let Some(Value::Object(o)) = self.stack.last() {
                        let o = o.clone();
                        if let Value::Object(func) = &f {
                            if let ObjKind::Function(c) = &func.kind {
                                *c.home.borrow_mut() = Some(o.clone());
                            }
                        }
                        self.define_accessor(&o, k, f, setter);
                    }
                }
                Op::DefineAccessor(k, setter, is_static) => {
                    let f = pop!();
                    let class = peek!();
                    tri!(self.define_class_accessor(&class, name!(k), f, setter, is_static));
                }
                Op::DefineAccessorComputed(setter, is_static) => {
                    let f = pop!();
                    let key = pop!();
                    let class = peek!();
                    let k = tri!(self.to_property_key(&key));
                    name_anonymous(&f, &k, if setter { "set " } else { "get " });
                    tri!(self.define_class_accessor(&class, k, f, setter, is_static));
                }
                Op::ExportAll => {
                    let ns = pop!();
                    let pairs = self.own_entries(&ns);
                    if let Some(Value::Object(o)) = self.stack.last() {
                        let mut props = o.props.borrow_mut();
                        for (k, v) in pairs {
                            if &*k != "default" {
                                props.insert(k, v);
                            }
                        }
                    }
                }
            }
        }
    }

    fn make_error_at(&mut self, kind: ErrorKind, msg: String, fi: usize, ip: usize) -> Value {
        self.frames[fi].ip = ip;
        self.make_error(kind, msg)
    }

    pub fn own_entries(&self, v: &Value) -> Vec<(JsStr, Value)> {
        self.own_entries_raw(v, false)
    }

    #[inline(never)]
    fn close_guards(&mut self, fi: usize) -> Result<(), Value> {
        while self
            .handlers
            .last()
            .is_some_and(|h| h.frame == fi && h.guard)
        {
            let h = self.handlers.pop().expect("handler");
            let it = self.stack[h.stack_len - 1].clone();
            self.iter_close(&it)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn with_has(&mut self, obj: &Value, n: &str) -> Result<bool, Value> {
        let mut has = match obj {
            Value::Object(o) => self.op_has_property(o, n)?,
            _ => false,
        };
        if has {
            let u = self.get_prop(obj, crate::object::UNSCOPABLES_KEY)?;
            if u.is_object() {
                let blocked = self.get_prop(&u, n)?;
                if blocked.truthy() {
                    has = false;
                }
            }
        }
        Ok(has)
    }

    #[inline(never)]
    fn with_get(&mut self, obj: &Value, n: &str, sloppy: bool) -> Result<Value, Value> {
        let has = match obj {
            Value::Object(o) => self.op_has_property(o, n)?,
            _ => false,
        };
        if !has {
            if sloppy {
                return Ok(Value::Undefined);
            }
            return Err(self.make_error(ErrorKind::ReferenceError, format!("{n} is not defined")));
        }
        self.get_prop(obj, n)
    }

    #[inline(never)]
    fn with_set(&mut self, obj: &Value, n: JsStr, v: Value, sloppy: bool) -> Result<(), Value> {
        let Value::Object(o) = obj else {
            return Ok(());
        };
        let has = self.op_has_property(o, &n)?;
        if !has && !sloppy {
            return Err(self.make_error(ErrorKind::ReferenceError, format!("{n} is not defined")));
        }
        self.silent_fail = false;
        if let Err(e) = self.set_prop(obj, n, v) {
            if !(sloppy && self.silent_fail) {
                return Err(e);
            }
        }
        Ok(())
    }

    #[inline(never)]
    fn can_declare_global_fn(&mut self, n: &str) -> Result<(), Value> {
        match self.globals.get(n) {
            Some(raw) => {
                let a = self.globals.attrs_of(n);
                if a & crate::object::NON_CONFIG != 0
                    && (a & (crate::object::READ_ONLY | crate::object::NON_ENUM) != 0
                        || crate::props::accessor_parts(raw).is_some())
                {
                    return Err(self.type_error(format!(
                        "Cannot redefine non-configurable global property '{n}'"
                    )));
                }
            }
            None => {
                if !self.global_extensible() {
                    return Err(self.type_error(format!(
                        "Cannot define global function '{n}', object is not extensible"
                    )));
                }
            }
        }
        Ok(())
    }

    fn global_extensible(&self) -> bool {
        match self.global_this() {
            Value::Object(o) => o.is_extensible(),
            _ => true,
        }
    }

    fn redeclaration_error(&mut self, n: &str) -> Value {
        let msg = format!("Identifier '{n}' has already been declared");
        self.make_error(ErrorKind::SyntaxError, msg)
    }

    #[inline(never)]
    fn check_global_lex(&mut self, n: &str) -> Result<(), Value> {
        if self.globals.has_lex(n)
            || (self.globals.slot(n).is_some()
                && self.globals.attrs_of(n) & crate::object::NON_CONFIG != 0)
        {
            return Err(self.redeclaration_error(n));
        }
        Ok(())
    }

    #[inline(never)]
    fn check_global_var(&mut self, n: &str) -> Result<(), Value> {
        if self.globals.has_lex(n) {
            return Err(self.redeclaration_error(n));
        }
        if self.globals.slot(n).is_none() && !self.load_lazy_global(n)? && !self.global_extensible()
        {
            return Err(self.type_error(format!(
                "Cannot define global variable '{n}', object is not extensible"
            )));
        }
        Ok(())
    }

    #[inline(never)]
    fn delete_global_binding(&mut self, n: &str) -> bool {
        if self.globals.has_lex(n) {
            return false;
        }
        match self.globals.slot(n) {
            None => true,
            Some(_) if self.globals.attrs_of(n) & crate::object::NON_CONFIG != 0 => false,
            Some(_) => self.globals.remove(n),
        }
    }

    #[inline(never)]
    fn define_global_binding(&mut self, n: JsStr, v: Value, flags: u8) -> Result<(), Value> {
        if flags & crate::bytecode::DG_LEXICAL != 0 {
            let is_const = flags & crate::bytecode::DG_CONST != 0;
            if self.globals.slot(&n).is_some() {
                self.global_accessors = true;
            }
            self.globals.declare_lex(n, v, is_const);
            return Ok(());
        }
        if flags & crate::bytecode::DG_KEEP != 0
            && (self.globals.slot(&n).is_some() || self.load_lazy_global(&n)?)
        {
            return Ok(());
        }
        let old = self.globals.attrs_of(&n);
        let exists = self.globals.slot(&n).is_some();
        let base = if flags & crate::bytecode::DG_FUNCTION != 0 {
            old & !(crate::object::NON_ENUM | crate::object::READ_ONLY)
        } else {
            old
        };
        let attrs = if exists && old & crate::object::NON_CONFIG != 0 {
            old
        } else if flags & crate::bytecode::DG_CONFIGURABLE != 0 {
            base & !crate::object::NON_CONFIG
        } else {
            base | crate::object::NON_CONFIG
        };
        self.globals.insert(n.clone(), v);
        self.globals.attrs.insert(n, attrs);
        Ok(())
    }

    #[inline(never)]
    fn async_iter_return(&mut self, it: Value, quiet: bool) -> Result<(Value, bool), Value> {
        let called = match self.get_prop(&it, "return") {
            Ok(ret) if ret.is_nullish() => Ok(None),
            Ok(ret) => self.call_value(&ret, it, &[]).map(Some),
            Err(e) => Err(e),
        };
        match called {
            Ok(Some(r)) => Ok((r, true)),
            Ok(None) => Ok((Value::Undefined, false)),
            Err(_) if quiet => Ok((Value::Undefined, false)),
            Err(e) => Err(e),
        }
    }

    #[inline(never)]
    fn obj_rest(&mut self, src: &Value, keys: &[Value]) -> Result<Value, Value> {
        let excluded: Vec<JsStr> = keys
            .iter()
            .filter_map(|k| match k {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        let o = self.new_object();
        let pairs = self.copy_data_entries(src, &excluded)?;
        for (k, v) in pairs {
            if !excluded.contains(&k) {
                o.set_own(k, v);
            }
        }
        Ok(Value::Object(o))
    }

    pub fn copy_data_entries(
        &mut self,
        src: &Value,
        excluded: &[JsStr],
    ) -> Result<Vec<(JsStr, Value)>, Value> {
        let Value::Object(o) = src else {
            return self.own_entries_resolved(src, true);
        };
        if !matches!(o.kind, ObjKind::Proxy(_)) {
            return self.own_entries_resolved(src, true);
        }
        let keys = self.op_own_keys(o)?;
        let mut out = Vec::new();
        for k in keys {
            if excluded.contains(&k) {
                continue;
            }
            let Some(d) = self.op_get_own_property(o, &k)? else {
                continue;
            };
            if d.enumerable != Some(true) {
                continue;
            }
            let v = self.op_get(o, &k, src)?;
            out.push((k, v));
        }
        Ok(out)
    }

    pub fn own_entries_resolved(
        &mut self,
        v: &Value,
        include_symbols: bool,
    ) -> Result<Vec<(JsStr, Value)>, Value> {
        let raw = self.own_entries_raw(v, include_symbols);
        let mut out = Vec::with_capacity(raw.len());
        for (k, x) in raw {
            let x = self.resolve_accessor(x, v)?;
            out.push((k, x));
        }
        Ok(out)
    }

    pub fn own_entries_raw(&self, v: &Value, include_symbols: bool) -> Vec<(JsStr, Value)> {
        let mut out = Vec::new();
        if let Value::Object(o) = v {
            self.sync_arguments(o);
            match &o.kind {
                ObjKind::Global => {
                    for k in self.globals.names() {
                        if self.globals.attrs_of(&k) & crate::object::NON_ENUM == 0 {
                            let x = self.globals.get(&k).cloned().unwrap_or_default();
                            out.push((k, x));
                        }
                    }
                    return out;
                }
                ObjKind::Proxy(p) => {
                    if let Some(t) = p.target.borrow().clone() {
                        return self.own_entries_raw(&Value::Object(t), include_symbols);
                    }
                    return out;
                }
                _ => {}
            }
        }
        match v {
            Value::Object(o) => {
                if let ObjKind::Array(a) = &o.kind {
                    for (i, x) in a.borrow().iter().enumerate() {
                        if !o.is_hole(i) {
                            out.push((JsStr::from(i.to_string()), x.clone()));
                        }
                    }
                }
                if let ObjKind::Primitive(Value::Str(s)) = &o.kind {
                    for i in 0..len16(s) {
                        let c = char_at16(s, i).unwrap_or_default();
                        out.push((JsStr::from(i.to_string()), Value::from(c)));
                    }
                }
                if let ObjKind::Typed(t) = &o.kind {
                    for (i, x) in t.values().into_iter().enumerate() {
                        out.push((JsStr::from(i.to_string()), x));
                    }
                }
                for (k, x) in o.props.borrow().enumerable() {
                    if (include_symbols && !crate::object::is_private_key(k)) || !is_symbol_key(k) {
                        out.push((k.clone(), x.clone()));
                    }
                }
            }
            Value::Str(s) => {
                for (i, c) in s.chars().enumerate() {
                    out.push((JsStr::from(i.to_string()), Value::Str(c.to_string().into())));
                }
            }
            _ => {}
        }
        out
    }

    pub fn delete_prop(&mut self, obj: &Value, key: &str) -> Result<bool, Value> {
        match obj {
            Value::Object(o) => {
                if matches!(o.kind, ObjKind::Proxy(_) | ObjKind::Global) {
                    if self.op_delete(o, key)? {
                        return Ok(true);
                    }
                } else if self.try_delete(o, key) {
                    return Ok(true);
                }
                let target = match &o.kind {
                    ObjKind::Array(_) => "[object Array]".to_string(),
                    _ => format!(
                        "#<{}>",
                        crate::inspect::class_name(o)
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "Object".to_string())
                    ),
                };
                Err(self.assign_error(format!(
                    "Cannot delete property '{}' of {target}",
                    crate::props::key_text(key)
                )))
            }
            Value::Undefined | Value::Null => Err(self.type_error(format!(
                "Cannot convert undefined or null to object (deleting '{key}')"
            ))),
            _ => Ok(true),
        }
    }

    fn make_class(&mut self, ctor: &Value, sup: Option<Value>, name: JsStr) -> Result<(), Value> {
        let Value::Object(ctor_obj) = ctor else {
            return Ok(());
        };
        let (proto_parent, ctor_parent) = match sup {
            None => (
                Some(self.realm.object_proto.clone()),
                self.realm.function_proto.clone(),
            ),
            Some(Value::Null) => (None, self.realm.function_proto.clone()),
            Some(Value::Object(s)) if self.is_constructor(&Value::Object(s.clone())) => {
                let sp = self.get_prop(&Value::Object(s.clone()), "prototype")?;
                let pp = match sp {
                    Value::Object(p) => Some(p),
                    Value::Null => None,
                    _ => {
                        return Err(self.type_error(
                            "Class extends value does not have valid prototype property",
                        ));
                    }
                };
                (pp, s)
            }
            Some(other) => {
                return Err(self.type_error(format!(
                    "Class extends value {} is not a constructor or null",
                    inspect_plain(&other)
                )));
            }
        };
        let prototype = Obj::new(ObjKind::Ordinary, proto_parent);
        prototype.set_hidden("constructor", ctor.clone());
        if !name.is_empty() {
            ctor_obj.set_with_attrs(
                "name",
                Value::Str(name),
                crate::object::NON_ENUM | crate::object::READ_ONLY,
            );
        }
        ctor_obj.set_with_attrs(
            "prototype",
            Value::Object(prototype),
            crate::object::NON_ENUM | crate::object::READ_ONLY | crate::object::NON_CONFIG,
        );
        *ctor_obj.proto.borrow_mut() = Some(ctor_parent);
        if let ObjKind::Function(c) = &ctor_obj.kind {
            *c.home.borrow_mut() = Some(ctor_obj.clone());
        }
        Ok(())
    }

    fn define_method(
        &mut self,
        class: &Value,
        key: JsStr,
        method: Value,
        is_static: bool,
    ) -> Result<(), Value> {
        let Value::Object(c) = class else {
            return Ok(());
        };
        if is_static && &*key == "prototype" {
            return Err(self.type_error("Cannot redefine property: prototype"));
        }
        let target = if is_static {
            c.clone()
        } else {
            match self.function_prototype(c) {
                Some(p) => p,
                None => return Err(self.type_error("Class has no prototype")),
            }
        };
        if let Value::Object(m) = &method {
            if let ObjKind::Function(mc) = &m.kind {
                *mc.home.borrow_mut() = Some(target.clone());
            }
        }
        if crate::object::is_private_key(&key) {
            target.set_with_attrs(
                key,
                method,
                crate::object::NON_ENUM | crate::object::READ_ONLY,
            );
        } else {
            target.set_hidden(key, method);
        }
        Ok(())
    }

    fn define_class_accessor(
        &mut self,
        class: &Value,
        key: JsStr,
        f: Value,
        setter: bool,
        is_static: bool,
    ) -> Result<(), Value> {
        let Value::Object(c) = class else {
            return Ok(());
        };
        if is_static && &*key == "prototype" {
            return Err(self.type_error("Cannot redefine property: prototype"));
        }
        let target = if is_static {
            c.clone()
        } else {
            match self.function_prototype(c) {
                Some(p) => p,
                None => return Err(self.type_error("Class has no prototype")),
            }
        };
        if let Value::Object(m) = &f {
            if let ObjKind::Function(mc) = &m.kind {
                *mc.home.borrow_mut() = Some(target.clone());
            }
        }
        self.define_accessor(&target, key.clone(), f, setter);
        target.props.borrow_mut().set_enumerable(&key, false);
        Ok(())
    }

    pub fn random(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn map_key(v: &Value) -> MapKey {
        MapKey::from_value(v)
    }
}

fn dirname_of(module: &ModuleInfo) -> String {
    module::display_path_abs(&module.dir)
}

fn meta_resolve(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    let dir = vm.to_string(&args.first().cloned().unwrap_or_default())?;
    let spec = vm.to_string(&args.get(1).cloned().unwrap_or_default())?;
    if spec.starts_with("node:") || spec.starts_with("squirt:") {
        return Ok(Value::Str(spec));
    }
    match module::resolve(&spec, Path::new(&*dir)) {
        Ok(p) => Ok(Vm::module_file_url(&p).into()),
        Err(msg) => Err(vm.make_error(ErrorKind::Error, msg)),
    }
}

fn name_anonymous(f: &Value, key: &str, prefix: &str) {
    let Value::Object(o) = f else {
        return;
    };
    let anonymous = match &o.kind {
        ObjKind::Function(c) => c.proto.name.is_empty() && !o.has_own("name"),
        _ => false,
    };
    if !anonymous {
        return;
    }
    let base = if let Some(rest) = key.strip_prefix("\u{0}#") {
        format!("#{}", rest.split('@').next().unwrap_or(rest))
    } else if crate::object::is_symbol_key(key) {
        let d = crate::object::symbol_key_description(key);
        if d.is_empty() {
            String::new()
        } else {
            format!("[{d}]")
        }
    } else {
        key.to_string()
    };
    o.set_with_attrs(
        "name",
        Value::Str(format!("{prefix}{base}").into()),
        crate::object::NON_ENUM | crate::object::READ_ONLY,
    );
}

fn private_label(key: &str) -> String {
    let body = key.strip_prefix('\u{0}').unwrap_or(key);
    body.rsplit_once('@').map_or(body, |(a, _)| a).to_string()
}

pub(crate) fn proxy_in_chain(o: &Rc<Obj>) -> bool {
    let mut cur = o.proto.borrow().clone();
    while let Some(p) = cur {
        if matches!(p.kind, ObjKind::Proxy(_)) {
            return true;
        }
        cur = p.proto.borrow().clone();
    }
    false
}

fn function_proto_call(_vm: &mut Vm, _this: Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::Undefined)
}

#[derive(Clone, PartialEq)]
enum Resolution {
    Found(PathBuf, JsStr),
    Namespace(PathBuf),
    Builtin(JsStr, JsStr),
    Ambiguous,
    NotFound,
}

fn ns_get(vm: &mut Vm, this: Value, args: &[Value]) -> Result<Value, Value> {
    let Some(Value::Str(key)) = args.first() else {
        return Ok(Value::Undefined);
    };
    vm.get_prop(&this, key)
}
