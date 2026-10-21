use crate::inspect::inspect_plain;
use crate::number::number_to_string;
use crate::object::{Obj, ObjKind, Symbol};
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Str(Rc<str>),
    Object(Rc<Obj>),
    Symbol(Rc<Symbol>),
    BigInt(Rc<num_bigint::BigInt>),
    Uninit,
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&inspect_plain(self))
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.into())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Str(s.into())
    }
}

impl From<Rc<str>> for Value {
    fn from(s: Rc<str>) -> Self {
        Value::Str(s)
    }
}

impl From<Rc<Obj>> for Value {
    fn from(o: Rc<Obj>) -> Self {
        Value::Object(o)
    }
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Uninit | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            Value::Object(_) | Value::Symbol(_) => true,
            Value::BigInt(b) => !crate::bigint::is_zero(b),
        }
    }

    pub fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Value::Undefined)
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    pub fn as_object(&self) -> Option<&Rc<Obj>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn is_callable(&self) -> bool {
        match self {
            Value::Object(o) => o.is_callable(),
            _ => false,
        }
    }

    pub fn type_of(&self) -> &'static str {
        match self {
            Value::Undefined | Value::Uninit => "undefined",
            Value::Null => "object",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::BigInt(_) => "bigint",
            Value::Object(o) => {
                if o.is_callable() {
                    "function"
                } else {
                    "object"
                }
            }
        }
    }

    pub fn strict_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            (Value::BigInt(a), Value::BigInt(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => a.key == b.key,
            _ => false,
        }
    }

    pub fn same_value_zero(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) if a.is_nan() && b.is_nan() => true,
            _ => self.strict_eq(other),
        }
    }

    pub fn to_primitive_string(&self) -> Option<Rc<str>> {
        Some(match self {
            Value::Undefined | Value::Uninit => "undefined".into(),
            Value::Null => "null".into(),
            Value::Bool(b) => if *b { "true" } else { "false" }.into(),
            Value::Number(n) => number_to_string(*n).into(),
            Value::Str(s) => s.clone(),
            Value::BigInt(b) => b.to_string().into(),
            Value::Object(_) | Value::Symbol(_) => return None,
        })
    }

    pub fn to_primitive_number(&self) -> Option<f64> {
        Some(match self {
            Value::Undefined | Value::Uninit => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Number(n) => *n,
            Value::Str(s) => crate::number::string_to_number(s),
            Value::Object(_) | Value::Symbol(_) | Value::BigInt(_) => return None,
        })
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Value::Object(o) if matches!(o.kind, ObjKind::Array(_)))
    }
}

pub fn is_well_formed(_s: &str) -> bool {
    true
}

pub fn to_well_formed(s: &str) -> String {
    s.to_string()
}

pub fn len16(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.encode_utf16().count()
    }
}

pub fn units16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

pub fn slice16(s: &str, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    if s.is_ascii() {
        let end = end.min(s.len());
        if start >= end {
            return String::new();
        }
        return s[start..end].to_string();
    }
    let u = units16(s);
    let end = end.min(u.len());
    if start >= end {
        return String::new();
    }
    String::from_utf16_lossy(&u[start..end])
}

pub fn code_unit_at(s: &str, i: usize) -> Option<u16> {
    if s.is_ascii() {
        return s.as_bytes().get(i).map(|b| *b as u16);
    }
    s.encode_utf16().nth(i)
}

pub fn char_at16(s: &str, i: usize) -> Option<String> {
    if s.is_ascii() {
        return s.get(i..i + 1).map(|c| c.to_string());
    }
    let u = units16(s);
    let c = *u.get(i)?;
    if (0xd800..0xdc00).contains(&c) {
        if let Some(&low) = u.get(i + 1) {
            if (0xdc00..0xe000).contains(&low) {
                return Some(String::from_utf16_lossy(&[c]));
            }
        }
    }
    Some(String::from_utf16_lossy(&[c]))
}

pub fn index_of16(hay: &str, needle: &str, from: usize) -> Option<usize> {
    if hay.is_ascii() && needle.is_ascii() {
        if from > hay.len() {
            return if needle.is_empty() {
                Some(hay.len())
            } else {
                None
            };
        }
        return hay[from..].find(needle).map(|i| i + from);
    }
    let h = units16(hay);
    let n = units16(needle);
    if n.is_empty() {
        return Some(from.min(h.len()));
    }
    if n.len() > h.len() {
        return None;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()] == n[..])
}

pub fn last_index_of16(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let h = units16(hay);
    let n = units16(needle);
    if n.len() > h.len() {
        return None;
    }
    let start = from.min(h.len() - n.len());
    (0..=start).rev().find(|&i| h[i..i + n.len()] == n[..])
}

pub fn compare_strings(a: &str, b: &str) -> std::cmp::Ordering {
    if a.is_ascii() && b.is_ascii() {
        a.cmp(b)
    } else {
        a.encode_utf16().cmp(b.encode_utf16())
    }
}
