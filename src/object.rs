use crate::bytecode::FunctionProto;
use crate::value::Value;
use crate::vm::Vm;
use indexmap::{IndexMap, IndexSet};
use rustc_hash::FxBuildHasher;
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub type PropMap = IndexMap<Rc<str>, Value, FxBuildHasher>;
pub type NativeFn = fn(&mut Vm, Value, &[Value]) -> Result<Value, Value>;

pub struct Obj {
    pub kind: ObjKind,
    pub props: RefCell<PropMap>,
    pub proto: RefCell<Option<Rc<Obj>>>,
    pub frozen: Cell<bool>,
}

pub enum ObjKind {
    Ordinary,
    Array(RefCell<Vec<Value>>),
    Function(Rc<Closure>),
    Native(Native),
    Bound(Bound),
    Error,
    Map(RefCell<IndexMap<MapKey, Value, FxBuildHasher>>),
    Set(RefCell<IndexSet<MapKey, FxBuildHasher>>),
    Iter(RefCell<IterState>),
}

pub struct Closure {
    pub proto: Rc<FunctionProto>,
    pub upvalues: Box<[Rc<RefCell<Upvalue>>]>,
    pub home: RefCell<Option<Rc<Obj>>>,
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
}

#[derive(Clone)]
pub enum MapKey {
    Undefined,
    Null,
    Bool(bool),
    Num(u64),
    Str(Rc<str>),
    Obj(Rc<Obj>),
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
        }
    }
}

impl Obj {
    pub fn new(kind: ObjKind, proto: Option<Rc<Obj>>) -> Rc<Obj> {
        Rc::new(Obj {
            kind,
            props: RefCell::new(PropMap::default()),
            proto: RefCell::new(proto),
            frozen: Cell::new(false),
        })
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
