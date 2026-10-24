use super::{arg, method, to_integer};
use crate::jsstr::JsStr;
use crate::number::number_to_string;
use crate::object::{Obj, ObjKind};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let json = vm.new_object();
    method(vm, &json, "stringify", 3, stringify);
    method(vm, &json, "parse", 2, parse);
    vm.define_global("JSON", Value::Object(json));
}

pub fn quote_json(s: &str, out: &mut String) {
    if !s
        .bytes()
        .any(|b| b < 0x20 || b == b'"' || b == b'\\' || b == 0xF4)
    {
        out.reserve(s.len() + 2);
        out.push('"');
        out.push_str(s);
        out.push('"');
        return;
    }
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if crate::value::is_lone(c) => {
                out.push_str(&format!("\\u{:04x}", crate::value::lone_unit(c)))
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[derive(Clone, Copy)]
enum Key<'a> {
    Index(usize),
    Name(&'a str),
}

impl Key<'_> {
    fn value(self) -> Value {
        match self {
            Key::Index(i) => Value::Str(i.to_string().into()),
            Key::Name(s) => s.into(),
        }
    }
}

fn to_json_method(vm: &mut Vm, o: &Rc<Obj>) -> Result<Value, Value> {
    let mut cur = Some(o.clone());
    while let Some(c) = cur {
        if !matches!(c.kind, ObjKind::Ordinary | ObjKind::Array(_))
            || c.props.borrow().contains_key("toJSON")
        {
            return vm.get_prop(&Value::Object(o.clone()), "toJSON");
        }
        cur = c.proto.borrow().clone();
    }
    Ok(Value::Undefined)
}

fn own_plain(o: &Obj, key: &str) -> Option<Value> {
    let props = o.props.borrow();
    match props.get(key) {
        Some(Value::Uninit) => None,
        Some(Value::Object(a)) if a.is_accessor() => None,
        Some(v) => Some(v.clone()),
        None => None,
    }
}

fn plain_keys(o: &Obj) -> Option<Vec<JsStr>> {
    if !matches!(o.kind, ObjKind::Ordinary) {
        return None;
    }
    let props = o.props.borrow();
    if props.may_have_accessors() {
        return None;
    }
    Some(
        props
            .enumerable()
            .filter(|(k, _)| !crate::object::is_symbol_key(k))
            .map(|(k, _)| k.clone())
            .collect(),
    )
}

struct Ser<'a> {
    indent: &'a str,
    stack: Vec<*const Obj>,
    replacer: Option<Value>,
    allow: Option<Vec<JsStr>>,
}

impl Ser<'_> {
    fn newline(&self, out: &mut String, depth: usize) {
        if !self.indent.is_empty() {
            out.push('\n');
            for _ in 0..depth {
                out.push_str(self.indent);
            }
        }
    }

    fn property(
        &mut self,
        vm: &mut Vm,
        holder: &Value,
        key: Key,
        value: Value,
        depth: usize,
        out: &mut String,
    ) -> Result<bool, Value> {
        let mut v = value;
        let f = match &v {
            Value::Object(o) => to_json_method(vm, o)?,
            Value::BigInt(_) => vm.get_prop(&v, "toJSON")?,
            _ => Value::Undefined,
        };
        if f.is_callable() {
            v = vm.call_value(&f, v.clone(), &[key.value()])?;
        }
        if let Some(r) = &self.replacer {
            let r = r.clone();
            v = vm.call_value(&r, holder.clone(), &[key.value(), v])?;
        }
        match crate::object::unwrap_primitive(&v) {
            Some(Value::Number(_)) => v = Value::Number(vm.to_number(&v)?),
            Some(Value::Str(_)) => v = Value::Str(vm.to_string(&v)?),
            Some(p @ (Value::Bool(_) | Value::BigInt(_))) => v = p,
            _ => {}
        }
        self.write(vm, &v, depth, out)
    }

    fn write(
        &mut self,
        vm: &mut Vm,
        v: &Value,
        depth: usize,
        out: &mut String,
    ) -> Result<bool, Value> {
        match v {
            Value::BigInt(_) => Err(vm.type_error("Do not know how to serialize a BigInt")),
            Value::Undefined | Value::Uninit | Value::Symbol(_) => Ok(false),
            Value::Null => {
                out.push_str("null");
                Ok(true)
            }
            Value::Bool(b) => {
                out.push_str(if *b { "true" } else { "false" });
                Ok(true)
            }
            Value::Number(n) => {
                if n.is_finite() {
                    crate::number::push_number(out, *n);
                } else {
                    out.push_str("null");
                }
                Ok(true)
            }
            Value::Str(s) => {
                quote_json(s, out);
                Ok(true)
            }
            Value::Object(o) => {
                if o.is_callable() {
                    return Ok(false);
                }
                let ptr = Rc::as_ptr(o);
                if self.stack.contains(&ptr) {
                    return Err(vm.type_error("Converting circular structure to JSON"));
                }
                self.stack.push(ptr);
                let r = self.write_object(vm, o, depth, out);
                self.stack.pop();
                r.map(|_| true)
            }
        }
    }

    fn is_array(o: &Obj) -> bool {
        match &o.kind {
            ObjKind::Array(_) => true,
            ObjKind::Proxy(p) => p
                .target
                .borrow()
                .as_ref()
                .is_some_and(|t| Self::is_array(t)),
            _ => false,
        }
    }

    fn write_object(
        &mut self,
        vm: &mut Vm,
        o: &Rc<Obj>,
        depth: usize,
        out: &mut String,
    ) -> Result<(), Value> {
        let holder = Value::Object(o.clone());
        if Self::is_array(o) {
            let len = vm.get_prop(&holder, "length")?;
            let len = vm.to_number(&len)? as usize;
            if len == 0 {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for i in 0..len {
                if i > 0 {
                    out.push(',');
                }
                self.newline(out, depth + 1);
                let item = match &o.kind {
                    ObjKind::Array(a) if o.flags.get() & crate::object::SLOW_ELEMENTS == 0 => {
                        a.borrow().get(i).cloned().unwrap_or_default()
                    }
                    _ => vm.get_prop(&holder, &i.to_string())?,
                };
                if !self.property(vm, &holder, Key::Index(i), item, depth + 1, out)? {
                    out.push_str("null");
                }
            }
            self.newline(out, depth);
            out.push(']');
            return Ok(());
        }
        let fast = if self.allow.is_none() {
            plain_keys(o)
        } else {
            None
        };
        let plain = fast.is_some();
        let keys: Vec<JsStr> = match (plain, &self.allow) {
            (true, _) => fast.unwrap_or_default(),
            (false, Some(list)) => list.clone(),
            (false, None) => match &o.kind {
                ObjKind::Map(_) | ObjKind::Set(_) | ObjKind::Iter(_) => Vec::new(),
                _ => vm.op_own_enumerable_keys(o)?,
            },
        };
        out.push('{');
        let mut first = true;
        for k in keys {
            let v = match plain.then(|| own_plain(o, &k)).flatten() {
                Some(v) => v,
                None => vm.get_prop(&holder, &k)?,
            };
            let mark = out.len();
            if !first {
                out.push(',');
            }
            self.newline(out, depth + 1);
            quote_json(&k, out);
            out.push(':');
            if !self.indent.is_empty() {
                out.push(' ');
            }
            if self.property(vm, &holder, Key::Name(&k), v, depth + 1, out)? {
                first = false;
            } else {
                out.truncate(mark);
            }
        }
        if !first {
            self.newline(out, depth);
        }
        out.push('}');
        Ok(())
    }
}

pub fn json_stringify(vm: &mut Vm, v: &Value, indent: &str) -> Result<Option<String>, Value> {
    let mut ser = Ser {
        indent,
        stack: Vec::new(),
        replacer: None,
        allow: None,
    };
    let holder = Value::Object(vm.new_object());
    let mut out = String::new();
    if ser.property(vm, &holder, Key::Name(""), v.clone(), 0, &mut out)? {
        Ok(Some(out))
    } else {
        Ok(None)
    }
}

fn stringify(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let space = arg(args, 2);
    let space = match crate::object::unwrap_primitive(&space) {
        Some(Value::Number(_)) => Value::Number(vm.to_number(&space)?),
        Some(Value::Str(_)) => Value::Str(vm.to_string(&space)?),
        _ => space,
    };
    let indent = match &space {
        Value::Number(_) => {
            let n = to_integer(vm, &space)?.clamp(0.0, 10.0) as usize;
            " ".repeat(n)
        }
        Value::Str(s) => s.chars().take(10).collect(),
        _ => String::new(),
    };
    let replacer = arg(args, 1);
    let mut ser = Ser {
        indent: &indent,
        stack: Vec::new(),
        replacer: None,
        allow: None,
    };
    if replacer.is_callable() {
        ser.replacer = Some(replacer.clone());
    } else if let Value::Object(r) = &replacer {
        if Ser::is_array(r) {
            let mut list: Vec<JsStr> = Vec::new();
            let len = vm.get_prop(&replacer, "length")?;
            let len = to_integer(vm, &len)?.clamp(0.0, 9007199254740991.0) as usize;
            for i in 0..len {
                let item = vm.get_prop(&replacer, &i.to_string())?;
                let key = match &item {
                    Value::Str(s) => Some(s.clone()),
                    Value::Number(n) => Some(number_to_string(*n).into()),
                    Value::Object(_)
                        if matches!(
                            crate::object::unwrap_primitive(&item),
                            Some(Value::Str(_) | Value::Number(_))
                        ) =>
                    {
                        Some(vm.to_string(&item)?)
                    }
                    _ => None,
                };
                if let Some(k) = key {
                    if !list.contains(&k) {
                        list.push(k);
                    }
                }
            }
            ser.allow = Some(list);
        }
    }
    let holder = vm.new_object();
    let v = arg(args, 0);
    holder.set_own("", v.clone());
    let mut out = String::new();
    if ser.property(vm, &Value::Object(holder), Key::Name(""), v, 0, &mut out)? {
        Ok(out.into())
    } else {
        Ok(Value::Undefined)
    }
}

struct Fast<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
    keys: rustc_hash::FxHashMap<&'a str, JsStr>,
    buf: String,
}

fn hex4(b: &[u8]) -> Option<u32> {
    let mut n = 0;
    for &c in b {
        n = n * 16 + (c as char).to_digit(16)?;
    }
    Some(n)
}

impl<'a> Fast<'a> {
    fn ws(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.b.get(self.i) {
            self.i += 1;
        }
    }

    fn value(&mut self, vm: &mut Vm, depth: u32) -> Option<Value> {
        if depth > 512 {
            return None;
        }
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => self.object(vm, depth),
            b'[' => self.array(vm, depth),
            b'"' => self.string(false).map(Value::Str),
            b't' => self.word("true", Value::Bool(true)),
            b'f' => self.word("false", Value::Bool(false)),
            b'n' => self.word("null", Value::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn word(&mut self, w: &str, v: Value) -> Option<Value> {
        if self.b[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Some(v)
        } else {
            None
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while let Some(b'0'..=b'9') = self.b.get(self.i) {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.i;
        let neg = self.b[self.i] == b'-';
        if neg {
            self.i += 1;
        }
        match self.b.get(self.i)? {
            b'0' => self.i += 1,
            b'1'..=b'9' => {
                self.digits();
            }
            _ => return None,
        }
        let int_end = self.i;
        let mut simple = true;
        if self.b.get(self.i) == Some(&b'.') {
            simple = false;
            self.i += 1;
            if self.digits() == 0 {
                return None;
            }
        }
        if let Some(b'e' | b'E') = self.b.get(self.i) {
            simple = false;
            self.i += 1;
            if let Some(b'+' | b'-') = self.b.get(self.i) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return None;
            }
        }
        let first = start + neg as usize;
        if simple && int_end - first <= 15 {
            let mut n = 0i64;
            for &c in &self.b[first..int_end] {
                n = n * 10 + (c - b'0') as i64;
            }
            let x = n as f64;
            return Some(Value::Number(if neg { -x } else { x }));
        }
        self.s[start..self.i].parse::<f64>().ok().map(Value::Number)
    }

    fn string(&mut self, key: bool) -> Option<JsStr> {
        let start = self.i + 1;
        let mut i = start;
        loop {
            match *self.b.get(i)? {
                b'"' => break,
                b'\\' => return self.escaped(start),
                c if c < 0x20 => return None,
                _ => i += 1,
            }
        }
        self.i = i + 1;
        let text = &self.s[start..i];
        if key {
            if let Some(k) = self.keys.get(text) {
                return Some(k.clone());
            }
            let k = JsStr::new(text);
            self.keys.insert(text, k.clone());
            return Some(k);
        }
        Some(JsStr::new(text))
    }

    fn escaped(&mut self, start: usize) -> Option<JsStr> {
        let mut out = std::mem::take(&mut self.buf);
        out.clear();
        let mut i = start;
        let mut seg = start;
        loop {
            match *self.b.get(i)? {
                b'"' => {
                    crate::value::push16(&mut out, &self.s[seg..i]);
                    break;
                }
                b'\\' => {
                    crate::value::push16(&mut out, &self.s[seg..i]);
                    let e = *self.b.get(i + 1)?;
                    i += 2;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let u = hex4(self.b.get(i..i + 4)?)?;
                            i += 4;
                            let low = if (0xD800..0xDC00).contains(&u)
                                && self.b.get(i) == Some(&b'\\')
                                && self.b.get(i + 1) == Some(&b'u')
                            {
                                self.b
                                    .get(i + 2..i + 6)
                                    .and_then(hex4)
                                    .filter(|l| (0xDC00..0xE000).contains(l))
                            } else {
                                None
                            };
                            if let Some(lo) = low {
                                out.push(char::from_u32(
                                    0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00),
                                )?);
                                i += 6;
                            } else if (0xD800..0xE000).contains(&u) {
                                let mut tmp = [0u8; 4];
                                crate::value::push16(
                                    &mut out,
                                    crate::value::lone_char(u as u16).encode_utf8(&mut tmp),
                                );
                            } else {
                                out.push(char::from_u32(u)?);
                            }
                        }
                        _ => return None,
                    }
                    seg = i;
                }
                c if c < 0x20 => return None,
                _ => i += 1,
            }
        }
        self.i = i + 1;
        let s = JsStr::new(&out);
        self.buf = out;
        Some(s)
    }

    fn object(&mut self, vm: &mut Vm, depth: u32) -> Option<Value> {
        self.i += 1;
        let o = vm.new_object();
        self.ws();
        if self.b.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Some(Value::Object(o));
        }
        loop {
            self.ws();
            if self.b.get(self.i) != Some(&b'"') {
                return None;
            }
            let k = self.string(true)?;
            self.ws();
            if self.b.get(self.i) != Some(&b':') {
                return None;
            }
            self.i += 1;
            let v = self.value(vm, depth + 1)?;
            o.set_own(k, v);
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Some(Value::Object(o));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self, vm: &mut Vm, depth: u32) -> Option<Value> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.b.get(self.i) == Some(&b']') {
            self.i += 1;
            return Some(vm.new_array(items));
        }
        loop {
            items.push(self.value(vm, depth + 1)?);
            self.ws();
            match self.b.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Some(vm.new_array(items));
                }
                _ => return None,
            }
        }
    }
}

fn fast_parse(vm: &mut Vm, text: &str) -> Option<Value> {
    let mut p = Fast {
        s: text,
        b: text.as_bytes(),
        i: 0,
        keys: Default::default(),
        buf: String::new(),
    };
    let v = p.value(vm, 0)?;
    p.ws();
    (p.i == p.b.len()).then_some(v)
}

pub fn json_parse(vm: &mut Vm, text: &str) -> R {
    if let Some(v) = fast_parse(vm, text) {
        return Ok(v);
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(j) => Ok(from_json(vm, &j)),
        Err(e) => Err(vm.make_error(ErrorKind::SyntaxError, json_error_message(text, &e))),
    }
}

fn json_error_message(text: &str, e: &serde_json::Error) -> String {
    let chars: Vec<char> = text.chars().collect();
    let line = e.line().max(1);
    let column = e.column();
    let mut position = 0usize;
    for (i, l) in text.split('\n').enumerate() {
        if i + 1 == line {
            position += column.saturating_sub(1).min(l.chars().count());
            break;
        }
        position += l.chars().count() + 1;
    }
    let position = position.min(chars.len());
    let detail = e.to_string();
    let detail = detail.split(" at line ").next().unwrap_or("").to_string();
    let at = |p: usize| {
        let mut line = 1;
        let mut col = 1;
        for &c in &chars[..p.min(chars.len())] {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        format!("in JSON at position {p} (line {line} column {col})")
    };
    let current = chars.get(position).copied();
    let previous = position.checked_sub(1).and_then(|i| chars.get(i)).copied();
    let unexpected_token = |p: usize, c: char| {
        let source: String = chars.iter().collect();
        if chars.len() <= 40 {
            format!("Unexpected token '{c}', \"{source}\" is not valid JSON")
        } else {
            let start = p.saturating_sub(10);
            let end = (p + 10).min(chars.len());
            let snippet: String = chars[start..end].iter().collect();
            let head = if start > 0 { "..." } else { "" };
            let tail = if end < chars.len() { "..." } else { "" };
            format!("Unexpected token '{c}', {head}\"{snippet}\"{tail} is not valid JSON")
        }
    };
    if detail.contains("EOF while parsing a string") {
        return format!("Unterminated string {}", at(position));
    }
    if detail.contains("EOF") || current.is_none() {
        let trimmed_len = chars
            .iter()
            .rposition(|c| !c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let last = trimmed_len
            .checked_sub(1)
            .and_then(|i| chars.get(i))
            .copied();
        let before_last = trimmed_len
            .checked_sub(2)
            .and_then(|i| chars.get(i))
            .copied();
        if last == Some('{') {
            return format!("Expected property name or '}}' {}", at(trimmed_len));
        }
        if last == Some('.') && before_last.is_some_and(|c| c.is_ascii_digit()) {
            return format!("Unterminated fractional number {}", at(trimmed_len));
        }
        if matches!(last, Some('e') | Some('E') | Some('+') | Some('-'))
            && before_last.is_some_and(|c| c.is_ascii_digit() || c == 'e' || c == 'E')
        {
            return format!("Exponent part is missing a number {}", at(trimmed_len));
        }
        return "Unexpected end of JSON input".to_string();
    }
    let c = current.unwrap_or(' ');
    if detail.contains("control character") {
        let p = (0..position)
            .rev()
            .find(|&i| chars[i].is_control())
            .unwrap_or(position);
        return format!("Bad control character in string literal {}", at(p));
    }
    if detail.contains("invalid escape") {
        return format!("Bad escaped character {}", at(position));
    }
    if detail.contains("unicode") || detail.contains("surrogate") {
        return format!("Bad Unicode escape {}", at(position));
    }
    if detail.contains("key must be a string") {
        return if previous == Some(',') {
            format!("Expected double-quoted property name {}", at(position))
        } else {
            format!("Expected property name or '}}' {}", at(position))
        };
    }
    if detail.contains("trailing comma") {
        return if c == '}' {
            format!("Expected double-quoted property name {}", at(position))
        } else {
            unexpected_token(position, c)
        };
    }
    if detail.contains("expected `:`") {
        return format!("Expected ':' after property name {}", at(position));
    }
    if detail.contains("expected `,` or `}`") {
        return format!("Expected ',' or '}}' after property value {}", at(position));
    }
    if detail.contains("expected `,` or `]`") {
        return format!("Expected ',' or ']' after array element {}", at(position));
    }
    if detail.contains("invalid number") || detail.contains("number out of range") {
        return match previous {
            Some('.') => format!("Unterminated fractional number {}", at(position)),
            Some('e') | Some('E') | Some('+') => {
                format!("Exponent part is missing a number {}", at(position))
            }
            Some('-') => format!("No number after minus sign {}", at(position)),
            _ => format!("Unexpected number {}", at(position)),
        };
    }
    if detail.contains("trailing characters") {
        return if c.is_ascii_digit() {
            format!("Unexpected number {}", at(position))
        } else if c == '"' || c.is_alphabetic() || c == '.' || c == '-' {
            unexpected_token(position, c)
        } else {
            format!(
                "Unexpected non-whitespace character after JSON {}",
                at(position)
            )
        };
    }
    if c.is_alphabetic() {
        let word: String = chars[position..]
            .iter()
            .take_while(|ch| ch.is_alphanumeric())
            .collect();
        let truncated = ["true", "false", "null"]
            .iter()
            .any(|w| w.starts_with(word.as_str()) && word.len() < w.len());
        if truncated && position + word.chars().count() == chars.len() {
            return "Unexpected end of JSON input".to_string();
        }
        if position == 0 && position + word.chars().count() == chars.len() {
            return format!("\"{word}\" is not valid JSON");
        }
        return unexpected_token(position, c);
    }
    if c.is_ascii_digit() && position > 0 {
        return format!("Unexpected number {}", at(position));
    }
    unexpected_token(position, c)
}

fn from_json(vm: &mut Vm, j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(f64::NAN)),
        serde_json::Value::String(s) => Value::Str(s.as_str().into()),
        serde_json::Value::Array(items) => {
            let list = items.iter().map(|x| from_json(vm, x)).collect();
            vm.new_array(list)
        }
        serde_json::Value::Object(map) => {
            let o = vm.new_object();
            for (k, x) in map {
                let v = from_json(vm, x);
                o.set_own(k.as_str(), v);
            }
            Value::Object(o)
        }
    }
}

fn parse(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let text = vm.to_string(&arg(args, 0))?;
    let v = json_parse(vm, &text)?;
    let reviver = arg(args, 1);
    if reviver.is_callable() {
        let holder = vm.new_object();
        holder.set_own("", v.clone());
        return revive(vm, &reviver, &Value::Object(holder), "".into());
    }
    Ok(v)
}

fn array_like_value(vm: &mut Vm, o: &Rc<Obj>) -> Result<bool, Value> {
    let mut cur = o.clone();
    loop {
        let next = match &cur.kind {
            ObjKind::Array(_) => return Ok(true),
            ObjKind::Proxy(p) => match p.target.borrow().clone() {
                Some(t) => t,
                None => {
                    return Err(
                        vm.type_error("Cannot perform 'IsArray' on a proxy that has been revoked")
                    );
                }
            },
            _ => return Ok(false),
        };
        cur = next;
    }
}

fn revive(vm: &mut Vm, f: &Value, holder: &Value, key: JsStr) -> R {
    let v = vm.get_prop(holder, &key)?;
    if let Value::Object(o) = &v {
        let keys: Vec<JsStr> = if array_like_value(vm, o)? {
            let len = vm.get_prop(&v, "length")?;
            let n = super::to_integer(vm, &len)?.clamp(0.0, 9007199254740991.0) as usize;
            (0..n).map(|i| i.to_string().into()).collect()
        } else {
            vm.op_own_enumerable_keys(o)?
        };
        for k in keys {
            let nv = revive(vm, f, &v, k.clone())?;
            if nv.is_undefined() {
                vm.op_delete(o, &k)?;
            } else {
                let d = crate::props::PropDesc {
                    value: Some(nv),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(true),
                    ..Default::default()
                };
                vm.op_define_own_property(o, k, d, false)?;
            }
        }
    }
    vm.call_value(f, holder.clone(), &[Value::Str(key), v])
}
