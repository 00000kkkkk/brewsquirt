use crate::bytecode::FunctionProto;
use crate::value::Value;
use crate::vm::Vm;
use indexmap::{IndexMap, IndexSet};
use rustc_hash::FxBuildHasher;
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub type NativeFn = fn(&mut Vm, Value, &[Value]) -> Result<Value, Value>;

const INDEX_AT: usize = 12;

#[derive(Default)]
struct PropExtra {
    index: rustc_hash::FxHashMap<Rc<str>, u32>,
    hidden: rustc_hash::FxHashSet<Rc<str>>,
}

#[derive(Default)]
pub struct PropMap {
    entries: Vec<(Rc<str>, Value)>,
    extra: Option<Box<PropExtra>>,
}

impl PropMap {
    fn index(&self) -> Option<&rustc_hash::FxHashMap<Rc<str>, u32>> {
        self.extra
            .as_ref()
            .map(|x| &x.index)
            .filter(|ix| !ix.is_empty())
    }

    fn find(&self, key: &str) -> Option<usize> {
        if let Some(ix) = self.index() {
            return ix.get(key).map(|&i| i as usize);
        }
        self.entries.iter().position(|(k, _)| **k == *key)
    }

    fn reindex(&mut self) {
        if self.entries.len() > INDEX_AT {
            let x = self.extra.get_or_insert_with(Box::default);
            x.index.clear();
            x.index.reserve(self.entries.len());
            for (i, (k, _)) in self.entries.iter().enumerate() {
                x.index.insert(k.clone(), i as u32);
            }
        } else if let Some(x) = &mut self.extra {
            x.index.clear();
            if x.hidden.is_empty() {
                self.extra = None;
            }
        }
    }

    pub fn is_enumerable(&self, key: &str) -> bool {
        self.extra.as_ref().is_none_or(|x| !x.hidden.contains(key))
    }

    pub fn set_enumerable(&mut self, key: &str, enumerable: bool) {
        if enumerable {
            if let Some(x) = &mut self.extra {
                x.hidden.remove(key);
                if x.hidden.is_empty() && x.index.is_empty() {
                    self.extra = None;
                }
            }
        } else if let Some(i) = self.find(key) {
            let k = self.entries[i].0.clone();
            self.extra.get_or_insert_with(Box::default).hidden.insert(k);
        }
    }

    pub fn hide_all(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let keys: Vec<Rc<str>> = self.entries.iter().map(|(k, _)| k.clone()).collect();
        self.extra
            .get_or_insert_with(Box::default)
            .hidden
            .extend(keys);
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.find(key).map(|i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.find(key).map(|i| &mut self.entries[i].1)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.find(key).is_some()
    }

    pub fn insert(&mut self, key: Rc<str>, v: Value) -> Option<Value> {
        if let Some(i) = self.find(&key) {
            return Some(std::mem::replace(&mut self.entries[i].1, v));
        }
        let i = self.entries.len() as u32;
        match self.extra.as_mut().filter(|x| !x.index.is_empty()) {
            Some(x) => {
                x.index.insert(key.clone(), i);
                self.entries.push((key, v));
            }
            None => {
                self.entries.push((key, v));
                if self.entries.len() > INDEX_AT {
                    self.reindex();
                }
            }
        }
        None
    }

    pub fn shift_remove(&mut self, key: &str) -> Option<Value> {
        let i = self.find(key)?;
        let (_, v) = self.entries.remove(i);
        if let Some(x) = &mut self.extra {
            x.hidden.remove(key);
            if !x.index.is_empty() || x.hidden.is_empty() {
                self.reindex();
            }
        }
        Some(v)
    }

    pub fn enumerable(&self) -> impl Iterator<Item = (&Rc<str>, &Value)> {
        self.entries
            .iter()
            .filter(|(k, _)| self.is_enumerable(k))
            .map(|(k, v)| (k, v))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Rc<str>, &Value)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    pub fn keys(&self) -> impl Iterator<Item = &Rc<str>> {
        self.entries.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn into_values(self) -> impl Iterator<Item = Value> {
        self.entries.into_iter().map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.extra = None;
    }
}

pub struct Obj {
    pub kind: ObjKind,
    pub props: RefCell<PropMap>,
    pub proto: RefCell<Option<Rc<Obj>>>,
    pub frozen: Cell<bool>,
    pub(crate) gc_slot: Cell<u32>,
}

pub enum ObjKind {
    Ordinary,
    Array(RefCell<Vec<Value>>),
    Function(Rc<Closure>),
    Native(Box<Native>),
    Bound(Box<Bound>),
    Error,
    Map(Box<RefCell<IndexMap<MapKey, Value, FxBuildHasher>>>),
    Set(Box<RefCell<IndexSet<MapKey, FxBuildHasher>>>),
    Iter(Box<RefCell<IterState>>),
    Accessor(Box<Accessor>),
    Promise(Box<RefCell<PromiseData>>),
    Coroutine(Box<Coroutine>),
    RegExp(RegExpObj),
    Date(Cell<f64>),
    WeakMap(Box<RefCell<WeakEntries>>),
    WeakSet(Box<RefCell<WeakKeys>>),
    WeakRef(std::rc::Weak<Obj>),
    ArrayBuffer(RefCell<Vec<u8>>),
    Typed(Box<View>),
    DataView(Box<View>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TypedKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float32,
    Float64,
}

fn modulo32(n: f64) -> u32 {
    if !n.is_finite() {
        return 0;
    }
    n.trunc().rem_euclid(4294967296.0) as u32
}

impl TypedKind {
    pub const ALL: [TypedKind; 9] = [
        TypedKind::Int8,
        TypedKind::Uint8,
        TypedKind::Uint8Clamped,
        TypedKind::Int16,
        TypedKind::Uint16,
        TypedKind::Int32,
        TypedKind::Uint32,
        TypedKind::Float32,
        TypedKind::Float64,
    ];

    pub fn size(self) -> usize {
        match self {
            TypedKind::Int8 | TypedKind::Uint8 | TypedKind::Uint8Clamped => 1,
            TypedKind::Int16 | TypedKind::Uint16 => 2,
            TypedKind::Int32 | TypedKind::Uint32 | TypedKind::Float32 => 4,
            TypedKind::Float64 => 8,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TypedKind::Int8 => "Int8Array",
            TypedKind::Uint8 => "Uint8Array",
            TypedKind::Uint8Clamped => "Uint8ClampedArray",
            TypedKind::Int16 => "Int16Array",
            TypedKind::Uint16 => "Uint16Array",
            TypedKind::Int32 => "Int32Array",
            TypedKind::Uint32 => "Uint32Array",
            TypedKind::Float32 => "Float32Array",
            TypedKind::Float64 => "Float64Array",
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, TypedKind::Float32 | TypedKind::Float64)
    }

    pub fn read(self, b: &[u8], little: bool) -> f64 {
        macro_rules! rd {
            ($t:ty, $n:expr) => {{
                let mut a = [0u8; $n];
                a.copy_from_slice(&b[..$n]);
                (if little {
                    <$t>::from_le_bytes(a)
                } else {
                    <$t>::from_be_bytes(a)
                }) as f64
            }};
        }
        match self {
            TypedKind::Int8 => b[0] as i8 as f64,
            TypedKind::Uint8 | TypedKind::Uint8Clamped => b[0] as f64,
            TypedKind::Int16 => rd!(i16, 2),
            TypedKind::Uint16 => rd!(u16, 2),
            TypedKind::Int32 => rd!(i32, 4),
            TypedKind::Uint32 => rd!(u32, 4),
            TypedKind::Float32 => rd!(f32, 4),
            TypedKind::Float64 => rd!(f64, 8),
        }
    }

    pub fn write(self, b: &mut [u8], n: f64, little: bool) {
        macro_rules! wr {
            ($v:expr) => {{
                let v = $v;
                let bytes = if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                };
                b[..bytes.len()].copy_from_slice(&bytes);
            }};
        }
        match self {
            TypedKind::Int8 | TypedKind::Uint8 => b[0] = modulo32(n) as u8,
            TypedKind::Uint8Clamped => {
                b[0] = if n.is_nan() || n <= 0.0 {
                    0
                } else if n >= 255.0 {
                    255
                } else {
                    let f = n.floor();
                    let d = n - f;
                    let r = if d > 0.5 || (d == 0.5 && f % 2.0 != 0.0) {
                        f + 1.0
                    } else {
                        f
                    };
                    r as u8
                }
            }
            TypedKind::Int16 | TypedKind::Uint16 => wr!(modulo32(n) as u16),
            TypedKind::Int32 | TypedKind::Uint32 => wr!(modulo32(n)),
            TypedKind::Float32 => wr!(n as f32),
            TypedKind::Float64 => wr!(n),
        }
    }
}

pub struct View {
    pub kind: TypedKind,
    pub buffer: Rc<Obj>,
    pub offset: usize,
    pub length: usize,
}

impl View {
    pub fn byte_len(&self) -> usize {
        self.length * self.kind.size()
    }

    pub fn store(&self) -> &RefCell<Vec<u8>> {
        match &self.buffer.kind {
            ObjKind::ArrayBuffer(b) => b,
            _ => unreachable!("view over a non-buffer"),
        }
    }

    pub fn get(&self, i: usize) -> Option<f64> {
        if i >= self.length {
            return None;
        }
        let size = self.kind.size();
        let start = self.offset + i * size;
        let b = self.store().borrow();
        b.get(start..start + size).map(|x| self.kind.read(x, true))
    }

    pub fn set(&self, i: usize, n: f64) {
        if i >= self.length {
            return;
        }
        let size = self.kind.size();
        let start = self.offset + i * size;
        let mut b = self.store().borrow_mut();
        if let Some(x) = b.get_mut(start..start + size) {
            self.kind.write(x, n, true);
        }
    }

    pub fn values(&self) -> Vec<f64> {
        let size = self.kind.size();
        let b = self.store().borrow();
        let end = (self.offset + self.byte_len()).min(b.len());
        b[self.offset.min(end)..end]
            .chunks_exact(size)
            .map(|c| self.kind.read(c, true))
            .collect()
    }

    pub fn bytes(&self) -> Vec<u8> {
        let b = self.store().borrow();
        let end = (self.offset + self.byte_len()).min(b.len());
        b[self.offset.min(end)..end].to_vec()
    }
}

pub type WeakEntries = rustc_hash::FxHashMap<usize, (std::rc::Weak<Obj>, Value)>;
pub type WeakKeys = rustc_hash::FxHashMap<usize, std::rc::Weak<Obj>>;

pub struct RegExpObj {
    pub data: Rc<crate::regexp::RegexData>,
    pub last_index: Cell<f64>,
}

pub struct Symbol {
    pub description: Option<Rc<str>>,
    pub key: Rc<str>,
}

pub const SYMBOL_PREFIX: char = '\u{0}';
pub const ITER_KEY: &str = "\u{0}@@iterator:Symbol.iterator";
pub const ASYNC_ITER_KEY: &str = "\u{0}@@asyncIterator:Symbol.asyncIterator";

pub fn is_symbol_key(k: &str) -> bool {
    k.starts_with(SYMBOL_PREFIX)
}

pub fn is_private_key(k: &str) -> bool {
    k.starts_with("\u{0}#")
}

pub fn symbol_key_description(k: &str) -> &str {
    k.split_once(':').map(|(_, d)| d).unwrap_or("")
}

#[derive(Default)]
pub struct Accessor {
    pub get: RefCell<Value>,
    pub set: RefCell<Value>,
}

pub enum PromiseState {
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

pub enum Reaction {
    Then {
        on_fulfilled: Value,
        on_rejected: Value,
        derived: Option<Rc<Obj>>,
    },
    Forward(Rc<Obj>),
    Resume(Rc<Obj>),
}

pub struct PromiseData {
    pub state: PromiseState,
    pub reactions: Vec<Reaction>,
    pub handled: bool,
}

impl Default for PromiseData {
    fn default() -> Self {
        PromiseData {
            state: PromiseState::Pending,
            reactions: Vec::new(),
            handled: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CoState {
    Suspended,
    Running,
    Done,
}

pub struct SavedFrame {
    pub closure: Rc<Closure>,
    pub ip: usize,
    pub stack: Vec<Value>,
    pub handlers: Vec<(usize, usize)>,
    pub upvalues: Vec<(Rc<UpCell>, usize)>,
    pub started: bool,
}

pub struct AsyncRequest {
    pub value: Value,
    pub mode: crate::event_loop::ResumeMode,
    pub promise: Rc<Obj>,
}

pub struct Coroutine {
    pub saved: RefCell<Option<SavedFrame>>,
    pub state: Cell<CoState>,
    pub promise: Option<Rc<Obj>>,
    pub is_generator: bool,
    pub is_async: bool,
    pub queue: RefCell<std::collections::VecDeque<AsyncRequest>>,
    pub awaiting: Cell<bool>,
}

pub struct Closure {
    pub(crate) gc_slot: Cell<u32>,
    pub proto: Rc<FunctionProto>,
    pub upvalues: Box<[Rc<UpCell>]>,
    pub home: RefCell<Option<Rc<Obj>>>,
}

pub struct UpCell {
    pub value: RefCell<Upvalue>,
    pub(crate) gc_slot: Cell<u32>,
}

impl UpCell {
    pub fn new(u: Upvalue) -> Rc<UpCell> {
        Rc::new(UpCell {
            value: RefCell::new(u),
            gc_slot: Cell::new(u32::MAX),
        })
    }

    pub fn borrow(&self) -> std::cell::Ref<'_, Upvalue> {
        self.value.borrow()
    }

    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, Upvalue> {
        self.value.borrow_mut()
    }
}

pub enum Upvalue {
    Open(usize),
    Closed(Value),
}

pub struct Native {
    pub name: Rc<str>,
    pub arity: u32,
    pub func: NativeFn,
    pub ctor: Option<fn() -> ObjKind>,
}

pub struct Bound {
    pub target: Value,
    pub this: Value,
    pub args: Vec<Value>,
}

pub enum IterState {
    Array(Rc<Obj>, usize),
    Str(Rc<str>, usize),
    MapEntries(Rc<Obj>, usize),
    MapKeys(Rc<Obj>, usize),
    MapValues(Rc<Obj>, usize),
    SetValues(Rc<Obj>, usize),
    List(Vec<Value>, usize),
    Protocol(Value, Value),
}

#[derive(Clone)]
pub enum MapKey {
    Undefined,
    Null,
    Bool(bool),
    Num(u64),
    Str(Rc<str>),
    Obj(Rc<Obj>),
    Sym(Rc<Symbol>),
}

impl MapKey {
    pub fn from_value(v: &Value) -> MapKey {
        match v {
            Value::Undefined => MapKey::Undefined,
            Value::Null => MapKey::Null,
            Value::Bool(b) => MapKey::Bool(*b),
            Value::Number(n) => {
                let n = if *n == 0.0 {
                    0.0
                } else if n.is_nan() {
                    f64::NAN
                } else {
                    *n
                };
                MapKey::Num(n.to_bits())
            }
            Value::Str(s) => MapKey::Str(s.clone()),
            Value::Object(o) => MapKey::Obj(o.clone()),
            Value::Symbol(s) => MapKey::Sym(s.clone()),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            MapKey::Undefined => Value::Undefined,
            MapKey::Null => Value::Null,
            MapKey::Bool(b) => Value::Bool(*b),
            MapKey::Num(bits) => Value::Number(f64::from_bits(*bits)),
            MapKey::Str(s) => Value::Str(s.clone()),
            MapKey::Obj(o) => Value::Object(o.clone()),
            MapKey::Sym(s) => Value::Symbol(s.clone()),
        }
    }
}

impl PartialEq for MapKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MapKey::Undefined, MapKey::Undefined) | (MapKey::Null, MapKey::Null) => true,
            (MapKey::Bool(a), MapKey::Bool(b)) => a == b,
            (MapKey::Num(a), MapKey::Num(b)) => a == b,
            (MapKey::Str(a), MapKey::Str(b)) => a == b,
            (MapKey::Obj(a), MapKey::Obj(b)) => Rc::ptr_eq(a, b),
            (MapKey::Sym(a), MapKey::Sym(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for MapKey {}

impl Hash for MapKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            MapKey::Undefined => 0u8.hash(state),
            MapKey::Null => 1u8.hash(state),
            MapKey::Bool(b) => {
                2u8.hash(state);
                b.hash(state)
            }
            MapKey::Num(n) => {
                3u8.hash(state);
                n.hash(state)
            }
            MapKey::Str(s) => {
                4u8.hash(state);
                s.hash(state)
            }
            MapKey::Obj(o) => {
                5u8.hash(state);
                (Rc::as_ptr(o) as usize).hash(state)
            }
            MapKey::Sym(s) => {
                6u8.hash(state);
                (Rc::as_ptr(s) as usize).hash(state)
            }
        }
    }
}

impl Obj {
    pub fn new(kind: ObjKind, proto: Option<Rc<Obj>>) -> Rc<Obj> {
        let o = Rc::new(Obj {
            kind,
            props: RefCell::new(PropMap::default()),
            proto: RefCell::new(proto),
            frozen: Cell::new(false),
            gc_slot: Cell::new(u32::MAX),
        });
        crate::gc::register(&o);
        o
    }

    pub fn is_callable(&self) -> bool {
        matches!(
            self.kind,
            ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_)
        )
    }

    pub fn get_own(&self, key: &str) -> Option<Value> {
        self.props.borrow().get(key).cloned()
    }

    pub fn set_own(&self, key: impl Into<Rc<str>>, v: Value) {
        self.props.borrow_mut().insert(key.into(), v);
    }

    pub fn set_hidden(&self, key: impl Into<Rc<str>>, v: Value) {
        let key = key.into();
        let mut props = self.props.borrow_mut();
        props.insert(key.clone(), v);
        props.set_enumerable(&key, false);
    }

    pub fn has_own(&self, key: &str) -> bool {
        self.props.borrow().contains_key(key)
    }

    pub fn get_chain(&self, key: &str) -> Option<Value> {
        if let Some(v) = self.props.borrow().get(key) {
            return Some(v.clone());
        }
        let mut cur = self.proto.borrow().clone();
        while let Some(o) = cur {
            if let Some(v) = o.props.borrow().get(key) {
                return Some(v.clone());
            }
            cur = o.proto.borrow().clone();
        }
        None
    }

    pub fn function_name(&self) -> Option<Rc<str>> {
        match &self.kind {
            ObjKind::Function(c) => Some(
                self.get_own("name")
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s),
                        _ => None,
                    })
                    .unwrap_or_else(|| c.proto.name.clone()),
            ),
            ObjKind::Native(n) => Some(n.name.clone()),
            ObjKind::Bound(b) => {
                let inner = b
                    .target
                    .as_object()
                    .and_then(|o| o.function_name())
                    .unwrap_or_else(|| "".into());
                Some(format!("bound {inner}").into())
            }
            _ => None,
        }
    }

    pub fn is_accessor(&self) -> bool {
        matches!(self.kind, ObjKind::Accessor(_))
    }

    pub fn array(&self) -> Option<&RefCell<Vec<Value>>> {
        match &self.kind {
            ObjKind::Array(a) => Some(a),
            _ => None,
        }
    }
}

pub fn array_index(key: &str) -> Option<usize> {
    let b = key.as_bytes();
    if b.is_empty() || b.len() > 15 || !b[0].is_ascii_digit() || (b[0] == b'0' && b.len() > 1) {
        return None;
    }
    let mut n = 0usize;
    for &c in b {
        if !c.is_ascii_digit() {
            return None;
        }
        n = n * 10 + (c - b'0') as usize;
    }
    Some(n)
}
