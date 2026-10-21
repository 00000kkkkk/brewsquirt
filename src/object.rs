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
pub struct PropMap {
    entries: Vec<(Rc<str>, Value)>,
    index: Option<Box<rustc_hash::FxHashMap<Rc<str>, u32>>>,
}

impl PropMap {
    fn find(&self, key: &str) -> Option<usize> {
        if let Some(ix) = &self.index {
            return ix.get(key).map(|&i| i as usize);
        }
        self.entries.iter().position(|(k, _)| **k == *key)
    }

    fn reindex(&mut self) {
        if self.entries.len() > INDEX_AT {
            let mut ix = rustc_hash::FxHashMap::default();
            ix.reserve(self.entries.len());
            for (i, (k, _)) in self.entries.iter().enumerate() {
                ix.insert(k.clone(), i as u32);
            }
            self.index = Some(Box::new(ix));
        } else {
            self.index = None;
        }
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
        match &mut self.index {
            Some(ix) => {
                ix.insert(key.clone(), i);
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
        if self.index.is_some() {
            self.reindex();
        }
        Some(v)
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
        self.index = None;
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
