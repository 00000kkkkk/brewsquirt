use crate::number::number_to_string;
use crate::object::{Obj, ObjKind};
use crate::value::Value;
use std::rc::Rc;

const MAX_DEPTH: usize = 2;
const MAX_ITEMS: usize = 100;
const BREAK_LENGTH: usize = 80;
const COMPACT: usize = 3;

struct Ctx<'a> {
    vm: Option<&'a mut crate::vm::Vm>,
    max_depth: usize,
    break_length: usize,
    compact: usize,
    indent: usize,
    current_depth: usize,
    seen: Vec<*const Obj>,
    circular: Vec<*const Obj>,
}

pub fn inspect_top_vm(vm: &mut crate::vm::Vm, v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        _ => {
            let mut ctx = Ctx::new();
            ctx.vm = Some(vm);
            format_value(&mut ctx, v, 0, true)
        }
    }
}

pub fn inspect_plain_vm(vm: &mut crate::vm::Vm, v: &Value) -> String {
    let mut ctx = Ctx::new();
    ctx.vm = Some(vm);
    format_value(&mut ctx, v, 0, false)
}

pub struct InspectOptions {
    pub depth: usize,
    pub break_length: usize,
    pub compact: usize,
}

impl Default for InspectOptions {
    fn default() -> Self {
        InspectOptions {
            depth: MAX_DEPTH,
            break_length: BREAK_LENGTH,
            compact: COMPACT,
        }
    }
}

pub fn inspect_with(vm: &mut crate::vm::Vm, v: &Value, options: InspectOptions) -> String {
    let mut ctx = Ctx::new();
    ctx.vm = Some(vm);
    ctx.max_depth = options.depth;
    ctx.break_length = options.break_length;
    ctx.compact = options.compact;
    format_value(&mut ctx, v, 0, false)
}

fn custom_inspect(ctx: &mut Ctx, o: &Rc<Obj>, rt: usize) -> Option<String> {
    let depth = Value::Number((ctx.max_depth as f64 - rt as f64).max(0.0));
    let vm = ctx.vm.as_deref_mut()?;
    let key = match vm.symbol_registry.get("nodejs.util.inspect.custom") {
        Some(Value::Symbol(s)) => s.key.clone(),
        _ => return None,
    };
    let f = o.get_chain(&key)?;
    if !f.is_callable() {
        return None;
    }
    let options = vm.new_object();
    let result = vm
        .call_value(
            &f,
            Value::Object(o.clone()),
            &[depth, Value::Object(options)],
        )
        .ok()?;
    let text = match &result {
        Value::Str(s) => s.to_string(),
        Value::Object(r) if Rc::ptr_eq(r, o) => return None,
        other => format_value(ctx, other, rt, false),
    };
    if ctx.indent > 0 && text.contains('\n') {
        let pad = " ".repeat(ctx.indent);
        return Some(text.replace('\n', &format!("\n{pad}")));
    }
    Some(text)
}

impl Ctx<'_> {
    fn new() -> Ctx<'static> {
        Ctx {
            vm: None,
            max_depth: MAX_DEPTH,
            break_length: BREAK_LENGTH,
            compact: COMPACT,
            indent: 0,
            current_depth: 0,
            seen: Vec::new(),
            circular: Vec::new(),
        }
    }
}

pub fn inspect_top(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        _ => format_value(&mut Ctx::new(), v, 0, true),
    }
}

pub fn inspect_plain(v: &Value) -> String {
    format_value(&mut Ctx::new(), v, 0, false)
}

fn len16(s: &str) -> usize {
    crate::value::len16(s)
}

fn width(s: &str) -> usize {
    if s.is_ascii() {
        return s.len();
    }
    s.chars()
        .map(|c| {
            let cp = c as u32;
            if cp >= 0x1100
                && (cp <= 0x115f
                    || (0x2e80..=0xa4cf).contains(&cp)
                    || (0xac00..=0xd7a3).contains(&cp)
                    || (0xf900..=0xfaff).contains(&cp)
                    || (0xfe30..=0xfe4f).contains(&cp)
                    || (0xff00..=0xff60).contains(&cp)
                    || (0xffe0..=0xffe6).contains(&cp)
                    || (0x1f300..=0x1f64f).contains(&cp)
                    || (0x1f900..=0x1f9ff).contains(&cp)
                    || (0x20000..=0x3fffd).contains(&cp))
            {
                2
            } else {
                1
            }
        })
        .sum()
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
            c if crate::value::is_lone(c) => {
                out.push_str(&format!("\\u{:04x}", crate::value::lone_unit(c)));
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

fn format_string(ctx: &Ctx, s: &str) -> String {
    let n = len16(s);
    if n > 16 && n > ctx.break_length.saturating_sub(ctx.indent + 4) && s.contains('\n') {
        let mut pieces = Vec::new();
        let mut start = 0;
        for (i, c) in s.char_indices() {
            if c == '\n' {
                pieces.push(&s[start..i + 1]);
                start = i + 1;
            }
        }
        if start < s.len() {
            pieces.push(&s[start..]);
        }
        let joiner = format!(" +\n{}", " ".repeat(ctx.indent + 2));
        return pieces
            .iter()
            .map(|p| quote(p))
            .collect::<Vec<_>>()
            .join(&joiner);
    }
    quote(s)
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
    if crate::object::is_symbol_key(k) {
        format!("Symbol({})", crate::object::symbol_key_description(k))
    } else if is_identifier(k) {
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
            let kind = match &o.kind {
                ObjKind::Function(c) if c.proto.is_async && c.proto.is_generator => {
                    "AsyncGeneratorFunction"
                }
                ObjKind::Function(c) if c.proto.is_async => "AsyncFunction",
                ObjKind::Function(c) if c.proto.is_generator => "GeneratorFunction",
                _ => "Function",
            };
            if name.is_empty() {
                format!("[{kind} (anonymous)]")
            } else {
                format!("[{kind}: {name}]")
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

fn format_value(ctx: &mut Ctx, v: &Value, rt: usize, top: bool) -> String {
    match v {
        Value::Undefined | Value::Uninit => "undefined".to_string(),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => format_number(*n),
        Value::Str(s) => {
            if top {
                s.to_string()
            } else {
                format_string(ctx, s)
            }
        }
        Value::Object(o) => format_object(ctx, o, rt, top),
        Value::Symbol(sym) => crate::builtins::symbol_text(sym),
        Value::BigInt(b) => format!("{b}n"),
    }
}

fn child(ctx: &mut Ctx, v: &Value, rt: usize) -> String {
    ctx.indent += 2;
    let s = format_value(ctx, v, rt, false);
    ctx.indent -= 2;
    s
}

fn format_prop(ctx: &mut Ctx, k: &str, v: &Value, rt: usize) -> String {
    let key = format_key(k);
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
    format!("{key}: {}", child(ctx, v, rt))
}

fn own_props(o: &Obj, skip_function_keys: bool) -> Vec<(Rc<str>, Value)> {
    let mut props: Vec<(Rc<str>, Value)> = o
        .props
        .borrow()
        .enumerable()
        .filter(|(k, _)| !crate::object::is_private_key(k))
        .filter(|(k, _)| !skip_function_keys || !matches!(&***k, "prototype" | "name" | "length"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    props.sort_by_key(|(k, _)| crate::object::is_symbol_key(k));
    props
}

fn below_break_length(output: &[String], start: usize, base: &str, break_length: usize) -> bool {
    let mut total = output.len() + start;
    if total + output.len() > break_length {
        return false;
    }
    for item in output {
        total += len16(item);
        if total > break_length {
            return false;
        }
    }
    base.is_empty() || !base.contains('\n')
}

fn pad_start(s: &str, target: usize) -> String {
    let n = len16(s);
    if n >= target {
        s.to_string()
    } else {
        format!("{}{s}", " ".repeat(target - n))
    }
}

fn pad_end(s: &str, target: usize) -> String {
    let n = len16(s);
    if n >= target {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(target - n))
    }
}

fn group_array_elements(
    ctx: &Ctx,
    output: Vec<String>,
    has_more: bool,
    numeric: bool,
) -> Vec<String> {
    let output_len = if has_more {
        output.len() - 1
    } else {
        output.len()
    };
    let sep_space = 2;
    let mut total_length = 0usize;
    let mut max_length = 0usize;
    let mut data_len = Vec::with_capacity(output_len);
    for item in output.iter().take(output_len) {
        let len = width(item);
        data_len.push(len);
        total_length += len + sep_space;
        max_length = max_length.max(len);
    }
    let actual_max = max_length + sep_space;
    if actual_max * 3 + ctx.indent < ctx.break_length
        && (total_length as f64 / actual_max as f64 > 5.0 || max_length <= 6)
    {
        let approx_char_heights = 2.5;
        let average_bias = (actual_max as f64 - total_length as f64 / output.len() as f64).sqrt();
        let biased_max = (actual_max as f64 - 3.0 - average_bias).max(1.0);
        let columns = ((approx_char_heights * biased_max * output_len as f64).sqrt() / biased_max)
            .round()
            .min(((ctx.break_length - ctx.indent) / actual_max) as f64)
            .min((COMPACT * 4) as f64)
            .min(15.0) as usize;
        if columns <= 1 {
            return output;
        }
        let mut max_line_length = Vec::with_capacity(columns);
        for i in 0..columns {
            let mut line_length = 0;
            let mut j = i;
            while j < output_len {
                line_length = line_length.max(data_len[j]);
                j += columns;
            }
            max_line_length.push(line_length + sep_space);
        }
        let mut rows = Vec::new();
        let mut i = 0;
        while i < output_len {
            let max = (i + columns).min(output_len);
            let mut row = String::new();
            let mut j = i;
            while j < max - 1 {
                let padding = max_line_length[j - i] + len16(&output[j]) - data_len[j];
                let cell = format!("{}, ", output[j]);
                row.push_str(&if numeric {
                    pad_start(&cell, padding)
                } else {
                    pad_end(&cell, padding)
                });
                j += 1;
            }
            if numeric {
                let padding = max_line_length[j - i] + len16(&output[j]) - data_len[j] - sep_space;
                row.push_str(&pad_start(&output[j], padding));
            } else {
                row.push_str(&output[j]);
            }
            rows.push(row);
            i += columns;
        }
        if has_more {
            rows.push(output[output_len].clone());
        }
        return rows;
    }
    output
}

struct Shape<'a> {
    base: String,
    open: String,
    close: &'a str,
    array: Option<(bool, bool)>,
}

fn reduce(ctx: &Ctx, mut output: Vec<String>, shape: Shape<'_>, rt: usize) -> String {
    let entries = output.len();
    if let Some((has_more, numeric)) = shape.array {
        if entries > 6 {
            output = group_array_elements(ctx, output, has_more, numeric);
        }
    }
    let base_prefix = if shape.base.is_empty() {
        String::new()
    } else {
        format!("{} ", shape.base)
    };
    if ctx.compact > 0
        && ctx.current_depth.saturating_sub(rt) < ctx.compact
        && entries == output.len()
    {
        let start = output.len() + ctx.indent + len16(&shape.open) + len16(&shape.base) + 10;
        if below_break_length(&output, start, &shape.base, ctx.break_length) {
            let joined = output.join(", ");
            if !joined.contains('\n') {
                return format!("{base_prefix}{} {joined} {}", shape.open, shape.close);
            }
        }
    }
    let ind = format!("\n{}", " ".repeat(ctx.indent));
    format!(
        "{base_prefix}{}{ind}  {}{ind}{}",
        shape.open,
        output.join(&format!(",{ind}  ")),
        shape.close
    )
}

fn depth_name(o: &Obj) -> String {
    match &o.kind {
        ObjKind::Array(_) => "[Array]".to_string(),
        ObjKind::Map(_) => "[Map]".to_string(),
        ObjKind::Set(_) => "[Set]".to_string(),
        ObjKind::Promise(_) => "[Promise]".to_string(),
        _ => match class_name(o) {
            Some(n) => format!("[{n}]"),
            None => "[Object]".to_string(),
        },
    }
}

fn enter(ctx: &mut Ctx, ptr: *const Obj, rt: usize) -> usize {
    let rt = rt + 1;
    ctx.seen.push(ptr);
    ctx.current_depth = rt;
    rt
}

fn leave(ctx: &mut Ctx, ptr: *const Obj, base: &mut String) {
    ctx.seen.pop();
    if ctx.circular.contains(&ptr) {
        *base = if base.is_empty() {
            "<ref *1>".to_string()
        } else {
            format!("<ref *1> {base}")
        };
    }
}

fn format_object(ctx: &mut Ctx, o: &Rc<Obj>, rt: usize, top: bool) -> String {
    let ptr = Rc::as_ptr(o);
    if ctx.seen.contains(&ptr) {
        if !ctx.circular.contains(&ptr) {
            ctx.circular.push(ptr);
        }
        return "[Circular *1]".to_string();
    }
    if let Some(custom) = custom_inspect(ctx, o, rt) {
        return custom;
    }
    match &o.kind {
        ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_) => {
            let desc = function_desc(o);
            let props = own_props(o, true);
            if props.is_empty() {
                return desc;
            }
            if rt > ctx.max_depth {
                return desc;
            }
            let inner = enter(ctx, ptr, rt);
            let output: Vec<String> = props
                .iter()
                .map(|(k, v)| format_prop(ctx, k, v, inner))
                .collect();
            let mut base = desc;
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: "{".to_string(),
                    close: "}",
                    array: None,
                },
                inner,
            )
        }
        ObjKind::Error => {
            let _ = top;
            let mut base = match o.get_own("stack") {
                Some(Value::Str(st)) if st.starts_with(&*error_header(o)) => st.to_string(),
                _ => error_full(o),
            };
            if !base.contains("\n    at ") {
                base = format!("[{base}]");
            }
            if ctx.indent > 0 {
                let pad = " ".repeat(ctx.indent);
                base = base.replace('\n', &format!("\n{pad}"));
            }
            let props = own_props(o, false);
            let cause = o
                .get_own("cause")
                .filter(|_| !o.props.borrow().is_enumerable("cause"));
            let errors = o
                .get_own("errors")
                .filter(|_| !o.props.borrow().is_enumerable("errors"));
            if props.is_empty() && cause.is_none() && errors.is_none() {
                return base;
            }
            if rt > ctx.max_depth {
                return format!("[{}]", error_header(o));
            }
            let inner = enter(ctx, ptr, rt);
            let mut output: Vec<String> = props
                .iter()
                .map(|(k, v)| format_prop(ctx, k, v, inner))
                .collect();
            if let Some(c) = cause {
                output.push(format!("[cause]: {}", child(ctx, &c, inner)));
            }
            if let Some(e) = errors {
                output.push(format!("[errors]: {}", child(ctx, &e, inner)));
            }
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: "{".to_string(),
                    close: "}",
                    array: None,
                },
                inner,
            )
        }
        ObjKind::Array(a) => {
            let items: Vec<Value> = a.borrow().iter().take(MAX_ITEMS).cloned().collect();
            let total = a.borrow().len();
            let props = own_props(o, false);
            if total == 0 && props.is_empty() {
                return "[]".to_string();
            }
            if rt > ctx.max_depth {
                return "[Array]".to_string();
            }
            let inner = enter(ctx, ptr, rt);
            let holey = o.has_holes();
            let mut output: Vec<String> = if holey {
                let all = a.borrow().clone();
                let mut out = Vec::new();
                let mut i = 0;
                while i < all.len() && out.len() < MAX_ITEMS {
                    if o.is_hole(i) {
                        let start = i;
                        while i < all.len() && o.is_hole(i) {
                            i += 1;
                        }
                        let n = i - start;
                        out.push(format!("<{n} empty item{}>", if n == 1 { "" } else { "s" }));
                    } else {
                        out.push(child(ctx, &all[i], inner));
                        i += 1;
                    }
                }
                out
            } else {
                items.iter().map(|v| child(ctx, v, inner)).collect()
            };
            let has_more = total > MAX_ITEMS;
            if has_more {
                let more = total - MAX_ITEMS;
                output.push(format!(
                    "... {more} more item{}",
                    if more == 1 { "" } else { "s" }
                ));
            }
            for (k, v) in &props {
                output.push(format_prop(ctx, k, v, inner));
            }
            let all = a.borrow();
            let numeric =
                !holey && (0..output.len()).all(|i| matches!(all.get(i), Some(Value::Number(_))));
            drop(all);
            let mut base = String::new();
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: "[".to_string(),
                    close: "]",
                    array: Some((has_more, numeric)),
                },
                inner,
            )
        }
        ObjKind::Map(m) => {
            let entries: Vec<(Value, Value)> = m
                .borrow()
                .iter()
                .map(|(k, v)| (k.to_value(), v.clone()))
                .collect();
            let prefix = format!("Map({})", entries.len());
            if entries.is_empty() {
                return format!("{prefix} {{}}");
            }
            if rt > ctx.max_depth {
                return "[Map]".to_string();
            }
            let inner = enter(ctx, ptr, rt);
            ctx.indent += 2;
            let output: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    let ks = format_value(ctx, k, inner, false);
                    let vs = format_value(ctx, v, inner, false);
                    format!("{ks} => {vs}")
                })
                .collect();
            ctx.indent -= 2;
            let mut base = String::new();
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: format!("{prefix} {{"),
                    close: "}",
                    array: None,
                },
                inner,
            )
        }
        ObjKind::Set(s) => {
            let vals: Vec<Value> = s.borrow().iter().map(|k| k.to_value()).collect();
            let prefix = format!("Set({})", vals.len());
            if vals.is_empty() {
                return format!("{prefix} {{}}");
            }
            if rt > ctx.max_depth {
                return "[Set]".to_string();
            }
            let inner = enter(ctx, ptr, rt);
            ctx.indent += 2;
            let output: Vec<String> = vals
                .iter()
                .map(|v| format_value(ctx, v, inner, false))
                .collect();
            ctx.indent -= 2;
            let mut base = String::new();
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: format!("{prefix} {{"),
                    close: "}",
                    array: None,
                },
                inner,
            )
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
        ObjKind::Accessor(acc) => {
            let get = !acc.get.borrow().is_undefined();
            let set = !acc.set.borrow().is_undefined();
            match (get, set) {
                (true, false) => "[Getter]".to_string(),
                (false, true) => "[Setter]".to_string(),
                _ => "[Getter/Setter]".to_string(),
            }
        }
        ObjKind::Primitive(p) => {
            let kind = match p {
                Value::Str(_) => "String",
                Value::Number(_) => "Number",
                Value::Bool(_) => "Boolean",
                Value::Symbol(_) => "Symbol",
                Value::BigInt(_) => "BigInt",
                _ => "Object",
            };
            let inner = format_value(ctx, p, rt, false);
            format!("[{kind}: {inner}]")
        }
        ObjKind::RegExp(r) => {
            let d = r.data.borrow();
            format!("/{}/{}", d.source, d.flags)
        }
        ObjKind::Date(t) => {
            crate::builtins::iso_string(t.get()).unwrap_or_else(|| "Invalid Date".to_string())
        }
        ObjKind::WeakMap(_) => "WeakMap { <items unknown> }".to_string(),
        ObjKind::WeakSet(_) => "WeakSet { <items unknown> }".to_string(),
        ObjKind::WeakRef(_) => "WeakRef {}".to_string(),
        ObjKind::Promise(p) => {
            if rt > ctx.max_depth {
                return "[Promise]".to_string();
            }
            let inner = enter(ctx, ptr, rt);
            let state = match &p.borrow().state {
                crate::object::PromiseState::Pending => None,
                crate::object::PromiseState::Fulfilled(v) => Some((v.clone(), false)),
                crate::object::PromiseState::Rejected(e) => Some((e.clone(), true)),
            };
            let item = match state {
                None => "<pending>".to_string(),
                Some((v, false)) => child(ctx, &v, inner),
                Some((e, true)) => format!("<rejected> {}", child(ctx, &e, inner)),
            };
            let mut base = String::new();
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                vec![item],
                Shape {
                    base,
                    open: "Promise {".to_string(),
                    close: "}",
                    array: None,
                },
                inner,
            )
        }
        ObjKind::Proxy(p) => {
            let t = p.target.borrow().clone();
            match t {
                Some(t) => format_object(ctx, &t, rt, top),
                None => "<Revoked Proxy>".to_string(),
            }
        }
        ObjKind::Global => "Object [global] {}".to_string(),
        ObjKind::Typed(t) if class_name(o).as_deref() == Some("Buffer") => {
            let bytes = t.bytes();
            let mut s = String::from("<Buffer");
            if bytes.is_empty() {
                s.push(' ');
            }
            for b in bytes.iter().take(50) {
                s.push_str(&format!(" {b:02x}"));
            }
            if bytes.len() > 50 {
                let more = bytes.len() - 50;
                s.push_str(&format!(
                    " ... {more} more byte{}",
                    if more == 1 { "" } else { "s" }
                ));
            }
            s.push('>');
            s
        }
        ObjKind::Typed(t) => {
            let values = t.values();
            let total = values.len();
            let name = t.kind.name();
            let big = t.kind.is_big();
            let props = own_props(o, false);
            if total == 0 && props.is_empty() {
                return format!("{name}(0) []");
            }
            if rt > ctx.max_depth {
                return format!("[{name}]");
            }
            let inner = enter(ctx, ptr, rt);
            let mut output: Vec<String> = values
                .iter()
                .take(MAX_ITEMS)
                .map(|v| match v {
                    Value::Number(n) if !big => format_number(*n),
                    Value::BigInt(b) => format!("{b}n"),
                    _ => "undefined".to_string(),
                })
                .collect();
            let has_more = total > MAX_ITEMS;
            if has_more {
                let more = total - MAX_ITEMS;
                output.push(format!(
                    "... {more} more item{}",
                    if more == 1 { "" } else { "s" }
                ));
            }
            let numeric_len = output.len();
            for (k, v) in &props {
                output.push(format_prop(ctx, k, v, inner));
            }
            let numeric = numeric_len == output.len();
            let mut base = format!("{name}({total})");
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: "[".to_string(),
                    close: "]",
                    array: Some((has_more, numeric)),
                },
                inner,
            )
        }
        ObjKind::ArrayBuffer(b) => {
            if rt > ctx.max_depth {
                return "[ArrayBuffer]".to_string();
            }
            let bytes = b.borrow();
            let shown: Vec<String> = bytes
                .iter()
                .take(MAX_ITEMS)
                .map(|x| format!("{x:02x}"))
                .collect();
            let mut contents = shown.join(" ");
            if bytes.len() > MAX_ITEMS {
                let more = bytes.len() - MAX_ITEMS;
                contents.push_str(&format!(
                    " ... {more} more byte{}",
                    if more == 1 { "" } else { "s" }
                ));
            }
            let len = bytes.len();
            drop(bytes);
            let inner = enter(ctx, ptr, rt);
            let mut output = vec![
                format!("[Uint8Contents]: <{contents}>"),
                format!("[byteLength]: {len}"),
            ];
            for (k, v) in own_props(o, false) {
                output.push(format_prop(ctx, &k, &v, inner));
            }
            let mut base = String::new();
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: if b.shared {
                        "SharedArrayBuffer {".to_string()
                    } else {
                        "ArrayBuffer {".to_string()
                    },
                    close: "}",
                    array: None,
                },
                inner,
            )
        }
        ObjKind::DataView(t) => {
            if rt > ctx.max_depth {
                return "[DataView]".to_string();
            }
            let inner = enter(ctx, ptr, rt);
            let buffer = child(ctx, &Value::Object(t.buffer.clone()), inner);
            let output = vec![
                format!("[byteLength]: {}", t.length),
                format!("[byteOffset]: {}", t.offset),
                format!("[buffer]: {buffer}"),
            ];
            let mut base = String::new();
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: "DataView {".to_string(),
                    close: "}",
                    array: None,
                },
                inner,
            )
        }
        ObjKind::Ordinary | ObjKind::Namespace | ObjKind::Arguments(_) => {
            let arguments = o.flags.get() & crate::object::ARGUMENTS != 0;
            let cname = if arguments { None } else { class_name(o) };
            let tag = match o.get_chain(crate::object::TO_STRING_TAG_KEY) {
                Some(Value::Str(t))
                    if !t.is_empty()
                        && cname.as_deref() != Some(&*t)
                        && !own_props(o, false)
                            .iter()
                            .any(|(k, _)| &**k == crate::object::TO_STRING_TAG_KEY) =>
                {
                    Some(t)
                }
                _ => None,
            };
            let prefix = match (&cname, o.proto.borrow().is_none(), &tag) {
                _ if arguments => "[Arguments] ".to_string(),
                (Some(n), _, Some(t)) => format!("{n} [{t}] "),
                (Some(n), _, None) => format!("{n} "),
                (None, true, Some(t)) if &**t == "Module" => {
                    "[Module: null prototype] ".to_string()
                }
                (None, true, Some(t)) => format!("[Object: null prototype] [{t}] "),
                (None, true, None) => "[Object: null prototype] ".to_string(),
                (None, false, Some(t)) => format!("Object [{t}] "),
                _ => String::new(),
            };
            let props = own_props(o, false);
            if props.is_empty() {
                return format!("{prefix}{{}}");
            }
            if rt > ctx.max_depth {
                return depth_name(o);
            }
            let inner = enter(ctx, ptr, rt);
            let output: Vec<String> = props
                .iter()
                .map(|(k, v)| format_prop(ctx, k, v, inner))
                .collect();
            let mut base = String::new();
            leave(ctx, ptr, &mut base);
            reduce(
                ctx,
                output,
                Shape {
                    base,
                    open: format!("{prefix}{{"),
                    close: "}",
                    array: None,
                },
                inner,
            )
        }
    }
}
