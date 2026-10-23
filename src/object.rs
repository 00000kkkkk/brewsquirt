use crate::bytecode::FunctionProto;
use crate::jsstr::JsStr;
use crate::value::Value;
use crate::vm::Vm;
use indexmap::{IndexMap, IndexSet};
use rustc_hash::FxBuildHasher;
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub type NativeFn = fn(&mut Vm, Value, &[Value]) -> Result<Value, Value>;

const INDEX_AT: usize = 12;

pub const NON_ENUM: u8 = 1;
pub const READ_ONLY: u8 = 2;
pub const NON_CONFIG: u8 = 4;

pub const NOT_EXTENSIBLE: u8 = 1;
pub const ELEMENTS_FROZEN: u8 = 2;
pub const ELEMENTS_SEALED: u8 = 4;
pub const HAS_HOLES: u8 = 8;
pub const NAME_DELETED: u8 = 16;
pub const LENGTH_DELETED: u8 = 32;
pub const DETACHED: u8 = 64;
pub const ARGUMENTS: u8 = 128;
pub const SLOW_ELEMENTS: u8 = 64;

#[derive(Default)]
struct PropExtra {
    index: rustc_hash::FxHashMap<JsStr, u32>,
    attrs: rustc_hash::FxHashMap<JsStr, u8>,
    holes: Vec<u64>,
}

impl PropExtra {
    fn is_empty(&self) -> bool {
        self.index.is_empty() && self.attrs.is_empty() && self.holes.is_empty()
    }
}

#[derive(Default)]
pub struct PropMap {
    entries: Vec<(JsStr, Value)>,
    extra: Option<Box<PropExtra>>,
}

impl PropMap {
    fn index(&self) -> Option<&rustc_hash::FxHashMap<JsStr, u32>> {
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
            if x.is_empty() {
                self.extra = None;
            }
        }
    }

    pub fn attrs(&self, key: &str) -> u8 {
        match &self.extra {
            Some(x) if !x.attrs.is_empty() => x.attrs.get(key).copied().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn has_attrs(&self) -> bool {
        self.extra.as_ref().is_some_and(|x| !x.attrs.is_empty())
    }

    pub fn set_attrs(&mut self, key: &str, bits: u8) {
        if bits == 0 {
            if let Some(x) = &mut self.extra {
                x.attrs.remove(key);
                if x.is_empty() {
                    self.extra = None;
                }
            }
            return;
        }
        if let Some(i) = self.find(key) {
            let k = self.entries[i].0.clone();
            self.extra
                .get_or_insert_with(Box::default)
                .attrs
                .insert(k, bits);
        }
    }

    pub fn attr_entries(&self) -> impl Iterator<Item = (&JsStr, u8)> {
        self.extra
            .iter()
            .flat_map(|x| x.attrs.iter().map(|(k, b)| (k, *b)))
    }

    pub fn set_index_attrs(&mut self, key: &JsStr, bits: u8) {
        if bits == 0 {
            if let Some(x) = &mut self.extra {
                x.attrs.remove(&**key);
                if x.is_empty() {
                    self.extra = None;
                }
            }
            return;
        }
        self.extra
            .get_or_insert_with(Box::default)
            .attrs
            .insert(key.clone(), bits);
    }

    pub fn is_enumerable(&self, key: &str) -> bool {
        self.attrs(key) & NON_ENUM == 0
    }

    pub fn set_enumerable(&mut self, key: &str, enumerable: bool) {
        let a = self.attrs(key);
        self.set_attrs(
            key,
            if enumerable {
                a & !NON_ENUM
            } else {
                a | NON_ENUM
            },
        );
    }

    pub fn add_all(&mut self, bits: u8, data_only: bool) {
        if self.entries.is_empty() {
            return;
        }
        let keys: Vec<(JsStr, bool)> = self
            .entries
            .iter()
            .map(|(k, v)| (k.clone(), matches!(v, Value::Object(o) if o.is_accessor())))
            .collect();
        let x = self.extra.get_or_insert_with(Box::default);
        for (k, accessor) in keys {
            if is_private_key(&k) {
                continue;
            }
            let b = if data_only && accessor {
                bits & !READ_ONLY
            } else {
                bits
            };
            *x.attrs.entry(k).or_insert(0) |= b;
        }
    }

    pub fn hide_all(&mut self) {
        self.add_all(NON_ENUM, false);
    }

    pub fn all_have(&self, bits: u8, data_only: bool) -> bool {
        self.entries
            .iter()
            .filter(|(k, _)| !is_private_key(k))
            .all(|(k, v)| {
                let need = if data_only && matches!(v, Value::Object(o) if o.is_accessor()) {
                    bits & !READ_ONLY
                } else {
                    bits
                };
                self.attrs(k) & need == need
            })
    }

    pub fn insert_fast(&mut self, key: JsStr, v: Value, adding_ok: bool) -> bool {
        if key.len() == 9 && &*key == "__proto__" {
            return false;
        }
        if let Some(i) = self.find(&key) {
            if self.extra.is_some() && self.attrs(&key) & READ_ONLY != 0 {
                return false;
            }
            self.entries[i].1 = v;
            return true;
        }
        if !adding_ok {
            return false;
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
        true
    }

    fn order(&self) -> Option<Vec<usize>> {
        let has_index = self.entries.iter().any(|(k, _)| {
            k.as_bytes().first().is_some_and(u8::is_ascii_digit) && array_index(k).is_some()
        });
        if !has_index {
            return None;
        }
        let mut ints: Vec<(usize, usize)> = Vec::new();
        let mut rest: Vec<usize> = Vec::new();
        for (i, (k, _)) in self.entries.iter().enumerate() {
            match array_index(k) {
                Some(n) => ints.push((n, i)),
                None => rest.push(i),
            }
        }
        ints.sort_unstable();
        let mut out: Vec<usize> = ints.into_iter().map(|(_, i)| i).collect();
        out.extend(rest);
        Some(out)
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

    pub fn insert(&mut self, key: JsStr, v: Value) -> Option<Value> {
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
            x.attrs.remove(key);
            if !x.index.is_empty() || x.attrs.is_empty() {
                self.reindex();
            }
        }
        Some(v)
    }

    pub fn is_hole(&self, i: usize) -> bool {
        match &self.extra {
            Some(x) => x
                .holes
                .get(i / 64)
                .is_some_and(|w| w & (1u64 << (i % 64)) != 0),
            None => false,
        }
    }

    pub fn set_hole(&mut self, i: usize, on: bool) {
        if !on {
            if let Some(x) = &mut self.extra {
                if let Some(w) = x.holes.get_mut(i / 64) {
                    *w &= !(1u64 << (i % 64));
                }
            }
            return;
        }
        let x = self.extra.get_or_insert_with(Box::default);
        if x.holes.len() <= i / 64 {
            x.holes.resize(i / 64 + 1, 0);
        }
        x.holes[i / 64] |= 1u64 << (i % 64);
    }

    pub fn mark_holes(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        let x = self.extra.get_or_insert_with(Box::default);
        if x.holes.len() <= (to - 1) / 64 {
            x.holes.resize((to - 1) / 64 + 1, 0);
        }
        for i in from..to {
            x.holes[i / 64] |= 1u64 << (i % 64);
        }
    }

    pub fn truncate_holes(&mut self, len: usize) {
        if let Some(x) = &mut self.extra {
            let words = len.div_ceil(64);
            x.holes.truncate(words);
            if len % 64 != 0 {
                if let Some(w) = x.holes.last_mut() {
                    *w &= (1u64 << (len % 64)) - 1;
                }
            }
        }
    }

    pub fn clear_holes(&mut self) {
        if let Some(x) = &mut self.extra {
            x.holes.clear();
            if x.is_empty() {
                self.extra = None;
            }
        }
    }

    pub fn enumerable(&self) -> impl Iterator<Item = (&JsStr, &Value)> {
        self.iter().filter(|(k, _)| self.is_enumerable(k))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&JsStr, &Value)> {
        let order = self.order();
        (0..self.entries.len()).map(move |i| {
            let j = order.as_ref().map_or(i, |o| o[i]);
            let (k, v) = &self.entries[j];
            (k, v)
        })
    }

    pub fn keys(&self) -> impl Iterator<Item = &JsStr> {
        self.iter().map(|(k, _)| k)
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
    pub flags: Cell<u8>,
    pub(crate) gc_slot: Cell<u32>,
    pub(crate) heap_slot: Cell<u32>,
}

impl Drop for Obj {
    fn drop(&mut self) {
        crate::gc::unregister(self);
    }
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
    ArrayBuffer(Box<BufferData>),
    Typed(Box<View>),
    DataView(Box<View>),
    Global,
    Proxy(Box<ProxyData>),
    Primitive(Value),
    Namespace,
    Arguments(Box<ArgMap>),
    Host(Box<crate::napi::HostData>),
}

pub struct ArgMap {
    pub cells: RefCell<Vec<Option<Rc<UpCell>>>>,
}

impl ArgMap {
    pub fn cell(&self, key: &str) -> Option<Rc<UpCell>> {
        let i = array_index(key)?;
        self.cells.borrow().get(i)?.clone()
    }

    pub fn unmap(&self, key: &str) {
        if let Some(i) = array_index(key) {
            if let Some(slot) = self.cells.borrow_mut().get_mut(i) {
                *slot = None;
            }
        }
    }
}

pub fn wrapper_marker() -> ObjKind {
    ObjKind::Primitive(Value::Undefined)
}

pub fn unwrap_primitive(v: &Value) -> Option<Value> {
    match v {
        Value::Object(o) => match &o.kind {
            ObjKind::Primitive(p) if !p.is_undefined() => Some(p.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub struct ProxyData {
    pub target: RefCell<Option<Rc<Obj>>>,
    pub handler: RefCell<Option<Rc<Obj>>>,
    pub callable: bool,
    pub constructible: bool,
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
    BigInt64,
    BigUint64,
    Float16,
}

pub fn f16_round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let a = x.abs();
    if a >= 65520.0 {
        return f64::INFINITY.copysign(x);
    }
    let e = a.log2().floor().max(-14.0);
    let step = 2f64.powf(e - 10.0);
    let q = a / step;
    let f = q.floor();
    let d = q - f;
    let r = if d > 0.5 || (d == 0.5 && f % 2.0 != 0.0) {
        f + 1.0
    } else {
        f
    };
    (r * step).copysign(x)
}

fn f16_to_f64(h: u16) -> f64 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let frac = (h & 0x3ff) as f64;
    let v = match exp {
        0 => frac * 2f64.powi(-24),
        31 if frac == 0.0 => f64::INFINITY,
        31 => f64::NAN,
        _ => (1.0 + frac / 1024.0) * 2f64.powi(exp - 15),
    };
    sign * v
}

fn f64_to_f16(x: f64) -> u16 {
    if x.is_nan() {
        return 0x7e00;
    }
    let sign: u16 = if x.is_sign_negative() { 0x8000 } else { 0 };
    let r = f16_round(x.abs());
    if r == 0.0 {
        return sign;
    }
    if r.is_infinite() {
        return sign | 0x7c00;
    }
    if r < 2f64.powi(-14) {
        return sign | (r / 2f64.powi(-24)).round() as u16;
    }
    let bits = r.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let mant = ((bits >> 42) & 0x3ff) as u16;
    sign | (((e + 15) as u16) << 10) | mant
}

fn modulo32(n: f64) -> u32 {
    if !n.is_finite() {
        return 0;
    }
    n.trunc().rem_euclid(4294967296.0) as u32
}

fn big_bits(v: &Value) -> u64 {
    match v {
        Value::BigInt(b) => {
            let m = crate::bigint::as_uint_n(64, b);
            num_traits::ToPrimitive::to_u64(&m).unwrap_or(0)
        }
        _ => 0,
    }
}

impl TypedKind {
    pub const ALL: [TypedKind; 12] = [
        TypedKind::Int8,
        TypedKind::Uint8,
        TypedKind::Uint8Clamped,
        TypedKind::Int16,
        TypedKind::Uint16,
        TypedKind::Int32,
        TypedKind::Uint32,
        TypedKind::Float32,
        TypedKind::Float64,
        TypedKind::BigInt64,
        TypedKind::BigUint64,
        TypedKind::Float16,
    ];

    pub fn size(self) -> usize {
        match self {
            TypedKind::Int8 | TypedKind::Uint8 | TypedKind::Uint8Clamped => 1,
            TypedKind::Int16 | TypedKind::Uint16 | TypedKind::Float16 => 2,
            TypedKind::Int32 | TypedKind::Uint32 | TypedKind::Float32 => 4,
            TypedKind::Float64 | TypedKind::BigInt64 | TypedKind::BigUint64 => 8,
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
            TypedKind::BigInt64 => "BigInt64Array",
            TypedKind::BigUint64 => "BigUint64Array",
            TypedKind::Float16 => "Float16Array",
        }
    }

    pub fn is_float(self) -> bool {
        matches!(
            self,
            TypedKind::Float16 | TypedKind::Float32 | TypedKind::Float64
        )
    }

    pub fn is_big(self) -> bool {
        matches!(self, TypedKind::BigInt64 | TypedKind::BigUint64)
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
            TypedKind::BigInt64 => rd!(i64, 8),
            TypedKind::BigUint64 => rd!(u64, 8),
            TypedKind::Float16 => {
                let mut a = [0u8; 2];
                a.copy_from_slice(&b[..2]);
                f16_to_f64(if little {
                    u16::from_le_bytes(a)
                } else {
                    u16::from_be_bytes(a)
                })
            }
        }
    }

    pub fn read_value(self, b: &[u8], little: bool) -> Value {
        if !self.is_big() {
            return Value::Number(self.read(b, little));
        }
        let mut a = [0u8; 8];
        a.copy_from_slice(&b[..8]);
        let bits = if little {
            u64::from_le_bytes(a)
        } else {
            u64::from_be_bytes(a)
        };
        let n = if self == TypedKind::BigInt64 {
            num_bigint::BigInt::from(bits as i64)
        } else {
            num_bigint::BigInt::from(bits)
        };
        Value::BigInt(Rc::new(n))
    }

    pub fn write_value(self, b: &mut [u8], v: &Value, little: bool) {
        if self.is_big() {
            let bits = big_bits(v);
            let bytes = if little {
                bits.to_le_bytes()
            } else {
                bits.to_be_bytes()
            };
            b[..8].copy_from_slice(&bytes);
            return;
        }
        let n = match v {
            Value::Number(n) => *n,
            _ => f64::NAN,
        };
        self.write(b, n, little);
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
            TypedKind::BigInt64 | TypedKind::BigUint64 => wr!(0u64),
            TypedKind::Float16 => wr!(f64_to_f16(n)),
        }
    }
}

pub struct BufferData {
    pub bytes: RefCell<Vec<u8>>,
    pub max: Option<usize>,
    pub shared: bool,
    pub external: bool,
    pub shared_mem: Option<std::sync::Arc<crate::builtins::threads::SharedMem>>,
}

impl BufferData {
    #[inline]
    pub fn sync_shared(&self) {
        if let Some(m) = &self.shared_mem {
            let n = m.len.load(std::sync::atomic::Ordering::Acquire);
            if let Ok(mut v) = self.bytes.try_borrow_mut() {
                if v.len() < n && n <= v.capacity() {
                    unsafe { v.set_len(n) };
                }
            }
        }
    }
}

impl Drop for BufferData {
    fn drop(&mut self) {
        if self.external {
            std::mem::forget(std::mem::take(self.bytes.get_mut()));
        }
    }
}

impl std::ops::Deref for BufferData {
    type Target = RefCell<Vec<u8>>;

    fn deref(&self) -> &RefCell<Vec<u8>> {
        &self.bytes
    }
}

pub struct View {
    pub kind: TypedKind,
    pub buffer: Rc<Obj>,
    pub offset: usize,
    pub length: usize,
    pub tracking: bool,
}

impl View {
    pub fn detached(&self) -> bool {
        self.buffer.flags.get() & DETACHED != 0
    }

    fn len_with(&self, buf_len: usize) -> usize {
        let size = self.kind.size();
        if self.tracking {
            if self.offset > buf_len {
                0
            } else {
                (buf_len - self.offset) / size
            }
        } else if self.offset + self.length * size > buf_len {
            0
        } else {
            self.length
        }
    }

    pub fn out_of_bounds(&self) -> bool {
        if self.detached() {
            return true;
        }
        let buf_len = self.store().borrow().len();
        if self.tracking {
            self.offset > buf_len
        } else {
            self.offset + self.length * self.kind.size() > buf_len
        }
    }

    pub fn len(&self) -> usize {
        self.len_with(self.store().borrow().len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn byte_len(&self) -> usize {
        self.len() * self.kind.size()
    }

    pub fn store(&self) -> &RefCell<Vec<u8>> {
        match &self.buffer.kind {
            ObjKind::ArrayBuffer(b) => {
                b.sync_shared();
                &b.bytes
            }
            _ => unreachable!("view over a non-buffer"),
        }
    }

    pub fn get(&self, i: usize) -> Option<Value> {
        let b = self.store().borrow();
        if i >= self.len_with(b.len()) {
            return None;
        }
        let size = self.kind.size();
        let start = self.offset + i * size;
        b.get(start..start + size)
            .map(|x| self.kind.read_value(x, true))
    }

    pub fn get_f64(&self, i: usize) -> Option<f64> {
        let b = self.store().borrow();
        if i >= self.len_with(b.len()) {
            return None;
        }
        let size = self.kind.size();
        let start = self.offset + i * size;
        b.get(start..start + size).map(|x| self.kind.read(x, true))
    }

    pub fn set(&self, i: usize, v: &Value) {
        let mut b = self.store().borrow_mut();
        if i >= self.len_with(b.len()) {
            return;
        }
        let size = self.kind.size();
        let start = self.offset + i * size;
        if let Some(x) = b.get_mut(start..start + size) {
            self.kind.write_value(x, v, true);
        }
    }

    pub fn set_f64(&self, i: usize, n: f64) {
        let mut b = self.store().borrow_mut();
        if i >= self.len_with(b.len()) {
            return;
        }
        let size = self.kind.size();
        let start = self.offset + i * size;
        if let Some(x) = b.get_mut(start..start + size) {
            self.kind.write(x, n, true);
        }
    }

    pub fn values(&self) -> Vec<Value> {
        (0..self.len()).filter_map(|i| self.get(i)).collect()
    }

    pub fn bytes(&self) -> Vec<u8> {
        let b = self.store().borrow();
        let end = (self.offset + self.byte_len()).min(b.len());
        b[self.offset.min(end)..end].to_vec()
    }
}

pub enum WeakHold {
    Obj(std::rc::Weak<Obj>),
    Sym(Rc<Symbol>),
}

impl WeakHold {
    pub fn alive(&self) -> bool {
        match self {
            WeakHold::Obj(w) => w.strong_count() > 0,
            WeakHold::Sym(_) => true,
        }
    }
}

pub type WeakEntries = rustc_hash::FxHashMap<usize, (WeakHold, Value)>;
pub type WeakKeys = rustc_hash::FxHashMap<usize, WeakHold>;

pub struct RegExpObj {
    pub data: RefCell<Rc<crate::regexp::RegexData>>,
    pub last_index: RefCell<Value>,
}

pub struct Symbol {
    pub description: Option<JsStr>,
    pub key: JsStr,
}

pub const SYMBOL_PREFIX: char = '\u{0}';
pub const ITER_KEY: &str = "\u{0}@@iterator:Symbol.iterator";
pub const ASYNC_ITER_KEY: &str = "\u{0}@@asyncIterator:Symbol.asyncIterator";
pub const TO_STRING_TAG_KEY: &str = "\u{0}@@toStringTag:Symbol.toStringTag";
pub const TO_PRIMITIVE_KEY: &str = "\u{0}@@toPrimitive:Symbol.toPrimitive";
pub const HAS_INSTANCE_KEY: &str = "\u{0}@@hasInstance:Symbol.hasInstance";
pub const SPECIES_KEY: &str = "\u{0}@@species:Symbol.species";
pub const IS_CONCAT_SPREADABLE_KEY: &str = "\u{0}@@isConcatSpreadable:Symbol.isConcatSpreadable";
pub const UNSCOPABLES_KEY: &str = "\u{0}@@unscopables:Symbol.unscopables";
pub const MATCH_KEY: &str = "\u{0}@@match:Symbol.match";
pub const MATCH_ALL_KEY: &str = "\u{0}@@matchAll:Symbol.matchAll";
pub const REPLACE_KEY: &str = "\u{0}@@replace:Symbol.replace";
pub const SEARCH_KEY: &str = "\u{0}@@search:Symbol.search";
pub const SPLIT_KEY: &str = "\u{0}@@split:Symbol.split";

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
    pub initialized: bool,
}

impl Default for PromiseData {
    fn default() -> Self {
        PromiseData {
            state: PromiseState::Pending,
            reactions: Vec::new(),
            handled: false,
            initialized: false,
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
    pub handlers: Vec<(usize, usize, bool)>,
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
    pub new_target: RefCell<Option<Value>>,
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
    pub name: JsStr,
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
    Array(Rc<Obj>, usize, u8),
    Str(JsStr, usize),
    MapEntries(Rc<Obj>, usize),
    MapKeys(Rc<Obj>, usize),
    MapValues(Rc<Obj>, usize),
    SetValues(Rc<Obj>, usize),
    SetEntries(Rc<Obj>, usize),
    List(Vec<Value>, usize),
    ForIn(Value, Vec<Value>, usize),
    Protocol(Value, Value),
    Typed(Rc<Obj>, usize, u8),
}

impl IterState {
    pub fn family(&self) -> usize {
        match self {
            IterState::MapEntries(..) | IterState::MapKeys(..) | IterState::MapValues(..) => 1,
            IterState::SetValues(..) | IterState::SetEntries(..) => 2,
            IterState::Str(..) => 3,
            _ => 0,
        }
    }
}

#[derive(Clone)]
pub enum MapKey {
    Undefined,
    Null,
    Bool(bool),
    Num(u64),
    Str(JsStr),
    Obj(Rc<Obj>),
    Sym(Rc<Symbol>),
    Big(Rc<num_bigint::BigInt>),
}

impl MapKey {
    pub fn from_value(v: &Value) -> MapKey {
        match v {
            Value::Undefined | Value::Uninit => MapKey::Undefined,
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
            Value::BigInt(b) => MapKey::Big(b.clone()),
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
            MapKey::Big(b) => Value::BigInt(b.clone()),
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
            (MapKey::Sym(a), MapKey::Sym(b)) => a.key == b.key,
            (MapKey::Big(a), MapKey::Big(b)) => a == b,
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
                s.key.hash(state)
            }
            MapKey::Big(b) => {
                7u8.hash(state);
                b.hash(state)
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
            flags: Cell::new(0),
            gc_slot: Cell::new(u32::MAX),
            heap_slot: Cell::new(u32::MAX),
        });
        crate::gc::register(&o);
        o
    }

    pub fn is_callable(&self) -> bool {
        match &self.kind {
            ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_) => true,
            ObjKind::Proxy(p) => p.callable,
            _ => false,
        }
    }

    pub fn get_own(&self, key: &str) -> Option<Value> {
        self.props.borrow().get(key).cloned()
    }

    pub fn set_own(&self, key: impl Into<JsStr>, v: Value) {
        self.props.borrow_mut().insert(key.into(), v);
    }

    pub fn set_hidden(&self, key: impl Into<JsStr>, v: Value) {
        let key = key.into();
        let mut props = self.props.borrow_mut();
        props.insert(key.clone(), v);
        props.set_enumerable(&key, false);
    }

    pub fn set_with_attrs(&self, key: impl Into<JsStr>, v: Value, bits: u8) {
        let key = key.into();
        let mut props = self.props.borrow_mut();
        props.insert(key.clone(), v);
        props.set_attrs(&key, bits);
    }

    pub fn is_extensible(&self) -> bool {
        self.flags.get() & NOT_EXTENSIBLE == 0
    }

    pub fn is_hole(&self, i: usize) -> bool {
        self.flags.get() & HAS_HOLES != 0 && self.props.borrow().is_hole(i)
    }

    pub fn mark_holes(&self, from: usize, to: usize) {
        if from < to {
            self.flags.set(self.flags.get() | HAS_HOLES);
            self.props.borrow_mut().mark_holes(from, to);
        }
    }

    pub fn fill_hole(&self, i: usize) {
        if self.flags.get() & HAS_HOLES != 0 {
            self.props.borrow_mut().set_hole(i, false);
        }
    }

    pub fn truncate_holes(&self, len: usize) {
        if self.flags.get() & HAS_HOLES != 0 {
            self.props.borrow_mut().truncate_holes(len);
        }
    }

    pub fn clear_holes(&self) {
        if self.flags.get() & HAS_HOLES != 0 {
            self.props.borrow_mut().clear_holes();
            self.flags.set(self.flags.get() & !HAS_HOLES);
        }
    }

    pub fn has_holes(&self) -> bool {
        self.flags.get() & HAS_HOLES != 0
    }

    pub fn has_own(&self, key: &str) -> bool {
        self.props.borrow().contains_key(key)
    }

    pub fn get_chain_plain(&self, key: &str) -> Option<Value> {
        if let Some(v) = self.props.borrow().get(key) {
            return Some(v.clone());
        }
        let mut cur = self.proto.borrow().clone();
        while let Some(o) = cur {
            if !matches!(o.kind, ObjKind::Ordinary) {
                return None;
            }
            if let Some(v) = o.props.borrow().get(key) {
                return Some(v.clone());
            }
            cur = o.proto.borrow().clone();
        }
        Some(Value::Undefined)
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

    pub fn function_name(&self) -> Option<JsStr> {
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
                let resolver = inner.is_empty()
                    && matches!(&b.this, Value::Object(p) if matches!(p.kind, ObjKind::Promise(_)));
                if resolver {
                    return Some("".into());
                }
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
    (n < 4294967295).then_some(n)
}
