use super::{arg, expect_fn, make_ctor, method, relative_index, to_integer};
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
        ("replaceAll", 2, replace_all),
        ("repeat", 1, repeat),
        ("padStart", 2, pad_start),
        ("padEnd", 2, pad_end),
        ("concat", 1, concat),
        ("localeCompare", 1, locale_compare),
        ("toString", 0, to_string),
        ("valueOf", 0, to_string),
    ];
    for (name, arity, f) in methods {
        method(vm, &sp, name, *arity, *f);
    }
    let string = make_ctor(vm, "String", 1, string_ctor, None, &sp);
    method(vm, &string, "fromCharCode", 1, from_char_code);
    method(vm, &string, "fromCodePoint", 1, from_code_point);
    vm.define_global("String", Value::Object(string));
}

fn this_str(vm: &mut Vm, this: &Value) -> Result<Rc<str>, Value> {
    if this.is_nullish() {
        return Err(vm.type_error("String.prototype method called on null or undefined"));
    }
    vm.to_string(this)
}

fn str_arg(vm: &mut Vm, args: &[Value], i: usize) -> Result<Rc<str>, Value> {
    vm.to_string(&arg(args, i))
}

fn string_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if args.is_empty() {
        return Ok("".into());
    }
    if let Value::Symbol(s) = &args[0] {
        return Ok(super::symbol_text(s).into());
    }
    Ok(Value::Str(vm.to_string(&args[0])?))
}

fn from_char_code(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut units = Vec::with_capacity(args.len());
    for a in args {
        let n = vm.to_number(a)?;
        units.push(crate::number::to_uint32(n) as u16);
    }
    Ok(String::from_utf16_lossy(&units).into())
}

fn from_code_point(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut s = String::new();
    for a in args {
        let n = vm.to_number(a)?;
        match char::from_u32(n as u32).filter(|_| n.fract() == 0.0 && n >= 0.0) {
            Some(c) => s.push(c),
            None => return Err(vm.range_error(format!("Invalid code point {n}"))),
        }
    }
    Ok(s.into())
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(Value::Str(this_str(vm, &this)?))
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

fn includes(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let needle = str_arg(vm, args, 0)?;
    let from = position(vm, args, 1, 0, len16(&s))?;
    Ok(Value::Bool(index_of16(&s, &needle, from).is_some()))
}

fn starts_with(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let needle = str_arg(vm, args, 0)?;
    let from = position(vm, args, 1, 0, len16(&s))?;
    let rest = slice16(&s, from, usize::MAX);
    Ok(Value::Bool(rest.starts_with(&*needle)))
}

fn ends_with(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
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
    Ok(this_str(vm, &this)?.trim().into())
}

fn trim_start(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this_str(vm, &this)?.trim_start().into())
}

fn trim_end(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this_str(vm, &this)?.trim_end().into())
}

fn split(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let limit = match arg(args, 1) {
        Value::Undefined => usize::MAX,
        v => crate::number::to_uint32(vm.to_number(&v)?) as usize,
    };
    let parts: Vec<Value> = match arg(args, 0) {
        Value::Undefined => vec![Value::Str(s.clone())],
        Value::Object(_) => return Err(vm.type_error("Regular expressions are not supported yet")),
        sep => {
            let sep = vm.to_string(&sep)?;
            if sep.is_empty() {
                s.chars()
                    .map(|c| Value::Str(c.to_string().into()))
                    .collect()
            } else {
                s.split(&*sep).map(Value::from).collect()
            }
        }
    };
    let parts = parts.into_iter().take(limit).collect();
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
    Ok(template.replace("$&", matched))
}

fn replace_impl(vm: &mut Vm, this: Value, args: &[Value], all: bool) -> R {
    let s = this_str(vm, &this)?;
    if arg(args, 0).is_object() {
        return Err(vm.type_error("Regular expressions are not supported yet"));
    }
    let pat = str_arg(vm, args, 0)?;
    let rep = arg(args, 1);
    if rep.is_object() && !rep.is_callable() {
        expect_fn(vm, &rep)?;
    }
    let mut out = String::new();
    let mut last = 0;
    if pat.is_empty() {
        let r = replacement(vm, &rep, "", 0, &s)?;
        if !all {
            return Ok(format!("{r}{s}").into());
        }
        for (i, c) in s.char_indices() {
            let r = replacement(vm, &rep, "", i, &s)?;
            out.push_str(&r);
            out.push(c);
        }
        let r = replacement(vm, &rep, "", s.len(), &s)?;
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
        out.push_str(&r);
        last = i + pat.len();
    }
    out.push_str(&s[last..]);
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
    Ok(s.repeat(n as usize).into())
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
    let padding = String::from_utf16_lossy(&padding);
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
        s.push_str(&vm.to_string(a)?);
    }
    Ok(s.into())
}

fn locale_compare(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let s = this_str(vm, &this)?;
    let other = str_arg(vm, args, 0)?;
    Ok(Value::Number(match s.cmp(&other) {
        std::cmp::Ordering::Less => -1.0,
        std::cmp::Ordering::Equal => 0.0,
        std::cmp::Ordering::Greater => 1.0,
    }))
}
