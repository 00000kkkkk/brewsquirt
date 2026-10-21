use crate::number::number_to_string;
use crate::object::{Obj, ObjKind};
use crate::value::Value;
use std::rc::Rc;

const MAX_DEPTH: usize = 2;
const MAX_ITEMS: usize = 100;

pub fn inspect_top(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        _ => with_refs(v, true),
    }
}

pub fn inspect_plain(v: &Value) -> String {
    with_refs(v, false)
}

fn with_refs(v: &Value, top: bool) -> String {
    let mut seen = Vec::new();
    let mut circular = Vec::new();
    let s = format_value(v, 0, &mut seen, &mut circular, top);
    match v {
        Value::Object(o) if circular.contains(&Rc::as_ptr(o)) => format!("<ref *1> {s}"),
        _ => s,
    }
}

pub fn quote(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else if s.contains('\'') && s.contains('"') && !s.contains('`') {
        '`'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(q);
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{b}' => out.push_str("\\v"),
            '\\' => out.push_str("\\\\"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

fn format_key(k: &str) -> String {
    if is_identifier(k) {
        k.to_string()
    } else {
        quote(k)
    }
}

pub fn class_name(o: &Obj) -> Option<Rc<str>> {
    let proto = o.proto.borrow().clone()?;
    let ctor = proto.get_own("constructor")?;
    let ctor = ctor.as_object()?;
    let name = ctor.function_name()?;
    if name.is_empty() || &*name == "Object" {
        None
    } else {
        Some(name)
    }
}

pub fn function_desc(o: &Obj) -> String {
    match &o.kind {
        ObjKind::Function(c) if c.proto.is_class() => {
            let name = o.function_name().unwrap_or_else(|| "".into());
            let mut s = if name.is_empty() {
                "[class (anonymous)".to_string()
            } else {
                format!("[class {name}")
            };
            if let Some(parent) = o.proto.borrow().as_ref() {
                if let ObjKind::Function(_) | ObjKind::Native(_) = parent.kind {
                    if let Some(parent_name) = parent.function_name() {
                        if !parent_name.is_empty() {
                            s.push_str(&format!(" extends {parent_name}"));
                        }
                    }
                }
            }
            s.push(']');
            s
        }
        _ => {
            let name = o.function_name().unwrap_or_else(|| "".into());
            if name.is_empty() {
                "[Function (anonymous)]".to_string()
            } else {
                format!("[Function: {name}]")
            }
        }
    }
}

pub fn error_header(o: &Obj) -> String {
    let name = match o.get_chain("name") {
        Some(Value::Str(s)) => s.to_string(),
        _ => "Error".to_string(),
    };
    let msg = match o.get_chain("message") {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::Undefined) | None => String::new(),
        Some(v) => inspect_plain(&v),
    };
    if msg.is_empty() {
        name
    } else {
        format!("{name}: {msg}")
    }
}

pub fn error_full(o: &Obj) -> String {
    let mut s = error_header(o);
    if let Some(Value::Str(stack)) = o.get_own("stack") {
        for line in stack.lines().filter(|l| l.trim_start().starts_with("at ")) {
            s.push('\n');
            s.push_str(line);
        }
    }
    s
}

fn format_number(n: f64) -> String {
    if n == 0.0 && n.is_sign_negative() {
        "-0".to_string()
    } else {
        number_to_string(n)
    }
}

fn wrap(open: &str, items: Vec<String>, close: &str) -> String {
    if items.is_empty() {
        format!("{}{}", open.trim_end(), close.trim_start())
    } else {
        format!("{open}{}{close}", items.join(", "))
    }
}

fn format_value(
    v: &Value,
    depth: usize,
    seen: &mut Vec<*const Obj>,
    circ: &mut Vec<*const Obj>,
    top: bool,
) -> String {
    match v {
        Value::Undefined => "undefined".to_string(),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => format_number(*n),
        Value::Str(s) => {
            if top {
                s.to_string()
            } else {
                quote(s)
            }
        }
        Value::Object(o) => format_object(o, depth, seen, circ, top),
        Value::Symbol(sym) => crate::builtins::symbol_text(sym),
    }
}

fn format_prop(
    k: &str,
    v: &Value,
    depth: usize,
    seen: &mut Vec<*const Obj>,
    circ: &mut Vec<*const Obj>,
) -> String {
    let key = if crate::object::is_symbol_key(k) {
        format!("Symbol({})", crate::object::symbol_key_description(k))
    } else {
        format_key(k)
    };
    if let Value::Object(a) = v {
        if let ObjKind::Accessor(acc) = &a.kind {
            let g = !acc.get.borrow().is_undefined();
            let st = !acc.set.borrow().is_undefined();
            let text = match (g, st) {
                (true, true) => "[Getter/Setter]",
                (true, false) => "[Getter]",
                _ => "[Setter]",
            };
            return format!("{key}: {text}");
        }
    }
    format!("{key}: {}", format_value(v, depth + 1, seen, circ, false))
}

fn props_items(
    o: &Obj,
    depth: usize,
    seen: &mut Vec<*const Obj>,
    circ: &mut Vec<*const Obj>,
) -> Vec<String> {
    let mut props: Vec<(Rc<str>, Value)> = o
        .props
        .borrow()
        .iter()
        .filter(|(k, _)| !crate::object::is_private_key(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    props.sort_by_key(|(k, _)| crate::object::is_symbol_key(k));
    props
        .iter()
        .map(|(k, v)| format_prop(k, v, depth, seen, circ))
        .collect()
}

fn format_object(
    o: &Rc<Obj>,
    depth: usize,
    seen: &mut Vec<*const Obj>,
    circ: &mut Vec<*const Obj>,
    top: bool,
) -> String {
    let ptr = Rc::as_ptr(o);
    if seen.contains(&ptr) {
        if !circ.contains(&ptr) {
            circ.push(ptr);
        }
        return "[Circular *1]".to_string();
    }
    match &o.kind {
        ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_) => {
            let desc = function_desc(o);
            let extra: Vec<(Rc<str>, Value)> = o
                .props
                .borrow()
                .iter()
                .filter(|(k, _)| !matches!(&***k, "prototype" | "name" | "length"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if extra.is_empty() || depth > MAX_DEPTH {
                return desc;
            }
            seen.push(ptr);
            let items: Vec<String> = extra
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}: {}",
                        format_key(k),
                        format_value(v, depth + 1, seen, circ, false)
                    )
                })
                .collect();
            seen.pop();
            wrap(&format!("{desc} {{ "), items, " }")
        }
        ObjKind::Error => {
            if top {
                error_full(o)
            } else {
                format!("[{}]", error_header(o))
            }
        }
        ObjKind::Array(a) => {
            if depth > MAX_DEPTH {
                return "[Array]".to_string();
            }
            seen.push(ptr);
            let items_v: Vec<Value> = a.borrow().iter().take(MAX_ITEMS).cloned().collect();
            let total = a.borrow().len();
            let mut items: Vec<String> = items_v
                .iter()
                .map(|v| format_value(v, depth + 1, seen, circ, false))
                .collect();
            if total > MAX_ITEMS {
                let more = total - MAX_ITEMS;
                items.push(format!(
                    "... {more} more item{}",
                    if more == 1 { "" } else { "s" }
                ));
            }
            items.extend(props_items(o, depth, seen, circ));
            seen.pop();
            wrap("[ ", items, " ]")
        }
        ObjKind::Map(m) => {
            if depth > MAX_DEPTH {
                return "[Map]".to_string();
            }
            seen.push(ptr);
            let entries: Vec<(Value, Value)> = m
                .borrow()
                .iter()
                .map(|(k, v)| (k.to_value(), v.clone()))
                .collect();
            let items: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{} => {}",
                        format_value(k, depth + 1, seen, circ, false),
                        format_value(v, depth + 1, seen, circ, false)
                    )
                })
                .collect();
            seen.pop();
            wrap(&format!("Map({}) {{ ", entries.len()), items, " }")
        }
        ObjKind::Set(s) => {
            if depth > MAX_DEPTH {
                return "[Set]".to_string();
            }
            seen.push(ptr);
            let vals: Vec<Value> = s.borrow().iter().map(|k| k.to_value()).collect();
            let items: Vec<String> = vals
                .iter()
                .map(|v| format_value(v, depth + 1, seen, circ, false))
                .collect();
            seen.pop();
            wrap(&format!("Set({}) {{ ", vals.len()), items, " }")
        }
        ObjKind::Iter(_) => "Object [Iterator] {}".to_string(),
        ObjKind::Coroutine(c) => {
            if c.is_generator && c.is_async {
                "Object [AsyncGenerator] {}".to_string()
            } else if c.is_generator {
                "Object [Generator] {}".to_string()
            } else {
                "Object [AsyncFunction] {}".to_string()
            }
        }
        ObjKind::Accessor(_) => "[Getter/Setter]".to_string(),
        ObjKind::RegExp(r) => format!("/{}/{}", r.data.source, r.data.flags),
        ObjKind::Date(t) => {
            crate::builtins::iso_string(t.get()).unwrap_or_else(|| "Invalid Date".to_string())
        }
        ObjKind::WeakMap(_) => "WeakMap { <items unknown> }".to_string(),
        ObjKind::WeakSet(_) => "WeakSet { <items unknown> }".to_string(),
        ObjKind::WeakRef(w) => match w.upgrade() {
            Some(target) if depth <= MAX_DEPTH => {
                seen.push(ptr);
                let inner = format_object(&target, depth + 1, seen, circ, false);
                seen.pop();
                format!("WeakRef {{ {inner} }}")
            }
            Some(_) => "WeakRef { [Object] }".to_string(),
            None => "WeakRef { <cleared> }".to_string(),
        },
        ObjKind::Promise(p) => {
            if depth > MAX_DEPTH {
                return "[Promise]".to_string();
            }
            seen.push(ptr);
            let inner = match &p.borrow().state {
                crate::object::PromiseState::Pending => "<pending>".to_string(),
                crate::object::PromiseState::Fulfilled(v) => {
                    format_value(v, depth + 1, seen, circ, false)
                }
                crate::object::PromiseState::Rejected(e) => {
                    format!(
                        "<rejected> {}",
                        format_value(e, depth + 1, seen, circ, false)
                    )
                }
            };
            seen.pop();
            format!("Promise {{ {inner} }}")
        }
        ObjKind::Ordinary => {
            let cname = class_name(o);
            if depth > MAX_DEPTH {
                return match cname {
                    Some(n) => format!("[{n}]"),
                    None => "[Object]".to_string(),
                };
            }
            seen.push(ptr);
            let items = props_items(o, depth, seen, circ);
            seen.pop();
            let prefix = match (&cname, o.proto.borrow().is_none()) {
                (Some(n), _) => format!("{n} "),
                (None, true) => "[Object: null prototype] ".to_string(),
                _ => String::new(),
            };
            wrap(&format!("{prefix}{{ "), items, " }")
        }
    }
}
