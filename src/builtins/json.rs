use super::{arg, method, to_integer};
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
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Ser<'a> {
    indent: &'a str,
    stack: Vec<*const Obj>,
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

    fn write(
        &mut self,
        vm: &mut Vm,
        v: &Value,
        depth: usize,
        out: &mut String,
    ) -> Result<bool, Value> {
        let mut v = v.clone();
        if let Value::Object(o) = &v {
            if let Some(f) = o.get_chain("toJSON") {
                if f.is_callable() {
                    v = vm.call_value(&f, v.clone(), &[])?;
                }
            }
        }
        match &v {
            Value::Undefined | Value::Symbol(_) => Ok(false),
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
                    out.push_str(&number_to_string(*n));
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

    fn write_object(
        &mut self,
        vm: &mut Vm,
        o: &Rc<Obj>,
        depth: usize,
        out: &mut String,
    ) -> Result<(), Value> {
        if let ObjKind::Array(a) = &o.kind {
            let items = a.borrow().clone();
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                self.newline(out, depth + 1);
                if !self.write(vm, item, depth + 1, out)? {
                    out.push_str("null");
                }
            }
            self.newline(out, depth);
            out.push(']');
            return Ok(());
        }
        let entries: Vec<(Rc<str>, Value)> = match &o.kind {
            ObjKind::Map(_) | ObjKind::Set(_) | ObjKind::Iter(_) => Vec::new(),
            _ => o
                .props
                .borrow()
                .enumerable()
                .filter(|(k, _)| !crate::object::is_symbol_key(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        out.push('{');
        let mut first = true;
        let receiver = Value::Object(o.clone());
        for (k, v) in entries {
            let v = vm.resolve_accessor(v, &receiver)?;
            let mut piece = String::new();
            if !self.write(vm, &v, depth + 1, &mut piece)? {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            self.newline(out, depth + 1);
            quote_json(&k, out);
            out.push(':');
            if !self.indent.is_empty() {
                out.push(' ');
            }
            out.push_str(&piece);
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
    };
    let mut out = String::new();
    if ser.write(vm, v, 0, &mut out)? {
        Ok(Some(out))
    } else {
        Ok(None)
    }
}

fn stringify(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let indent = match arg(args, 2) {
        Value::Number(_) => {
            let n = to_integer(vm, &arg(args, 2))?.clamp(0.0, 10.0) as usize;
            " ".repeat(n)
        }
        Value::Str(s) => s.chars().take(10).collect(),
        _ => String::new(),
    };
    let mut v = arg(args, 0);
    let replacer = arg(args, 1);
    if replacer.is_callable() {
        let holder = vm.new_object();
        holder.set_own("", v.clone());
        v = apply_replacer(vm, &replacer, &Value::Object(holder), "", v)?;
    }
    match json_stringify(vm, &v, &indent)? {
        Some(s) => Ok(s.into()),
        None => Ok(Value::Undefined),
    }
}

fn apply_replacer(vm: &mut Vm, f: &Value, holder: &Value, key: &str, v: Value) -> R {
    let v = vm.call_value(f, holder.clone(), &[key.into(), v])?;
    if let Value::Object(o) = &v {
        if !o.is_callable() {
            for (k, child) in vm.own_entries(&v) {
                let nv = apply_replacer(vm, f, &v, &k, child)?;
                vm.set_prop(&v, k, nv)?;
            }
        }
    }
    Ok(v)
}

pub fn json_parse(vm: &mut Vm, text: &str) -> R {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(j) => Ok(from_json(vm, &j)),
        Err(e) => Err(vm.make_error(
            ErrorKind::SyntaxError,
            format!(
                "Unexpected token in JSON at line {} column {}",
                e.line(),
                e.column()
            ),
        )),
    }
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

fn revive(vm: &mut Vm, f: &Value, holder: &Value, key: Rc<str>) -> R {
    let v = vm.get_prop(holder, &key)?;
    if v.is_object() {
        for (k, _) in vm.own_entries(&v) {
            let nv = revive(vm, f, &v, k.clone())?;
            if nv.is_undefined() {
                vm.delete_prop(&v, &k)?;
            } else {
                vm.set_prop(&v, k, nv)?;
            }
        }
    }
    vm.call_value(f, holder.clone(), &[Value::Str(key), v])
}
