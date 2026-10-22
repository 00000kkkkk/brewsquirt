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
            c if crate::value::is_lone(c) => {
                out.push_str(&format!("\\u{:04x}", crate::value::lone_unit(c)))
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Ser<'a> {
    indent: &'a str,
    stack: Vec<*const Obj>,
    replacer: Option<Value>,
    allow: Option<Vec<Rc<str>>>,
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
        key: &str,
        value: Value,
        depth: usize,
        out: &mut String,
    ) -> Result<bool, Value> {
        let mut v = value;
        if matches!(v, Value::Object(_) | Value::BigInt(_)) {
            let f = vm.get_prop(&v, "toJSON")?;
            if f.is_callable() {
                v = vm.call_value(&f, v.clone(), &[key.into()])?;
            }
        }
        if let Some(r) = &self.replacer {
            let r = r.clone();
            v = vm.call_value(&r, holder.clone(), &[key.into(), v])?;
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
                let key = i.to_string();
                let item = match &o.kind {
                    ObjKind::Array(a) => a.borrow().get(i).cloned().unwrap_or_default(),
                    _ => vm.get_prop(&holder, &key)?,
                };
                if !self.property(vm, &holder, &key, item, depth + 1, out)? {
                    out.push_str("null");
                }
            }
            self.newline(out, depth);
            out.push(']');
            return Ok(());
        }
        let keys: Vec<Rc<str>> = match &self.allow {
            Some(list) => list.clone(),
            None => match &o.kind {
                ObjKind::Map(_) | ObjKind::Set(_) | ObjKind::Iter(_) => Vec::new(),
                _ => vm.op_own_enumerable_keys(o)?,
            },
        };
        out.push('{');
        let mut first = true;
        for k in keys {
            let v = vm.get_prop(&holder, &k)?;
            let mut piece = String::new();
            if !self.property(vm, &holder, &k, v, depth + 1, &mut piece)? {
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
        replacer: None,
        allow: None,
    };
    let holder = Value::Object(vm.new_object());
    let mut out = String::new();
    if ser.property(vm, &holder, "", v.clone(), 0, &mut out)? {
        Ok(Some(out))
    } else {
        Ok(None)
    }
}

fn stringify(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let space = arg(args, 2);
    let space = crate::object::unwrap_primitive(&space).unwrap_or(space);
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
            let mut list: Vec<Rc<str>> = Vec::new();
            for item in vm.iterate(&replacer)? {
                let item = crate::object::unwrap_primitive(&item).unwrap_or(item);
                let key = match &item {
                    Value::Str(s) => Some(s.clone()),
                    Value::Number(n) => Some(number_to_string(*n).into()),
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
    if ser.property(vm, &Value::Object(holder), "", v, 0, &mut out)? {
        Ok(out.into())
    } else {
        Ok(Value::Undefined)
    }
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
