use crate::inspect::inspect_plain;
use crate::object::{
    Accessor, ELEMENTS_FROZEN, ELEMENTS_SEALED, LENGTH_DELETED, NAME_DELETED, NON_CONFIG, NON_ENUM,
    NOT_EXTENSIBLE, Obj, ObjKind, READ_ONLY, array_index,
};
use crate::value::Value;
use crate::vm::Vm;
use std::rc::Rc;

#[derive(Default, Clone)]
pub struct PropDesc {
    pub value: Option<Value>,
    pub get: Option<Value>,
    pub set: Option<Value>,
    pub writable: Option<bool>,
    pub enumerable: Option<bool>,
    pub configurable: Option<bool>,
}

impl PropDesc {
    pub fn is_accessor(&self) -> bool {
        self.get.is_some() || self.set.is_some()
    }

    pub fn is_data(&self) -> bool {
        self.value.is_some() || self.writable.is_some()
    }

    pub fn data(value: Value, attrs: u8) -> PropDesc {
        PropDesc {
            value: Some(value),
            writable: Some(attrs & READ_ONLY == 0),
            enumerable: Some(attrs & NON_ENUM == 0),
            configurable: Some(attrs & NON_CONFIG == 0),
            ..PropDesc::default()
        }
    }
}

pub fn key_text(k: &str) -> String {
    if crate::object::is_symbol_key(k) {
        if let Some(name) = k.strip_prefix("\u{0}#") {
            return format!("#{}", name.split('@').next().unwrap_or(name));
        }
        format!("Symbol({})", crate::object::symbol_key_description(k))
    } else {
        k.to_string()
    }
}

pub fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.is_nan() && y.is_nan()) || (x == y && x.is_sign_negative() == y.is_sign_negative())
        }
        _ => a.strict_eq(b),
    }
}

fn element_attrs(o: &Obj, key: &str) -> u8 {
    let f = o.flags.get();
    let mut a = 0;
    if f & crate::object::SLOW_ELEMENTS != 0 {
        a |= o.props.borrow().attrs(key);
    }
    if f & ELEMENTS_FROZEN != 0 {
        a |= READ_ONLY | NON_CONFIG;
    }
    if f & ELEMENTS_SEALED != 0 {
        a |= NON_CONFIG;
    }
    a
}

pub fn accessor_parts(v: &Value) -> Option<(Value, Value)> {
    match v {
        Value::Object(a) => match &a.kind {
            ObjKind::Accessor(acc) => Some((acc.get.borrow().clone(), acc.set.borrow().clone())),
            _ => None,
        },
        _ => None,
    }
}

impl Vm {
    pub fn describe_target(&self, o: &Obj) -> String {
        match &o.kind {
            ObjKind::Array(_) => "object '[object Array]'".to_string(),
            ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_) => {
                format!("function '{}'", crate::inspect::function_desc(o))
            }
            _ => format!(
                "object '#<{}>'",
                crate::inspect::class_name(o)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "Object".to_string())
            ),
        }
    }

    pub fn read_only_error(&mut self, o: &Obj, key: &str) -> Value {
        let target = self.describe_target(o);
        self.type_error(format!(
            "Cannot assign to read only property '{}' of {target}",
            key_text(key)
        ))
    }

    pub fn not_extensible_error(&mut self, key: &str) -> Value {
        self.type_error(format!(
            "Cannot add property {}, object is not extensible",
            key_text(key)
        ))
    }

    pub fn own_property(&self, o: &Rc<Obj>, key: &str) -> Option<(Value, u8)> {
        match &o.kind {
            ObjKind::Array(a) => {
                if let Some(i) = array_index(key) {
                    if o.is_hole(i) {
                        return None;
                    }
                    return a
                        .borrow()
                        .get(i)
                        .cloned()
                        .map(|v| (v, element_attrs(o, key)));
                }
                if key == "length" {
                    let mut attrs = NON_ENUM | NON_CONFIG;
                    if o.flags.get() & ELEMENTS_FROZEN != 0 {
                        attrs |= READ_ONLY;
                    }
                    return Some((Value::Number(a.borrow().len() as f64), attrs));
                }
            }
            ObjKind::RegExp(r) if key == "lastIndex" => {
                return Some((r.last_index.borrow().clone(), NON_ENUM | NON_CONFIG));
            }
            ObjKind::Primitive(Value::Str(s)) => {
                if key == "length" {
                    return Some((
                        Value::Number(crate::value::len16(s) as f64),
                        NON_ENUM | READ_ONLY | NON_CONFIG,
                    ));
                }
                if let Some(i) = array_index(key) {
                    if let Some(c) = crate::value::char_at16(s, i) {
                        return Some((Value::from(c), READ_ONLY | NON_CONFIG));
                    }
                }
            }
            ObjKind::Typed(t) => {
                if let Some(n) = crate::builtins::typed::canonical_numeric(key) {
                    let i = crate::builtins::typed::valid_index(t, n)?;
                    return t.get(i).map(|v| (v, 0));
                }
            }
            ObjKind::Global => {
                let raw = self.globals.get(key)?.clone();
                return Some((raw, self.globals.attrs_of(key)));
            }
            ObjKind::Function(c) => {
                let props = o.props.borrow();
                if let Some(v) = props.get(key) {
                    return Some((v.clone(), props.attrs(key)));
                }
                drop(props);
                let gone = o.flags.get();
                return match key {
                    "name" if gone & NAME_DELETED == 0 => {
                        Some((Value::Str(c.proto.name.clone()), NON_ENUM | READ_ONLY))
                    }
                    "length" if gone & LENGTH_DELETED == 0 => {
                        Some((Value::Number(c.proto.arity as f64), NON_ENUM | READ_ONLY))
                    }
                    "prototype" if c.proto.is_constructor() || c.proto.is_generator => {
                        let v = self.get_obj(o, "prototype");
                        Some((v, NON_ENUM | NON_CONFIG))
                    }
                    _ => None,
                };
            }
            ObjKind::Native(n) => {
                let props = o.props.borrow();
                if let Some(v) = props.get(key) {
                    return Some((v.clone(), props.attrs(key)));
                }
                let gone = o.flags.get();
                return match key {
                    "name" if gone & NAME_DELETED == 0 => {
                        Some((Value::Str(n.name.clone()), NON_ENUM | READ_ONLY))
                    }
                    "length" if gone & LENGTH_DELETED == 0 => {
                        Some((Value::Number(n.arity as f64), NON_ENUM | READ_ONLY))
                    }
                    _ => None,
                };
            }
            ObjKind::Bound(_) => {
                let props = o.props.borrow();
                if let Some(v) = props.get(key) {
                    return Some((v.clone(), props.attrs(key)));
                }
                drop(props);
                return match key {
                    "name" => Some((self.get_obj(o, "name"), NON_ENUM | READ_ONLY)),
                    "length" => Some((self.get_obj(o, "length"), NON_ENUM | READ_ONLY)),
                    _ => None,
                };
            }
            _ => {}
        }
        let props = o.props.borrow();
        props.get(key).map(|v| (v.clone(), props.attrs(key)))
    }

    pub fn to_prop_desc(&mut self, v: &Value) -> Result<PropDesc, Value> {
        if !v.is_object() {
            return Err(self.type_error(format!(
                "Property description must be an object: {}",
                inspect_plain(v)
            )));
        }
        let mut d = PropDesc::default();
        let has = |vm: &mut Vm, k: &str| vm.has_property(v, &Value::Str(k.into()));
        if has(self, "enumerable")? {
            d.enumerable = Some(self.get_prop(v, "enumerable")?.truthy());
        }
        if has(self, "configurable")? {
            d.configurable = Some(self.get_prop(v, "configurable")?.truthy());
        }
        if has(self, "value")? {
            d.value = Some(self.get_prop(v, "value")?);
        }
        if has(self, "writable")? {
            d.writable = Some(self.get_prop(v, "writable")?.truthy());
        }
        if has(self, "get")? {
            let g = self.get_prop(v, "get")?;
            if !g.is_undefined() && !g.is_callable() {
                return Err(
                    self.type_error(format!("Getter must be a function: {}", inspect_plain(&g)))
                );
            }
            d.get = Some(g);
        }
        if has(self, "set")? {
            let s = self.get_prop(v, "set")?;
            if !s.is_undefined() && !s.is_callable() {
                return Err(
                    self.type_error(format!("Setter must be a function: {}", inspect_plain(&s)))
                );
            }
            d.set = Some(s);
        }
        if d.is_accessor() && d.is_data() {
            return Err(self.type_error(format!(
                "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute, {}",
                inspect_plain(v)
            )));
        }
        Ok(d)
    }

    pub fn from_prop_desc(&mut self, d: &PropDesc) -> Value {
        let o = self.new_object();
        if let Some(v) = &d.value {
            o.set_own("value", v.clone());
        }
        if let Some(w) = d.writable {
            o.set_own("writable", Value::Bool(w));
        }
        if let Some(g) = &d.get {
            o.set_own("get", g.clone());
        }
        if let Some(s) = &d.set {
            o.set_own("set", s.clone());
        }
        if let Some(e) = d.enumerable {
            o.set_own("enumerable", Value::Bool(e));
        }
        if let Some(c) = d.configurable {
            o.set_own("configurable", Value::Bool(c));
        }
        Value::Object(o)
    }

    pub fn own_descriptor(&self, o: &Rc<Obj>, key: &str) -> Option<PropDesc> {
        let (raw, attrs) = self.own_property(o, key)?;
        Some(match accessor_parts(&raw) {
            Some((g, s)) => PropDesc {
                get: Some(g),
                set: Some(s),
                enumerable: Some(attrs & NON_ENUM == 0),
                configurable: Some(attrs & NON_CONFIG == 0),
                ..PropDesc::default()
            },
            None => PropDesc::data(raw, attrs),
        })
    }

    fn store_raw(
        &mut self,
        o: &Rc<Obj>,
        key: &Rc<str>,
        raw: Value,
        attrs: u8,
    ) -> Result<(), Value> {
        match &o.kind {
            ObjKind::Array(a) => {
                if let Some(i) = array_index(key) {
                    let mut items = a.borrow_mut();
                    let old = items.len();
                    if i >= items.len() {
                        if i - items.len() > 50_000_000 {
                            drop(items);
                            return Err(self.range_error("Invalid array length"));
                        }
                        items.resize(i + 1, Value::Undefined);
                    }
                    let slow = attrs != 0 || accessor_parts(&raw).is_some();
                    items[i] = raw;
                    drop(items);
                    o.mark_holes(old, i);
                    o.fill_hole(i);
                    if slow {
                        o.props.borrow_mut().set_index_attrs(key, attrs);
                        o.flags.set(o.flags.get() | crate::object::SLOW_ELEMENTS);
                        if accessor_parts(
                            &o.array().map(|a| a.borrow()[i].clone()).unwrap_or_default(),
                        )
                        .is_some()
                        {
                            self.accessors_used = true;
                        }
                    } else if o.flags.get() & crate::object::SLOW_ELEMENTS != 0 {
                        o.props.borrow_mut().set_index_attrs(key, 0);
                    }
                    return Ok(());
                }
                if &**key == "length" {
                    let n = self.to_number(&raw)?;
                    if n < 0.0 || n.fract() != 0.0 || n > 4294967295.0 {
                        return Err(self.range_error("Invalid array length"));
                    }
                    let old = a.borrow().len();
                    a.borrow_mut().resize(n as usize, Value::Undefined);
                    if (n as usize) < old {
                        o.truncate_holes(n as usize);
                    } else {
                        o.mark_holes(old, n as usize);
                    }
                    if attrs & READ_ONLY != 0 {
                        o.flags.set(o.flags.get() | ELEMENTS_FROZEN);
                    }
                    return Ok(());
                }
            }
            ObjKind::RegExp(r) if &**key == "lastIndex" => {
                *r.last_index.borrow_mut() = raw;
                return Ok(());
            }
            ObjKind::Primitive(Value::Str(s)) => {
                if &**key == "length"
                    || array_index(key).is_some_and(|i| i < crate::value::len16(s))
                {
                    return Ok(());
                }
            }
            ObjKind::Typed(t) => {
                if let Some(i) = array_index(key) {
                    let x = crate::builtins::typed::coerce(self, t.kind, &raw)?;
                    t.set(i, &x);
                    return Ok(());
                }
            }
            _ => {}
        }
        if attrs & READ_ONLY != 0 {
            self.readonly_used = true;
        }
        if accessor_parts(&raw).is_some() {
            self.accessors_used = true;
        }
        let mut props = o.props.borrow_mut();
        props.insert(key.clone(), raw);
        props.set_attrs(key, attrs);
        Ok(())
    }

    fn make_accessor(&mut self, get: Value, set: Value) -> Value {
        self.accessors_used = true;
        let a = Obj::new(
            ObjKind::Accessor(Box::new(Accessor {
                get: std::cell::RefCell::new(get),
                set: std::cell::RefCell::new(set),
            })),
            None,
        );
        Value::Object(a)
    }

    pub fn define_own(
        &mut self,
        o: &Rc<Obj>,
        key: Rc<str>,
        d: PropDesc,
        throw: bool,
    ) -> Result<bool, Value> {
        self.note_proto_index(o, &key);
        if let ObjKind::Typed(t) = &o.kind {
            if let Some(n) = crate::builtins::typed::canonical_numeric(&key) {
                let ok = crate::builtins::typed::valid_index(t, n).is_some()
                    && d.configurable != Some(false)
                    && d.enumerable != Some(false)
                    && !d.is_accessor()
                    && d.writable != Some(false);
                if !ok {
                    if throw {
                        return Err(self
                            .type_error(format!("Cannot redefine property: {}", key_text(&key))));
                    }
                    return Ok(false);
                }
                if let Some(v) = &d.value {
                    let x = crate::builtins::typed::coerce(self, t.kind, v)?;
                    if let Some(i) = crate::builtins::typed::valid_index(t, n) {
                        t.set(i, &x);
                    }
                }
                return Ok(true);
            }
        }
        let current = self.own_property(o, &key);
        let Some((raw, attrs)) = current else {
            if !o.is_extensible() {
                if throw {
                    return Err(self.type_error(format!(
                        "Cannot define property {}, object is not extensible",
                        key_text(&key)
                    )));
                }
                return Ok(false);
            }
            let mut bits = 0;
            if !d.enumerable.unwrap_or(false) {
                bits |= NON_ENUM;
            }
            if !d.configurable.unwrap_or(false) {
                bits |= NON_CONFIG;
            }
            let raw = if d.is_accessor() {
                let g = d.get.clone().unwrap_or_default();
                let s = d.set.clone().unwrap_or_default();
                self.make_accessor(g, s)
            } else {
                if !d.writable.unwrap_or(false) {
                    bits |= READ_ONLY;
                }
                d.value.clone().unwrap_or_default()
            };
            self.store_raw(o, &key, raw, bits)?;
            return Ok(true);
        };
        let current_acc = accessor_parts(&raw);
        let fail = |vm: &mut Vm| -> Result<bool, Value> {
            if throw {
                Err(vm.type_error(format!("Cannot redefine property: {}", key_text(&key))))
            } else {
                Ok(false)
            }
        };
        if attrs & NON_CONFIG != 0 {
            if d.configurable == Some(true) {
                return fail(self);
            }
            if let Some(e) = d.enumerable {
                if e == (attrs & NON_ENUM != 0) {
                    return fail(self);
                }
            }
            if (current_acc.is_some() && d.is_data()) || (current_acc.is_none() && d.is_accessor())
            {
                return fail(self);
            }
            match &current_acc {
                None => {
                    if attrs & READ_ONLY != 0 {
                        if d.writable == Some(true) {
                            return fail(self);
                        }
                        if let Some(v) = &d.value {
                            if !same_value(v, &raw) {
                                return fail(self);
                            }
                        }
                    }
                }
                Some((g, s)) => {
                    if let Some(ng) = &d.get {
                        if !same_value(ng, g) {
                            return fail(self);
                        }
                    }
                    if let Some(ns) = &d.set {
                        if !same_value(ns, s) {
                            return fail(self);
                        }
                    }
                }
            }
        }
        let mut bits = attrs;
        if let Some(e) = d.enumerable {
            bits = if e { bits & !NON_ENUM } else { bits | NON_ENUM };
        }
        if let Some(c) = d.configurable {
            bits = if c {
                bits & !NON_CONFIG
            } else {
                bits | NON_CONFIG
            };
        }
        let new_raw = if d.is_accessor() {
            bits &= !READ_ONLY;
            let (g0, s0) = current_acc.unwrap_or_default();
            let g = d.get.clone().unwrap_or(g0);
            let s = d.set.clone().unwrap_or(s0);
            self.make_accessor(g, s)
        } else if d.is_data() || current_acc.is_none() {
            if current_acc.is_some() {
                bits |= READ_ONLY;
            }
            if let Some(w) = d.writable {
                bits = if w {
                    bits & !READ_ONLY
                } else {
                    bits | READ_ONLY
                };
            }
            match &d.value {
                Some(v) => v.clone(),
                None if current_acc.is_some() => Value::Undefined,
                None => raw,
            }
        } else {
            raw
        };
        if matches!(
            o.kind,
            ObjKind::Function(_) | ObjKind::Native(_) | ObjKind::Bound(_)
        ) && !o.has_own(&key)
            && matches!(&*key, "name" | "length")
        {
            let mut props = o.props.borrow_mut();
            props.insert(key.clone(), new_raw);
            props.set_attrs(&key, bits);
            return Ok(true);
        }
        self.store_raw(o, &key, new_raw, bits)?;
        Ok(true)
    }

    pub fn try_delete(&mut self, o: &Rc<Obj>, key: &str) -> bool {
        match &o.kind {
            ObjKind::Array(a) => {
                if let Some(i) = array_index(key) {
                    if o.flags.get() & ELEMENTS_SEALED != 0 && i < a.borrow().len() {
                        return false;
                    }
                    if o.flags.get() & crate::object::SLOW_ELEMENTS != 0 {
                        if o.props.borrow().attrs(key) & NON_CONFIG != 0 && !o.is_hole(i) {
                            return false;
                        }
                        let k: Rc<str> = key.into();
                        o.props.borrow_mut().set_index_attrs(&k, 0);
                    }
                    let len = a.borrow().len();
                    if let Some(slot) = a.borrow_mut().get_mut(i) {
                        *slot = Value::Undefined;
                    }
                    if i < len {
                        o.mark_holes(i, i + 1);
                    }
                    return true;
                }
                if key == "length" {
                    return false;
                }
            }
            ObjKind::Primitive(Value::Str(s)) => {
                if key == "length" || array_index(key).is_some_and(|i| i < crate::value::len16(s)) {
                    return false;
                }
            }
            ObjKind::Typed(t) => {
                if let Some(n) = crate::builtins::typed::canonical_numeric(key) {
                    return crate::builtins::typed::valid_index(t, n).is_none();
                }
            }
            _ => {}
        }
        let attrs = o.props.borrow().attrs(key);
        if attrs & NON_CONFIG != 0 && o.has_own(key) {
            return false;
        }
        if o.is_callable() {
            match key {
                "name" => o.flags.set(o.flags.get() | NAME_DELETED),
                "length" => o.flags.set(o.flags.get() | LENGTH_DELETED),
                _ => {}
            }
        }
        o.props.borrow_mut().shift_remove(key);
        true
    }

    pub fn prevent_extensions(&mut self, o: &Rc<Obj>) {
        o.flags.set(o.flags.get() | NOT_EXTENSIBLE);
    }

    pub fn seal(&mut self, o: &Rc<Obj>) {
        o.flags
            .set(o.flags.get() | NOT_EXTENSIBLE | ELEMENTS_SEALED);
        o.props.borrow_mut().add_all(NON_CONFIG, false);
    }

    pub fn freeze(&mut self, o: &Rc<Obj>) -> Result<(), Value> {
        if let ObjKind::Typed(t) = &o.kind {
            if !t.is_empty() {
                return Err(self.type_error("Cannot freeze array buffer views with elements"));
            }
        }
        o.flags
            .set(o.flags.get() | NOT_EXTENSIBLE | ELEMENTS_SEALED | ELEMENTS_FROZEN);
        o.props.borrow_mut().add_all(READ_ONLY | NON_CONFIG, true);
        self.readonly_used = true;
        Ok(())
    }

    pub fn test_integrity(&self, o: &Rc<Obj>, frozen: bool) -> bool {
        if o.is_extensible() {
            return false;
        }
        let need = if frozen {
            ELEMENTS_FROZEN | ELEMENTS_SEALED
        } else {
            ELEMENTS_SEALED
        };
        let has_elements = match &o.kind {
            ObjKind::Array(a) => !a.borrow().is_empty(),
            ObjKind::Typed(t) => !t.is_empty(),
            _ => false,
        };
        if has_elements && o.flags.get() & need != need {
            return false;
        }
        if matches!(o.kind, ObjKind::Typed(_)) && has_elements {
            return false;
        }
        let bits = if frozen {
            READ_ONLY | NON_CONFIG
        } else {
            NON_CONFIG
        };
        o.props.borrow().all_have(bits, true)
    }

    pub fn inherited_read_only(&self, o: &Rc<Obj>, key: &str) -> bool {
        let mut cur = o.proto.borrow().clone();
        while let Some(p) = cur {
            let props = p.props.borrow();
            if let Some(v) = props.get(key) {
                return props.attrs(key) & READ_ONLY != 0 && accessor_parts(v).is_none();
            }
            drop(props);
            cur = p.proto.borrow().clone();
        }
        false
    }
}
