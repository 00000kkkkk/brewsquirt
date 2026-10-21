use crate::builtins;
use crate::bytecode::{FunctionProto, ModuleInfo, NO_NAME, Op, ProtoKind};
use crate::compiler::{Mode, compile_module};
use crate::inspect::{inspect_plain, inspect_top};
use crate::module;
use crate::number::{js_pow, to_int32, to_uint32};
use crate::object::{Closure, IterState, MapKey, Obj, ObjKind, Upvalue, array_index};
use crate::parser;
use crate::token::SyntaxError;
use crate::value::{Value, char_at16, compare_strings, len16};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
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
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 5] = [
        ErrorKind::Error,
        ErrorKind::TypeError,
        ErrorKind::RangeError,
        ErrorKind::ReferenceError,
        ErrorKind::SyntaxError,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::Error => "Error",
            ErrorKind::TypeError => "TypeError",
            ErrorKind::RangeError => "RangeError",
            ErrorKind::ReferenceError => "ReferenceError",
            ErrorKind::SyntaxError => "SyntaxError",
        }
    }
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
    pub error_protos: Vec<Rc<Obj>>,
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
        Realm {
            function_proto: mk(),
            array_proto: mk(),
            string_proto: mk(),
            number_proto: mk(),
            boolean_proto: mk(),
            map_proto: mk(),
            set_proto: mk(),
            iter_proto: mk(),
            error_protos,
            object_proto,
        }
    }
}

struct Frame {
    closure: Rc<Closure>,
    ip: usize,
    base: usize,
    construct: bool,
}

struct Handler {
    frame: usize,
    stack_len: usize,
    catch_ip: usize,
}

pub struct Vm {
    pub stack: Vec<Value>,
    frames: Vec<Frame>,
    handlers: Vec<Handler>,
    open_upvalues: Vec<Rc<RefCell<Upvalue>>>,
    pub globals: FxHashMap<Rc<str>, Value>,
    pub realm: Realm,
    modules: FxHashMap<PathBuf, Value>,
    loading: Vec<PathBuf>,
    builtin_modules: FxHashMap<String, Value>,
    const_globals: Vec<Rc<str>>,
    pub out: Box<dyn Write>,
    pub err: Box<dyn Write>,
    pub argv: Vec<String>,
    pub start: Instant,
    pub rng: u64,
    native_depth: usize,
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
            open_upvalues: Vec::new(),
            globals: FxHashMap::default(),
            realm: Realm::new(),
            modules: FxHashMap::default(),
            loading: Vec::new(),
            builtin_modules: FxHashMap::default(),
            const_globals: Vec::new(),
            out,
            err,
            argv,
            start: Instant::now(),
            rng: seed | 1,
            native_depth: 0,
        };
        builtins::install(&mut vm);
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
        let ast = parser::parse(src)?;
        compile_module(
            &ast,
            Rc::new(ModuleInfo {
                name: name.into(),
                dir,
            }),
            mode,
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

    pub fn run_proto(&mut self, proto: Rc<FunctionProto>) -> Result<Value, Value> {
        let closure = Rc::new(Closure {
            proto,
            upvalues: Box::new([]),
            home: RefCell::new(None),
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
        self.run_proto(proto)
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
        let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let dir = abs.parent().map(Path::to_path_buf).unwrap_or_default();
        let proto = match self.compile(&src, &name, dir, Mode::Module) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, &name)),
        };
        self.loading.push(abs.clone());
        let r = self.run_proto(proto);
        self.loading.pop();
        if let Ok(ns) = &r {
            self.modules.insert(abs, ns.clone());
        }
        r
    }

    pub fn format_uncaught(&self, e: &Value) -> String {
        format!("Uncaught {}", inspect_top(e))
    }

    pub fn write_out(&mut self, s: &str) {
        let _ = self.out.write_all(s.as_bytes());
    }

    pub fn write_err(&mut self, s: &str) {
        let _ = self.out.flush();
        let _ = self.err.write_all(s.as_bytes());
        let _ = self.err.flush();
    }

    pub fn import_module(&mut self, spec: &str, from: &Rc<ModuleInfo>) -> Result<Value, Value> {
        if let Some(name) = spec.strip_prefix("squirt:") {
            if let Some(v) = self.builtin_modules.get(name) {
                return Ok(v.clone());
            }
            let Some(v) = builtins::builtin_module(self, name) else {
                return Err(
                    self.make_error(ErrorKind::Error, format!("Cannot find module '{spec}'"))
                );
            };
            self.builtin_modules.insert(name.to_string(), v.clone());
            return Ok(v);
        }
        let path = match module::resolve(spec, &from.dir) {
            Ok(p) => p,
            Err(msg) => return Err(self.make_error(ErrorKind::Error, msg)),
        };
        if let Some(v) = self.modules.get(&path) {
            return Ok(v.clone());
        }
        if self.loading.contains(&path) {
            return Err(self.make_error(
                ErrorKind::Error,
                format!("Circular import of '{spec}' is not supported yet"),
            ));
        }
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                return Err(self.make_error(
                    ErrorKind::Error,
                    format!("Cannot read module '{spec}': {e}"),
                ));
            }
        };
        let name = module::display_path(&path);
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let proto = match self.compile(&src, &name, dir, Mode::Module) {
            Ok(p) => p,
            Err(e) => return Err(self.syntax_error_value(&e, &name)),
        };
        self.loading.push(path.clone());
        let r = self.run_proto(proto);
        self.loading.pop();
        let ns = r?;
        self.modules.insert(path, ns.clone());
        Ok(ns)
    }

    pub fn stack_trace(&self) -> String {
        let mut s = String::new();
        for f in self.frames.iter().rev().take(12) {
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
        o.set_own("message", msg.into());
        o.set_own("stack", stack.into());
        Value::Object(o)
    }

    pub fn type_error(&mut self, msg: impl Into<String>) -> Value {
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
        Value::Object(Obj::new(
            ObjKind::Iter(RefCell::new(state)),
            Some(self.realm.iter_proto.clone()),
        ))
    }

    fn capture_upvalue(&mut self, idx: usize) -> Rc<RefCell<Upvalue>> {
        let mut i = self.open_upvalues.len();
        while i > 0 {
            let ui = match &*self.open_upvalues[i - 1].borrow() {
                Upvalue::Open(x) => *x,
                Upvalue::Closed(_) => 0,
            };
            if ui == idx {
                return self.open_upvalues[i - 1].clone();
            }
            if ui < idx {
                break;
            }
            i -= 1;
        }
        let u = Rc::new(RefCell::new(Upvalue::Open(idx)));
        self.open_upvalues.insert(i, u.clone());
        u
    }

    fn close_upvalues(&mut self, from: usize) {
        while let Some(u) = self.open_upvalues.last() {
            let idx = match &*u.borrow() {
                Upvalue::Open(x) => *x,
                Upvalue::Closed(_) => usize::MAX,
            };
            if idx < from {
                break;
            }
            let u = self.open_upvalues.pop().expect("upvalue");
            if idx != usize::MAX {
                let v = self.stack[idx].clone();
                *u.borrow_mut() = Upvalue::Closed(v);
            }
        }
    }

    fn push_frame(&mut self, c: Rc<Closure>, argc: usize, construct: bool) -> Result<(), Value> {
        if self.frames.len() >= MAX_FRAMES {
            return Err(self.range_error("Maximum call stack size exceeded"));
        }
        let p = &c.proto;
        let np = p.num_params as usize;
        let ci = self.stack.len() - argc - 2;
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
        self.frames.push(Frame {
            closure: c,
            ip: 0,
            base: ci + 1,
            construct,
        });
        Ok(())
    }

    fn unwind(&mut self, err: Value, stop: usize) -> Result<(), Value> {
        if let Some(h) = self.handlers.last() {
            if h.frame >= stop {
                let h = self.handlers.pop().expect("handler");
                self.frames.truncate(h.frame + 1);
                self.close_upvalues(h.stack_len);
                self.stack.truncate(h.stack_len);
                self.stack.push(err);
                self.frames[h.frame].ip = h.catch_ip;
                return Ok(());
            }
        }
        let base = self.frames[stop].base;
        self.close_upvalues(base);
        self.stack.truncate(base - 1);
        self.frames.truncate(stop);
        Err(err)
    }

    pub fn call_value(&mut self, f: &Value, this: Value, args: &[Value]) -> Result<Value, Value> {
        let Value::Object(o) = f else {
            return Err(self.type_error(format!("{} is not a function", inspect_plain(f))));
        };
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

    pub fn construct(&mut self, callee: &Value, args: &[Value]) -> Result<Value, Value> {
        let Value::Object(o) = callee else {
            return Err(self.not_constructor(callee, NO_NAME, None));
        };
        match &o.kind {
            ObjKind::Function(c) => {
                if !c.proto.is_constructor() {
                    return Err(self.not_constructor(callee, NO_NAME, None));
                }
                let this = self.alloc_this(o);
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
                    return a.borrow().get(i).cloned().unwrap_or_default();
                }
            }
            ObjKind::Function(c) => {
                if !o.has_own(key) {
                    match key {
                        "prototype" if c.proto.is_constructor() => {
                            let p = self.new_object();
                            p.set_own("constructor", Value::Object(o.clone()));
                            o.set_own("prototype", Value::Object(p.clone()));
                            return Value::Object(p);
                        }
                        "name" => return Value::Str(c.proto.name.clone()),
                        "length" => return Value::Number(c.proto.arity as f64),
                        _ => {}
                    }
                }
            }
            ObjKind::Native(n) => {
                if !o.has_own(key) {
                    match key {
                        "name" => return Value::Str(n.name.clone()),
                        "length" => return Value::Number(n.arity as f64),
                        _ => {}
                    }
                }
            }
            ObjKind::Bound(_) if key == "name" => {
                return o.function_name().map(Value::Str).unwrap_or_default();
            }
            ObjKind::Map(m) if key == "size" => {
                return Value::Number(m.borrow().len() as f64);
            }
            ObjKind::Set(s) if key == "size" => {
                return Value::Number(s.borrow().len() as f64);
            }
            _ => {}
        }
        o.get_chain(key).unwrap_or_default()
    }

    pub fn get_prop(&mut self, v: &Value, key: &str) -> Result<Value, Value> {
        match v {
            Value::Object(o) => Ok(self.get_obj(o, key)),
            Value::Str(s) => {
                if key == "length" {
                    return Ok(Value::Number(len16(s) as f64));
                }
                if let Some(i) = array_index(key) {
                    return Ok(char_at16(s, i).map(Value::from).unwrap_or_default());
                }
                Ok(self.realm.string_proto.get_chain(key).unwrap_or_default())
            }
            Value::Number(_) => Ok(self.realm.number_proto.get_chain(key).unwrap_or_default()),
            Value::Bool(_) => Ok(self.realm.boolean_proto.get_chain(key).unwrap_or_default()),
            Value::Undefined | Value::Null => Err(self.type_error(format!(
                "Cannot read properties of {} (reading '{}')",
                inspect_plain(v),
                key
            ))),
        }
    }

    pub fn set_prop(&mut self, target: &Value, key: Rc<str>, val: Value) -> Result<(), Value> {
        match target {
            Value::Object(o) => {
                if o.frozen.get() {
                    return Err(self.type_error(format!(
                        "Cannot assign to read only property '{key}' of object"
                    )));
                }
                if let ObjKind::Array(a) = &o.kind {
                    if let Some(i) = array_index(&key) {
                        return self.set_array_index(a, i, val);
                    }
                    if &*key == "length" {
                        let n = self.to_number(&val)?;
                        if n < 0.0 || n.fract() != 0.0 || n > 4294967295.0 {
                            return Err(self.range_error("Invalid array length"));
                        }
                        a.borrow_mut().resize(n as usize, Value::Undefined);
                        return Ok(());
                    }
                }
                o.props.borrow_mut().insert(key, val);
                Ok(())
            }
            Value::Undefined | Value::Null => Err(self.type_error(format!(
                "Cannot set properties of {} (setting '{}')",
                inspect_plain(target),
                key
            ))),
            _ => Ok(()),
        }
    }

    fn set_array_index(
        &mut self,
        a: &RefCell<Vec<Value>>,
        i: usize,
        val: Value,
    ) -> Result<(), Value> {
        let mut arr = a.borrow_mut();
        if i < arr.len() {
            arr[i] = val;
        } else if i - arr.len() > 50_000_000 {
            drop(arr);
            return Err(self.range_error("Array index too far beyond the end of the array"));
        } else {
            arr.resize(i, Value::Undefined);
            arr.push(val);
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
                            return Ok(a.borrow().get(i).cloned().unwrap_or_default());
                        }
                    }
                    Value::Str(s) => {
                        return Ok(char_at16(s, i).map(Value::from).unwrap_or_default());
                    }
                    _ => {}
                }
            }
        }
        let k = self.to_property_key(key)?;
        self.get_prop(obj, &k)
    }

    fn set_index(&mut self, obj: &Value, key: &Value, val: Value) -> Result<(), Value> {
        if let (Value::Object(o), Value::Number(n)) = (obj, key) {
            if let ObjKind::Array(a) = &o.kind {
                let i = *n as usize;
                if i as f64 == *n && !o.frozen.get() {
                    return self.set_array_index(a, i, val);
                }
            }
        }
        let k = self.to_property_key(key)?;
        self.set_prop(obj, k, val)
    }

    pub fn to_primitive(&mut self, v: &Value, prefer_number: bool) -> Result<Value, Value> {
        let Value::Object(o) = v else {
            return Ok(v.clone());
        };
        let order = if prefer_number {
            ["valueOf", "toString"]
        } else {
            ["toString", "valueOf"]
        };
        for name in order {
            if let Some(f) = o.get_chain(name) {
                if f.is_callable() {
                    let r = self.call_value(&f, v.clone(), &[])?;
                    if !r.is_object() {
                        return Ok(r);
                    }
                }
            }
        }
        Err(self.type_error("Cannot convert object to primitive value"))
    }

    pub fn to_string(&mut self, v: &Value) -> Result<Rc<str>, Value> {
        if let Some(s) = v.to_primitive_string() {
            return Ok(s);
        }
        let p = self.to_primitive(v, false)?;
        Ok(p.to_primitive_string().unwrap_or_else(|| "".into()))
    }

    pub fn to_number(&mut self, v: &Value) -> Result<f64, Value> {
        if let Some(n) = v.to_primitive_number() {
            return Ok(n);
        }
        let p = self.to_primitive(v, true)?;
        Ok(p.to_primitive_number().unwrap_or(f64::NAN))
    }

    pub fn to_property_key(&mut self, v: &Value) -> Result<Rc<str>, Value> {
        match v {
            Value::Str(s) => Ok(s.clone()),
            _ => self.to_string(v),
        }
    }

    pub fn loose_eq(&mut self, a: &Value, b: &Value) -> Result<bool, Value> {
        Ok(match (a, b) {
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            (Value::Number(_), Value::Number(_))
            | (Value::Str(_), Value::Str(_))
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Object(_), Value::Object(_)) => a.strict_eq(b),
            (Value::Object(_), _) => {
                let p = self.to_primitive(a, true)?;
                return self.loose_eq(&p, b);
            }
            (_, Value::Object(_)) => {
                let p = self.to_primitive(b, true)?;
                return self.loose_eq(a, &p);
            }
            _ => {
                let x = a.to_primitive_number().unwrap_or(f64::NAN);
                let y = b.to_primitive_number().unwrap_or(f64::NAN);
                x == y
            }
        })
    }

    fn add_slow(&mut self, a: &Value, b: &Value) -> Result<Value, Value> {
        let pa = self.to_primitive(a, true)?;
        let pb = self.to_primitive(b, true)?;
        if matches!(pa, Value::Str(_)) || matches!(pb, Value::Str(_)) {
            let sa = self.to_string(&pa)?;
            let sb = self.to_string(&pb)?;
            let mut s = String::with_capacity(sa.len() + sb.len());
            s.push_str(&sa);
            s.push_str(&sb);
            return Ok(Value::Str(s.into()));
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
        let x = self.to_number(&pa)?;
        let y = self.to_number(&pb)?;
        Ok(x.partial_cmp(&y))
    }

    fn num_pair(&mut self, a: &Value, b: &Value) -> Result<(f64, f64), Value> {
        Ok((self.to_number(a)?, self.to_number(b)?))
    }

    pub fn instance_of(&mut self, v: &Value, ctor: &Value) -> Result<bool, Value> {
        let Value::Object(c) = ctor else {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        };
        if !c.is_callable() {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        }
        if let ObjKind::Bound(b) = &c.kind {
            let t = b.target.clone();
            return self.instance_of(v, &t);
        }
        let Value::Object(o) = v else {
            return Ok(false);
        };
        let Some(proto) = self.function_prototype(c) else {
            return Ok(false);
        };
        let mut cur = o.proto.borrow().clone();
        while let Some(p) = cur {
            if Rc::ptr_eq(&p, &proto) {
                return Ok(true);
            }
            cur = p.proto.borrow().clone();
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
        match &o.kind {
            ObjKind::Array(a) => {
                if &*k == "length" {
                    return Ok(true);
                }
                if let Some(i) = array_index(&k) {
                    return Ok(i < a.borrow().len());
                }
            }
            ObjKind::Function(_) | ObjKind::Native(_) => {
                if matches!(&*k, "name" | "length" | "prototype") {
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(o.get_chain(&k).is_some())
    }

    pub fn get_iterator(&mut self, v: &Value) -> Result<Value, Value> {
        match v {
            Value::Str(s) => Ok(self.new_iter(IterState::Str(s.clone(), 0))),
            Value::Object(o) => match &o.kind {
                ObjKind::Array(_) => Ok(self.new_iter(IterState::Array(o.clone(), 0))),
                ObjKind::Map(_) => Ok(self.new_iter(IterState::MapEntries(o.clone(), 0))),
                ObjKind::Set(_) => Ok(self.new_iter(IterState::SetValues(o.clone(), 0))),
                ObjKind::Iter(_) => Ok(v.clone()),
                _ => Err(self.type_error(format!("{} is not iterable", inspect_plain(v)))),
            },
            _ => Err(self.type_error(format!("{} is not iterable", inspect_plain(v)))),
        }
    }

    pub fn iter_next(&mut self, it: &Value) -> Option<Value> {
        let Value::Object(o) = it else {
            return None;
        };
        let ObjKind::Iter(state) = &o.kind else {
            return None;
        };
        fn map_at(m: &Obj, i: usize) -> Option<(Value, Value)> {
            let ObjKind::Map(map) = &m.kind else {
                return None;
            };
            let map = map.borrow();
            let (k, v) = map.get_index(i)?;
            Some((k.to_value(), v.clone()))
        }
        let pair = {
            let mut st = state.borrow_mut();
            match &mut *st {
                IterState::Array(arr, i) => {
                    let v = arr.array()?.borrow().get(*i).cloned();
                    if v.is_some() {
                        *i += 1;
                    }
                    return v;
                }
                IterState::Str(s, pos) => {
                    let c = s[*pos..].chars().next()?;
                    *pos += c.len_utf8();
                    return Some(Value::Str(c.to_string().into()));
                }
                IterState::MapKeys(m, i) => {
                    let (k, _) = map_at(m, *i)?;
                    *i += 1;
                    return Some(k);
                }
                IterState::MapValues(m, i) => {
                    let (_, v) = map_at(m, *i)?;
                    *i += 1;
                    return Some(v);
                }
                IterState::MapEntries(m, i) => {
                    let pair = map_at(m, *i)?;
                    *i += 1;
                    pair
                }
                IterState::SetValues(s, i) => {
                    let ObjKind::Set(set) = &s.kind else {
                        return None;
                    };
                    let v = set.borrow().get_index(*i)?.to_value();
                    *i += 1;
                    return Some(v);
                }
                IterState::List(items, i) => {
                    let v = items.get(*i).cloned();
                    if v.is_some() {
                        *i += 1;
                    }
                    return v;
                }
            }
        };
        Some(self.new_array(vec![pair.0, pair.1]))
    }

    pub fn iterate(&mut self, v: &Value) -> Result<Vec<Value>, Value> {
        if let Value::Object(o) = v {
            if let ObjKind::Array(a) = &o.kind {
                return Ok(a.borrow().clone());
            }
        }
        let it = self.get_iterator(v)?;
        let mut out = Vec::new();
        while let Some(x) = self.iter_next(&it) {
            out.push(x);
        }
        Ok(out)
    }

    pub fn own_keys(&self, v: &Value) -> Vec<Value> {
        let mut keys = Vec::new();
        match v {
            Value::Object(o) => {
                if let ObjKind::Array(a) = &o.kind {
                    for i in 0..a.borrow().len() {
                        keys.push(Value::Str(i.to_string().into()));
                    }
                }
                for k in o.props.borrow().keys() {
                    keys.push(Value::Str(k.clone()));
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

    fn super_lookup(&mut self, cl: &Closure, key: &str, is_static: bool) -> Result<Value, Value> {
        let home = self.super_home(cl)?;
        let start = if is_static {
            home.proto.borrow().clone()
        } else if home.is_callable() {
            self.function_prototype(&home)
                .and_then(|p| p.proto.borrow().clone())
        } else {
            home.proto.borrow().clone()
        };
        Ok(match start {
            Some(s) => self.get_obj(&s, key),
            None => Value::Undefined,
        })
    }

    pub fn run(&mut self, stop: usize) -> Result<Value, Value> {
        let mut fi = self.frames.len() - 1;
        let mut cl = self.frames[fi].closure.clone();
        let mut ip = self.frames[fi].ip;
        let mut base = self.frames[fi].base;

        macro_rules! reload {
            () => {{
                fi = self.frames.len() - 1;
                let f = &self.frames[fi];
                cl = f.closure.clone();
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
                match &cl.proto.consts[$k as usize] {
                    Value::Str(s) => s.clone(),
                    _ => Rc::from(""),
                }
            };
        }
        macro_rules! num_op {
            ($f:expr) => {{
                let b = pop!();
                let a = pop!();
                let r = match (&a, &b) {
                    (Value::Number(x), Value::Number(y)) => $f(*x, *y),
                    _ => {
                        let (x, y) = tri!(self.num_pair(&a, &b));
                        $f(x, y)
                    }
                };
                self.stack.push(Value::Number(r));
            }};
        }
        macro_rules! int_op {
            ($f:expr) => {{
                let b = pop!();
                let a = pop!();
                let (x, y) = match (&a, &b) {
                    (Value::Number(x), Value::Number(y)) => (*x, *y),
                    _ => tri!(self.num_pair(&a, &b)),
                };
                self.stack.push(Value::Number($f(x, y)));
            }};
        }
        macro_rules! cmp_op {
            ($num:expr, $ord:expr) => {{
                let b = pop!();
                let a = pop!();
                let r = match (&a, &b) {
                    (Value::Number(x), Value::Number(y)) => $num(*x, *y),
                    _ => match tri!(self.compare(&a, &b)) {
                        Some(o) => $ord(o),
                        None => false,
                    },
                };
                self.stack.push(Value::Bool(r));
            }};
        }

        loop {
            let op = cl.proto.code[ip];
            ip += 1;
            match op {
                Op::Const(i) => self.stack.push(cl.proto.consts[i as usize].clone()),
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
                    let v = self.stack[base + s as usize].clone();
                    self.stack.push(v);
                }
                Op::SetLocal(s) => {
                    let v = peek!();
                    self.stack[base + s as usize] = v;
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
                    let n = name!(k);
                    match self.globals.get(&n) {
                        Some(v) => {
                            let v = v.clone();
                            self.stack.push(v);
                        }
                        None => throw!(self.make_error_at(
                            ErrorKind::ReferenceError,
                            format!("{n} is not defined"),
                            fi,
                            ip
                        )),
                    }
                }
                Op::SetGlobal(k) => {
                    let n = name!(k);
                    let v = peek!();
                    if self.const_globals.contains(&n) {
                        throw!(self.make_error_at(
                            ErrorKind::TypeError,
                            "Assignment to constant variable.".to_string(),
                            fi,
                            ip
                        ));
                    }
                    match self.globals.get_mut(&n) {
                        Some(slot) => *slot = v,
                        None => throw!(self.make_error_at(
                            ErrorKind::ReferenceError,
                            format!("{n} is not defined"),
                            fi,
                            ip
                        )),
                    }
                }
                Op::DefineGlobal(k, is_const) => {
                    let n = name!(k);
                    let v = peek!();
                    if is_const {
                        if !self.const_globals.contains(&n) {
                            self.const_globals.push(n.clone());
                        }
                    } else {
                        self.const_globals.retain(|m| *m != n);
                    }
                    self.globals.insert(n, v);
                }
                Op::TypeofGlobal(k) => {
                    let n = name!(k);
                    let t = self
                        .globals
                        .get(&n)
                        .map(|v| v.type_of())
                        .unwrap_or("undefined");
                    self.stack.push(Value::Str(t.into()));
                }
                Op::GetProp(k) => {
                    let obj = pop!();
                    let v = match &obj {
                        Value::Object(o) if matches!(o.kind, ObjKind::Ordinary) => {
                            let key = &cl.proto.consts[k as usize];
                            let Value::Str(key) = key else { unreachable!() };
                            o.get_chain(key).unwrap_or_default()
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
                    match &obj {
                        Value::Object(o)
                            if matches!(o.kind, ObjKind::Ordinary) && !o.frozen.get() =>
                        {
                            o.props.borrow_mut().insert(name!(k), val.clone());
                        }
                        _ => {
                            tri!(self.set_prop(&obj, name!(k), val.clone()));
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
                    tri!(self.set_index(&obj, &key, val.clone()));
                    self.stack.push(val);
                }
                Op::DeleteProp(k) => {
                    let obj = pop!();
                    let n = name!(k);
                    let r = tri!(self.delete_prop(&obj, &n));
                    self.stack.push(Value::Bool(r));
                }
                Op::DeleteIndex => {
                    let key = pop!();
                    let obj = pop!();
                    let n = tri!(self.to_property_key(&key));
                    let r = tri!(self.delete_prop(&obj, &n));
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
                    let _this = pop!();
                    let n = name!(k);
                    let v = tri!(self.super_lookup(&cl, &n, is_static));
                    self.stack.push(v);
                }
                Op::SuperGetMethod(k, is_static) => {
                    let this = pop!();
                    let n = name!(k);
                    let v = tri!(self.super_lookup(&cl, &n, is_static));
                    self.stack.push(v);
                    self.stack.push(this);
                }
                Op::SuperCall(argc) => {
                    let argc = if argc == u32::MAX {
                        let arr = pop!();
                        let items = tri!(self.iterate(&arr));
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
                    let ti = self.stack.len() - argc - 1;
                    match &sup.kind {
                        ObjKind::Function(c) => {
                            let c = c.clone();
                            self.stack.insert(ti, Value::Object(sup.clone()));
                            self.frames[fi].ip = ip;
                            tri!(self.push_frame(c, argc, true));
                            reload!();
                        }
                        ObjKind::Native(n) => {
                            let f = n.func;
                            let args = self.stack.split_off(ti + 1);
                            let this = pop!();
                            let r = tri!(f(self, this.clone(), &args));
                            self.stack.push(if r.is_object() { r } else { this });
                        }
                        _ => throw!(self.type_error("Super constructor is not a constructor")),
                    }
                }
                Op::Add => {
                    let b = pop!();
                    let a = pop!();
                    let r = match (&a, &b) {
                        (Value::Number(x), Value::Number(y)) => Value::Number(x + y),
                        (Value::Str(x), Value::Str(y)) => {
                            let mut s = String::with_capacity(x.len() + y.len());
                            s.push_str(x);
                            s.push_str(y);
                            Value::Str(s.into())
                        }
                        _ => tri!(self.add_slow(&a, &b)),
                    };
                    self.stack.push(r);
                }
                Op::Sub => num_op!(|x: f64, y: f64| x - y),
                Op::Mul => num_op!(|x: f64, y: f64| x * y),
                Op::Div => num_op!(|x: f64, y: f64| x / y),
                Op::Mod => num_op!(|x: f64, y: f64| x % y),
                Op::Pow => num_op!(js_pow),
                Op::BitAnd => int_op!(|x, y| (to_int32(x) & to_int32(y)) as f64),
                Op::BitOr => int_op!(|x, y| (to_int32(x) | to_int32(y)) as f64),
                Op::BitXor => int_op!(|x, y| (to_int32(x) ^ to_int32(y)) as f64),
                Op::Shl => int_op!(|x, y| to_int32(x).wrapping_shl(to_uint32(y) & 31) as f64),
                Op::Shr => int_op!(|x, y| (to_int32(x) >> (to_uint32(y) & 31)) as f64),
                Op::UShr => int_op!(|x, y| (to_uint32(x) >> (to_uint32(y) & 31)) as f64),
                Op::Neg => {
                    let a = pop!();
                    let n = match a {
                        Value::Number(n) => n,
                        _ => tri!(self.to_number(&a)),
                    };
                    self.stack.push(Value::Number(-n));
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
                    let n = match a {
                        Value::Number(n) => n,
                        _ => tri!(self.to_number(&a)),
                    };
                    self.stack
                        .push(Value::Number(if op == Op::Inc { n + 1.0 } else { n - 1.0 }));
                }
                Op::Not => {
                    let a = pop!();
                    self.stack.push(Value::Bool(!a.truthy()));
                }
                Op::BitNot => {
                    let a = pop!();
                    let n = match a {
                        Value::Number(n) => n,
                        _ => tri!(self.to_number(&a)),
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
                        tri!(self.push_frame(c, argc, false));
                        reload!();
                        continue;
                    }
                    let args = self.stack.split_off(ci + 2);
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
                        let this = self.alloc_this(&o);
                        self.stack.insert(ci + 1, this);
                        tri!(self.push_frame(c, argc, true));
                        reload!();
                        continue;
                    }
                    let args = self.stack.split_off(ci + 1);
                    let f = pop!();
                    let constructible = match &f {
                        Value::Object(o) => {
                            matches!(o.kind, ObjKind::Native(_) | ObjKind::Bound(_))
                        }
                        _ => false,
                    };
                    if !constructible {
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
                    let frame = self.frames.pop().expect("frame");
                    self.close_upvalues(frame.base);
                    while self
                        .handlers
                        .last()
                        .is_some_and(|h| h.frame >= self.frames.len())
                    {
                        self.handlers.pop();
                    }
                    let result = if frame.construct && !result.is_object() {
                        self.stack[frame.base].clone()
                    } else {
                        result
                    };
                    self.stack.truncate(frame.base - 1);
                    if self.frames.len() == stop {
                        return Ok(result);
                    }
                    self.stack.push(result);
                    reload!();
                }
                Op::Closure(i) => {
                    let p = cl.proto.functions[i as usize].clone();
                    let mut ups = Vec::with_capacity(p.upvalues.len());
                    for u in &p.upvalues {
                        if u.is_local {
                            ups.push(self.capture_upvalue(base + u.index as usize));
                        } else {
                            ups.push(cl.upvalues[u.index as usize].clone());
                        }
                    }
                    let home = if p.kind == ProtoKind::Arrow {
                        cl.home.borrow().clone()
                    } else {
                        None
                    };
                    let c = Rc::new(Closure {
                        proto: p,
                        upvalues: ups.into_boxed_slice(),
                        home: RefCell::new(home),
                    });
                    let o = Obj::new(
                        ObjKind::Function(c),
                        Some(self.realm.function_proto.clone()),
                    );
                    self.stack.push(Value::Object(o));
                }
                Op::PopTo(n) => {
                    let to = base + n as usize;
                    self.close_upvalues(to);
                    self.stack.truncate(to);
                }
                Op::CloseUpvalues(n) => {
                    self.close_upvalues(base + n as usize);
                }
                Op::Collapse(n) => {
                    let top = pop!();
                    let to = base + n as usize;
                    self.close_upvalues(to);
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
                            a.borrow_mut().push(Value::Undefined);
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
                    let o = self.new_object();
                    self.stack.push(Value::Object(o));
                }
                Op::InitProp(k) => {
                    let v = pop!();
                    if let Some(Value::Object(o)) = self.stack.last() {
                        o.props.borrow_mut().insert(name!(k), v);
                    }
                }
                Op::InitIndex => {
                    let v = pop!();
                    let key = pop!();
                    let k = tri!(self.to_property_key(&key));
                    if let Some(Value::Object(o)) = self.stack.last() {
                        o.props.borrow_mut().insert(k, v);
                    }
                }
                Op::ObjSpread => {
                    let src = pop!();
                    let pairs = self.own_entries(&src);
                    if let Some(Value::Object(o)) = self.stack.last() {
                        let mut props = o.props.borrow_mut();
                        for (k, v) in pairs {
                            props.insert(k, v);
                        }
                    }
                }
                Op::ObjRest(n) => {
                    let keys = self.stack.split_off(self.stack.len() - n as usize);
                    let src = pop!();
                    let excluded: Vec<Rc<str>> = keys
                        .iter()
                        .filter_map(|k| match k {
                            Value::Str(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect();
                    let o = self.new_object();
                    for (k, v) in self.own_entries(&src) {
                        if !excluded.contains(&k) {
                            o.set_own(k, v);
                        }
                    }
                    self.stack.push(Value::Object(o));
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
                    tri!(self.define_method(&class, k, m, is_static));
                }
                Op::GetIter => {
                    let v = pop!();
                    let it = tri!(self.get_iterator(&v));
                    self.stack.push(it);
                }
                Op::ForInIter => {
                    let v = pop!();
                    let keys = self.own_keys(&v);
                    let it = self.new_iter(IterState::List(keys, 0));
                    self.stack.push(it);
                }
                Op::IterNext(t) => {
                    let it = pop!();
                    match self.iter_next(&it) {
                        Some(v) => self.stack.push(v),
                        None => ip = t as usize,
                    }
                }
                Op::IterNextOrUndef => {
                    let it = pop!();
                    let v = self.iter_next(&it).unwrap_or_default();
                    self.stack.push(v);
                }
                Op::IterRest => {
                    let it = pop!();
                    let mut items = Vec::new();
                    while let Some(v) = self.iter_next(&it) {
                        items.push(v);
                    }
                    let a = self.new_array(items);
                    self.stack.push(a);
                }
                Op::Template(n) => {
                    let parts = self.stack.split_off(self.stack.len() - n as usize);
                    let mut s = String::new();
                    for p in &parts {
                        match p {
                            Value::Str(x) => s.push_str(x),
                            _ => {
                                let x = tri!(self.to_string(p));
                                s.push_str(&x);
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
                    });
                }
                Op::TryEnd => {
                    self.handlers.pop();
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

    pub fn own_entries(&self, v: &Value) -> Vec<(Rc<str>, Value)> {
        let mut out = Vec::new();
        match v {
            Value::Object(o) => {
                if let ObjKind::Array(a) = &o.kind {
                    for (i, x) in a.borrow().iter().enumerate() {
                        out.push((Rc::from(i.to_string()), x.clone()));
                    }
                }
                for (k, x) in o.props.borrow().iter() {
                    out.push((k.clone(), x.clone()));
                }
            }
            Value::Str(s) => {
                for (i, c) in s.chars().enumerate() {
                    out.push((Rc::from(i.to_string()), Value::Str(c.to_string().into())));
                }
            }
            _ => {}
        }
        out
    }

    pub fn delete_prop(&mut self, obj: &Value, key: &str) -> Result<bool, Value> {
        match obj {
            Value::Object(o) => {
                if o.frozen.get() {
                    return Err(
                        self.type_error(format!("Cannot delete property '{key}' of frozen object"))
                    );
                }
                if let ObjKind::Array(a) = &o.kind {
                    if let Some(i) = array_index(key) {
                        if let Some(slot) = a.borrow_mut().get_mut(i) {
                            *slot = Value::Undefined;
                        }
                        return Ok(true);
                    }
                }
                o.props.borrow_mut().shift_remove(key);
                Ok(true)
            }
            Value::Undefined | Value::Null => Err(self.type_error(format!(
                "Cannot convert undefined or null to object (deleting '{key}')"
            ))),
            _ => Ok(true),
        }
    }

    fn make_class(&mut self, ctor: &Value, sup: Option<Value>, name: Rc<str>) -> Result<(), Value> {
        let Value::Object(ctor_obj) = ctor else {
            return Ok(());
        };
        let (proto_parent, ctor_parent) = match sup {
            None => (
                Some(self.realm.object_proto.clone()),
                self.realm.function_proto.clone(),
            ),
            Some(Value::Null) => (None, self.realm.function_proto.clone()),
            Some(Value::Object(s)) if s.is_callable() => {
                let sp = self.get_obj(&s, "prototype");
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
        prototype.set_own("constructor", ctor.clone());
        ctor_obj.set_own("prototype", Value::Object(prototype));
        if !name.is_empty() {
            ctor_obj.set_own("name", Value::Str(name));
        }
        *ctor_obj.proto.borrow_mut() = Some(ctor_parent);
        if let ObjKind::Function(c) = &ctor_obj.kind {
            *c.home.borrow_mut() = Some(ctor_obj.clone());
        }
        Ok(())
    }

    fn define_method(
        &mut self,
        class: &Value,
        key: Rc<str>,
        method: Value,
        is_static: bool,
    ) -> Result<(), Value> {
        let Value::Object(c) = class else {
            return Ok(());
        };
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
        target.set_own(key, method);
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
