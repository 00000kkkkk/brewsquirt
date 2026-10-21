use super::{arg, make_ctor, method, native_fn};
use crate::inspect::inspect_plain;
use crate::object::{MapKey, Obj, ObjKind, TypedKind, View};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::rc::Rc;

type R = Result<Value, Value>;

pub use super::typed::{buffer_source_bytes, new_buffer, new_typed, new_uint8};

pub fn install(vm: &mut Vm) {
    super::typed::install(vm);
    let tep = vm.new_object();
    method(vm, &tep, "encode", 0, te_encode);
    method(vm, &tep, "encodeInto", 2, te_encode_into);
    let te = make_ctor(
        vm,
        "TextEncoder",
        0,
        te_ctor,
        Some(|| ObjKind::Ordinary),
        &tep,
    );
    vm.define_global("TextEncoder", Value::Object(te));
    let tdp = vm.new_object();
    method(vm, &tdp, "decode", 1, td_decode);
    let td = make_ctor(
        vm,
        "TextDecoder",
        0,
        td_ctor,
        Some(|| ObjKind::Ordinary),
        &tdp,
    );
    vm.define_global("TextDecoder", Value::Object(td));

    let f = native_fn(vm, "atob", 1, atob);
    vm.define_global("atob", f);
    let f = native_fn(vm, "btoa", 1, btoa);
    vm.define_global("btoa", f);
    let f = native_fn(vm, "structuredClone", 1, structured_clone);
    vm.define_global("structuredClone", f);

    let crypto = vm.new_object();
    method(vm, &crypto, "getRandomValues", 1, get_random_values);
    method(vm, &crypto, "randomUUID", 0, random_uuid);
    vm.define_global("crypto", Value::Object(crypto));
}

fn this_u8(vm: &mut Vm, this: &Value, name: &str) -> Result<Vec<u8>, Value> {
    match this {
        Value::Object(o) => match &o.kind {
            ObjKind::Typed(t) if t.kind == TypedKind::Uint8 => Ok(t.bytes()),
            _ => Err(vm.type_error(format!(
                "Uint8Array.prototype.{name} called on incompatible receiver"
            ))),
        },
        _ => Err(vm.type_error(format!(
            "Uint8Array.prototype.{name} called on incompatible receiver"
        ))),
    }
}

pub(super) fn ta_to_hex(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let bytes = this_u8(vm, &this, "toHex")?;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s.into())
}

pub(super) fn ta_to_base64(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let bytes = this_u8(vm, &this, "toBase64")?;
    let url = base64_url_option(vm, &arg(args, 0))?;
    let mut s = base64_encode(&bytes, url);
    if arg(args, 0).is_object() && vm.get_prop(&arg(args, 0), "omitPadding")?.truthy() {
        while s.ends_with('=') {
            s.pop();
        }
    }
    Ok(s.into())
}

fn base64_url_option(vm: &mut Vm, options: &Value) -> Result<bool, Value> {
    if !options.is_object() {
        return Ok(false);
    }
    let a = vm.get_prop(options, "alphabet")?;
    Ok(matches!(&a, Value::Str(s) if &**s == "base64url"))
}

pub(super) fn u8_from_hex(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    if s.len() % 2 != 0 {
        return Err(vm.make_error(
            ErrorKind::SyntaxError,
            "Input string must contain hex characters in even length",
        ));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let b = s.as_bytes();
    for pair in b.chunks(2) {
        let hi = (pair[0] as char).to_digit(16);
        let lo = (pair[1] as char).to_digit(16);
        match (hi, lo) {
            (Some(h), Some(l)) => out.push((h * 16 + l) as u8),
            _ => {
                return Err(vm.make_error(
                    ErrorKind::SyntaxError,
                    "Input string must contain hex characters in even length",
                ));
            }
        }
    }
    Ok(new_uint8(vm, out))
}

pub(super) fn u8_from_base64(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    let url = base64_url_option(vm, &arg(args, 1))?;
    match base64_decode(&s, url) {
        Some(bytes) => Ok(new_uint8(vm, bytes)),
        None => Err(vm.make_error(ErrorKind::SyntaxError, "Invalid base64 string")),
    }
}

fn te_ctor(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let Value::Object(o) = &this else {
        return Err(vm.type_error("Constructor TextEncoder requires 'new'"));
    };
    o.set_own("encoding", "utf-8".into());
    Ok(this)
}

fn te_encode(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = match arg(args, 0) {
        Value::Undefined => Rc::from(""),
        v => vm.to_string(&v)?,
    };
    Ok(new_uint8(
        vm,
        crate::value::utf8_text(&s).as_bytes().to_vec(),
    ))
}

fn te_encode_into(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    let target = match arg(args, 1) {
        Value::Object(o) if matches!(&o.kind, ObjKind::Typed(t) if t.kind == TypedKind::Uint8) => o,
        _ => return Err(vm.type_error("The \"dest\" argument must be an instance of Uint8Array.")),
    };
    let ObjKind::Typed(v) = &target.kind else {
        return Err(vm.type_error("The \"dest\" argument must be an instance of Uint8Array."));
    };
    let mut read = 0usize;
    let mut written = 0usize;
    for c in s.chars() {
        let mut buf = [0u8; 4];
        let bytes = c.encode_utf8(&mut buf).as_bytes();
        if written + bytes.len() > v.length {
            break;
        }
        for (i, b) in bytes.iter().enumerate() {
            v.set_f64(written + i, *b as f64);
        }
        written += bytes.len();
        read += c.len_utf16();
    }
    let r = vm.new_object();
    r.set_own("read", (read as f64).into());
    r.set_own("written", (written as f64).into());
    Ok(Value::Object(r))
}

fn td_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Value::Object(o) = &this else {
        return Err(vm.type_error("Constructor TextDecoder requires 'new'"));
    };
    let label = match arg(args, 0) {
        Value::Undefined => Rc::from("utf-8"),
        v => vm.to_string(&v)?,
    };
    let norm = label.trim().to_ascii_lowercase();
    if !matches!(norm.as_str(), "utf-8" | "utf8" | "unicode-1-1-utf-8") {
        return Err(vm.range_error(format!("The \"{label}\" encoding is not supported")));
    }
    let options = arg(args, 1);
    let (fatal, ignore_bom) = if options.is_object() {
        let f = vm.get_prop(&options, "fatal")?.truthy();
        let b = vm.get_prop(&options, "ignoreBOM")?.truthy();
        (f, b)
    } else {
        (false, false)
    };
    o.set_own("encoding", "utf-8".into());
    o.set_own("fatal", Value::Bool(fatal));
    o.set_own("ignoreBOM", Value::Bool(ignore_bom));
    Ok(this)
}

fn td_decode(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let input = arg(args, 0);
    let bytes = if input.is_undefined() {
        Vec::new()
    } else {
        match buffer_source_bytes(&input) {
            Some(b) => b,
            None => {
                return Err(vm.type_error(
                    "The \"input\" argument must be an instance of ArrayBuffer or ArrayBufferView",
                ));
            }
        }
    };
    let fatal = vm.get_prop(&this, "fatal")?.truthy();
    let ignore_bom = vm.get_prop(&this, "ignoreBOM")?.truthy();
    let body = if !ignore_bom && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        &bytes[..]
    };
    if fatal {
        return match std::str::from_utf8(body) {
            Ok(s) => Ok(s.into()),
            Err(_) => Err(vm.type_error("The encoded data was not valid for encoding utf-8")),
        };
    }
    Ok(String::from_utf8_lossy(body).into_owned().into())
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64_encode(bytes: &[u8], url: bool) -> String {
    let table = if url { B64_URL } else { B64 };
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(table[(n >> 18) as usize & 63] as char);
        out.push(table[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            table[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            table[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(s: &str, url: bool) -> Option<Vec<u8>> {
    let mut vals = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_whitespace() {
            continue;
        }
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' if !url => 62,
            '/' if !url => 63,
            '-' if url => 62,
            '_' if url => 63,
            '=' => 64,
            _ => return None,
        };
        vals.push(v);
    }
    while vals.last() == Some(&64) {
        vals.pop();
    }
    if vals.contains(&64) || vals.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() {
            n |= v << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

fn dom_error(vm: &mut Vm, name: &str, msg: &str) -> Value {
    let e = vm.make_error(ErrorKind::Error, msg);
    if let Value::Object(o) = &e {
        o.set_hidden("name", name.into());
        o.set_hidden("stack", format!("{name}: {msg}").into());
    }
    e
}

fn btoa(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c as u32 > 255 {
            return Err(dom_error(vm, "InvalidCharacterError", "Invalid character"));
        }
        bytes.push(c as u8);
    }
    Ok(base64_encode(&bytes, false).into())
}

fn atob(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    match base64_decode(&s, false) {
        Some(bytes) => Ok(bytes
            .into_iter()
            .map(|b| b as char)
            .collect::<String>()
            .into()),
        None => Err(dom_error(
            vm,
            "InvalidCharacterError",
            "The string to be decoded is not correctly encoded.",
        )),
    }
}

fn fill_random(buf: &mut [u8]) {
    if getrandom::getrandom(buf).is_err() {
        let mut x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15)
            | 1;
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
}

fn get_random_values(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let target = arg(args, 0);
    let Value::Object(o) = &target else {
        return Err(vm.type_error("The \"typedArray\" argument must be an integer typed array"));
    };
    let ObjKind::Typed(t) = &o.kind else {
        return Err(vm.type_error("The \"typedArray\" argument must be an integer typed array"));
    };
    if t.kind.is_float() {
        return Err(dom_error(
            vm,
            "TypeMismatchError",
            "The data argument must be an integer-type TypedArray",
        ));
    }
    let n = t.byte_len();
    if n > 65536 {
        return Err(dom_error(
            vm,
            "QuotaExceededError",
            &format!(
                "The ArrayBufferView's byte length ({n}) exceeds the number of bytes of entropy available via this API (65536)"
            ),
        ));
    }
    let mut buf = vec![0u8; n];
    fill_random(&mut buf);
    let mut store = t.store().borrow_mut();
    store[t.offset..t.offset + n].copy_from_slice(&buf);
    drop(store);
    Ok(target)
}

fn random_uuid(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let mut b = [0u8; 16];
    fill_random(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    )
    .into())
}

fn structured_clone(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut memo: Vec<(*const Obj, Value)> = Vec::new();
    clone_value(vm, &arg(args, 0), &mut memo)
}

fn not_cloneable(vm: &mut Vm, v: &Value) -> Value {
    dom_error(
        vm,
        "DataCloneError",
        &format!("{} could not be cloned.", inspect_plain(v)),
    )
}

fn clone_value(vm: &mut Vm, v: &Value, memo: &mut Vec<(*const Obj, Value)>) -> R {
    let o = match v {
        Value::Symbol(_) => return Err(not_cloneable(vm, v)),
        Value::Object(o) => o,
        other => return Ok(other.clone()),
    };
    let ptr = Rc::as_ptr(o);
    if let Some((_, c)) = memo.iter().find(|(p, _)| *p == ptr) {
        return Ok(c.clone());
    }
    let out = match &o.kind {
        ObjKind::Array(a) => {
            let out = vm.new_array(Vec::new());
            memo.push((ptr, out.clone()));
            let items = a.borrow().clone();
            let mut cloned = Vec::with_capacity(items.len());
            for x in &items {
                cloned.push(clone_value(vm, x, memo)?);
            }
            if let Value::Object(n) = &out {
                if let ObjKind::Array(na) = &n.kind {
                    *na.borrow_mut() = cloned;
                }
            }
            return Ok(out);
        }
        ObjKind::Ordinary | ObjKind::Error => {
            let n = if matches!(o.kind, ObjKind::Error) {
                let e = vm.make_error(ErrorKind::Error, "");
                if let Value::Object(e) = &e {
                    for key in ["name", "message", "stack"] {
                        if let Some(x) = o.get_chain(key) {
                            if matches!(x, Value::Str(_)) {
                                e.set_hidden(key, x);
                            }
                        }
                    }
                    if let Some(Value::Str(name)) = o.get_chain("name") {
                        if let Some(Value::Object(c)) = vm.globals.get(&name).cloned() {
                            if let Some(Value::Object(p)) = c.get_own("prototype") {
                                *e.proto.borrow_mut() = Some(p);
                            }
                        }
                    }
                }
                e
            } else {
                Value::Object(vm.new_object())
            };
            memo.push((ptr, n.clone()));
            let entries = vm.own_entries_resolved(v, false)?;
            for (k, x) in entries {
                if x.is_callable() {
                    return Err(not_cloneable(vm, &x));
                }
                let c = clone_value(vm, &x, memo)?;
                if let Value::Object(no) = &n {
                    no.set_own(k, c);
                }
            }
            return Ok(n);
        }
        ObjKind::Map(m) => {
            let out = Obj::new(
                ObjKind::Map(Box::default()),
                Some(vm.realm.map_proto.clone()),
            );
            let outv = Value::Object(out.clone());
            memo.push((ptr, outv.clone()));
            let entries: Vec<(Value, Value)> = m
                .borrow()
                .iter()
                .map(|(k, v)| (k.to_value(), v.clone()))
                .collect();
            for (k, x) in entries {
                let ck = clone_value(vm, &k, memo)?;
                let cx = clone_value(vm, &x, memo)?;
                if let ObjKind::Map(nm) = &out.kind {
                    nm.borrow_mut().insert(MapKey::from_value(&ck), cx);
                }
            }
            return Ok(outv);
        }
        ObjKind::Set(s) => {
            let out = Obj::new(
                ObjKind::Set(Box::default()),
                Some(vm.realm.set_proto.clone()),
            );
            let outv = Value::Object(out.clone());
            memo.push((ptr, outv.clone()));
            let items: Vec<Value> = s.borrow().iter().map(|k| k.to_value()).collect();
            for x in items {
                let cx = clone_value(vm, &x, memo)?;
                if let ObjKind::Set(ns) = &out.kind {
                    ns.borrow_mut().insert(MapKey::from_value(&cx));
                }
            }
            return Ok(outv);
        }
        ObjKind::Date(t) => Value::Object(Obj::new(
            ObjKind::Date(std::cell::Cell::new(t.get())),
            Some(vm.realm.date_proto.clone()),
        )),
        ObjKind::RegExp(r) => crate::builtins::make_regexp(vm, &r.data.source, &r.data.flags)?,
        ObjKind::ArrayBuffer(b) => {
            let bytes = b.borrow().clone();
            Value::Object(new_buffer(vm, bytes))
        }
        ObjKind::Typed(t) => {
            let buf = clone_value(vm, &Value::Object(t.buffer.clone()), memo)?;
            let Value::Object(buf) = buf else {
                return Err(not_cloneable(vm, v));
            };
            new_typed(vm, t.kind, buf, t.offset, t.length)
        }
        ObjKind::DataView(t) => {
            let buf = clone_value(vm, &Value::Object(t.buffer.clone()), memo)?;
            let Value::Object(buf) = buf else {
                return Err(not_cloneable(vm, v));
            };
            Value::Object(Obj::new(
                ObjKind::DataView(Box::new(View {
                    kind: TypedKind::Uint8,
                    buffer: buf,
                    offset: t.offset,
                    length: t.length,
                    tracking: t.tracking,
                })),
                Some(vm.realm.dataview_proto.clone()),
            ))
        }
        _ => return Err(not_cloneable(vm, v)),
    };
    memo.push((ptr, out.clone()));
    Ok(out)
}
