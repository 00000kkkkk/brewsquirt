use super::{arg, make_ctor, method};
use crate::inspect::inspect_plain;
use crate::jsstr::JsStr;
use crate::object::{Obj, ObjKind, RegExpObj};
use crate::regexp::{Match, RegexData, compile, next_char_boundary};
use crate::value::Value;
use crate::value::{byte_index_rc, len16_rc, utf16_index_rc};
use crate::vm::{ErrorKind, Vm};
use std::cell::RefCell;
use std::rc::Rc;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let rp = vm.realm.regexp_proto.clone();
    method(vm, &rp, "exec", 1, exec);
    method(vm, &rp, "test", 1, test);
    method(vm, &rp, "toString", 0, to_string);
    method(vm, &rp, "compile", 2, regexp_compile);
    let ctor = make_ctor(vm, "RegExp", 2, regexp_ctor, None, &rp);
    vm.define_global("RegExp", Value::Object(ctor));
}

pub fn new_regexp(vm: &Vm, data: Rc<RegexData>) -> Value {
    Value::Object(Obj::new(
        ObjKind::RegExp(RegExpObj {
            data: RefCell::new(data),
            last_index: RefCell::new(Value::Number(0.0)),
        }),
        Some(vm.realm.regexp_proto.clone()),
    ))
}

pub fn make_regexp(vm: &mut Vm, source: &str, flags: &str) -> R {
    match compile(source, flags) {
        Ok(data) => Ok(new_regexp(vm, data)),
        Err(msg) => Err(vm.make_error(ErrorKind::SyntaxError, msg)),
    }
}

pub fn as_regexp(v: &Value) -> Option<(Rc<Obj>, Rc<RegexData>)> {
    match v {
        Value::Object(o) => match &o.kind {
            ObjKind::RegExp(r) => Some((o.clone(), r.data.borrow().clone())),
            _ => None,
        },
        _ => None,
    }
}

fn this_regexp(vm: &mut Vm, this: &Value) -> Result<(Rc<Obj>, Rc<RegexData>), Value> {
    as_regexp(this).ok_or_else(|| {
        vm.type_error(format!(
            "RegExp method called on incompatible receiver {}",
            inspect_plain(this)
        ))
    })
}

fn last_index(vm: &mut Vm, o: &Obj) -> Result<f64, Value> {
    let v = match &o.kind {
        ObjKind::RegExp(r) => r.last_index.borrow().clone(),
        _ => Value::Number(0.0),
    };
    let n = match v {
        Value::Number(n) => n,
        other => vm.to_number(&other)?,
    };
    Ok(if n.is_nan() || n <= 0.0 {
        0.0
    } else {
        n.trunc().min(9007199254740991.0)
    })
}

fn set_last_index(vm: &mut Vm, o: &Obj, v: f64) -> Result<(), Value> {
    if let ObjKind::RegExp(r) = &o.kind {
        if o.flags.get() & crate::object::ELEMENTS_FROZEN != 0 {
            return Err(vm.read_only_error(o, "lastIndex"));
        }
        *r.last_index.borrow_mut() = Value::Number(v);
    }
    Ok(())
}

pub fn pristine(vm: &Vm, v: &Value, sym: usize) -> Option<(Rc<Obj>, Rc<RegexData>)> {
    let (o, data) = as_regexp(v)?;
    if !o.props.borrow().is_empty()
        || !o
            .proto
            .borrow()
            .as_ref()
            .is_some_and(|p| Rc::ptr_eq(p, &vm.realm.regexp_proto))
    {
        return None;
    }
    let p = vm.pristine.as_ref()?;
    let rp = vm.realm.regexp_proto.props.borrow();
    let key = [
        crate::object::MATCH_KEY,
        crate::object::MATCH_ALL_KEY,
        crate::object::REPLACE_KEY,
        crate::object::SEARCH_KEY,
        crate::object::SPLIT_KEY,
    ][sym];
    let ok = rp.get("exec").is_some_and(|x| x.strict_eq(&p.regexp_exec))
        && rp
            .get(key)
            .is_some_and(|x| x.strict_eq(&p.regexp_symbols[sym]))
        && rp
            .get("flags")
            .is_some_and(|x| x.strict_eq(&p.regexp_flags));
    drop(rp);
    ok.then_some((o, data))
}

fn run_error(vm: &mut Vm, msg: String) -> Value {
    vm.make_error(ErrorKind::Error, msg)
}

pub fn exec_raw(
    vm: &mut Vm,
    obj: &Obj,
    data: &RegexData,
    s: &JsStr,
) -> Result<Option<Match>, Value> {
    let tracks = data.global || data.sticky;
    let li = last_index(vm, obj)?;
    let start16 = if tracks { li } else { 0.0 };
    if start16 > len16_rc(s) as f64 {
        if tracks {
            set_last_index(vm, obj, 0.0)?;
        }
        return Ok(None);
    }
    let pos = byte_index_rc(s, start16 as usize);
    let found = data.find_at(s, pos).map_err(|e| run_error(vm, e))?;
    if tracks {
        match &found {
            Some(m) => set_last_index(vm, obj, utf16_index_rc(s, m.end) as f64)?,
            None => set_last_index(vm, obj, 0.0)?,
        }
    }
    Ok(found)
}

pub fn match_array(vm: &mut Vm, data: &RegexData, s: &JsStr, m: &Match) -> Value {
    let mut items = Vec::with_capacity(m.groups.len());
    for g in &m.groups {
        items.push(match g {
            Some((a, b)) => Value::Str(s[*a..*b].into()),
            None => Value::Undefined,
        });
    }
    let arr = vm.new_array(items.clone());
    if let Value::Object(o) = &arr {
        o.set_own("index", Value::Number(utf16_index_rc(s, m.start) as f64));
        o.set_own("input", Value::Str(s.clone()));
        o.set_own("groups", groups_object(vm, data, &items));
        if data.flags.contains('d') {
            let mut pairs = Vec::with_capacity(m.groups.len());
            for g in &m.groups {
                pairs.push(match g {
                    Some((a, b)) => vm.new_array(vec![
                        Value::Number(utf16_index_rc(s, *a) as f64),
                        Value::Number(utf16_index_rc(s, *b) as f64),
                    ]),
                    None => Value::Undefined,
                });
            }
            let indices = vm.new_array(pairs.clone());
            if let Value::Object(io) = &indices {
                io.set_own("groups", groups_object(vm, data, &pairs));
            }
            o.set_own("indices", indices);
        }
    }
    arr
}

fn groups_object(vm: &mut Vm, data: &RegexData, items: &[Value]) -> Value {
    if !data.has_names() {
        return Value::Undefined;
    }
    let g = Obj::new(ObjKind::Ordinary, None);
    for (i, name) in data.names.iter().enumerate() {
        if let Some(n) = name {
            g.set_own(n.clone(), items.get(i).cloned().unwrap_or_default());
        }
    }
    let _ = vm;
    Value::Object(g)
}

fn regexp_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let pattern = arg(args, 0);
    let flags_arg = arg(args, 1);
    let pattern_is_regexp = match &pattern {
        Value::Object(_) => {
            let m = vm.get_prop(&pattern, crate::object::MATCH_KEY)?;
            if m.is_undefined() {
                as_regexp(&pattern).is_some()
            } else {
                m.truthy()
            }
        }
        _ => false,
    };
    let (source, default_flags): (JsStr, JsStr) = match as_regexp(&pattern) {
        Some((_, d)) => (d.source.clone(), d.flags.clone()),
        None if pattern_is_regexp => {
            let p = vm.get_prop(&pattern, "source")?;
            let p = if p.is_undefined() {
                JsStr::from("")
            } else {
                vm.to_string(&p)?
            };
            let f = if flags_arg.is_undefined() {
                let f = vm.get_prop(&pattern, "flags")?;
                if f.is_undefined() {
                    JsStr::from("")
                } else {
                    vm.to_string(&f)?
                }
            } else {
                JsStr::from("")
            };
            (p, f)
        }
        None => match pattern {
            Value::Undefined => ("(?:)".into(), "".into()),
            p => (vm.to_string(&p)?, "".into()),
        },
    };
    let flags = match flags_arg {
        Value::Undefined => default_flags,
        f => vm.to_string(&f)?,
    };
    make_regexp(vm, &source, &flags)
}

fn exec(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, data) = this_regexp(vm, &this)?;
    let s = vm.to_string(&arg(args, 0))?;
    match exec_raw(vm, &o, &data, &s)? {
        Some(m) => Ok(match_array(vm, &data, &s, &m)),
        None => Ok(Value::Null),
    }
}

fn test(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !this.is_object() {
        return Err(vm.type_error(format!(
            "Method RegExp.prototype.test called on incompatible receiver {}",
            inspect_plain(&this)
        )));
    }
    let s = vm.to_string(&arg(args, 0))?;
    let exec = vm.get_prop(&this, "exec")?;
    let builtin = vm
        .pristine
        .as_ref()
        .is_some_and(|p| exec.strict_eq(&p.regexp_exec));
    if !builtin && exec.is_callable() {
        let r = vm.call_value(&exec, this.clone(), &[Value::Str(s)])?;
        if !matches!(r, Value::Null) && !r.is_object() {
            return Err(vm.type_error("exec result must be an object or null"));
        }
        return Ok(Value::Bool(!matches!(r, Value::Null)));
    }
    let (o, data) = this_regexp(vm, &this)?;
    let tracks = data.global || data.sticky;
    let li = last_index(vm, &o)?;
    let start16 = if tracks { li } else { 0.0 };
    if start16 > len16_rc(&s) as f64 {
        if tracks {
            set_last_index(vm, &o, 0.0)?;
        }
        return Ok(Value::Bool(false));
    }
    let pos = byte_index_rc(&s, start16 as usize);
    let found = data
        .find_whole(&s, pos, data.sticky)
        .map_err(|e| run_error(vm, e))?;
    if tracks {
        match found {
            Some((_, end)) => set_last_index(vm, &o, utf16_index_rc(&s, end) as f64)?,
            None => set_last_index(vm, &o, 0.0)?,
        }
    }
    Ok(Value::Bool(found.is_some()))
}

fn regexp_compile(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, _) = this_regexp(vm, &this)?;
    let pattern = arg(args, 0);
    let flags = arg(args, 1);
    let (source, flags): (JsStr, JsStr) = match as_regexp(&pattern) {
        Some((_, d)) => {
            if !flags.is_undefined() {
                return Err(
                    vm.type_error("Cannot supply flags when constructing one RegExp from another")
                );
            }
            (d.source.clone(), d.flags.clone())
        }
        None => {
            let s = match pattern {
                Value::Undefined => JsStr::from(""),
                p => vm.to_string(&p)?,
            };
            let f = match flags {
                Value::Undefined => JsStr::from(""),
                f => vm.to_string(&f)?,
            };
            (s, f)
        }
    };
    let data = compile(&source, &flags).map_err(|m| vm.make_error(ErrorKind::SyntaxError, m))?;
    if let ObjKind::RegExp(r) = &o.kind {
        *r.data.borrow_mut() = data;
    }
    vm.set_prop(&this, "lastIndex".into(), Value::Number(0.0))?;
    Ok(this)
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    if !this.is_object() {
        return Err(vm.type_error("RegExp.prototype.toString requires that 'this' be an Object"));
    }
    let source = vm.get_prop(&this, "source")?;
    let source = vm.to_string(&source)?;
    let flags = vm.get_prop(&this, "flags")?;
    let flags = vm.to_string(&flags)?;
    Ok(format!("/{source}/{flags}").into())
}

pub fn coerce_regexp(
    vm: &mut Vm,
    v: &Value,
    flags: &str,
) -> Result<(Rc<Obj>, Rc<RegexData>), Value> {
    if let Some(r) = as_regexp(v) {
        return Ok(r);
    }
    let source = match v {
        Value::Undefined => JsStr::from("(?:)"),
        other => vm.to_string(other)?,
    };
    let re = make_regexp(vm, &source, flags)?;
    Ok(as_regexp(&re).expect("regexp"))
}

fn all_matches(vm: &mut Vm, data: &RegexData, s: &str) -> Result<Vec<Match>, Value> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos <= s.len() {
        let Some(m) = data.find_at(s, pos).map_err(|e| run_error(vm, e))? else {
            break;
        };
        pos = if m.end == m.start {
            next_char_boundary(s, m.end)
        } else {
            m.end
        };
        out.push(m);
    }
    Ok(out)
}

pub fn string_match(vm: &mut Vm, s: JsStr, pattern: &Value) -> R {
    let (o, data) = coerce_regexp(vm, pattern, "")?;
    if !data.global {
        return match exec_raw(vm, &o, &data, &s)? {
            Some(m) => Ok(match_array(vm, &data, &s, &m)),
            None => Ok(Value::Null),
        };
    }
    let found = all_matches(vm, &data, &s)?;
    set_last_index(vm, &o, 0.0)?;
    if found.is_empty() {
        return Ok(Value::Null);
    }
    let list = found
        .iter()
        .map(|m| Value::Str(s[m.start..m.end].into()))
        .collect();
    Ok(vm.new_array(list))
}

pub fn string_search(vm: &mut Vm, s: JsStr, pattern: &Value) -> R {
    let (_, data) = coerce_regexp(vm, pattern, "")?;
    let found = data.find_from(&s, 0, false).map_err(|e| run_error(vm, e))?;
    Ok(Value::Number(match found {
        Some(m) => utf16_index_rc(&s, m.start) as f64,
        None => -1.0,
    }))
}

pub fn expand_template(
    template: &str,
    matched: &str,
    before: &str,
    after: &str,
    groups: &[Value],
    named: &Value,
) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '$' || i + 1 >= chars.len() {
            out.push(c);
            i += 1;
            continue;
        }
        let n = chars[i + 1];
        match n {
            '$' => {
                out.push('$');
                i += 2;
            }
            '&' => {
                out.push_str(matched);
                i += 2;
            }
            '`' => {
                out.push_str(before);
                i += 2;
            }
            '\'' => {
                out.push_str(after);
                i += 2;
            }
            '0'..='9' => {
                let d1 = n.to_digit(10).unwrap_or(0) as usize;
                let two = chars
                    .get(i + 2)
                    .and_then(|d| d.to_digit(10))
                    .map(|d2| d1 * 10 + d2 as usize);
                let (idx, width) = match two {
                    Some(t) if t >= 1 && t < groups.len() => (t, 3),
                    _ => (d1, 2),
                };
                if idx >= 1 && idx < groups.len() {
                    if let Value::Str(g) = &groups[idx] {
                        out.push_str(g);
                    }
                    i += width;
                } else {
                    out.push('$');
                    i += 1;
                }
            }
            '<' if !named.is_undefined() => match chars[i + 2..].iter().position(|&ch| ch == '>') {
                Some(end) => {
                    let name: String = chars[i + 2..i + 2 + end].iter().collect();
                    if let Value::Object(g) = named {
                        if let Some(Value::Str(v)) = g.get_own(&name) {
                            out.push_str(&v);
                        }
                    }
                    i += 3 + end;
                }
                None => {
                    out.push('$');
                    i += 1;
                }
            },
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}

pub fn string_replace_regexp(
    vm: &mut Vm,
    s: JsStr,
    obj: &Rc<Obj>,
    data: &RegexData,
    replacement: &Value,
    require_global: bool,
) -> R {
    if require_global && !data.global {
        return Err(vm.type_error("replaceAll must be called with a global RegExp"));
    }
    let matches = if data.global {
        let m = all_matches(vm, data, &s)?;
        set_last_index(vm, obj, 0.0)?;
        m
    } else {
        exec_raw(vm, obj, data, &s)?.into_iter().collect()
    };
    let template = if replacement.is_callable() {
        None
    } else {
        Some(vm.to_string(replacement)?)
    };
    let mut out = String::new();
    let mut last = 0;
    for m in matches {
        out.push_str(&s[last..m.start]);
        let groups: Vec<Value> = m
            .groups
            .iter()
            .map(|g| match g {
                Some((a, b)) => Value::Str(s[*a..*b].into()),
                None => Value::Undefined,
            })
            .collect();
        let named = groups_object(vm, data, &groups);
        let matched = &s[m.start..m.end];
        let piece = match &template {
            Some(t) => expand_template(t, matched, &s[..m.start], &s[m.end..], &groups, &named),
            None => {
                let mut call_args = groups.clone();
                call_args.push(Value::Number(utf16_index_rc(&s, m.start) as f64));
                call_args.push(Value::Str(s.clone()));
                if !named.is_undefined() {
                    call_args.push(named);
                }
                let r = vm.call_value(replacement, Value::Undefined, &call_args)?;
                vm.to_string(&r)?.to_string()
            }
        };
        out.push_str(&piece);
        last = m.end;
    }
    out.push_str(&s[last..]);
    Ok(out.into())
}

pub fn string_split_regexp(vm: &mut Vm, s: JsStr, data: &RegexData, limit: usize) -> R {
    if limit == 0 {
        return Ok(vm.new_array(Vec::new()));
    }
    if s.is_empty() {
        let hit = data.find_from(&s, 0, true).map_err(|e| run_error(vm, e))?;
        return Ok(vm.new_array(if hit.is_some() {
            Vec::new()
        } else {
            vec![Value::Str(s.clone())]
        }));
    }
    let mut parts = Vec::new();
    let mut p = 0;
    let mut q = 0;
    while q < s.len() {
        let Some(m) = data.find_from(&s, q, false).map_err(|e| run_error(vm, e))? else {
            break;
        };
        if m.start >= s.len() {
            break;
        }
        if m.end == p || (m.start == m.end && m.start == p) {
            q = next_char_boundary(&s, m.start.max(q));
            continue;
        }
        parts.push(Value::Str(s[p..m.start].into()));
        if parts.len() >= limit {
            return Ok(vm.new_array(parts));
        }
        for g in m.groups.iter().skip(1) {
            parts.push(match g {
                Some((a, b)) => Value::Str(s[*a..*b].into()),
                None => Value::Undefined,
            });
            if parts.len() >= limit {
                return Ok(vm.new_array(parts));
            }
        }
        p = m.end;
        q = if m.end == m.start {
            next_char_boundary(&s, m.end)
        } else {
            m.end
        };
    }
    parts.push(Value::Str(s[p..].into()));
    Ok(vm.new_array(parts))
}
