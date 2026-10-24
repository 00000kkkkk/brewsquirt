use super::{arg, method};
use crate::jsstr::JsStr;
use crate::object::{MapKey, Obj, ObjKind, TypedKind, View};
use crate::value::{Value, from16, units16};
use crate::vm::{ErrorKind, Vm};
use num_bigint::{BigInt, Sign};
use rustc_hash::FxHashMap;
use std::rc::Rc;

type R = Result<Value, Value>;

const VERSION: u8 = 15;
const DATA_VIEW_INDEX: u64 = 9;
const BUFFER_INDEX: u64 = 10;
const MAX_ELEMENTS: u64 = 1 << 26;

fn host_index(kind: TypedKind) -> u64 {
    match kind {
        TypedKind::Int8 => 0,
        TypedKind::Uint8 => 1,
        TypedKind::Uint8Clamped => 2,
        TypedKind::Int16 => 3,
        TypedKind::Uint16 => 4,
        TypedKind::Int32 => 5,
        TypedKind::Uint32 => 6,
        TypedKind::Float32 => 7,
        TypedKind::Float64 => 8,
        TypedKind::BigInt64 => 11,
        TypedKind::BigUint64 => 12,
        TypedKind::Float16 => 13,
    }
}

fn host_kind(index: u64) -> Option<TypedKind> {
    Some(match index {
        0 => TypedKind::Int8,
        1 | BUFFER_INDEX => TypedKind::Uint8,
        2 => TypedKind::Uint8Clamped,
        3 => TypedKind::Int16,
        4 => TypedKind::Uint16,
        5 => TypedKind::Int32,
        6 => TypedKind::Uint32,
        7 => TypedKind::Float32,
        8 => TypedKind::Float64,
        11 => TypedKind::BigInt64,
        12 => TypedKind::BigUint64,
        13 => TypedKind::Float16,
        _ => return None,
    })
}

fn view_subtag(kind: TypedKind) -> u8 {
    match kind {
        TypedKind::Int8 => b'b',
        TypedKind::Uint8 => b'B',
        TypedKind::Uint8Clamped => b'C',
        TypedKind::Int16 => b'w',
        TypedKind::Uint16 => b'W',
        TypedKind::Int32 => b'd',
        TypedKind::Uint32 => b'D',
        TypedKind::Float16 => b'h',
        TypedKind::Float32 => b'f',
        TypedKind::Float64 => b'F',
        TypedKind::BigInt64 => b'q',
        TypedKind::BigUint64 => b'Q',
    }
}

fn subtag_kind(tag: u8) -> Option<TypedKind> {
    TypedKind::ALL.into_iter().find(|k| view_subtag(*k) == tag)
}

const REGEXP_FLAGS: [(char, u64); 9] = [
    ('d', 128),
    ('g', 1),
    ('i', 2),
    ('l', 64),
    ('m', 4),
    ('s', 32),
    ('u', 16),
    ('v', 256),
    ('y', 8),
];

fn array_index(k: &str) -> Option<u32> {
    if k.is_empty() || k.len() > 10 || (k.len() > 1 && k.starts_with('0')) {
        return None;
    }
    let n: u64 = k.parse().ok()?;
    (n < u32::MAX as u64).then_some(n as u32)
}

fn varint_len(mut n: u64) -> usize {
    let mut len = 1;
    while n >= 0x80 {
        n >>= 7;
        len += 1;
    }
    len
}

fn uncloneable(vm: &mut Vm, v: &Value) -> Value {
    let text = match v {
        Value::Object(_) if v.is_callable() => match vm.to_string(v) {
            Ok(s) => s.to_string(),
            Err(e) => return e,
        },
        Value::Object(o) => {
            let name = match o.get_chain("constructor") {
                Some(c @ Value::Object(_)) => match vm.get_prop(&c, "name") {
                    Ok(Value::Str(s)) if !s.is_empty() => s.to_string(),
                    _ => "Object".to_string(),
                },
                _ => "Object".to_string(),
            };
            format!("#<{name}>")
        }
        _ => crate::inspect::inspect_plain(v),
    };
    vm.make_error(ErrorKind::Error, format!("{text} could not be cloned."))
}

struct Writer<'a> {
    out: Vec<u8>,
    ids: FxHashMap<*const Obj, u32>,
    buffer_proto: Option<&'a Rc<Obj>>,
}

impl Writer<'_> {
    fn varint(&mut self, mut n: u64) {
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                self.out.push(b);
                return;
            }
            self.out.push(b | 0x80);
        }
    }

    fn double(&mut self, n: f64) {
        self.out.extend_from_slice(&n.to_le_bytes());
    }

    fn number(&mut self, n: f64) {
        let int = n.fract() == 0.0
            && n >= i32::MIN as f64
            && n <= i32::MAX as f64
            && !(n == 0.0 && n.is_sign_negative());
        if int {
            let i = n as i32;
            self.out.push(b'I');
            self.varint(((i << 1) ^ (i >> 31)) as u32 as u64);
        } else {
            self.out.push(b'N');
            self.double(n);
        }
    }

    fn string(&mut self, s: &str) {
        if s.is_ascii() {
            self.out.push(b'"');
            self.varint(s.len() as u64);
            self.out.extend_from_slice(s.as_bytes());
            return;
        }
        let units = units16(s);
        if units.iter().all(|u| *u <= 0xff) {
            self.out.push(b'"');
            self.varint(units.len() as u64);
            self.out.extend(units.iter().map(|u| *u as u8));
            return;
        }
        let bytes = units.len() as u64 * 2;
        if (self.out.len() + 1 + varint_len(bytes)) & 1 == 1 {
            self.out.push(0);
        }
        self.out.push(b'c');
        self.varint(bytes);
        for u in units {
            self.out.extend_from_slice(&u.to_le_bytes());
        }
    }

    fn bigint(&mut self, b: &BigInt) {
        let (sign, mut digits) = b.to_bytes_le();
        if digits.iter().all(|d| *d == 0) {
            digits.clear();
        }
        while digits.len() % 8 != 0 {
            digits.push(0);
        }
        self.varint(((digits.len() as u64) << 1) | (sign == Sign::Minus) as u64);
        self.out.extend_from_slice(&digits);
    }

    fn key(&mut self, k: &str) {
        match array_index(k) {
            Some(i) => self.number(i as f64),
            None => self.string(k),
        }
    }

    fn value(&mut self, vm: &mut Vm, v: &Value) -> Result<(), Value> {
        match v {
            Value::Undefined | Value::Uninit => self.out.push(b'_'),
            Value::Null => self.out.push(b'0'),
            Value::Bool(true) => self.out.push(b'T'),
            Value::Bool(false) => self.out.push(b'F'),
            Value::Number(n) => self.number(*n),
            Value::BigInt(b) => {
                self.out.push(b'Z');
                self.bigint(b);
            }
            Value::Str(s) => self.string(s.as_str()),
            Value::Symbol(_) => return Err(uncloneable(vm, v)),
            Value::Object(o) => return self.object(vm, v, o),
        }
        Ok(())
    }

    fn object(&mut self, vm: &mut Vm, v: &Value, o: &Rc<Obj>) -> Result<(), Value> {
        let ptr = Rc::as_ptr(o);
        if let Some(id) = self.ids.get(&ptr) {
            let id = *id as u64;
            self.out.push(b'^');
            self.varint(id);
            return Ok(());
        }
        if v.is_callable() {
            return Err(uncloneable(vm, v));
        }
        let id = self.ids.len() as u32;
        self.ids.insert(ptr, id);
        match &o.kind {
            ObjKind::Array(_) => self.array(vm, v, o),
            ObjKind::Ordinary | ObjKind::Arguments(_) => self.plain(vm, v),
            ObjKind::Date(t) => {
                self.out.push(b'D');
                self.double(t.get());
                Ok(())
            }
            ObjKind::RegExp(r) => {
                let (source, flags) = {
                    let d = r.data.borrow();
                    (d.source.to_string(), d.flags.to_string())
                };
                self.out.push(b'R');
                self.string(&source);
                let bits = REGEXP_FLAGS
                    .iter()
                    .filter(|(c, _)| flags.contains(*c))
                    .map(|(_, b)| b)
                    .sum();
                self.varint(bits);
                Ok(())
            }
            ObjKind::Map(m) => {
                let entries: Vec<(Value, Value)> = m
                    .borrow()
                    .iter()
                    .map(|(k, x)| (k.to_value(), x.clone()))
                    .collect();
                self.out.push(b';');
                for (k, x) in &entries {
                    self.value(vm, k)?;
                    self.value(vm, x)?;
                }
                self.out.push(b':');
                self.varint(entries.len() as u64 * 2);
                Ok(())
            }
            ObjKind::Set(s) => {
                let items: Vec<Value> = s.borrow().iter().map(|k| k.to_value()).collect();
                self.out.push(b'\'');
                for x in &items {
                    self.value(vm, x)?;
                }
                self.out.push(b',');
                self.varint(items.len() as u64);
                Ok(())
            }
            ObjKind::ArrayBuffer(b) => {
                if b.shared {
                    return Err(uncloneable(vm, v));
                }
                if super::typed::is_detached(o) {
                    return Err(vm.make_error(
                        ErrorKind::Error,
                        "An ArrayBuffer is detached and could not be cloned.",
                    ));
                }
                let bytes = b.bytes.borrow().clone();
                match b.max {
                    Some(max) => {
                        self.out.push(b'~');
                        self.varint(bytes.len() as u64);
                        self.varint(max as u64);
                    }
                    None => {
                        self.out.push(b'B');
                        self.varint(bytes.len() as u64);
                    }
                }
                self.out.extend_from_slice(&bytes);
                Ok(())
            }
            ObjKind::Typed(t) => {
                let is_buffer = self
                    .buffer_proto
                    .is_some_and(|p| o.proto.borrow().as_ref().is_some_and(|q| Rc::ptr_eq(p, q)));
                let index = if is_buffer {
                    BUFFER_INDEX
                } else {
                    host_index(t.kind)
                };
                self.host(index, t);
                Ok(())
            }
            ObjKind::DataView(t) => {
                self.host(DATA_VIEW_INDEX, t);
                Ok(())
            }
            ObjKind::Primitive(p) => {
                match p {
                    Value::Number(n) => {
                        self.out.push(b'n');
                        self.double(*n);
                    }
                    Value::Bool(true) => self.out.push(b'y'),
                    Value::Bool(false) => self.out.push(b'x'),
                    Value::Str(s) => {
                        self.out.push(b's');
                        self.string(s.as_str());
                    }
                    Value::BigInt(b) => {
                        self.out.push(b'z');
                        self.bigint(b);
                    }
                    _ => return Err(uncloneable(vm, v)),
                }
                Ok(())
            }
            ObjKind::Error => self.error(vm, v, o),
            _ => Err(uncloneable(vm, v)),
        }
    }

    fn host(&mut self, index: u64, t: &View) {
        let bytes = if t.out_of_bounds() {
            Vec::new()
        } else {
            let store = t.store().borrow();
            let end = (t.offset + t.byte_len()).min(store.len());
            store[t.offset.min(end)..end].to_vec()
        };
        self.out.push(b'\\');
        self.varint(index);
        self.varint(bytes.len() as u64);
        self.out.extend_from_slice(&bytes);
    }

    fn plain(&mut self, vm: &mut Vm, v: &Value) -> Result<(), Value> {
        self.out.push(b'o');
        let entries = vm.own_entries_resolved(v, false)?;
        for (k, x) in &entries {
            self.key(k.as_str());
            self.value(vm, x)?;
        }
        self.out.push(b'{');
        self.varint(entries.len() as u64);
        Ok(())
    }

    fn array(&mut self, vm: &mut Vm, v: &Value, o: &Rc<Obj>) -> Result<(), Value> {
        let ObjKind::Array(a) = &o.kind else {
            return Ok(());
        };
        let items = a.borrow().clone();
        let len = items.len() as u64;
        let entries = vm.own_entries_resolved(v, false)?;
        if o.has_holes() && (0..items.len()).any(|i| o.is_hole(i)) {
            self.out.push(b'a');
            self.varint(len);
            for (k, x) in &entries {
                self.key(k.as_str());
                self.value(vm, x)?;
            }
            self.out.push(b'@');
            self.varint(entries.len() as u64);
            self.varint(len);
            return Ok(());
        }
        self.out.push(b'A');
        self.varint(len);
        for x in items {
            let x = vm.resolve_accessor(x, v)?;
            self.value(vm, &x)?;
        }
        let extra: Vec<&(JsStr, Value)> = entries
            .iter()
            .filter(|(k, _)| array_index(k.as_str()).is_none_or(|i| i as u64 >= len))
            .collect();
        for (k, x) in &extra {
            self.key(k.as_str());
            self.value(vm, x)?;
        }
        self.out.push(b'$');
        self.varint(extra.len() as u64);
        self.varint(len);
        Ok(())
    }

    fn error(&mut self, vm: &mut Vm, v: &Value, o: &Rc<Obj>) -> Result<(), Value> {
        self.out.push(b'r');
        let name = vm.get_prop(v, "name")?;
        let name = vm.to_string(&name)?;
        let tag = match name.as_str() {
            "EvalError" => Some(b'E'),
            "RangeError" => Some(b'R'),
            "ReferenceError" => Some(b'F'),
            "SyntaxError" => Some(b'S'),
            "TypeError" => Some(b'T'),
            "URIError" => Some(b'U'),
            _ => None,
        };
        if let Some(tag) = tag {
            self.out.push(tag);
        }
        if let Some(m) = o.get_own("message") {
            if !matches!(&m, Value::Object(a) if matches!(a.kind, ObjKind::Accessor(_))) {
                let m = vm.to_string(&m)?;
                self.out.push(b'm');
                self.string(m.as_str());
            }
        }
        if let Value::Str(s) = vm.get_prop(v, "stack")? {
            self.out.push(b's');
            self.string(s.as_str());
        }
        if let Some(c) = o.get_own("cause") {
            let c = vm.resolve_accessor(c, v)?;
            self.out.push(b'c');
            self.value(vm, &c)?;
        }
        self.out.push(b'.');
        Ok(())
    }
}

pub fn serialize(vm: &mut Vm, v: &Value, buffer_proto: Option<&Rc<Obj>>) -> Result<Vec<u8>, Value> {
    let mut w = Writer {
        out: vec![0xff, VERSION],
        ids: FxHashMap::default(),
        buffer_proto,
    };
    w.value(vm, v)?;
    Ok(w.out)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    version: u64,
    objects: Vec<Value>,
    buffer_proto: Option<Rc<Obj>>,
}

type Read<T> = Result<Option<T>, Value>;

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(b)
    }

    fn varint(&mut self) -> Option<u64> {
        let mut n = 0u64;
        let mut shift = 0;
        loop {
            let b = self.byte()?;
            if shift < 64 {
                n |= ((b & 0x7f) as u64) << shift;
            }
            shift += 7;
            if b & 0x80 == 0 {
                return Some(n);
            }
        }
    }

    fn raw(&mut self, n: u64) -> Option<&[u8]> {
        let n = usize::try_from(n).ok()?;
        let end = self.at.checked_add(n)?;
        let out = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(out)
    }

    fn double(&mut self) -> Option<f64> {
        let b = self.raw(8)?;
        Some(f64::from_le_bytes(b.try_into().ok()?))
    }

    fn peek_tag(&mut self) -> Option<u8> {
        while self.bytes.get(self.at) == Some(&0) {
            self.at += 1;
        }
        self.bytes.get(self.at).copied()
    }

    fn tag(&mut self) -> Option<u8> {
        let t = self.peek_tag()?;
        self.at += 1;
        Some(t)
    }

    fn reserve(&mut self) -> usize {
        self.objects.push(Value::Undefined);
        self.objects.len() - 1
    }

    fn bigint(&mut self) -> Option<BigInt> {
        let bits = self.varint()?;
        let digits = self.raw(bits >> 1)?;
        let sign = if bits & 1 == 1 {
            Sign::Minus
        } else {
            Sign::Plus
        };
        Some(BigInt::from_bytes_le(sign, digits))
    }

    fn one_byte(&mut self) -> Option<String> {
        let n = self.varint()?;
        Some(self.raw(n)?.iter().map(|b| *b as char).collect())
    }

    fn two_byte(&mut self) -> Option<String> {
        let n = self.varint()?;
        if n % 2 != 0 {
            return None;
        }
        let units: Vec<u16> = self
            .raw(n)?
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        Some(from16(&units))
    }

    fn utf8(&mut self) -> Option<String> {
        let n = self.varint()?;
        Some(String::from_utf8_lossy(self.raw(n)?).into_owned())
    }

    fn string(&mut self) -> Option<String> {
        match self.tag()? {
            b'"' => self.one_byte(),
            b'c' => self.two_byte(),
            b'S' => self.utf8(),
            _ => None,
        }
    }

    fn key(&mut self, vm: &mut Vm) -> Read<JsStr> {
        let Some(k) = self.value(vm)? else {
            return Ok(None);
        };
        Ok(match k {
            Value::Str(s) => Some(s),
            Value::Number(_) => Some(vm.to_string(&k)?),
            _ => None,
        })
    }

    fn properties(&mut self, vm: &mut Vm, target: &Rc<Obj>, end: u8) -> Read<()> {
        loop {
            if self.peek_tag() == Some(end) {
                self.at += 1;
                return Ok(Some(()));
            }
            let Some(k) = self.key(vm)? else {
                return Ok(None);
            };
            let Some(x) = self.value(vm)? else {
                return Ok(None);
            };
            let index = array_index(k.as_str());
            if let (ObjKind::Array(a), Some(i)) = (&target.kind, index) {
                let i = i as usize;
                if i < a.borrow().len() {
                    a.borrow_mut()[i] = x;
                    target.fill_hole(i);
                    continue;
                }
            }
            target.set_own(k, x);
        }
    }

    fn value(&mut self, vm: &mut Vm) -> Read<Value> {
        let Some(tag) = self.tag() else {
            return Ok(None);
        };
        Ok(Some(match tag {
            b'_' => Value::Undefined,
            b'0' => Value::Null,
            b'T' => Value::Bool(true),
            b'F' => Value::Bool(false),
            b'I' => {
                let Some(z) = self.varint() else {
                    return Ok(None);
                };
                let z = z as u32;
                Value::Number(((z >> 1) as i32 ^ -((z & 1) as i32)) as f64)
            }
            b'U' => match self.varint() {
                Some(n) => Value::Number(n as u32 as f64),
                None => return Ok(None),
            },
            b'N' => match self.double() {
                Some(n) => Value::Number(n),
                None => return Ok(None),
            },
            b'Z' => match self.bigint() {
                Some(b) => Value::BigInt(Rc::new(b)),
                None => return Ok(None),
            },
            b'"' | b'c' | b'S' => {
                let s = match tag {
                    b'"' => self.one_byte(),
                    b'c' => self.two_byte(),
                    _ => self.utf8(),
                };
                match s {
                    Some(s) => Value::Str(s.into()),
                    None => return Ok(None),
                }
            }
            b'^' => {
                let Some(id) = self.varint() else {
                    return Ok(None);
                };
                match self.objects.get(id as usize) {
                    Some(v) => v.clone(),
                    None => return Ok(None),
                }
            }
            b'?' => {
                if self.varint().is_none() {
                    return Ok(None);
                }
                return self.value(vm);
            }
            b'o' => {
                let o = vm.new_object();
                let out = Value::Object(o.clone());
                self.objects.push(out.clone());
                if self.properties(vm, &o, b'{')?.is_none() || self.varint().is_none() {
                    return Ok(None);
                }
                out
            }
            b'A' | b'a' => {
                let Some(len) = self.varint() else {
                    return Ok(None);
                };
                if len > MAX_ELEMENTS {
                    return Ok(None);
                }
                let out = vm.new_array(vec![Value::Undefined; len as usize]);
                let Value::Object(o) = &out else {
                    return Ok(None);
                };
                let o = o.clone();
                self.objects.push(out.clone());
                if tag == b'A' {
                    for i in 0..len as usize {
                        if self.peek_tag() == Some(b'-') {
                            self.at += 1;
                            o.mark_holes(i, i + 1);
                            continue;
                        }
                        let Some(x) = self.value(vm)? else {
                            return Ok(None);
                        };
                        if let ObjKind::Array(a) = &o.kind {
                            a.borrow_mut()[i] = x;
                        }
                    }
                } else if len > 0 {
                    o.mark_holes(0, len as usize);
                }
                let end = if tag == b'A' { b'$' } else { b'@' };
                if self.properties(vm, &o, end)?.is_none()
                    || self.varint().is_none()
                    || self.varint().is_none()
                {
                    return Ok(None);
                }
                out
            }
            b'D' => {
                let Some(t) = self.double() else {
                    return Ok(None);
                };
                let o = Obj::new(
                    ObjKind::Date(std::cell::Cell::new(t)),
                    Some(vm.realm.date_proto.clone()),
                );
                let out = Value::Object(o);
                self.objects.push(out.clone());
                out
            }
            b'y' | b'x' | b'n' | b's' | b'z' => {
                let p = match tag {
                    b'y' => Value::Bool(true),
                    b'x' => Value::Bool(false),
                    b'n' => match self.double() {
                        Some(n) => Value::Number(n),
                        None => return Ok(None),
                    },
                    b's' => match self.string() {
                        Some(s) => Value::Str(s.into()),
                        None => return Ok(None),
                    },
                    _ => match self.bigint() {
                        Some(b) => Value::BigInt(Rc::new(b)),
                        None => return Ok(None),
                    },
                };
                let out = Value::Object(vm.to_object(&p)?);
                self.objects.push(out.clone());
                out
            }
            b'R' => {
                let Some(source) = self.string() else {
                    return Ok(None);
                };
                let Some(bits) = self.varint() else {
                    return Ok(None);
                };
                let flags: String = REGEXP_FLAGS
                    .iter()
                    .filter(|(_, b)| bits & b != 0)
                    .map(|(c, _)| *c)
                    .collect();
                let out = super::make_regexp(vm, &source, &flags)?;
                self.objects.push(out.clone());
                out
            }
            b';' => {
                let o = Obj::new(
                    ObjKind::Map(Box::default()),
                    Some(vm.realm.map_proto.clone()),
                );
                let out = Value::Object(o.clone());
                self.objects.push(out.clone());
                loop {
                    if self.peek_tag() == Some(b':') {
                        self.at += 1;
                        break;
                    }
                    let (Some(k), Some(x)) = (self.value(vm)?, self.value(vm)?) else {
                        return Ok(None);
                    };
                    if let ObjKind::Map(m) = &o.kind {
                        m.borrow_mut().insert(MapKey::from_value(&k), x);
                    }
                }
                if self.varint().is_none() {
                    return Ok(None);
                }
                out
            }
            b'\'' => {
                let o = Obj::new(
                    ObjKind::Set(Box::default()),
                    Some(vm.realm.set_proto.clone()),
                );
                let out = Value::Object(o.clone());
                self.objects.push(out.clone());
                loop {
                    if self.peek_tag() == Some(b',') {
                        self.at += 1;
                        break;
                    }
                    let Some(x) = self.value(vm)? else {
                        return Ok(None);
                    };
                    if let ObjKind::Set(s) = &o.kind {
                        s.borrow_mut().insert(MapKey::from_value(&x));
                    }
                }
                if self.varint().is_none() {
                    return Ok(None);
                }
                out
            }
            b'B' | b'~' => {
                let Some(len) = self.varint() else {
                    return Ok(None);
                };
                let max = if tag == b'~' {
                    match self.varint() {
                        Some(m) => Some(m as usize),
                        None => return Ok(None),
                    }
                } else {
                    None
                };
                let Some(bytes) = self.raw(len).map(<[u8]>::to_vec) else {
                    return Ok(None);
                };
                let buffer = super::typed::new_buffer_with_max(vm, bytes, max);
                let out = Value::Object(buffer.clone());
                self.objects.push(out.clone());
                if self.peek_tag() == Some(b'V') {
                    self.at += 1;
                    return self.view(vm, buffer);
                }
                out
            }
            b'\\' => {
                let (Some(index), Some(len)) = (self.varint(), self.varint()) else {
                    return Ok(None);
                };
                let Some(bytes) = self.raw(len).map(<[u8]>::to_vec) else {
                    return Ok(None);
                };
                let buffer = super::typed::new_buffer(vm, bytes);
                let out = if index == DATA_VIEW_INDEX {
                    data_view(vm, buffer, 0, len as usize)
                } else {
                    let Some(kind) = host_kind(index) else {
                        return Ok(None);
                    };
                    let out =
                        super::typed::new_typed(vm, kind, buffer, 0, len as usize / kind.size());
                    if index == BUFFER_INDEX {
                        if let (Value::Object(o), Some(p)) = (&out, &self.buffer_proto) {
                            *o.proto.borrow_mut() = Some(p.clone());
                        }
                    }
                    out
                };
                self.objects.push(out.clone());
                out
            }
            b'r' => {
                let slot = self.reserve();
                let mut kind = ErrorKind::Error;
                let mut message = None;
                let mut stack = None;
                let mut cause = None;
                loop {
                    let Some(sub) = self.varint() else {
                        return Ok(None);
                    };
                    match sub as u8 {
                        b'E' => kind = ErrorKind::EvalError,
                        b'R' => kind = ErrorKind::RangeError,
                        b'F' => kind = ErrorKind::ReferenceError,
                        b'S' => kind = ErrorKind::SyntaxError,
                        b'T' => kind = ErrorKind::TypeError,
                        b'U' => kind = ErrorKind::URIError,
                        b'm' => match self.string() {
                            Some(s) => message = Some(s),
                            None => return Ok(None),
                        },
                        b's' => match self.string() {
                            Some(s) => stack = Some(s),
                            None => return Ok(None),
                        },
                        b'c' => match self.value(vm)? {
                            Some(c) => cause = Some(c),
                            None => return Ok(None),
                        },
                        b'.' => break,
                        _ => return Ok(None),
                    }
                }
                let e = vm.make_error(kind, message.clone().unwrap_or_default());
                if let Value::Object(o) = &e {
                    if message.is_none() {
                        o.props.borrow_mut().shift_remove("message");
                    }
                    if let Some(s) = stack {
                        o.set_hidden("stack", Value::Str(s.into()));
                    }
                    if let Some(c) = cause {
                        o.set_hidden("cause", c);
                    }
                }
                self.objects[slot] = e.clone();
                e
            }
            _ => return Ok(None),
        }))
    }

    fn view(&mut self, vm: &mut Vm, buffer: Rc<Obj>) -> Read<Value> {
        let Some(sub) = self.byte() else {
            return Ok(None);
        };
        let (Some(offset), Some(len)) = (self.varint(), self.varint()) else {
            return Ok(None);
        };
        if self.version >= 14 && self.varint().is_none() {
            return Ok(None);
        }
        let (offset, len) = (offset as usize, len as usize);
        let out = if sub == b'?' {
            data_view(vm, buffer, offset, len)
        } else {
            let Some(kind) = subtag_kind(sub) else {
                return Ok(None);
            };
            super::typed::new_typed(vm, kind, buffer, offset, len / kind.size())
        };
        self.objects.push(out.clone());
        Ok(Some(out))
    }
}

fn data_view(vm: &Vm, buffer: Rc<Obj>, offset: usize, length: usize) -> Value {
    let view = View {
        kind: TypedKind::Uint8,
        buffer,
        offset,
        length,
        tracking: false,
    };
    Value::Object(Obj::new(
        ObjKind::DataView(Box::new(view)),
        Some(vm.realm.dataview_proto.clone()),
    ))
}

pub fn deserialize(vm: &mut Vm, bytes: &[u8], buffer_proto: Option<Rc<Obj>>) -> R {
    let mut r = Reader {
        bytes,
        at: 0,
        version: 0,
        objects: Vec::new(),
        buffer_proto,
    };
    let version = match r.byte() {
        Some(0xff) => r.varint(),
        _ => None,
    };
    match version {
        Some(v) if (13..=VERSION as u64).contains(&v) => r.version = v,
        _ => {
            return Err(vm.make_error(
                ErrorKind::Error,
                "Unable to deserialize cloned data due to invalid or unsupported version.",
            ));
        }
    }
    match r.value(vm)? {
        Some(v) => Ok(v),
        None => Err(vm.make_error(ErrorKind::Error, "Unable to deserialize cloned data.")),
    }
}

fn proto_arg(args: &[Value], i: usize) -> Option<Rc<Obj>> {
    match arg(args, i) {
        Value::Object(o) => Some(o),
        _ => None,
    }
}

fn native_serialize(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let proto = proto_arg(args, 1);
    let bytes = serialize(vm, &arg(args, 0), proto.as_ref())?;
    Ok(super::typed::new_uint8(vm, bytes))
}

fn native_deserialize(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bytes = super::typed::buffer_source_bytes(&arg(args, 0)).unwrap_or_default();
    deserialize(vm, &bytes, proto_arg(args, 1))
}

pub fn install_natives(vm: &mut Vm, natives: &Rc<Obj>) {
    method(vm, natives, "v8Serialize", 2, native_serialize);
    method(vm, natives, "v8Deserialize", 2, native_deserialize);
}
