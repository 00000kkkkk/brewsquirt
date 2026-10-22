use super::{arg, make_ctor, method, relative_index, to_integer};
use crate::value::{
    Value, char_at16, code_unit_at, index_of16, last_index_of16, len16, slice16, units16,
};
use crate::vm::Vm;
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let sp = vm.realm.string_proto.clone();
    let methods: &[(&str, u32, crate::object::NativeFn)] = &[
        ("charAt", 1, char_at),
        ("charCodeAt", 1, char_code_at),
        ("codePointAt", 1, code_point_at),
        ("at", 1, at),
        ("indexOf", 1, index_of),
        ("lastIndexOf", 1, last_index_of),
        ("includes", 1, includes),
        ("startsWith", 1, starts_with),
        ("endsWith", 1, ends_with),
        ("slice", 2, slice),
        ("substring", 2, substring),
        ("substr", 2, substr),
        ("toUpperCase", 0, to_upper),
        ("toLowerCase", 0, to_lower),
        ("toLocaleUpperCase", 0, to_upper),
        ("toLocaleLowerCase", 0, to_lower),
        ("trim", 0, trim),
        ("trimStart", 0, trim_start),
        ("trimEnd", 0, trim_end),
        ("split", 2, split),
        ("replace", 2, replace),
        ("match", 1, match_fn),
        ("matchAll", 1, match_all),
        ("search", 1, search),
        ("replaceAll", 2, replace_all),
        ("repeat", 1, repeat),
        ("padStart", 2, pad_start),
        ("padEnd", 2, pad_end),
        ("concat", 1, concat),
        ("localeCompare", 1, locale_compare),
        ("toString", 0, this_string_value),
        ("valueOf", 0, this_string_value),
    ];
    for (name, arity, f) in methods {
        method(vm, &sp, name, *arity, *f);
    }
    let string = make_ctor(
        vm,
        "String",
        1,
        string_ctor,
        Some(crate::object::wrapper_marker),
        &sp,
    );
    method(vm, &string, "fromCharCode", 1, from_char_code);
    method(vm, &string, "fromCodePoint", 1, from_code_point);
    method(vm, &string, "raw", 1, string_raw);
    vm.define_global("String", Value::Object(string));
}

fn this_str(vm: &mut Vm, this: &Value) -> Result<Rc<str>, Value> {
    if let Some(Value::Str(s)) = crate::object::unwrap_primitive(this) {
        return Ok(s);
    }
    if this.is_nullish() {
        return Err(vm.type_error("String.prototype method called on null or undefined"));
    }
    vm.to_string(this)
}

fn str_arg(vm: &mut Vm, args: &[Value], i: usize) -> Result<Rc<str>, Value> {
    vm.to_string(&arg(args, i))
}

fn string_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = if args.is_empty() {
        Value::from("")
    } else if let (Value::Symbol(s), false) = (&args[0], super::constructing(&this)) {
        Value::from(super::symbol_text(s))
    } else {
        Value::Str(vm.to_string(&args[0])?)
    };
    Ok(super::box_for(&this, s.clone()).unwrap_or(s))
}

fn this_string_value(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match crate::object::unwrap_primitive(&this).unwrap_or(this) {
        Value::Str(s) => Ok(Value::Str(s)),
        _ => Err(vm.type_error("String.prototype.valueOf requires that 'this' be a String")),
    }
}

fn string_raw(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let strings = arg(args, 0);
    let raw = vm.get_prop(&strings, "raw")?;
    let len = vm.get_prop(&raw, "length")?;
    let len = vm.to_number(&len)?;
    let len = if len.is_nan() || len < 0.0 {
        0
    } else {
        len as usize
    };
    let mut out = String::new();
    for i in 0..len {
        let part = vm.get_prop(&raw, &i.to_string())?;
        out.push_str(&vm.to_string(&part)?);
        if i + 1 < len {
            if let Some(sub) = args.get(i + 1) {
                out.push_str(&vm.to_string(sub)?);
            }
        }
    }
    Ok(out.into())
}

fn from_char_code(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut units = Vec::with_capacity(args.len());
    for a in args {
        let n = vm.to_number(a)?;
        units.push(crate::number::to_uint32(n) as u16);
    }
    Ok(crate::value::from16(&units).into())
}

fn from_code_point(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut s = String::new();
    for a in args {
        let n = vm.to_number(a)?;
        if n.fract() != 0.0 || !(0.0..=1114111.0).contains(&n) {
            return Err(vm.range_error(format!(
                "Invalid code point {}",
                crate::number::number_to_string(n)
            )));
        }
        let c = char::from_u32(n as u32).unwrap_or_else(|| crate::value::lone_char(n as u16));
        let mut buf = [0u8; 4];
        crate::value::push16(&mut s, c.encode_utf8(&mut buf));
    }
    Ok(s.into())
}

fn char_at(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let i = to_integer(vm, &arg(args, 0))?;
    if i < 0.0 {
        return Ok("".into());
    }
    Ok(char_at16(&s, i as usize).unwrap_or_default().into())
}

fn char_code_at(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let i = to_integer(vm, &arg(args, 0))?;
    if i < 0.0 {
        return Ok(Value::Number(f64::NAN));
    }
    Ok(Value::Number(
        code_unit_at(&s, i as usize)
            .map(|u| u as f64)
            .unwrap_or(f64::NAN),
    ))
}

fn code_point_at(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let i = to_integer(vm, &arg(args, 0))?;
    if i < 0.0 {
        return Ok(Value::Undefined);
    }
    let u = units16(&s);
    let i = i as usize;
    let Some(&hi) = u.get(i) else {
        return Ok(Value::Undefined);
    };
    if (0xd800..0xdc00).contains(&hi) {
        if let Some(&lo) = u.get(i + 1) {
            if (0xdc00..0xe000).contains(&lo) {
                let cp = 0x10000 + ((hi as u32 - 0xd800) << 10) + (lo as u32 - 0xdc00);
                return Ok(Value::Number(cp as f64));
            }
        }
    }
    Ok(Value::Number(hi as f64))
}

fn at(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let n = len16(&s) as f64;
    let i = to_integer(vm, &arg(args, 0))?;
    let i = if i < 0.0 { n + i } else { i };
    if i < 0.0 || i >= n {
        return Ok(Value::Undefined);
    }
    Ok(char_at16(&s, i as usize)
        .map(Value::from)
        .unwrap_or_default())
}

fn position(
    vm: &mut Vm,
    args: &[Value],
    i: usize,
    default: usize,
    len: usize,
) -> Result<usize, Value> {
    match args.get(i) {
        Some(v) if !v.is_undefined() => Ok(to_integer(vm, v)?.max(0.0).min(len as f64) as usize),
        _ => Ok(default),
    }
}

fn index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let needle = str_arg(vm, args, 0)?;
    let from = position(vm, args, 1, 0, len16(&s))?;
    Ok(Value::Number(
        index_of16(&s, &needle, from)
            .map(|i| i as f64)
            .unwrap_or(-1.0),
    ))
}

fn last_index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let needle = str_arg(vm, args, 0)?;
    let n = len16(&s);
    let from = position(vm, args, 1, n, n)?;
    Ok(Value::Number(
        last_index_of16(&s, &needle, from)
            .map(|i| i as f64)
            .unwrap_or(-1.0),
    ))
}

fn no_regexp(vm: &mut Vm, args: &[Value], name: &str) -> Result<(), Value> {
    if is_regexp(vm, &arg(args, 0))? {
        return Err(vm.type_error(format!(
            "First argument to String.prototype.{name} must not be a regular expression"
        )));
    }
    Ok(())
}

fn includes(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    no_regexp(vm, args, "includes")?;
    let needle = str_arg(vm, args, 0)?;
    let from = position(vm, args, 1, 0, len16(&s))?;
    Ok(Value::Bool(index_of16(&s, &needle, from).is_some()))
}

fn starts_with(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    no_regexp(vm, args, "startsWith")?;
    let needle = str_arg(vm, args, 0)?;
    let from = position(vm, args, 1, 0, len16(&s))?;
    let rest = slice16(&s, from, usize::MAX);
    Ok(Value::Bool(rest.starts_with(&*needle)))
}

fn ends_with(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    no_regexp(vm, args, "endsWith")?;
    let needle = str_arg(vm, args, 0)?;
    let n = len16(&s);
    let end = position(vm, args, 1, n, n)?;
    let head = slice16(&s, 0, end);
    Ok(Value::Bool(head.ends_with(&*needle)))
}

fn slice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let n = len16(&s);
    let start = match arg(args, 0) {
        Value::Undefined => 0,
        v => relative_index(to_integer(vm, &v)?, n),
    };
    let end = match arg(args, 1) {
        Value::Undefined => n,
        v => relative_index(to_integer(vm, &v)?, n),
    };
    Ok(slice16(&s, start, end).into())
}

fn substring(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let n = len16(&s);
    let a = position(vm, args, 0, 0, n)?;
    let b = position(vm, args, 1, n, n)?;
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    Ok(slice16(&s, lo, hi).into())
}

fn substr(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let n = len16(&s);
    let start = match arg(args, 0) {
        Value::Undefined => 0,
        v => relative_index(to_integer(vm, &v)?, n),
    };
    let count = match arg(args, 1) {
        Value::Undefined => n,
        v => to_integer(vm, &v)?.max(0.0) as usize,
    };
    Ok(slice16(&s, start, start.saturating_add(count)).into())
}

fn to_upper(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this_str(vm, &this)?.to_uppercase().into())
}

fn to_lower(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this_str(vm, &this)?.to_lowercase().into())
}

fn trim(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this_str(vm, &this)?
        .trim_matches(crate::value::is_js_space)
        .into())
}

fn trim_start(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this_str(vm, &this)?
        .trim_start_matches(crate::value::is_js_space)
        .into())
}

fn trim_end(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this_str(vm, &this)?
        .trim_end_matches(crate::value::is_js_space)
        .into())
}

const MATCH: usize = 0;
const MATCH_ALL: usize = 1;
const REPLACE: usize = 2;
const SEARCH: usize = 3;
const SPLIT: usize = 4;

const SYMBOL_KEYS: [&str; 5] = [
    crate::object::MATCH_KEY,
    crate::object::MATCH_ALL_KEY,
    crate::object::REPLACE_KEY,
    crate::object::SEARCH_KEY,
    crate::object::SPLIT_KEY,
];

enum Dispatch {
    Done(Value),
    Fast(Rc<crate::object::Obj>, Rc<crate::regexp::RegexData>),
    Plain,
}

fn coercible(vm: &mut Vm, this: &Value, name: &str) -> Result<(), Value> {
    if this.is_nullish() {
        return Err(vm.type_error(format!(
            "String.prototype.{name} called on null or undefined"
        )));
    }
    Ok(())
}

fn dispatch(
    vm: &mut Vm,
    this: &Value,
    target: &Value,
    sym: usize,
    rest: &[Value],
) -> Result<Dispatch, Value> {
    if target.is_nullish() || matches!(target, Value::Str(_) | Value::Number(_) | Value::Bool(_)) {
        return Ok(Dispatch::Plain);
    }
    if let Some((o, data)) = super::regexp::pristine(vm, target, sym) {
        return Ok(Dispatch::Fast(o, data));
    }
    let m = vm.get_prop(target, SYMBOL_KEYS[sym])?;
    if m.is_nullish() {
        return Ok(Dispatch::Plain);
    }
    if !m.is_callable() {
        return Err(vm.type_error(format!(
            "{} is not a function",
            crate::inspect::inspect_plain(&m)
        )));
    }
    let mut call_args = Vec::with_capacity(rest.len() + 1);
    call_args.push(this.clone());
    call_args.extend_from_slice(rest);
    Ok(Dispatch::Done(vm.call_value(
        &m,
        target.clone(),
        &call_args,
    )?))
}

fn is_regexp(vm: &mut Vm, v: &Value) -> Result<bool, Value> {
    if !v.is_object() {
        return Ok(false);
    }
    let m = vm.get_prop(v, crate::object::MATCH_KEY)?;
    if !m.is_undefined() {
        return Ok(m.truthy());
    }
    Ok(super::regexp::as_regexp(v).is_some())
}

fn require_global(vm: &mut Vm, v: &Value, name: &str) -> Result<(), Value> {
    if is_regexp(vm, v)? {
        let flags = vm.get_prop(v, "flags")?;
        if flags.is_nullish() {
            return Err(vm.type_error("Cannot convert undefined or null to object"));
        }
        if !vm.to_string(&flags)?.contains('g') {
            return Err(vm.type_error(format!(
                "String.prototype.{name} called with a non-global RegExp argument"
            )));
        }
    }
    Ok(())
}

fn regexp_create(vm: &mut Vm, pattern: &Value, flags: &str) -> R {
    let source = match pattern {
        Value::Undefined => Rc::from(""),
        p => vm.to_string(p)?,
    };
    super::regexp::make_regexp(vm, &source, flags)
}

fn invoke_symbol(vm: &mut Vm, rx: &Value, sym: usize, args: &[Value]) -> R {
    let m = vm.get_prop(rx, SYMBOL_KEYS[sym])?;
    if !m.is_callable() {
        return Err(vm.type_error(format!(
            "{} is not a function",
            crate::inspect::inspect_plain(&m)
        )));
    }
    vm.call_value(&m, rx.clone(), args)
}

fn split(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    coercible(vm, &this, "split")?;
    let separator = arg(args, 0);
    let limit_arg = arg(args, 1);
    match dispatch(
        vm,
        &this,
        &separator,
        SPLIT,
        std::slice::from_ref(&limit_arg),
    )? {
        Dispatch::Done(v) => return Ok(v),
        Dispatch::Fast(_, data) => {
            let s = vm.to_string(&this)?;
            let limit = match &limit_arg {
                Value::Undefined => u32::MAX as usize,
                v => crate::number::to_uint32(vm.to_number(v)?) as usize,
            };
            return super::regexp::string_split_regexp(vm, s, &data, limit);
        }
        Dispatch::Plain => {}
    }
    let s = vm.to_string(&this)?;
    let lim = match &limit_arg {
        Value::Undefined => u32::MAX as usize,
        v => crate::number::to_uint32(vm.to_number(v)?) as usize,
    };
    let sep = vm.to_string(&separator)?;
    if lim == 0 {
        return Ok(vm.new_array(Vec::new()));
    }
    if separator.is_undefined() {
        return Ok(vm.new_array(vec![Value::Str(s)]));
    }
    let parts: Vec<Value> = if sep.is_empty() {
        if s.is_ascii() {
            s.bytes()
                .take(lim)
                .map(|b| Value::Str((b as char).to_string().into()))
                .collect()
        } else {
            units16(&s)
                .iter()
                .take(lim)
                .map(|u| Value::Str(crate::value::from16(&[*u]).into()))
                .collect()
        }
    } else if s.is_empty() {
        vec![Value::Str(s.clone())]
    } else {
        s.split(&*sep).take(lim).map(Value::from).collect()
    };
    Ok(vm.new_array(parts))
}

fn replacement(
    vm: &mut Vm,
    rep: &Value,
    matched: &str,
    offset: usize,
    whole: &Rc<str>,
) -> Result<String, Value> {
    if rep.is_callable() {
        let r = vm.call_value(
            rep,
            Value::Undefined,
            &[
                matched.into(),
                Value::Number(offset as f64),
                Value::Str(whole.clone()),
            ],
        )?;
        return Ok(vm.to_string(&r)?.to_string());
    }
    let template = vm.to_string(rep)?;
    let b = crate::regexp::byte_index(whole, offset);
    let before = &whole[..b];
    let after = &whole[(b + matched.len()).min(whole.len())..];
    Ok(super::regexp::expand_template(
        &template,
        matched,
        before,
        after,
        &[Value::Str(matched.into())],
        &Value::Undefined,
    ))
}

fn match_fn(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    coercible(vm, &this, "match")?;
    let r = arg(args, 0);
    match dispatch(vm, &this, &r, MATCH, &[])? {
        Dispatch::Done(v) => return Ok(v),
        Dispatch::Fast(..) => {
            let s = vm.to_string(&this)?;
            return super::regexp::string_match(vm, s, &r);
        }
        Dispatch::Plain => {}
    }
    let s = vm.to_string(&this)?;
    let rx = regexp_create(vm, &r, "")?;
    if super::regexp::pristine(vm, &rx, MATCH).is_some() {
        return super::regexp::string_match(vm, s, &rx);
    }
    invoke_symbol(vm, &rx, MATCH, &[Value::Str(s)])
}

fn match_all(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    coercible(vm, &this, "matchAll")?;
    let r = arg(args, 0);
    if !r.is_nullish() {
        require_global(vm, &r, "matchAll")?;
    }
    match dispatch(vm, &this, &r, MATCH_ALL, &[])? {
        Dispatch::Done(v) => return Ok(v),
        Dispatch::Fast(..) => {
            let s = vm.to_string(&this)?;
            return invoke_symbol(vm, &r, MATCH_ALL, &[Value::Str(s)]);
        }
        Dispatch::Plain => {}
    }
    let s = vm.to_string(&this)?;
    let rx = regexp_create(vm, &r, "g")?;
    invoke_symbol(vm, &rx, MATCH_ALL, &[Value::Str(s)])
}

fn search(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    coercible(vm, &this, "search")?;
    let r = arg(args, 0);
    match dispatch(vm, &this, &r, SEARCH, &[])? {
        Dispatch::Done(v) => return Ok(v),
        Dispatch::Fast(..) => {
            let s = vm.to_string(&this)?;
            return super::regexp::string_search(vm, s, &r);
        }
        Dispatch::Plain => {}
    }
    let s = vm.to_string(&this)?;
    let rx = regexp_create(vm, &r, "")?;
    if super::regexp::pristine(vm, &rx, SEARCH).is_some() {
        return super::regexp::string_search(vm, s, &rx);
    }
    invoke_symbol(vm, &rx, SEARCH, &[Value::Str(s)])
}

fn replace_impl(vm: &mut Vm, this: Value, args: &[Value], all: bool) -> R {
    let name = if all { "replaceAll" } else { "replace" };
    coercible(vm, &this, name)?;
    let search_value = arg(args, 0);
    let rep = arg(args, 1);
    if all && !search_value.is_nullish() {
        require_global(vm, &search_value, name)?;
    }
    match dispatch(
        vm,
        &this,
        &search_value,
        REPLACE,
        std::slice::from_ref(&rep),
    )? {
        Dispatch::Done(v) => return Ok(v),
        Dispatch::Fast(o, data) => {
            let s = vm.to_string(&this)?;
            return super::regexp::string_replace_regexp(vm, s, &o, &data, &rep, all);
        }
        Dispatch::Plain => {}
    }
    let s = vm.to_string(&this)?;
    let pat = vm.to_string(&search_value)?;
    let rep = if rep.is_callable() {
        rep
    } else {
        Value::Str(vm.to_string(&rep)?)
    };
    let mut out = String::new();
    let mut last = 0;
    if pat.is_empty() {
        if !all {
            let r = replacement(vm, &rep, "", 0, &s)?;
            return Ok(format!("{r}{s}").into());
        }
        let units = units16(&s);
        for (i, u) in units.iter().enumerate() {
            let r = replacement(vm, &rep, "", i, &s)?;
            out.push_str(&r);
            crate::value::push16(&mut out, &crate::value::from16(&[*u]));
        }
        let r = replacement(vm, &rep, "", units.len(), &s)?;
        out.push_str(&r);
        return Ok(out.into());
    }
    let positions: Vec<usize> = if all {
        s.match_indices(&*pat).map(|(i, _)| i).collect()
    } else {
        s.find(&*pat).into_iter().collect()
    };
    for i in positions {
        out.push_str(&s[last..i]);
        let offset = len16(&s[..i]);
        let r = replacement(vm, &rep, &pat, offset, &s)?;
        crate::value::push16(&mut out, &r);
        last = i + pat.len();
    }
    crate::value::push16(&mut out, &s[last..]);
    Ok(out.into())
}

fn replace(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    replace_impl(vm, this, args, false)
}

fn replace_all(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    replace_impl(vm, this, args, true)
}

fn repeat(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let n = to_integer(vm, &arg(args, 0))?;
    if n < 0.0 || n.is_infinite() || n * s.len() as f64 > 512.0 * 1024.0 * 1024.0 {
        return Err(vm.range_error(format!(
            "Invalid count value: {}",
            crate::number::number_to_string(n)
        )));
    }
    let n = n as usize;
    let mut out = String::with_capacity(s.len() * n);
    for _ in 0..n {
        crate::value::push16(&mut out, &s);
    }
    Ok(out.into())
}

fn pad(vm: &mut Vm, this: Value, args: &[Value], start: bool) -> R {
    let s = this_str(vm, &this)?;
    let target = to_integer(vm, &arg(args, 0))?.max(0.0) as usize;
    let fill = match arg(args, 1) {
        Value::Undefined => Rc::from(" "),
        v => vm.to_string(&v)?,
    };
    let n = len16(&s);
    if target <= n || fill.is_empty() {
        return Ok(Value::Str(s));
    }
    let need = target - n;
    let fill_units = units16(&fill);
    let padding: Vec<u16> = fill_units.iter().cycle().take(need).copied().collect();
    let padding = crate::value::from16(&padding);
    Ok(if start {
        format!("{padding}{s}")
    } else {
        format!("{s}{padding}")
    }
    .into())
}

fn pad_start(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    pad(vm, this, args, true)
}

fn pad_end(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    pad(vm, this, args, false)
}

fn concat(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let mut s = this_str(vm, &this)?.to_string();
    for a in args {
        let x = vm.to_string(a)?;
        crate::value::push16(&mut s, &x);
    }
    Ok(s.into())
}

fn locale_compare(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let other = str_arg(vm, args, 0)?;
    let intl = super::intl::load(vm)?;
    let c = vm.get_prop(&intl, "Collator")?;
    let col = vm.construct(&c, &[arg(args, 1), arg(args, 2)])?;
    let f = vm.get_prop(&col, "compare")?;
    vm.call_value(&f, col, &[Value::Str(s), Value::Str(other)])
}
