use super::{arg, make_ctor, method, native_fn};
use crate::jsstr::JsStr;
use crate::object::{ObjKind, TypedKind};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};

type R = Result<Value, Value>;

pub use super::typed::{buffer_source_bytes, new_buffer, new_typed, new_uint8};

pub fn install(vm: &mut Vm) {
    super::typed::install(vm);
    let tep = vm.new_object();
    let tag_attrs = crate::object::NON_ENUM | crate::object::READ_ONLY;
    tep.set_with_attrs(
        crate::object::TO_STRING_TAG_KEY,
        "TextEncoder".into(),
        tag_attrs,
    );
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
    tdp.set_with_attrs(
        crate::object::TO_STRING_TAG_KEY,
        "TextDecoder".into(),
        tag_attrs,
    );
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
    crypto.set_with_attrs(crate::object::TO_STRING_TAG_KEY, "Crypto".into(), tag_attrs);
    method(vm, &crypto, "getRandomValues", 1, get_random_values);
    method(vm, &crypto, "randomUUID", 0, random_uuid);
    let subtle = vm.new_object();
    subtle.set_with_attrs(
        crate::object::TO_STRING_TAG_KEY,
        "SubtleCrypto".into(),
        tag_attrs,
    );
    method(vm, &subtle, "digest", 2, subtle_digest);
    let delegates: [(&str, u32, crate::object::NativeFn); 11] = [
        ("importKey", 5, subtle_import_key),
        ("exportKey", 2, subtle_export_key),
        ("generateKey", 3, subtle_generate_key),
        ("sign", 3, subtle_sign),
        ("verify", 4, subtle_verify),
        ("deriveBits", 3, subtle_derive_bits),
        ("deriveKey", 5, subtle_derive_key),
        ("encrypt", 3, subtle_encrypt),
        ("decrypt", 3, subtle_decrypt),
        ("wrapKey", 4, subtle_wrap_key),
        ("unwrapKey", 7, subtle_unwrap_key),
    ];
    for (name, arity, f) in delegates {
        method(vm, &subtle, name, arity, f);
    }
    crypto.set_own("subtle", Value::Object(subtle));
    vm.define_global("crypto", Value::Object(crypto));
}

pub(crate) fn hash_name(alg: &str) -> Option<&'static str> {
    Some(match alg.to_ascii_lowercase().replace('-', "").as_str() {
        "md5" => "md5",
        "sha1" => "sha1",
        "sha224" => "sha224",
        "sha256" => "sha256",
        "sha384" => "sha384",
        "sha512" => "sha512",
        _ => return None,
    })
}

pub(crate) fn digest_bytes(alg: &str, data: &[u8]) -> Option<Vec<u8>> {
    use sha2::Digest;
    Some(match alg {
        "md5" => md5::Md5::digest(data).to_vec(),
        "sha1" => sha1::Sha1::digest(data).to_vec(),
        "sha224" => sha2::Sha224::digest(data).to_vec(),
        "sha256" => sha2::Sha256::digest(data).to_vec(),
        "sha384" => sha2::Sha384::digest(data).to_vec(),
        "sha512" => sha2::Sha512::digest(data).to_vec(),
        _ => return None,
    })
}

pub(crate) fn hmac_bytes(alg: &str, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    use hmac::{Hmac, Mac};
    macro_rules! run {
        ($d:ty) => {{
            let mut m = <Hmac<$d> as Mac>::new_from_slice(key).ok()?;
            m.update(data);
            Some(m.finalize().into_bytes().to_vec())
        }};
    }
    match alg {
        "md5" => run!(md5::Md5),
        "sha1" => run!(sha1::Sha1),
        "sha224" => run!(sha2::Sha224),
        "sha256" => run!(sha2::Sha256),
        "sha384" => run!(sha2::Sha384),
        "sha512" => run!(sha2::Sha512),
        _ => None,
    }
}

fn algorithm_name(vm: &mut Vm, v: &Value) -> Result<String, Value> {
    let name = match v {
        Value::Object(_) => vm.get_prop(v, "name")?,
        other => other.clone(),
    };
    vm.to_string(&name).map(|s| s.to_string())
}

fn subtle_call(vm: &mut Vm, name: &str, args: &[Value]) -> R {
    let node = super::node::load(vm)?;
    let subtle = vm.get_prop(&node, "subtleCrypto")?;
    let f = vm.get_prop(&subtle, name)?;
    vm.call_value(&f, subtle, args)
}

macro_rules! subtle_delegates {
    ($($f:ident => $name:literal),* $(,)?) => {
        $(
            fn $f(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
                subtle_call(vm, $name, args)
            }
        )*
    };
}

subtle_delegates! {
    subtle_import_key => "importKey",
    subtle_export_key => "exportKey",
    subtle_generate_key => "generateKey",
    subtle_sign => "sign",
    subtle_verify => "verify",
    subtle_derive_bits => "deriveBits",
    subtle_derive_key => "deriveKey",
    subtle_encrypt => "encrypt",
    subtle_decrypt => "decrypt",
    subtle_wrap_key => "wrapKey",
    subtle_unwrap_key => "unwrapKey",
}

fn subtle_digest(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = algorithm_name(vm, &arg(args, 0))?;
    let p = vm.new_promise();
    let Some(alg) = hash_name(&name) else {
        let e = dom_error(vm, "NotSupportedError", "Unrecognized algorithm name");
        vm.reject_promise(&p, e);
        return Ok(Value::Object(p));
    };
    let Some(data) = super::typed::buffer_source_bytes(&arg(args, 1)) else {
        let e = vm.type_error("The data argument must be a BufferSource");
        vm.reject_promise(&p, e);
        return Ok(Value::Object(p));
    };
    let out = digest_bytes(alg, &data).unwrap_or_default();
    let buf = Value::Object(super::typed::new_buffer(vm, out));
    vm.resolve_promise(&p, buf);
    Ok(Value::Object(p))
}

pub(crate) fn random_vec(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    fill_random(&mut buf);
    buf
}

fn uint8_this(
    vm: &mut Vm,
    this: &Value,
    name: &str,
) -> Result<std::rc::Rc<crate::object::Obj>, Value> {
    match this {
        Value::Object(o) if matches!(&o.kind, ObjKind::Typed(t) if t.kind == TypedKind::Uint8) => {
            Ok(o.clone())
        }
        _ => Err(vm.type_error(format!(
            "Uint8Array.prototype.{name} called on incompatible receiver"
        ))),
    }
}

fn uint8_bytes(vm: &mut Vm, o: &crate::object::Obj) -> Result<Vec<u8>, Value> {
    let ObjKind::Typed(t) = &o.kind else {
        return Err(vm.type_error("not a Uint8Array"));
    };
    if t.detached() || t.out_of_bounds() {
        return Err(vm.type_error("Cannot perform this operation on a detached ArrayBuffer"));
    }
    Ok(t.bytes())
}

fn options_object(vm: &mut Vm, v: &Value) -> Result<Option<Value>, Value> {
    match v {
        Value::Undefined => Ok(None),
        v if v.is_object() => Ok(Some(v.clone())),
        _ => Err(vm.type_error("The options argument must be an object")),
    }
}

fn alphabet_option(vm: &mut Vm, opts: &Option<Value>) -> Result<bool, Value> {
    let Some(o) = opts else {
        return Ok(false);
    };
    match vm.get_prop(o, "alphabet")? {
        Value::Undefined => Ok(false),
        Value::Str(s) if &*s == "base64" => Ok(false),
        Value::Str(s) if &*s == "base64url" => Ok(true),
        _ => Err(vm.type_error("Expected alphabet to be either \"base64\" or \"base64url\"")),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum LastChunk {
    Loose,
    Strict,
    StopBeforePartial,
}

fn last_chunk_option(vm: &mut Vm, opts: &Option<Value>) -> Result<LastChunk, Value> {
    let Some(o) = opts else {
        return Ok(LastChunk::Loose);
    };
    match vm.get_prop(o, "lastChunkHandling")? {
        Value::Undefined => Ok(LastChunk::Loose),
        Value::Str(s) if &*s == "loose" => Ok(LastChunk::Loose),
        Value::Str(s) if &*s == "strict" => Ok(LastChunk::Strict),
        Value::Str(s) if &*s == "stop-before-partial" => Ok(LastChunk::StopBeforePartial),
        _ => Err(vm.type_error(
            "Expected lastChunkHandling to be either \"loose\", \"strict\", or \"stop-before-partial\"",
        )),
    }
}

struct Decoded {
    read: usize,
    bytes: Vec<u8>,
    error: bool,
}

fn decode_chunk(chunk: &[u8], strict: bool) -> Option<Vec<u8>> {
    let mut s = [0u32; 4];
    for (i, v) in chunk.iter().enumerate() {
        s[i] = *v as u32;
    }
    let n = s[0] << 18 | s[1] << 12 | s[2] << 6 | s[3];
    let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
    match chunk.len() {
        2 => (!strict || bytes[1] == 0).then(|| vec![bytes[0]]),
        3 => (!strict || bytes[2] == 0).then(|| vec![bytes[0], bytes[1]]),
        _ => Some(bytes.to_vec()),
    }
}

fn sextet(c: u16) -> Option<u8> {
    let c = char::from_u32(c as u32)?;
    Some(match c {
        'A'..='Z' => c as u8 - b'A',
        'a'..='z' => c as u8 - b'a' + 26,
        '0'..='9' => c as u8 - b'0' + 52,
        '+' => 62,
        '/' => 63,
        _ => return None,
    })
}

fn from_base64(units: &[u16], url: bool, last: LastChunk, max: usize) -> Decoded {
    let mut res = Decoded {
        read: 0,
        bytes: Vec::new(),
        error: false,
    };
    if max == 0 {
        return res;
    }
    let len = units.len();
    let skip = |mut i: usize| {
        while i < len && matches!(units[i], 0x09 | 0x0a | 0x0c | 0x0d | 0x20) {
            i += 1;
        }
        i
    };
    let mut chunk: Vec<u8> = Vec::with_capacity(4);
    let mut index = 0;
    loop {
        index = skip(index);
        if index == len {
            if !chunk.is_empty() {
                match last {
                    LastChunk::StopBeforePartial => return res,
                    LastChunk::Strict => {
                        res.error = true;
                        return res;
                    }
                    LastChunk::Loose => {
                        if chunk.len() == 1 {
                            res.error = true;
                            return res;
                        }
                        res.bytes
                            .extend(decode_chunk(&chunk, false).unwrap_or_default());
                    }
                }
            }
            res.read = len;
            return res;
        }
        let mut c = units[index];
        index += 1;
        if c == b'=' as u16 {
            if chunk.len() < 2 {
                res.error = true;
                return res;
            }
            index = skip(index);
            if chunk.len() == 2 {
                if index == len {
                    if last != LastChunk::StopBeforePartial {
                        res.error = true;
                    }
                    return res;
                }
                if units[index] == b'=' as u16 {
                    index = skip(index + 1);
                }
            }
            if index < len {
                res.error = true;
                return res;
            }
            match decode_chunk(&chunk, last == LastChunk::Strict) {
                Some(b) => res.bytes.extend(b),
                None => {
                    res.error = true;
                    return res;
                }
            }
            res.read = len;
            return res;
        }
        if url {
            if c == b'+' as u16 || c == b'/' as u16 {
                res.error = true;
                return res;
            }
            if c == b'-' as u16 {
                c = b'+' as u16;
            } else if c == b'_' as u16 {
                c = b'/' as u16;
            }
        }
        let Some(v) = sextet(c) else {
            res.error = true;
            return res;
        };
        let remaining = max - res.bytes.len();
        if (remaining == 1 && chunk.len() == 2) || (remaining == 2 && chunk.len() == 3) {
            return res;
        }
        chunk.push(v);
        if chunk.len() == 4 {
            res.bytes
                .extend(decode_chunk(&chunk, false).unwrap_or_default());
            chunk.clear();
            res.read = index;
            if res.bytes.len() == max {
                return res;
            }
        }
    }
}

fn from_hex(units: &[u16], max: usize) -> Decoded {
    let mut res = Decoded {
        read: 0,
        bytes: Vec::new(),
        error: false,
    };
    if !units.len().is_multiple_of(2) {
        res.error = true;
        return res;
    }
    let digit = |u: u16| char::from_u32(u as u32).and_then(|c| c.to_digit(16));
    while res.read < units.len() && res.bytes.len() < max {
        match (digit(units[res.read]), digit(units[res.read + 1])) {
            (Some(h), Some(l)) => res.bytes.push((h * 16 + l) as u8),
            _ => {
                res.error = true;
                return res;
            }
        }
        res.read += 2;
    }
    res
}

fn string_units(vm: &mut Vm, v: &Value) -> Result<Vec<u16>, Value> {
    match v {
        Value::Str(s) => Ok(s.as_str().encode_utf16().collect()),
        _ => Err(vm.type_error("The input must be a string")),
    }
}

fn decode_error(vm: &mut Vm, base64: bool) -> Value {
    vm.make_error(
        ErrorKind::SyntaxError,
        if base64 {
            "Invalid base64 string"
        } else {
            "Input string must contain hex characters in even length"
        },
    )
}

pub(super) fn ta_to_hex(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = uint8_this(vm, &this, "toHex")?;
    let bytes = uint8_bytes(vm, &o)?;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s.into())
}

pub(super) fn ta_to_base64(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = uint8_this(vm, &this, "toBase64")?;
    let opts = options_object(vm, &arg(args, 0))?;
    let url = alphabet_option(vm, &opts)?;
    let omit = match &opts {
        Some(o) => vm.get_prop(o, "omitPadding")?.truthy(),
        None => false,
    };
    let bytes = uint8_bytes(vm, &o)?;
    let mut s = base64_encode(&bytes, url);
    if omit {
        while s.ends_with('=') {
            s.pop();
        }
    }
    Ok(s.into())
}

pub(super) fn u8_from_hex(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let units = string_units(vm, &arg(args, 0))?;
    let res = from_hex(&units, usize::MAX);
    if res.error {
        return Err(decode_error(vm, false));
    }
    Ok(new_uint8(vm, res.bytes))
}

pub(super) fn u8_from_base64(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let units = string_units(vm, &arg(args, 0))?;
    let opts = options_object(vm, &arg(args, 1))?;
    let url = alphabet_option(vm, &opts)?;
    let last = last_chunk_option(vm, &opts)?;
    let res = from_base64(&units, url, last, usize::MAX);
    if res.error {
        return Err(decode_error(vm, true));
    }
    Ok(new_uint8(vm, res.bytes))
}

fn write_decoded(vm: &mut Vm, o: &crate::object::Obj, res: Decoded, base64: bool) -> R {
    if let ObjKind::Typed(t) = &o.kind {
        let mut store = t.store().borrow_mut();
        for (i, b) in res.bytes.iter().enumerate() {
            if let Some(slot) = store.get_mut(t.offset + i) {
                *slot = *b;
            }
        }
    }
    if res.error {
        return Err(decode_error(vm, base64));
    }
    let out = vm.new_object();
    out.set_own("read", Value::Number(res.read as f64));
    out.set_own("written", Value::Number(res.bytes.len() as f64));
    Ok(Value::Object(out))
}

fn writable_length(vm: &mut Vm, o: &crate::object::Obj) -> Result<usize, Value> {
    let ObjKind::Typed(t) = &o.kind else {
        return Err(vm.type_error("not a Uint8Array"));
    };
    if t.detached() || t.out_of_bounds() {
        return Err(vm.type_error("Cannot perform this operation on a detached ArrayBuffer"));
    }
    Ok(t.len())
}

pub(super) fn ta_set_from_base64(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = uint8_this(vm, &this, "setFromBase64")?;
    let units = string_units(vm, &arg(args, 0))?;
    let opts = options_object(vm, &arg(args, 1))?;
    let url = alphabet_option(vm, &opts)?;
    let last = last_chunk_option(vm, &opts)?;
    let max = writable_length(vm, &o)?;
    let res = from_base64(&units, url, last, max);
    write_decoded(vm, &o, res, true)
}

pub(super) fn ta_set_from_hex(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = uint8_this(vm, &this, "setFromHex")?;
    let units = string_units(vm, &arg(args, 0))?;
    let max = writable_length(vm, &o)?;
    let res = from_hex(&units, max);
    write_decoded(vm, &o, res, false)
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
        Value::Undefined => JsStr::from(""),
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
        Value::Undefined => JsStr::from("utf-8"),
        v => vm.to_string(&v)?,
    };
    let encoding = match encoding_rs::Encoding::for_label(label.to_string().as_bytes()) {
        Some(e) if e != encoding_rs::REPLACEMENT => e,
        _ => {
            return Err(vm.range_error(format!("The \"{label}\" encoding is not supported")));
        }
    };
    let options = arg(args, 1);
    let (fatal, ignore_bom) = if options.is_object() {
        let f = vm.get_prop(&options, "fatal")?.truthy();
        let b = vm.get_prop(&options, "ignoreBOM")?.truthy();
        (f, b)
    } else {
        (false, false)
    };
    o.set_own("encoding", encoding.name().to_ascii_lowercase().into());
    o.set_own("fatal", Value::Bool(fatal));
    o.set_own("ignoreBOM", Value::Bool(ignore_bom));
    Ok(this)
}

struct StreamDecoder {
    owner: std::rc::Weak<crate::object::Obj>,
    decoder: encoding_rs::Decoder,
}

thread_local! {
    static DECODERS: std::cell::RefCell<Vec<StreamDecoder>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn take_decoder(owner: &std::rc::Rc<crate::object::Obj>) -> Option<encoding_rs::Decoder> {
    DECODERS.with(|d| {
        let mut list = d.borrow_mut();
        list.retain(|s| s.owner.strong_count() > 0);
        let i = list
            .iter()
            .position(|s| std::ptr::eq(s.owner.as_ptr(), std::rc::Rc::as_ptr(owner)))?;
        Some(list.swap_remove(i).decoder)
    })
}

fn keep_decoder(owner: &std::rc::Rc<crate::object::Obj>, decoder: encoding_rs::Decoder) {
    DECODERS.with(|d| {
        d.borrow_mut().push(StreamDecoder {
            owner: std::rc::Rc::downgrade(owner),
            decoder,
        })
    });
}

fn td_decode(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let Value::Object(owner) = &this else {
        return Err(vm.type_error("Illegal invocation"));
    };
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
    let options = arg(args, 1);
    let stream = options.is_object() && vm.get_prop(&options, "stream")?.truthy();
    let fatal = vm.get_prop(&this, "fatal")?.truthy();
    let ignore_bom = vm.get_prop(&this, "ignoreBOM")?.truthy();
    let label = vm.get_prop(&this, "encoding")?;
    let label = vm.to_string(&label)?;
    let encoding = encoding_rs::Encoding::for_label(label.to_string().as_bytes())
        .unwrap_or(encoding_rs::UTF_8);
    let mut decoder = take_decoder(owner).unwrap_or_else(|| {
        if ignore_bom {
            encoding.new_decoder_without_bom_handling()
        } else {
            encoding.new_decoder_with_bom_removal()
        }
    });
    let last = !stream;
    let mut out = String::new();
    if fatal {
        let cap = decoder
            .max_utf8_buffer_length_without_replacement(bytes.len())
            .unwrap_or(bytes.len() * 3 + 16);
        out.reserve(cap);
        let (result, _) = decoder.decode_to_string_without_replacement(&bytes, &mut out, last);
        if let encoding_rs::DecoderResult::Malformed(_, _) = result {
            let e = vm.type_error(format!(
                "The encoded data was not valid for encoding {label}"
            ));
            if let Value::Object(o) = &e {
                o.set_own("code", "ERR_ENCODING_INVALID_ENCODED_DATA".into());
            }
            return Err(e);
        }
    } else {
        let cap = decoder
            .max_utf8_buffer_length(bytes.len())
            .unwrap_or(bytes.len() * 3 + 16);
        out.reserve(cap);
        let _ = decoder.decode_to_string(&bytes, &mut out, last);
    }
    if stream {
        keep_decoder(owner, decoder);
    }
    Ok(out.into())
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

pub(crate) fn dom_error(vm: &mut Vm, name: &str, msg: &str) -> Value {
    let _ = vm.load_lazy_global("DOMException");
    if let Some(ctor) = vm.globals.get("DOMException").cloned()
        && ctor.is_callable()
        && let Ok(e) = vm.construct(&ctor, &[msg.into(), name.into()])
    {
        return e;
    }
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
        None => Err(dom_error(vm, "InvalidCharacterError", "Invalid character")),
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
    let options = arg(args, 1);
    let transfer = match &options {
        Value::Undefined | Value::Null => Vec::new(),
        Value::Object(_) => match vm.get_prop(&options, "transfer")? {
            Value::Undefined => Vec::new(),
            t => vm.iterate(&t)?,
        },
        _ => return Err(vm.type_error("The \"options\" argument must be of type object.")),
    };
    let wire = super::threads::serialize(vm, &arg(args, 0), &transfer)?;
    super::threads::deserialize(vm, &wire)
}
