use crate::inspect::inspect_plain;
use crate::jsstr::JsStr;
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
    Str(JsStr),
    Object(Rc<Obj>),
    Symbol(Rc<Symbol>),
    BigInt(Rc<num_bigint::BigInt>),
    Uninit,
}

const _: () = assert!(std::mem::size_of::<Value>() == 16);

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

impl From<JsStr> for Value {
    fn from(s: JsStr) -> Self {
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

    pub fn to_primitive_string(&self) -> Option<JsStr> {
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

const INDEX_MIN: usize = 512;
const MARK_STEP: usize = 128;

pub struct StrIndex {
    ascii: bool,
    units: usize,
    marks: Vec<(u32, u32)>,
    code_units: Vec<u16>,
}

thread_local! {
    static STR_CACHE: std::cell::RefCell<Vec<(JsStr, Rc<StrIndex>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn build_index(s: &str) -> StrIndex {
    if s.is_ascii() {
        return StrIndex {
            ascii: true,
            units: s.len(),
            marks: Vec::new(),
            code_units: Vec::new(),
        };
    }
    let mut marks = Vec::with_capacity(s.len() / MARK_STEP + 1);
    let mut code_units = Vec::with_capacity(s.len());
    let mut next = 0usize;
    for (byte, c) in s.char_indices() {
        if code_units.len() >= next {
            marks.push((code_units.len() as u32, byte as u32));
            next = code_units.len() + MARK_STEP;
        }
        if is_lone(c) {
            code_units.push(lone_unit(c));
        } else {
            let mut buf = [0u16; 2];
            code_units.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    StrIndex {
        ascii: false,
        units: code_units.len(),
        marks,
        code_units,
    }
}

pub fn str_index(s: &JsStr) -> Rc<StrIndex> {
    STR_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((k, idx)) = cache.first() {
            if JsStr::ptr_eq(k, s) {
                return idx.clone();
            }
        }
        if let Some(pos) = cache.iter().position(|(k, _)| JsStr::ptr_eq(k, s)) {
            let entry = cache.remove(pos);
            let idx = entry.1.clone();
            cache.insert(0, entry);
            return idx;
        }
        let idx = Rc::new(build_index(s));
        if cache.len() >= 8 {
            cache.pop();
        }
        cache.insert(0, (s.clone(), idx.clone()));
        idx
    })
}

impl StrIndex {
    pub fn len16(&self) -> usize {
        self.units
    }

    pub fn byte_of(&self, s: &str, unit: usize) -> usize {
        if self.ascii {
            return unit.min(s.len());
        }
        if unit >= self.units {
            return s.len();
        }
        let (mut pos, byte) = self.locate(unit);
        for (b, c) in s[byte..].char_indices() {
            if pos >= unit {
                return byte + b;
            }
            pos += char_len16(c);
        }
        s.len()
    }

    pub fn unit_of(&self, s: &str, byte: usize) -> usize {
        let byte = byte.min(s.len());
        if self.ascii {
            return byte;
        }
        self.unit_at_byte(s, byte)
    }

    fn locate(&self, i: usize) -> (usize, usize) {
        let k = self.marks.partition_point(|m| m.0 as usize <= i);
        let (u, b) = self.marks[k.saturating_sub(1)];
        (u as usize, b as usize)
    }

    pub fn code_unit_at(&self, s: &str, i: usize) -> Option<u16> {
        if self.ascii {
            return s.as_bytes().get(i).map(|b| *b as u16);
        }
        self.code_units.get(i).copied()
    }

    fn byte_at(&self, s: &str, i: usize) -> (usize, bool) {
        if i >= self.units {
            return (s.len(), false);
        }
        let (mut pos, mut byte) = self.locate(i);
        for c in s[byte..].chars() {
            let n = char_len16(c);
            if pos == i {
                return (byte, false);
            }
            if i < pos + n {
                return (byte, true);
            }
            pos += n;
            byte += c.len_utf8();
        }
        (s.len(), false)
    }

    fn unit_at_byte(&self, s: &str, byte: usize) -> usize {
        let k = self.marks.partition_point(|m| m.1 as usize <= byte);
        let (u, b) = self.marks[k.saturating_sub(1)];
        let mut u = u as usize;
        for c in s[b as usize..byte].chars() {
            u += char_len16(c);
        }
        u
    }

    pub fn slice16(&self, s: &str, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }
        if self.ascii {
            let end = end.min(s.len());
            if start >= end {
                return String::new();
            }
            return s[start..end].to_string();
        }
        let end = end.min(self.units);
        if start >= end {
            return String::new();
        }
        let (sb, smid) = self.byte_at(s, start);
        let (eb, emid) = self.byte_at(s, end);
        let mut out = String::with_capacity(eb.saturating_sub(sb) + 8);
        let mut body_start = sb;
        if smid {
            let c = s[sb..].chars().next().unwrap_or('\u{fffd}');
            let mut buf = [0u16; 2];
            let units = c.encode_utf16(&mut buf);
            out.push(lone_char(units[units.len() - 1]));
            body_start = sb + c.len_utf8();
        }
        if body_start < eb {
            out.push_str(&s[body_start..eb]);
        }
        if emid {
            let c = s[eb..].chars().next().unwrap_or('\u{fffd}');
            let mut buf = [0u16; 2];
            let units = c.encode_utf16(&mut buf);
            out.push(lone_char(units[0]));
        }
        out
    }

    pub fn index_of16(&self, s: &str, needle: &str, from: usize) -> Option<usize> {
        if self.ascii {
            if from > s.len() {
                return if needle.is_empty() {
                    Some(s.len())
                } else {
                    None
                };
            }
            return s[from..].find(needle).map(|i| i + from);
        }
        if from > self.units {
            return if needle.is_empty() {
                Some(self.units)
            } else {
                None
            };
        }
        let (fb, mid) = self.byte_at(s, from);
        if needle.is_empty() {
            return Some(from);
        }
        let start = if mid {
            fb + s[fb..].chars().next().map_or(0, char::len_utf8)
        } else {
            fb
        };
        let found = s.get(start..)?.find(needle)? + start;
        Some(self.unit_at_byte(s, found))
    }
}

pub fn len16_rc(s: &JsStr) -> usize {
    if s.is_ascii() {
        return s.len();
    }
    if s.len() < INDEX_MIN {
        len16(s)
    } else {
        str_index(s).len16()
    }
}

pub fn byte_index_rc(s: &JsStr, unit: usize) -> usize {
    if s.is_ascii() {
        return unit.min(s.len());
    }
    if s.len() < INDEX_MIN {
        crate::regexp::byte_index(s, unit)
    } else {
        str_index(s).byte_of(s, unit)
    }
}

pub fn utf16_index_rc(s: &JsStr, byte: usize) -> usize {
    if s.is_ascii() {
        return byte;
    }
    if s.len() < INDEX_MIN {
        crate::regexp::utf16_index(s, byte)
    } else {
        str_index(s).unit_of(s, byte)
    }
}

pub fn code_unit_at_rc(s: &JsStr, i: usize) -> Option<u16> {
    if s.is_ascii() {
        return s.as_bytes().get(i).map(|b| *b as u16);
    }
    if s.len() < INDEX_MIN {
        code_unit_at(s, i)
    } else {
        str_index(s).code_unit_at(s, i)
    }
}

pub fn char_at16_rc(s: &JsStr, i: usize) -> Option<String> {
    if s.is_ascii() {
        return s.as_bytes().get(i).map(|b| (*b as char).to_string());
    }
    if s.len() < INDEX_MIN {
        char_at16(s, i)
    } else {
        str_index(s).code_unit_at(s, i).map(|u| from16(&[u]))
    }
}

pub fn slice16_rc(s: &JsStr, start: usize, end: usize) -> String {
    if s.is_ascii() {
        let e = end.min(s.len());
        return s[start.min(e)..e].to_string();
    }
    if s.len() < INDEX_MIN {
        slice16(s, start, end)
    } else {
        str_index(s).slice16(s, start, end)
    }
}

pub fn index_of16_rc(s: &JsStr, needle: &str, from: usize) -> Option<usize> {
    if s.is_ascii() {
        if from > s.len() {
            return needle.is_empty().then_some(s.len());
        }
        return s[from..].find(needle).map(|i| i + from);
    }
    if s.len() < INDEX_MIN || needle.chars().any(is_lone) {
        index_of16(s, needle, from)
    } else {
        str_index(s).index_of16(s, needle, from)
    }
}

pub fn len16(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().map(char_len16).sum()
    }
}

pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
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
