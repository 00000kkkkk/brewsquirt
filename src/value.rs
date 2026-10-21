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

pub const LONE_BASE: u32 = 0x10F800;

pub fn is_lone(c: char) -> bool {
    (c as u32) >= LONE_BASE
}

pub fn lone_unit(c: char) -> u16 {
    ((c as u32) - LONE_BASE + 0xD800) as u16
}

pub fn lone_char(u: u16) -> char {
    char::from_u32(LONE_BASE + (u as u32).saturating_sub(0xD800)).unwrap_or('\u{fffd}')
}

pub fn char_len16(c: char) -> usize {
    if is_lone(c) { 1 } else { c.len_utf16() }
}

fn has_lone(s: &str) -> bool {
    s.as_bytes()
        .windows(2)
        .any(|w| w[0] == 0xF4 && w[1] >= 0x8F)
}

pub fn is_well_formed(s: &str) -> bool {
    s.is_ascii() || !s.chars().any(is_lone)
}

pub fn to_well_formed(s: &str) -> String {
    s.chars()
        .map(|c| if is_lone(c) { '\u{fffd}' } else { c })
        .collect()
}

pub fn utf8_text(s: &str) -> std::borrow::Cow<'_, str> {
    if s.is_ascii() || !has_lone(s) {
        std::borrow::Cow::Borrowed(s)
    } else {
        std::borrow::Cow::Owned(to_well_formed(s))
    }
}

pub fn from16(u: &[u16]) -> String {
    char::decode_utf16(u.iter().copied())
        .map(|r| r.unwrap_or_else(|e| lone_char(e.unpaired_surrogate())))
        .collect()
}

pub fn push16(dst: &mut String, src: &str) {
    let b = dst.as_bytes();
    if src.as_bytes().first() == Some(&0xF4) && b.len() >= 4 && b[b.len() - 4] == 0xF4 {
        if let (Some(l), Some(f)) = (dst.chars().next_back(), src.chars().next()) {
            if is_lone(l) && is_lone(f) {
                let (hi, lo) = (lone_unit(l), lone_unit(f));
                if (0xD800..0xDC00).contains(&hi) && (0xDC00..0xE000).contains(&lo) {
                    let cp = 0x10000 + (((hi - 0xD800) as u32) << 10) + (lo - 0xDC00) as u32;
                    dst.truncate(dst.len() - l.len_utf8());
                    dst.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                    dst.push_str(&src[f.len_utf8()..]);
                    return;
                }
            }
        }
    }
    dst.push_str(src);
}

pub fn concat16(a: &str, b: &str) -> String {
    let mut s = String::with_capacity(a.len() + b.len());
    s.push_str(a);
    push16(&mut s, b);
    s
}

pub fn len16(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().map(char_len16).sum()
    }
}

pub fn units16(s: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if is_lone(c) {
            out.push(lone_unit(c));
        } else {
            let mut buf = [0u16; 2];
            out.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    out
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
    from16(&u[start..end])
}

pub fn code_unit_at(s: &str, i: usize) -> Option<u16> {
    if s.is_ascii() {
        return s.as_bytes().get(i).map(|b| *b as u16);
    }
    let mut pos = 0usize;
    for c in s.chars() {
        let n = char_len16(c);
        if i < pos + n {
            if is_lone(c) {
                return Some(lone_unit(c));
            }
            let mut buf = [0u16; 2];
            return Some(c.encode_utf16(&mut buf)[i - pos]);
        }
        pos += n;
    }
    None
}

pub fn char_at16(s: &str, i: usize) -> Option<String> {
    if s.is_ascii() {
        return s.get(i..i + 1).map(|c| c.to_string());
    }
    code_unit_at(s, i).map(|u| from16(&[u]))
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
        units16(a).cmp(&units16(b))
    }
}
