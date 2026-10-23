use super::intrinsics::raw_accessor;
use super::{arg, constructing, make_ctor, method, native_fn, relative_index, to_integer};
use crate::inspect::inspect_plain;
use crate::jsstr::JsStr;
use crate::object::{
    BufferData, DETACHED, ITER_KEY, IterState, NON_CONFIG, NON_ENUM, NativeFn, Obj, ObjKind,
    READ_ONLY, SPECIES_KEY, TO_STRING_TAG_KEY, TypedKind, View,
};
use crate::value::Value;
use crate::vm::Vm;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

type R = Result<Value, Value>;

const MAX_INDEX: f64 = 9007199254740991.0;
const MAX_ALLOC: usize = 1 << 31;

pub fn new_buffer(vm: &Vm, bytes: Vec<u8>) -> Rc<Obj> {
    new_buffer_with_max(vm, bytes, None)
}

fn new_buffer_with_max(vm: &Vm, bytes: Vec<u8>, max: Option<usize>) -> Rc<Obj> {
    Obj::new(
        ObjKind::ArrayBuffer(Box::new(BufferData {
            bytes: RefCell::new(bytes),
            max,
            shared: false,
            external: false,
        })),
        Some(vm.realm.array_buffer_proto.clone()),
    )
}

pub(crate) unsafe fn new_external_buffer(vm: &Vm, ptr: *mut u8, len: usize) -> Rc<Obj> {
    let bytes = if len == 0 || ptr.is_null() {
        Vec::new()
    } else {
        unsafe { Vec::from_raw_parts(ptr, len, len) }
    };
    Obj::new(
        ObjKind::ArrayBuffer(Box::new(BufferData {
            bytes: RefCell::new(bytes),
            max: None,
            shared: false,
            external: true,
        })),
        Some(vm.realm.array_buffer_proto.clone()),
    )
}

pub fn is_external(o: &Obj) -> bool {
    matches!(&o.kind, ObjKind::ArrayBuffer(b) if b.external)
}

fn new_shared_buffer(vm: &Vm, bytes: Vec<u8>, max: Option<usize>) -> Rc<Obj> {
    Obj::new(
        ObjKind::ArrayBuffer(Box::new(BufferData {
            bytes: RefCell::new(bytes),
            max,
            shared: true,
            external: false,
        })),
        Some(vm.realm.shared_buffer_proto.clone()),
    )
}

fn is_shared(o: &Obj) -> bool {
    matches!(&o.kind, ObjKind::ArrayBuffer(b) if b.shared)
}

fn buffer_max(o: &Obj) -> Option<usize> {
    match &o.kind {
        ObjKind::ArrayBuffer(b) => b.max,
        _ => None,
    }
}

pub fn is_detached(buffer: &Obj) -> bool {
    buffer.flags.get() & DETACHED != 0
}

pub fn detach(buffer: &Obj) {
    if let ObjKind::ArrayBuffer(b) = &buffer.kind {
        buffer.flags.set(buffer.flags.get() | DETACHED);
        let old = std::mem::take(&mut *b.borrow_mut());
        if b.external {
            std::mem::forget(old);
        }
    }
}

fn alloc_bytes(vm: &mut Vm, n: usize) -> Result<Vec<u8>, Value> {
    let mut v = Vec::new();
    if n > MAX_ALLOC || v.try_reserve_exact(n).is_err() {
        return Err(vm.range_error("Array buffer allocation failed"));
    }
    v.resize(n, 0);
    Ok(v)
}

fn make_view(
    proto: Rc<Obj>,
    kind: TypedKind,
    buffer: Rc<Obj>,
    offset: usize,
    length: usize,
) -> Rc<Obj> {
    Obj::new(
        ObjKind::Typed(Box::new(View {
            kind,
            buffer,
            offset,
            length,
            tracking: false,
        })),
        Some(proto),
    )
}

fn make_tracking_view(proto: Rc<Obj>, kind: TypedKind, buffer: Rc<Obj>, offset: usize) -> Rc<Obj> {
    Obj::new(
        ObjKind::Typed(Box::new(View {
            kind,
            buffer,
            offset,
            length: 0,
            tracking: true,
        })),
        Some(proto),
    )
}

pub fn new_typed(vm: &Vm, kind: TypedKind, buffer: Rc<Obj>, offset: usize, length: usize) -> Value {
    let proto = vm.realm.typed_protos[kind as usize].clone();
    Value::Object(make_view(proto, kind, buffer, offset, length))
}

pub fn new_uint8(vm: &Vm, bytes: Vec<u8>) -> Value {
    let len = bytes.len();
    let buffer = new_buffer(vm, bytes);
    new_typed(vm, TypedKind::Uint8, buffer, 0, len)
}

pub fn buffer_source_bytes(v: &Value) -> Option<Vec<u8>> {
    let Value::Object(o) = v else {
        return None;
    };
    match &o.kind {
        ObjKind::ArrayBuffer(b) => Some(b.borrow().clone()),
        ObjKind::Typed(t) => Some(t.bytes()),
        ObjKind::DataView(t) => {
            let n = t.len();
            let b = t.store().borrow();
            let end = (t.offset + n).min(b.len());
            Some(b[t.offset.min(end)..end].to_vec())
        }
        _ => None,
    }
}

fn allocate(
    vm: &mut Vm,
    kind: TypedKind,
    len: usize,
    proto: Option<Rc<Obj>>,
) -> Result<Rc<Obj>, Value> {
    let Some(bytes) = len.checked_mul(kind.size()).filter(|n| *n <= MAX_ALLOC) else {
        return Err(vm.range_error(format!("Invalid typed array length: {len}")));
    };
    let data = alloc_bytes(vm, bytes)?;
    let buffer = new_buffer(vm, data);
    let proto = proto.unwrap_or_else(|| vm.realm.typed_protos[kind as usize].clone());
    Ok(make_view(proto, kind, buffer, 0, len))
}

fn to_index(vm: &mut Vm, v: &Value, what: &str) -> Result<usize, Value> {
    if v.is_undefined() {
        return Ok(0);
    }
    let n = to_integer(vm, v)?;
    if !(0.0..=MAX_INDEX).contains(&n) {
        return Err(vm.range_error(format!("Invalid {what}: {}", inspect_plain(v))));
    }
    Ok(n as usize)
}

pub fn coerce(vm: &mut Vm, kind: TypedKind, v: &Value) -> R {
    if kind.is_big() {
        return Ok(Value::BigInt(super::number::to_bigint(vm, v)?));
    }
    match v {
        Value::Number(_) => Ok(v.clone()),
        _ => Ok(Value::Number(vm.to_number(v)?)),
    }
}

pub fn canonical_numeric(key: &str) -> Option<f64> {
    if key == "-0" {
        return Some(-0.0);
    }
    let first = key.as_bytes().first()?;
    if !(first.is_ascii_digit() || matches!(first, b'-' | b'I' | b'N')) {
        return None;
    }
    let n = crate::number::string_to_number(key);
    (crate::number::number_to_string(n) == key).then_some(n)
}

pub fn valid_index(t: &View, n: f64) -> Option<usize> {
    if n.fract() != 0.0 || (n == 0.0 && n.is_sign_negative()) || n < 0.0 {
        return None;
    }
    let i = n as usize;
    (i < t.len()).then_some(i)
}

fn view_of(o: &Obj) -> &View {
    match &o.kind {
        ObjKind::Typed(t) | ObjKind::DataView(t) => t,
        _ => unreachable!("not a view"),
    }
}

fn require_ta(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::Typed(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method %TypedArray%.prototype.{name} called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn validate(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    let o = require_ta(vm, this, name)?;
    if view_of(&o).out_of_bounds() {
        return Err(oob_error(vm, &o, name));
    }
    Ok(o)
}

fn oob_error(vm: &mut Vm, o: &Obj, name: &str) -> Value {
    if view_of(o).detached() {
        vm.type_error(format!(
            "Cannot perform %TypedArray%.prototype.{name} on a detached ArrayBuffer"
        ))
    } else {
        vm.type_error(format!(
            "Cannot perform %TypedArray%.prototype.{name} on an out of bounds TypedArray"
        ))
    }
}

fn get_el(o: &Obj, i: usize) -> Value {
    view_of(o).get(i).unwrap_or_default()
}

fn set_el(vm: &mut Vm, o: &Obj, i: usize, v: &Value) -> Result<(), Value> {
    let x = coerce(vm, view_of(o).kind, v)?;
    view_of(o).set(i, &x);
    Ok(())
}

fn length_of(vm: &mut Vm, v: &Value) -> Result<usize, Value> {
    let l = vm.get_prop(v, "length")?;
    let n = to_integer(vm, &l)?;
    Ok(n.clamp(0.0, MAX_INDEX) as usize)
}

fn relative(vm: &mut Vm, v: &Value, len: usize, default: usize) -> Result<usize, Value> {
    if v.is_undefined() {
        return Ok(default);
    }
    let n = to_integer(vm, v)?;
    Ok(relative_index(n, len))
}

fn callback(vm: &mut Vm, args: &[Value]) -> R {
    let f = arg(args, 0);
    if !f.is_callable() {
        return Err(vm.type_error(format!("{} is not a function", inspect_plain(&f))));
    }
    Ok(f)
}

fn default_ctor(vm: &Vm, kind: TypedKind) -> Value {
    vm.binary_ctors
        .get(kind as usize)
        .cloned()
        .unwrap_or_default()
}

fn buffer_ctor(vm: &Vm) -> Value {
    vm.binary_ctors
        .get(TypedKind::ALL.len())
        .cloned()
        .unwrap_or_default()
}

pub fn species_constructor(vm: &mut Vm, o: &Value, default: Value) -> R {
    let c = vm.get_prop(o, "constructor")?;
    if c.is_undefined() {
        return Ok(default);
    }
    if !c.is_object() {
        return Err(vm.type_error("object.constructor is not an object"));
    }
    let s = vm.get_prop(&c, SPECIES_KEY)?;
    if s.is_nullish() {
        return Ok(default);
    }
    if vm.is_constructor(&s) {
        Ok(s)
    } else {
        Err(vm.type_error("object.constructor[Symbol.species] is not a constructor"))
    }
}

fn create_from_ctor(vm: &mut Vm, c: &Value, args: &[Value]) -> Result<Rc<Obj>, Value> {
    let r = vm.construct(c, args)?;
    let o = validate(vm, &r, "constructor")?;
    if let [Value::Number(n)] = args {
        if (view_of(&o).len() as f64) < *n {
            return Err(vm.type_error(
                "Derived TypedArray constructor created an array which was too small",
            ));
        }
    }
    Ok(o)
}

fn species_create(vm: &mut Vm, exemplar: &Rc<Obj>, args: &[Value]) -> Result<Rc<Obj>, Value> {
    let kind = view_of(exemplar).kind;
    let default = default_ctor(vm, kind);
    let c = species_constructor(vm, &Value::Object(exemplar.clone()), default.clone())?;
    if c.strict_eq(&default) {
        if let [Value::Number(n)] = args {
            return allocate(vm, kind, *n as usize, None);
        }
    }
    let r = create_from_ctor(vm, &c, args)?;
    if view_of(&r).kind.is_big() != kind.is_big() {
        return Err(vm.type_error("Content type of the new typed array does not match"));
    }
    Ok(r)
}

fn list_from_method(vm: &mut Vm, obj: &Value, method: &Value) -> Result<Vec<Value>, Value> {
    let it = vm.iterator_from_method(obj, method)?;
    vm.drain_iterator(&it)
}

fn copy_elements(src: &View, dst: &View, len: usize) {
    if src.kind == dst.kind {
        let bytes = src.bytes();
        let n = bytes.len().min(len * dst.kind.size());
        let mut b = dst.store().borrow_mut();
        if let Some(d) = b.get_mut(dst.offset..dst.offset + n) {
            d.copy_from_slice(&bytes[..n]);
        }
        return;
    }
    for i in 0..len {
        if let Some(v) = src.get(i) {
            dst.set(i, &v);
        }
    }
}

pub fn install(vm: &mut Vm) {
    install_buffer(vm);
    install_typed(vm);
    install_dataview(vm);
}

fn getter(vm: &mut Vm, target: &Rc<Obj>, name: &str, f: NativeFn) {
    let label = match name.strip_prefix('\u{0}') {
        Some(_) => "get [Symbol.toStringTag]".to_string(),
        None => format!("get {name}"),
    };
    let g = native_fn(vm, &label, 0, f);
    target.set_with_attrs(name, raw_accessor(g, Value::Undefined), NON_ENUM);
}

fn frozen(target: &Rc<Obj>, name: &str, v: Value) {
    target.set_with_attrs(name, v, NON_ENUM | READ_ONLY | NON_CONFIG);
}

fn install_buffer(vm: &mut Vm) {
    let abp = vm.realm.array_buffer_proto.clone();
    method(vm, &abp, "slice", 2, ab_slice);
    method(vm, &abp, "resize", 1, ab_resize);

    method(vm, &abp, "transfer", 0, ab_transfer::<false>);
    method(vm, &abp, "transferToFixedLength", 0, ab_transfer::<true>);
    getter(vm, &abp, "byteLength", ab_byte_length);
    getter(vm, &abp, "maxByteLength", ab_max_byte_length);
    getter(vm, &abp, "resizable", ab_resizable);
    getter(vm, &abp, "detached", ab_detached);
    let ab = make_ctor(
        vm,
        "ArrayBuffer",
        1,
        ab_ctor,
        Some(crate::object::wrapper_marker),
        &abp,
    );
    method(vm, &ab, "isView", 1, ab_is_view);
    vm.define_global("ArrayBuffer", Value::Object(ab));

    let sbp = vm.realm.shared_buffer_proto.clone();
    method(vm, &sbp, "slice", 2, sab_slice);
    method(vm, &sbp, "grow", 1, sab_grow);
    getter(vm, &sbp, "byteLength", sab_byte_length);
    getter(vm, &sbp, "maxByteLength", sab_max_byte_length);
    getter(vm, &sbp, "growable", sab_growable);
    sbp.set_with_attrs(
        TO_STRING_TAG_KEY,
        "SharedArrayBuffer".into(),
        NON_ENUM | READ_ONLY,
    );
    let sab = make_ctor(
        vm,
        "SharedArrayBuffer",
        1,
        sab_ctor,
        Some(crate::object::wrapper_marker),
        &sbp,
    );
    let species = native_fn(vm, "get [Symbol.species]", 0, species_getter);
    sab.set_with_attrs(
        SPECIES_KEY,
        raw_accessor(species, Value::Undefined),
        NON_ENUM,
    );
    vm.define_global("SharedArrayBuffer", Value::Object(sab));
    install_atomics(vm);
}

fn install_typed(vm: &mut Vm) {
    let tp = vm.realm.typed_proto.clone();
    let methods: [(&str, u32, NativeFn); 30] = [
        ("at", 1, ta_at),
        ("copyWithin", 2, ta_copy_within),
        ("entries", 0, ta_iter::<2>),
        ("every", 1, ta_every),
        ("fill", 1, ta_fill),
        ("filter", 1, ta_filter),
        ("find", 1, ta_find),
        ("findIndex", 1, ta_find_index),
        ("findLast", 1, ta_find_last),
        ("findLastIndex", 1, ta_find_last_index),
        ("forEach", 1, ta_for_each),
        ("includes", 1, ta_includes),
        ("indexOf", 1, ta_index_of),
        ("join", 1, ta_join),
        ("keys", 0, ta_iter::<0>),
        ("lastIndexOf", 1, ta_last_index_of),
        ("map", 1, ta_map),
        ("reduce", 1, ta_reduce),
        ("reduceRight", 1, ta_reduce_right),
        ("reverse", 0, ta_reverse),
        ("set", 1, ta_set),
        ("slice", 2, ta_slice),
        ("some", 1, ta_some),
        ("sort", 1, ta_sort),
        ("subarray", 2, ta_subarray),
        ("toLocaleString", 0, ta_to_locale_string),
        ("toReversed", 0, ta_to_reversed),
        ("toSorted", 1, ta_to_sorted),
        ("values", 0, ta_iter::<1>),
        ("with", 2, ta_with),
    ];
    for (name, arity, f) in methods {
        method(vm, &tp, name, arity, f);
    }
    if let Some(v) = tp.get_own("values") {
        tp.set_own(ITER_KEY, v);
    }
    if let Some(v) = vm.realm.array_proto.get_own("toString") {
        tp.set_own("toString", v);
    }
    getter(vm, &tp, "buffer", ta_buffer);
    getter(vm, &tp, "byteLength", ta_byte_length);
    getter(vm, &tp, "byteOffset", ta_byte_offset);
    getter(vm, &tp, "length", ta_length);
    getter(vm, &tp, TO_STRING_TAG_KEY, ta_tag);

    let abstract_ctor = make_ctor(vm, "TypedArray", 0, ta_abstract, None, &tp);
    method(vm, &abstract_ctor, "from", 1, ta_from);
    method(vm, &abstract_ctor, "of", 0, ta_of);
    let species = native_fn(vm, "get [Symbol.species]", 0, species_getter);
    abstract_ctor.set_with_attrs(
        SPECIES_KEY,
        raw_accessor(species, Value::Undefined),
        NON_ENUM,
    );

    let ctors: [NativeFn; 12] = [
        typed_ctor::<0>,
        typed_ctor::<1>,
        typed_ctor::<2>,
        typed_ctor::<3>,
        typed_ctor::<4>,
        typed_ctor::<5>,
        typed_ctor::<6>,
        typed_ctor::<7>,
        typed_ctor::<8>,
        typed_ctor::<9>,
        typed_ctor::<10>,
        typed_ctor::<11>,
    ];
    let mut made = Vec::with_capacity(ctors.len() + 1);
    for (i, kind) in TypedKind::ALL.iter().enumerate() {
        let proto = vm.realm.typed_protos[i].clone();
        let c = make_ctor(
            vm,
            kind.name(),
            3,
            ctors[i],
            Some(crate::object::wrapper_marker),
            &proto,
        );
        *c.proto.borrow_mut() = Some(abstract_ctor.clone());
        let size = Value::Number(kind.size() as f64);
        frozen(&c, "BYTES_PER_ELEMENT", size.clone());
        frozen(&proto, "BYTES_PER_ELEMENT", size);
        if *kind == TypedKind::Uint8 {
            method(vm, &c, "fromHex", 1, super::binary::u8_from_hex);
            method(vm, &c, "fromBase64", 1, super::binary::u8_from_base64);
            method(vm, &proto, "toHex", 0, super::binary::ta_to_hex);
            method(vm, &proto, "toBase64", 0, super::binary::ta_to_base64);
        }
        made.push(Value::Object(c.clone()));
        vm.define_global(kind.name(), Value::Object(c));
    }
    if let Some(ab) = vm.globals.get("ArrayBuffer").cloned() {
        made.push(ab);
    }
    vm.binary_ctors = made;
}

fn install_dataview(vm: &mut Vm) {
    let dvp = vm.realm.dataview_proto.clone();
    let accessors: [(&str, NativeFn, &str, NativeFn); 11] = [
        ("getInt8", dv_get::<0>, "setInt8", dv_set::<0>),
        ("getUint8", dv_get::<1>, "setUint8", dv_set::<1>),
        ("getInt16", dv_get::<3>, "setInt16", dv_set::<3>),
        ("getUint16", dv_get::<4>, "setUint16", dv_set::<4>),
        ("getInt32", dv_get::<5>, "setInt32", dv_set::<5>),
        ("getUint32", dv_get::<6>, "setUint32", dv_set::<6>),
        ("getFloat32", dv_get::<7>, "setFloat32", dv_set::<7>),
        ("getFloat64", dv_get::<8>, "setFloat64", dv_set::<8>),
        ("getBigInt64", dv_get::<9>, "setBigInt64", dv_set::<9>),
        ("getBigUint64", dv_get::<10>, "setBigUint64", dv_set::<10>),
        ("getFloat16", dv_get::<11>, "setFloat16", dv_set::<11>),
    ];
    for (g, gf, s, sf) in accessors {
        method(vm, &dvp, g, 1, gf);
        method(vm, &dvp, s, 2, sf);
    }
    getter(vm, &dvp, "buffer", dv_buffer);
    getter(vm, &dvp, "byteLength", dv_byte_length);
    getter(vm, &dvp, "byteOffset", dv_byte_offset);
    let dv = make_ctor(
        vm,
        "DataView",
        1,
        dv_ctor,
        Some(crate::object::wrapper_marker),
        &dvp,
    );
    vm.define_global("DataView", Value::Object(dv));
}

fn marker_proto(this: &Value) -> Option<Rc<Obj>> {
    match this {
        Value::Object(o) => o.proto.borrow().clone(),
        _ => None,
    }
}

fn this_buffer(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::ArrayBuffer(_)) && !is_shared(o) => {
            Ok(o.clone())
        }
        _ => Err(vm.type_error(format!(
            "Method ArrayBuffer.prototype.{name} called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn buffer_len(o: &Obj) -> usize {
    match &o.kind {
        ObjKind::ArrayBuffer(b) => b.borrow().len(),
        _ => 0,
    }
}

fn ab_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !constructing(&this) {
        return Err(vm.type_error("Constructor ArrayBuffer requires 'new'"));
    }
    let len = to_index(vm, &arg(args, 0), "array buffer length")?;
    let options = arg(args, 1);
    let max = if options.is_object() {
        let m = vm.get_prop(&options, "maxByteLength")?;
        if m.is_undefined() {
            None
        } else {
            Some(to_index(vm, &m, "array buffer max length")?)
        }
    } else {
        None
    };
    if let Some(m) = max {
        if len > m {
            return Err(vm.range_error(format!(
                "Invalid array buffer max length: byte length {len} exceeds max {m}"
            )));
        }
        if m > MAX_ALLOC {
            return Err(vm.range_error("Array buffer allocation failed"));
        }
    }
    let bytes = alloc_bytes(vm, len)?;
    let buf = new_buffer_with_max(vm, bytes, max);
    if let Some(p) = marker_proto(&this) {
        *buf.proto.borrow_mut() = Some(p);
    }
    Ok(Value::Object(buf))
}

fn ab_is_view(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(matches!(
        arg(args, 0),
        Value::Object(o) if matches!(o.kind, ObjKind::Typed(_) | ObjKind::DataView(_))
    )))
}

fn ab_byte_length(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_buffer(vm, &this, "byteLength")?;
    Ok(Value::Number(buffer_len(&o) as f64))
}

fn ab_resizable(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_buffer(vm, &this, "resizable")?;
    Ok(Value::Bool(buffer_max(&o).is_some()))
}

fn ab_max_byte_length(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_buffer(vm, &this, "maxByteLength")?;
    if is_detached(&o) {
        return Ok(Value::Number(0.0));
    }
    Ok(Value::Number(
        buffer_max(&o).unwrap_or_else(|| buffer_len(&o)) as f64,
    ))
}

fn ab_detached(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_buffer(vm, &this, "detached")?;
    Ok(Value::Bool(is_detached(&o)))
}

fn ab_resize(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_buffer(vm, &this, "resize")?;
    let Some(max) = buffer_max(&o) else {
        return Err(vm.type_error(format!(
            "Method ArrayBuffer.prototype.resize called on incompatible receiver {}",
            inspect_plain(&this)
        )));
    };
    let new_len = to_index(vm, &arg(args, 0), "length")?;
    if is_detached(&o) {
        return Err(
            vm.type_error("Cannot perform ArrayBuffer.prototype.resize on a detached ArrayBuffer")
        );
    }
    if new_len > max {
        return Err(vm.range_error("ArrayBuffer.prototype.resize: Invalid length parameter"));
    }
    if let ObjKind::ArrayBuffer(b) = &o.kind {
        b.borrow_mut().resize(new_len, 0);
    }
    Ok(Value::Undefined)
}

fn ab_slice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_buffer(vm, &this, "slice")?;
    if is_detached(&o) {
        return Err(
            vm.type_error("Cannot perform ArrayBuffer.prototype.slice on a detached ArrayBuffer")
        );
    }
    let len = buffer_len(&o);
    let first = relative(vm, &arg(args, 0), len, 0)?;
    let fin = relative(vm, &arg(args, 1), len, len)?;
    let new_len = fin.saturating_sub(first);
    let default = buffer_ctor(vm);
    let c = species_constructor(vm, &this, default.clone())?;
    let target = if c.strict_eq(&default) {
        let bytes = alloc_bytes(vm, new_len)?;
        new_buffer(vm, bytes)
    } else {
        let r = vm.construct(&c, &[Value::Number(new_len as f64)])?;
        let t = match &r {
            Value::Object(t) if matches!(t.kind, ObjKind::ArrayBuffer(_)) => t.clone(),
            _ => return Err(vm.type_error("Species constructor did not return an ArrayBuffer")),
        };
        if is_detached(&t) {
            return Err(vm.type_error("Species constructor returned a detached ArrayBuffer"));
        }
        if Rc::ptr_eq(&t, &o) {
            return Err(
                vm.type_error("ArrayBuffer subclass returned this from species constructor")
            );
        }
        if buffer_len(&t) < new_len {
            return Err(vm.type_error("Species constructor returned a too small ArrayBuffer"));
        }
        t
    };
    if is_detached(&o) {
        return Err(
            vm.type_error("Cannot perform ArrayBuffer.prototype.slice on a detached ArrayBuffer")
        );
    }
    if let (ObjKind::ArrayBuffer(src), ObjKind::ArrayBuffer(dst)) = (&o.kind, &target.kind) {
        let src = src.borrow();
        let end = fin.min(src.len());
        if first < end {
            let part = src[first..end].to_vec();
            drop(src);
            let mut dst = dst.borrow_mut();
            let n = part.len().min(dst.len());
            dst[..n].copy_from_slice(&part[..n]);
        }
    }
    Ok(Value::Object(target))
}

fn ab_transfer<const FIXED: bool>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let name = if FIXED {
        "transferToFixedLength"
    } else {
        "transfer"
    };
    let o = this_buffer(vm, &this, name)?;
    let new_len = match arg(args, 0) {
        Value::Undefined => buffer_len(&o),
        v => to_index(vm, &v, "array buffer length")?,
    };
    if is_detached(&o) {
        return Err(vm.type_error(format!(
            "Cannot perform ArrayBuffer.prototype.{name} on a detached ArrayBuffer"
        )));
    }
    if is_external(&o) {
        return Err(vm.type_error(format!(
            "Cannot perform ArrayBuffer.prototype.{name} on a WebAssembly.Memory buffer"
        )));
    }
    let max = if FIXED { None } else { buffer_max(&o) };
    if new_len > max.unwrap_or(MAX_ALLOC) {
        return Err(vm.range_error("Array buffer allocation failed"));
    }
    let mut bytes = match &o.kind {
        ObjKind::ArrayBuffer(b) => std::mem::take(&mut *b.borrow_mut()),
        _ => Vec::new(),
    };
    bytes.resize(new_len, 0);
    detach(&o);
    Ok(Value::Object(new_buffer_with_max(vm, bytes, max)))
}

fn typed_ctor<const K: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let kind = TypedKind::ALL[K];
    if !constructing(&this) {
        return Err(vm.type_error(format!("Constructor {} requires 'new'", kind.name())));
    }
    let proto = marker_proto(&this);
    let first = arg(args, 0);
    let src = match &first {
        Value::Object(o) => o.clone(),
        _ => {
            let len = to_index(vm, &first, "typed array length")?;
            return Ok(Value::Object(allocate(vm, kind, len, proto)?));
        }
    };
    match &src.kind {
        ObjKind::Typed(st) => {
            if st.out_of_bounds() {
                return Err(vm.type_error("Cannot perform Construct on a detached ArrayBuffer"));
            }
            if st.kind.is_big() != kind.is_big() {
                return Err(vm.type_error(format!(
                    "Cannot construct {} from {}",
                    kind.name(),
                    st.kind.name()
                )));
            }
            let len = st.len();
            let out = allocate(vm, kind, len, proto)?;
            copy_elements(st, view_of(&out), len);
            Ok(Value::Object(out))
        }
        ObjKind::ArrayBuffer(b) => {
            let size = kind.size();
            let offset = to_index(vm, &arg(args, 1), "typed array offset")?;
            if offset % size != 0 {
                return Err(vm.range_error(format!(
                    "start offset of {} should be a multiple of {size}",
                    kind.name()
                )));
            }
            let len_arg = arg(args, 2);
            let new_len = if len_arg.is_undefined() {
                None
            } else {
                Some(to_index(vm, &len_arg, "typed array length")?)
            };
            if is_detached(&src) {
                return Err(vm.type_error("Cannot perform Construct on a detached ArrayBuffer"));
            }
            let buf_len = b.borrow().len();
            if new_len.is_none() && b.max.is_some() {
                if offset > buf_len {
                    return Err(vm.range_error(format!(
                        "Start offset {offset} is outside the bounds of the buffer"
                    )));
                }
                let proto = proto.unwrap_or_else(|| vm.realm.typed_protos[kind as usize].clone());
                return Ok(Value::Object(make_tracking_view(
                    proto,
                    kind,
                    src.clone(),
                    offset,
                )));
            }
            let byte_len = match new_len {
                None => {
                    if buf_len % size != 0 {
                        return Err(vm.range_error(format!(
                            "byte length of {} should be a multiple of {size}",
                            kind.name()
                        )));
                    }
                    if offset > buf_len {
                        return Err(vm.range_error(format!(
                            "Start offset {offset} is outside the bounds of the buffer"
                        )));
                    }
                    buf_len - offset
                }
                Some(l) => {
                    let nb = l.saturating_mul(size);
                    if offset.checked_add(nb).is_none_or(|e| e > buf_len) {
                        return Err(vm.range_error(format!("Invalid typed array length: {l}")));
                    }
                    nb
                }
            };
            let proto = proto.unwrap_or_else(|| vm.realm.typed_protos[kind as usize].clone());
            Ok(Value::Object(make_view(
                proto,
                kind,
                src.clone(),
                offset,
                byte_len / size,
            )))
        }
        _ => {
            let using = vm.get_prop(&first, ITER_KEY)?;
            if !using.is_nullish() {
                if !using.is_callable() {
                    return Err(
                        vm.type_error(format!("{} is not a function", inspect_plain(&using)))
                    );
                }
                let values = list_from_method(vm, &first, &using)?;
                let out = allocate(vm, kind, values.len(), proto)?;
                for (k, v) in values.iter().enumerate() {
                    set_el(vm, &out, k, v)?;
                }
                return Ok(Value::Object(out));
            }
            let len = length_of(vm, &first)?;
            let out = allocate(vm, kind, len, proto)?;
            for k in 0..len {
                let v = vm.get_prop(&first, &k.to_string())?;
                set_el(vm, &out, k, &v)?;
            }
            Ok(Value::Object(out))
        }
    }
}

fn ta_abstract(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Err(vm.type_error("Abstract class TypedArray not directly constructable"))
}

fn species_getter(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this)
}

fn ta_from(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !vm.is_constructor(&this) {
        return Err(vm.type_error(format!("{} is not a constructor", inspect_plain(&this))));
    }
    let mapfn = arg(args, 1);
    let mapping = !mapfn.is_undefined();
    if mapping && !mapfn.is_callable() {
        return Err(vm.type_error(format!("{} is not a function", inspect_plain(&mapfn))));
    }
    let this_arg = arg(args, 2);
    let source = arg(args, 0);
    if source.is_nullish() {
        return Err(vm.type_error("Cannot convert undefined or null to object"));
    }
    let using = vm.get_prop(&source, ITER_KEY)?;
    let (values, from_list) = if using.is_nullish() {
        (Vec::new(), false)
    } else {
        if !using.is_callable() {
            return Err(vm.type_error(format!("{} is not a function", inspect_plain(&using))));
        }
        (list_from_method(vm, &source, &using)?, true)
    };
    if from_list {
        let target = create_from_ctor(vm, &this, &[Value::Number(values.len() as f64)])?;
        for (k, v) in values.into_iter().enumerate() {
            let mapped = if mapping {
                vm.call_value(&mapfn, this_arg.clone(), &[v, Value::Number(k as f64)])?
            } else {
                v
            };
            set_el(vm, &target, k, &mapped)?;
        }
        return Ok(Value::Object(target));
    }
    let array_like = Value::Object(vm.to_object(&source)?);
    let len = length_of(vm, &array_like)?;
    let target = create_from_ctor(vm, &this, &[Value::Number(len as f64)])?;
    for k in 0..len {
        let v = vm.get_prop(&array_like, &k.to_string())?;
        let mapped = if mapping {
            vm.call_value(&mapfn, this_arg.clone(), &[v, Value::Number(k as f64)])?
        } else {
            v
        };
        set_el(vm, &target, k, &mapped)?;
    }
    Ok(Value::Object(target))
}

fn ta_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !vm.is_constructor(&this) {
        return Err(vm.type_error(format!("{} is not a constructor", inspect_plain(&this))));
    }
    let target = create_from_ctor(vm, &this, &[Value::Number(args.len() as f64)])?;
    for (k, v) in args.iter().enumerate() {
        set_el(vm, &target, k, v)?;
    }
    Ok(Value::Object(target))
}

fn ta_buffer(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = require_ta(vm, &this, "buffer")?;
    Ok(Value::Object(view_of(&o).buffer.clone()))
}

fn ta_byte_length(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = require_ta(vm, &this, "byteLength")?;
    Ok(Value::Number(view_of(&o).byte_len() as f64))
}

fn ta_byte_offset(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = require_ta(vm, &this, "byteOffset")?;
    let t = view_of(&o);
    Ok(Value::Number(if t.out_of_bounds() {
        0.0
    } else {
        t.offset as f64
    }))
}

fn ta_length(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = require_ta(vm, &this, "length")?;
    Ok(Value::Number(view_of(&o).len() as f64))
}

fn ta_tag(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    match &this {
        Value::Object(o) => match &o.kind {
            ObjKind::Typed(t) => Ok(t.kind.name().into()),
            _ => Ok(Value::Undefined),
        },
        _ => Ok(Value::Undefined),
    }
}

fn ta_at(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "at")?;
    let len = view_of(&o).len() as f64;
    let rel = to_integer(vm, &arg(args, 0))?;
    let k = if rel >= 0.0 { rel } else { len + rel };
    if k < 0.0 || k >= len {
        return Ok(Value::Undefined);
    }
    Ok(get_el(&o, k as usize))
}

fn ta_copy_within(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "copyWithin")?;
    let len = view_of(&o).len();
    let to = relative(vm, &arg(args, 0), len, 0)?;
    let from = relative(vm, &arg(args, 1), len, 0)?;
    let fin = relative(vm, &arg(args, 2), len, len)?;
    let count = fin.saturating_sub(from).min(len.saturating_sub(to));
    if count > 0 {
        let t = view_of(&o);
        if t.out_of_bounds() {
            return Err(oob_error(vm, &o, "copyWithin"));
        }
        let size = t.kind.size();
        let limit = t.len() * size + t.offset;
        let mut b = t.store().borrow_mut();
        let src = t.offset + from * size;
        let dst = t.offset + to * size;
        let n = count * size;
        if src < dst && dst < src + n {
            for j in (0..n).rev() {
                if src + j < limit && dst + j < limit {
                    b[dst + j] = b[src + j];
                }
            }
        } else {
            for j in 0..n {
                if src + j < limit && dst + j < limit {
                    b[dst + j] = b[src + j];
                }
            }
        }
    }
    Ok(this)
}

fn ta_iter<const K: u8>(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let name = ["keys", "values", "entries"][K as usize];
    let o = validate(vm, &this, name)?;
    Ok(vm.new_iter(IterState::Typed(o, 0, K)))
}

fn scan(
    vm: &mut Vm,
    this: &Value,
    args: &[Value],
    name: &str,
    reverse: bool,
    mut stop: impl FnMut(usize, &Value, bool) -> Option<Value>,
) -> Result<Option<Value>, Value> {
    let o = validate(vm, this, name)?;
    let len = view_of(&o).len();
    let f = callback(vm, args)?;
    let this_arg = arg(args, 1);
    for step in 0..len {
        let k = if reverse { len - 1 - step } else { step };
        let v = get_el(&o, k);
        let r = vm.call_value(
            &f,
            this_arg.clone(),
            &[v.clone(), Value::Number(k as f64), this.clone()],
        )?;
        if let Some(out) = stop(k, &v, r.truthy()) {
            return Ok(Some(out));
        }
    }
    Ok(None)
}

fn ta_every(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = scan(vm, &this, args, "every", false, |_, _, t| {
        (!t).then_some(Value::Bool(false))
    })?;
    Ok(r.unwrap_or(Value::Bool(true)))
}

fn ta_some(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = scan(vm, &this, args, "some", false, |_, _, t| {
        t.then_some(Value::Bool(true))
    })?;
    Ok(r.unwrap_or(Value::Bool(false)))
}

fn ta_find(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = scan(vm, &this, args, "find", false, |_, v, t| {
        t.then(|| v.clone())
    })?;
    Ok(r.unwrap_or_default())
}

fn ta_find_index(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = scan(vm, &this, args, "findIndex", false, |k, _, t| {
        t.then_some(Value::Number(k as f64))
    })?;
    Ok(r.unwrap_or(Value::Number(-1.0)))
}

fn ta_find_last(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = scan(vm, &this, args, "findLast", true, |_, v, t| {
        t.then(|| v.clone())
    })?;
    Ok(r.unwrap_or_default())
}

fn ta_find_last_index(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let r = scan(vm, &this, args, "findLastIndex", true, |k, _, t| {
        t.then_some(Value::Number(k as f64))
    })?;
    Ok(r.unwrap_or(Value::Number(-1.0)))
}

fn ta_for_each(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    scan(vm, &this, args, "forEach", false, |_, _, _| None)?;
    Ok(Value::Undefined)
}

fn ta_fill(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "fill")?;
    let len = view_of(&o).len();
    let kind = view_of(&o).kind;
    let value = coerce(vm, kind, &arg(args, 0))?;
    let k = relative(vm, &arg(args, 1), len, 0)?;
    let fin = relative(vm, &arg(args, 2), len, len)?;
    let t = view_of(&o);
    if t.out_of_bounds() {
        return Err(
            vm.type_error("Cannot perform %TypedArray%.prototype.fill on a detached ArrayBuffer")
        );
    }
    let fin = fin.min(t.len());
    let size = kind.size();
    let mut cell = [0u8; 8];
    kind.write_value(&mut cell[..size], &value, true);
    let mut b = t.store().borrow_mut();
    for i in k..fin {
        let s = t.offset + i * size;
        if let Some(dst) = b.get_mut(s..s + size) {
            dst.copy_from_slice(&cell[..size]);
        }
    }
    Ok(this)
}

fn ta_filter(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let mut kept = Vec::new();
    scan(vm, &this, args, "filter", false, |_, v, t| {
        if t {
            kept.push(v.clone());
        }
        None
    })?;
    let o = require_ta(vm, &this, "filter")?;
    let a = species_create(vm, &o, &[Value::Number(kept.len() as f64)])?;
    for (n, v) in kept.iter().enumerate() {
        set_el(vm, &a, n, v)?;
    }
    Ok(Value::Object(a))
}

fn search_start(vm: &mut Vm, from: &Value, len: usize) -> Result<Option<usize>, Value> {
    let n = to_integer(vm, from)?;
    if n == f64::INFINITY {
        return Ok(None);
    }
    let n = if n == f64::NEG_INFINITY { 0.0 } else { n };
    Ok(Some(if n >= 0.0 {
        n as usize
    } else {
        (len as f64 + n).max(0.0) as usize
    }))
}

fn ta_includes(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "includes")?;
    let len = view_of(&o).len();
    if len == 0 {
        return Ok(Value::Bool(false));
    }
    let Some(start) = search_start(vm, &arg(args, 1), len)? else {
        return Ok(Value::Bool(false));
    };
    let target = arg(args, 0);
    for k in start..len {
        if get_el(&o, k).same_value_zero(&target) {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

fn ta_index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "indexOf")?;
    let len = view_of(&o).len();
    if len == 0 {
        return Ok(Value::Number(-1.0));
    }
    let Some(start) = search_start(vm, &arg(args, 1), len)? else {
        return Ok(Value::Number(-1.0));
    };
    let target = arg(args, 0);
    if !matches!(target, Value::Number(_) | Value::BigInt(_)) {
        return Ok(Value::Number(-1.0));
    }
    let t = view_of(&o);
    for k in start..len.min(t.len()) {
        if let Some(v) = t.get(k) {
            if v.strict_eq(&target) {
                return Ok(Value::Number(k as f64));
            }
        }
    }
    Ok(Value::Number(-1.0))
}

fn ta_last_index_of(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "lastIndexOf")?;
    let len = view_of(&o).len();
    if len == 0 {
        return Ok(Value::Number(-1.0));
    }
    let n = if args.len() > 1 {
        to_integer(vm, &args[1])?
    } else {
        len as f64 - 1.0
    };
    if n == f64::NEG_INFINITY {
        return Ok(Value::Number(-1.0));
    }
    let mut k = if n >= 0.0 {
        n.min(len as f64 - 1.0)
    } else {
        len as f64 + n
    };
    let target = arg(args, 0);
    let t = view_of(&o);
    while k >= 0.0 {
        if let Some(v) = t.get(k as usize).filter(|_| (k as usize) < t.len()) {
            if v.strict_eq(&target) {
                return Ok(Value::Number(k as usize as f64));
            }
        }
        k -= 1.0;
    }
    Ok(Value::Number(-1.0))
}

fn ta_join(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "join")?;
    let len = view_of(&o).len();
    let sep = match arg(args, 0) {
        Value::Undefined => JsStr::from(","),
        v => vm.to_string(&v)?,
    };
    let mut out = String::new();
    for k in 0..len {
        if k > 0 {
            out.push_str(&sep);
        }
        match get_el(&o, k) {
            Value::Undefined => {}
            Value::Number(n) => out.push_str(&crate::number::number_to_string(n)),
            v => out.push_str(&vm.to_string(&v)?),
        }
    }
    Ok(out.into())
}

fn ta_map(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "map")?;
    let len = view_of(&o).len();
    let f = callback(vm, args)?;
    let this_arg = arg(args, 1);
    let a = species_create(vm, &o, &[Value::Number(len as f64)])?;
    for k in 0..len {
        let v = get_el(&o, k);
        let mapped = vm.call_value(
            &f,
            this_arg.clone(),
            &[v, Value::Number(k as f64), this.clone()],
        )?;
        set_el(vm, &a, k, &mapped)?;
    }
    Ok(Value::Object(a))
}

fn reduce_impl(vm: &mut Vm, this: &Value, args: &[Value], right: bool) -> R {
    let o = validate(vm, this, if right { "reduceRight" } else { "reduce" })?;
    let len = view_of(&o).len();
    let f = callback(vm, args)?;
    let mut order: Box<dyn Iterator<Item = usize>> = if right {
        Box::new((0..len).rev())
    } else {
        Box::new(0..len)
    };
    let mut acc = if args.len() > 1 {
        args[1].clone()
    } else {
        match order.next() {
            Some(k) => get_el(&o, k),
            None => return Err(vm.type_error("Reduce of empty array with no initial value")),
        }
    };
    for k in order {
        let v = get_el(&o, k);
        acc = vm.call_value(
            &f,
            Value::Undefined,
            &[acc, v, Value::Number(k as f64), this.clone()],
        )?;
    }
    Ok(acc)
}

fn ta_reduce(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    reduce_impl(vm, &this, args, false)
}

fn ta_reduce_right(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    reduce_impl(vm, &this, args, true)
}

fn ta_reverse(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = validate(vm, &this, "reverse")?;
    let t = view_of(&o);
    let len = t.len();
    let size = t.kind.size();
    let mut b = t.store().borrow_mut();
    let base = t.offset;
    if base + len * size <= b.len() {
        for lower in 0..len / 2 {
            let upper = len - 1 - lower;
            for j in 0..size {
                b.swap(base + lower * size + j, base + upper * size + j);
            }
        }
    }
    drop(b);
    Ok(this)
}

fn ta_set(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = require_ta(vm, &this, "set")?;
    let off = to_integer(vm, &arg(args, 1))?;
    if off < 0.0 {
        return Err(vm.range_error("offset is out of bounds"));
    }
    let source = arg(args, 0);
    let detached_error = |vm: &mut Vm| {
        vm.type_error("Cannot perform %TypedArray%.prototype.set on a detached ArrayBuffer")
    };
    if let Value::Object(s) = &source {
        if let ObjKind::Typed(st) = &s.kind {
            let t = view_of(&o);
            if t.out_of_bounds() || st.out_of_bounds() {
                return Err(detached_error(vm));
            }
            if st.kind.is_big() != t.kind.is_big() {
                return Err(
                    vm.type_error("Cannot mix BigInt and other types, use explicit conversions")
                );
            }
            let src_len = st.len();
            if off == f64::INFINITY || src_len as f64 + off > t.len() as f64 {
                return Err(vm.range_error("offset is out of bounds"));
            }
            let off = off as usize;
            if st.kind == t.kind {
                let bytes = st.bytes();
                let size = t.kind.size();
                let start = t.offset + off * size;
                let mut b = t.store().borrow_mut();
                if let Some(d) = b.get_mut(start..start + bytes.len()) {
                    d.copy_from_slice(&bytes);
                }
            } else {
                let values = st.values();
                for (i, v) in values.iter().enumerate() {
                    t.set(off + i, v);
                }
            }
            return Ok(Value::Undefined);
        }
    }
    if view_of(&o).out_of_bounds() {
        return Err(detached_error(vm));
    }
    let target_len = view_of(&o).len();
    let src = Value::Object(vm.to_object(&source)?);
    let src_len = length_of(vm, &src)?;
    if off == f64::INFINITY || src_len as f64 + off > target_len as f64 {
        return Err(vm.range_error("offset is out of bounds"));
    }
    let off = off as usize;
    if let Value::Object(s) = &src {
        if let ObjKind::Array(a) = &s.kind {
            if !s.has_holes() {
                for k in 0..src_len {
                    let v = a.borrow().get(k).cloned().unwrap_or_default();
                    set_el(vm, &o, off + k, &v)?;
                }
                return Ok(Value::Undefined);
            }
        }
    }
    for k in 0..src_len {
        let v = vm.get_prop(&src, &k.to_string())?;
        set_el(vm, &o, off + k, &v)?;
    }
    Ok(Value::Undefined)
}

fn ta_slice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "slice")?;
    let len = view_of(&o).len();
    let k = relative(vm, &arg(args, 0), len, 0)?;
    let fin = relative(vm, &arg(args, 1), len, len)?;
    let count = fin.saturating_sub(k);
    let a = species_create(vm, &o, &[Value::Number(count as f64)])?;
    if count > 0 {
        let t = view_of(&o);
        if t.out_of_bounds() {
            return Err(vm.type_error(
                "Cannot perform %TypedArray%.prototype.slice on a detached ArrayBuffer",
            ));
        }
        let fin = fin.min(t.len());
        let at = view_of(&a);
        if t.kind == at.kind && Rc::ptr_eq(&t.buffer, &at.buffer) {
            let size = t.kind.size();
            let src = t.offset + k * size;
            let limit = (at.offset + (count * size).min((fin.saturating_sub(k)) * size))
                .min(at.offset + at.len() * size);
            let mut b = t.store().borrow_mut();
            let mut s = src;
            let mut d = at.offset;
            while d < limit && s < b.len() && d < b.len() {
                b[d] = b[s];
                s += 1;
                d += 1;
            }
        } else if t.kind == at.kind {
            let size = t.kind.size();
            let part = {
                let b = t.store().borrow();
                let s = t.offset + k * size;
                let e = (t.offset + fin * size).min(b.len());
                if s < e { b[s..e].to_vec() } else { Vec::new() }
            };
            let n = part.len().min(at.len() * size);
            let mut db = at.store().borrow_mut();
            if let Some(d) = db.get_mut(at.offset..at.offset + n) {
                d.copy_from_slice(&part[..n]);
            }
        } else {
            for (n, i) in (k..fin).enumerate() {
                let v = get_el(&o, i);
                set_el(vm, &a, n, &v)?;
            }
        }
    }
    Ok(Value::Object(a))
}

fn default_order(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if x.is_nan() {
                return if y.is_nan() {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                };
            }
            if y.is_nan() {
                return Ordering::Less;
            }
            if x < y {
                Ordering::Less
            } else if x > y {
                Ordering::Greater
            } else {
                match (x.is_sign_negative(), y.is_sign_negative()) {
                    (true, false) if *x == 0.0 => Ordering::Less,
                    (false, true) if *x == 0.0 => Ordering::Greater,
                    _ => Ordering::Equal,
                }
            }
        }
        (Value::BigInt(x), Value::BigInt(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

fn merge_sort(vm: &mut Vm, mut items: Vec<Value>, cmp: &Value) -> Result<Vec<Value>, Value> {
    if items.len() <= 1 {
        return Ok(items);
    }
    let right = items.split_off(items.len() / 2);
    let left = merge_sort(vm, items, cmp)?;
    let right = merge_sort(vm, right, cmp)?;
    let mut out = Vec::with_capacity(left.len() + right.len());
    let mut l = left.into_iter().peekable();
    let mut r = right.into_iter().peekable();
    while let (Some(a), Some(b)) = (l.peek(), r.peek()) {
        let res = vm.call_value(cmp, Value::Undefined, &[a.clone(), b.clone()])?;
        let n = vm.to_number(&res)?;
        if n > 0.0 {
            out.extend(r.next());
        } else {
            out.extend(l.next());
        }
    }
    out.extend(l);
    out.extend(r);
    Ok(out)
}

fn sort_values(vm: &mut Vm, mut items: Vec<Value>, cmp: &Value) -> Result<Vec<Value>, Value> {
    if cmp.is_undefined() {
        items.sort_by(default_order);
        return Ok(items);
    }
    merge_sort(vm, items, cmp)
}

fn check_comparator(vm: &mut Vm, cmp: &Value) -> Result<(), Value> {
    if !cmp.is_undefined() && !cmp.is_callable() {
        return Err(vm.type_error("The comparison function must be either a function or undefined"));
    }
    Ok(())
}

fn ta_sort(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let cmp = arg(args, 0);
    check_comparator(vm, &cmp)?;
    let o = validate(vm, &this, "sort")?;
    let items = view_of(&o).values();
    let sorted = sort_values(vm, items, &cmp)?;
    let t = view_of(&o);
    for (i, v) in sorted.iter().enumerate() {
        t.set(i, v);
    }
    Ok(this)
}

fn ta_to_sorted(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let cmp = arg(args, 0);
    check_comparator(vm, &cmp)?;
    let o = validate(vm, &this, "toSorted")?;
    let kind = view_of(&o).kind;
    let items = view_of(&o).values();
    let a = allocate(vm, kind, items.len(), None)?;
    let sorted = sort_values(vm, items, &cmp)?;
    let at = view_of(&a);
    for (i, v) in sorted.iter().enumerate() {
        at.set(i, v);
    }
    Ok(Value::Object(a))
}

fn ta_subarray(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = require_ta(vm, &this, "subarray")?;
    let src_len = view_of(&o).len();
    let begin = relative(vm, &arg(args, 0), src_len, 0)?;
    let fin = relative(vm, &arg(args, 1), src_len, src_len)?;
    let t = view_of(&o);
    let size = t.kind.size();
    let new_len = fin.saturating_sub(begin);
    let buffer = Value::Object(t.buffer.clone());
    let start = Value::Number((t.offset + begin * size) as f64);
    if t.tracking && arg(args, 1).is_undefined() {
        return Ok(Value::Object(species_create(vm, &o, &[buffer, start])?));
    }
    let list = [buffer, start, Value::Number(new_len as f64)];
    Ok(Value::Object(species_create(vm, &o, &list)?))
}

fn ta_to_locale_string(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "toLocaleString")?;
    let len = view_of(&o).len();
    let mut out = String::new();
    for k in 0..len {
        if k > 0 {
            out.push(',');
        }
        let e = get_el(&o, k);
        if e.is_nullish() {
            continue;
        }
        let f = vm.get_prop(&e, "toLocaleString")?;
        let r = vm.call_value(&f, e, args)?;
        out.push_str(&vm.to_string(&r)?);
    }
    Ok(out.into())
}

fn ta_to_reversed(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = validate(vm, &this, "toReversed")?;
    let len = view_of(&o).len();
    let a = allocate(vm, view_of(&o).kind, len, None)?;
    for k in 0..len {
        view_of(&a).set(k, &get_el(&o, len - 1 - k));
    }
    Ok(Value::Object(a))
}

fn ta_with(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = validate(vm, &this, "with")?;
    let len = view_of(&o).len();
    let kind = view_of(&o).kind;
    let rel = to_integer(vm, &arg(args, 0))?;
    let actual = if rel >= 0.0 { rel } else { len as f64 + rel };
    let value = coerce(vm, kind, &arg(args, 1))?;
    if !(actual >= 0.0 && actual < view_of(&o).len() as f64) {
        return Err(vm.range_error("Invalid typed array index"));
    }
    let a = allocate(vm, kind, len, None)?;
    let idx = actual as usize;
    for k in 0..len {
        let v = if k == idx {
            value.clone()
        } else {
            get_el(&o, k)
        };
        set_el(vm, &a, k, &v)?;
    }
    Ok(Value::Object(a))
}

fn this_dataview(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if matches!(o.kind, ObjKind::DataView(_)) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method DataView.prototype.{name} called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn dv_detached(vm: &mut Vm, name: &str) -> Value {
    vm.type_error(format!(
        "Cannot perform DataView.prototype.{name} on a detached ArrayBuffer"
    ))
}

fn dv_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !constructing(&this) {
        return Err(vm.type_error("Constructor DataView requires 'new'"));
    }
    let buffer = match arg(args, 0) {
        Value::Object(o) if matches!(o.kind, ObjKind::ArrayBuffer(_)) => o,
        _ => {
            return Err(
                vm.type_error("First argument to DataView constructor must be an ArrayBuffer")
            );
        }
    };
    let offset = to_index(vm, &arg(args, 1), "DataView offset")?;
    if is_detached(&buffer) {
        return Err(vm.type_error("Cannot perform DataView constructor on a detached ArrayBuffer"));
    }
    let len = buffer_len(&buffer);
    if offset > len {
        return Err(vm.range_error(format!(
            "Start offset {offset} is outside the bounds of the buffer"
        )));
    }
    let len_arg = arg(args, 2);
    let tracking = len_arg.is_undefined() && buffer_max(&buffer).is_some();
    let length = if len_arg.is_undefined() {
        len - offset
    } else {
        let l = to_index(vm, &len_arg, "DataView length")?;
        if offset.checked_add(l).is_none_or(|e| e > len) {
            return Err(vm.range_error(format!("Invalid DataView length {l}")));
        }
        l
    };
    if is_detached(&buffer) {
        return Err(vm.type_error("Cannot perform DataView constructor on a detached ArrayBuffer"));
    }
    Ok(Value::Object(Obj::new(
        ObjKind::DataView(Box::new(View {
            kind: TypedKind::Uint8,
            buffer,
            offset,
            length,
            tracking,
        })),
        marker_proto(&this),
    )))
}

fn dv_buffer(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_dataview(vm, &this, "buffer")?;
    Ok(Value::Object(view_of(&o).buffer.clone()))
}

fn dv_byte_length(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_dataview(vm, &this, "byteLength")?;
    if view_of(&o).out_of_bounds() {
        return Err(dv_detached(vm, "byteLength"));
    }
    Ok(Value::Number(view_of(&o).len() as f64))
}

fn dv_byte_offset(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_dataview(vm, &this, "byteOffset")?;
    if view_of(&o).out_of_bounds() {
        return Err(dv_detached(vm, "byteOffset"));
    }
    Ok(Value::Number(view_of(&o).offset as f64))
}

fn dv_get<const K: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let kind = TypedKind::ALL[K];
    let name = format!("get{}", kind.name().trim_end_matches("Array"));
    let o = this_dataview(vm, &this, &name)?;
    let at = to_index(vm, &arg(args, 0), "offset")?;
    let little = arg(args, 1).truthy();
    let v = view_of(&o);
    if v.out_of_bounds() {
        return Err(dv_detached(vm, &name));
    }
    if at.checked_add(kind.size()).is_none_or(|e| e > v.len()) {
        return Err(vm.range_error("Offset is outside the bounds of the DataView"));
    }
    let b = v.store().borrow();
    let start = v.offset + at;
    match b.get(start..start + kind.size()) {
        Some(bytes) => Ok(kind.read_value(bytes, little)),
        None => Ok(Value::Undefined),
    }
}

fn dv_set<const K: usize>(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let kind = TypedKind::ALL[K];
    let name = format!("set{}", kind.name().trim_end_matches("Array"));
    let o = this_dataview(vm, &this, &name)?;
    let at = to_index(vm, &arg(args, 0), "offset")?;
    let value = coerce(vm, kind, &arg(args, 1))?;
    let little = arg(args, 2).truthy();
    let v = view_of(&o);
    if v.out_of_bounds() {
        return Err(dv_detached(vm, &name));
    }
    if at.checked_add(kind.size()).is_none_or(|e| e > v.len()) {
        return Err(vm.range_error("Offset is outside the bounds of the DataView"));
    }
    let mut b = v.store().borrow_mut();
    let start = v.offset + at;
    if let Some(bytes) = b.get_mut(start..start + kind.size()) {
        kind.write_value(bytes, &value, little);
    }
    Ok(Value::Undefined)
}

fn this_shared(vm: &mut Vm, this: &Value, name: &str) -> Result<Rc<Obj>, Value> {
    match this {
        Value::Object(o) if is_shared(o) => Ok(o.clone()),
        _ => Err(vm.type_error(format!(
            "Method SharedArrayBuffer.prototype.{name} called on incompatible receiver {}",
            inspect_plain(this)
        ))),
    }
}

fn sab_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    if !constructing(&this) {
        return Err(vm.type_error("Constructor SharedArrayBuffer requires 'new'"));
    }
    let len = to_index(vm, &arg(args, 0), "array buffer length")?;
    let options = arg(args, 1);
    let max = if options.is_object() {
        let m = vm.get_prop(&options, "maxByteLength")?;
        if m.is_undefined() {
            None
        } else {
            Some(to_index(vm, &m, "array buffer max length")?)
        }
    } else {
        None
    };
    if let Some(m) = max {
        if len > m {
            return Err(vm.range_error(format!(
                "Invalid array buffer max length: byte length {len} exceeds max {m}"
            )));
        }
        if m > MAX_ALLOC {
            return Err(vm.range_error("Array buffer allocation failed"));
        }
    }
    let bytes = alloc_bytes(vm, len)?;
    let buf = new_shared_buffer(vm, bytes, max);
    if let Some(p) = marker_proto(&this) {
        *buf.proto.borrow_mut() = Some(p);
    }
    Ok(Value::Object(buf))
}

fn sab_byte_length(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_shared(vm, &this, "byteLength")?;
    Ok(Value::Number(buffer_len(&o) as f64))
}

fn sab_max_byte_length(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_shared(vm, &this, "maxByteLength")?;
    Ok(Value::Number(
        buffer_max(&o).unwrap_or_else(|| buffer_len(&o)) as f64,
    ))
}

fn sab_growable(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let o = this_shared(vm, &this, "growable")?;
    Ok(Value::Bool(buffer_max(&o).is_some()))
}

fn sab_grow(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_shared(vm, &this, "grow")?;
    let Some(max) = buffer_max(&o) else {
        return Err(vm.type_error(format!(
            "Method SharedArrayBuffer.prototype.grow called on incompatible receiver {}",
            inspect_plain(&this)
        )));
    };
    let new_len = to_index(vm, &arg(args, 0), "length")?;
    let cur = buffer_len(&o);
    if new_len > max || new_len < cur {
        return Err(vm.range_error("SharedArrayBuffer.prototype.grow: Invalid length parameter"));
    }
    if let ObjKind::ArrayBuffer(b) = &o.kind {
        b.borrow_mut().resize(new_len, 0);
    }
    Ok(Value::Undefined)
}

fn sab_slice(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let o = this_shared(vm, &this, "slice")?;
    let len = buffer_len(&o);
    let first = relative(vm, &arg(args, 0), len, 0)?;
    let fin = relative(vm, &arg(args, 1), len, len)?;
    let new_len = fin.saturating_sub(first);
    let default = vm
        .globals
        .get("SharedArrayBuffer")
        .cloned()
        .unwrap_or_default();
    let c = species_constructor(vm, &this, default.clone())?;
    let target = if c.strict_eq(&default) {
        let bytes = alloc_bytes(vm, new_len)?;
        new_shared_buffer(vm, bytes, None)
    } else {
        let r = vm.construct(&c, &[Value::Number(new_len as f64)])?;
        let t = match &r {
            Value::Object(t) if is_shared(t) => t.clone(),
            _ => {
                return Err(vm.type_error("Species constructor did not return a SharedArrayBuffer"));
            }
        };
        if Rc::ptr_eq(&t, &o) {
            return Err(
                vm.type_error("SharedArrayBuffer subclass returned this from species constructor")
            );
        }
        if buffer_len(&t) < new_len {
            return Err(vm.type_error("Species constructor returned a too small SharedArrayBuffer"));
        }
        t
    };
    if let (ObjKind::ArrayBuffer(src), ObjKind::ArrayBuffer(dst)) = (&o.kind, &target.kind) {
        let part = {
            let src = src.borrow();
            let end = fin.min(src.len());
            if first < end {
                src[first..end].to_vec()
            } else {
                Vec::new()
            }
        };
        let mut dst = dst.borrow_mut();
        let n = part.len().min(dst.len());
        dst[..n].copy_from_slice(&part[..n]);
    }
    Ok(Value::Object(target))
}

fn install_atomics(vm: &mut Vm) {
    let atomics = vm.new_object();
    let fns: [(&str, u32, NativeFn); 13] = [
        ("add", 3, atomics_rmw::<0>),
        ("and", 3, atomics_rmw::<1>),
        ("compareExchange", 4, atomics_compare_exchange),
        ("exchange", 3, atomics_rmw::<2>),
        ("isLockFree", 1, atomics_is_lock_free),
        ("load", 2, atomics_load),
        ("notify", 3, atomics_notify),
        ("or", 3, atomics_rmw::<3>),
        ("store", 3, atomics_store),
        ("sub", 3, atomics_rmw::<4>),
        ("wait", 4, atomics_wait),
        ("xor", 3, atomics_rmw::<5>),
        ("pause", 0, atomics_pause),
    ];
    for (name, arity, f) in fns {
        method(vm, &atomics, name, arity, f);
        atomics.props.borrow_mut().set_enumerable(name, false);
    }
    vm.define_global("Atomics", Value::Object(atomics));
}

fn integer_view(vm: &mut Vm, v: &Value, waitable: bool) -> Result<Rc<Obj>, Value> {
    let o = match v {
        Value::Object(o) if matches!(o.kind, ObjKind::Typed(_)) => o.clone(),
        _ => return Err(vm.type_error("Atomics operations require an integer TypedArray")),
    };
    let t = view_of(&o);
    if t.out_of_bounds() {
        return Err(vm.type_error("Atomics operation on a detached or out of bounds TypedArray"));
    }
    let ok = if waitable {
        matches!(t.kind, TypedKind::Int32 | TypedKind::BigInt64)
    } else {
        !matches!(
            t.kind,
            TypedKind::Uint8Clamped | TypedKind::Float16 | TypedKind::Float32 | TypedKind::Float64
        )
    };
    if !ok {
        return Err(vm.type_error("Atomics operation on an invalid TypedArray type"));
    }
    Ok(o)
}

fn atomic_index(vm: &mut Vm, o: &Obj, v: &Value) -> Result<usize, Value> {
    let i = to_index(vm, v, "atomic access index")?;
    if i >= view_of(o).len() {
        return Err(vm.range_error("Invalid atomic access index"));
    }
    Ok(i)
}

fn atomic_value(vm: &mut Vm, kind: TypedKind, v: &Value) -> R {
    if kind.is_big() {
        return Ok(Value::BigInt(super::number::to_bigint(vm, v)?));
    }
    let n = to_integer(vm, v)?;
    Ok(Value::Number(if n == 0.0 { 0.0 } else { n }))
}

fn revalidate(vm: &mut Vm, o: &Obj, i: usize) -> Result<(), Value> {
    let t = view_of(o);
    if t.out_of_bounds() {
        return Err(vm.type_error("Atomics operation on a detached or out of bounds TypedArray"));
    }
    if i >= t.len() {
        return Err(vm.range_error("Invalid atomic access index"));
    }
    Ok(())
}

fn atomics_load(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = integer_view(vm, &arg(args, 0), false)?;
    let i = atomic_index(vm, &o, &arg(args, 1))?;
    revalidate(vm, &o, i)?;
    Ok(get_el(&o, i))
}

fn atomics_store(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = integer_view(vm, &arg(args, 0), false)?;
    let i = atomic_index(vm, &o, &arg(args, 1))?;
    let kind = view_of(&o).kind;
    let v = atomic_value(vm, kind, &arg(args, 2))?;
    revalidate(vm, &o, i)?;
    view_of(&o).set(i, &v);
    Ok(v)
}

fn big_op(op: usize, a: &num_bigint::BigInt, b: &num_bigint::BigInt) -> num_bigint::BigInt {
    match op {
        0 => a + b,
        1 => a & b,
        2 => b.clone(),
        3 => a | b,
        4 => a - b,
        _ => a ^ b,
    }
}

fn num_op(op: usize, a: f64, b: f64) -> f64 {
    let (x, y) = (a as i64, b.rem_euclid(18446744073709551616.0) as u64 as i64);
    match op {
        0 => (x.wrapping_add(y)) as f64,
        1 => (x & y) as f64,
        2 => b,
        3 => (x | y) as f64,
        4 => (x.wrapping_sub(y)) as f64,
        _ => (x ^ y) as f64,
    }
}

fn atomics_rmw<const OP: usize>(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = integer_view(vm, &arg(args, 0), false)?;
    let i = atomic_index(vm, &o, &arg(args, 1))?;
    let kind = view_of(&o).kind;
    let v = coerce(vm, kind, &arg(args, 2))?;
    revalidate(vm, &o, i)?;
    let old = get_el(&o, i);
    let new = match (&old, &v) {
        (Value::BigInt(a), Value::BigInt(b)) => Value::BigInt(Rc::new(big_op(OP, a, b))),
        (Value::Number(a), Value::Number(b)) => {
            let b = if b.is_finite() { b.trunc() } else { 0.0 };
            Value::Number(num_op(OP, *a, b))
        }
        _ => v.clone(),
    };
    view_of(&o).set(i, &new);
    Ok(old)
}

fn atomics_compare_exchange(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = integer_view(vm, &arg(args, 0), false)?;
    let i = atomic_index(vm, &o, &arg(args, 1))?;
    let kind = view_of(&o).kind;
    let expected = coerce(vm, kind, &arg(args, 2))?;
    let replacement = coerce(vm, kind, &arg(args, 3))?;
    revalidate(vm, &o, i)?;
    let t = view_of(&o);
    let old = get_el(&o, i);
    let size = kind.size();
    let mut a = [0u8; 8];
    let mut b = [0u8; 8];
    kind.write_value(&mut a[..size], &expected, true);
    let cur = {
        let st = t.store().borrow();
        let s = t.offset + i * size;
        st[s..s + size].to_vec()
    };
    kind.write_value(&mut b[..size], &old, true);
    if cur[..] == a[..size] {
        t.set(i, &replacement);
    }
    Ok(old)
}

fn atomics_is_lock_free(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let n = to_integer(vm, &arg(args, 0))?;
    Ok(Value::Bool(matches!(n as i64, 1 | 2 | 4 | 8)))
}

fn atomics_notify(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = integer_view(vm, &arg(args, 0), true)?;
    let i = atomic_index(vm, &o, &arg(args, 1))?;
    let count = arg(args, 2);
    if !count.is_undefined() {
        let c = to_integer(vm, &count)?;
        let _ = c.max(0.0);
    }
    revalidate(vm, &o, i)?;
    Ok(Value::Number(0.0))
}

fn atomics_wait(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let o = integer_view(vm, &arg(args, 0), true)?;
    if !is_shared(&view_of(&o).buffer) {
        return Err(vm.type_error("Atomics.wait requires a shared typed array"));
    }
    let i = atomic_index(vm, &o, &arg(args, 1))?;
    let kind = view_of(&o).kind;
    let v = coerce(vm, kind, &arg(args, 2))?;
    let t = vm.to_number(&arg(args, 3))?;
    let timeout = if t.is_nan() {
        f64::INFINITY
    } else {
        t.max(0.0)
    };
    if !get_el(&o, i).strict_eq(&v) {
        return Ok("not-equal".into());
    }
    if timeout.is_finite() {
        std::thread::sleep(std::time::Duration::from_secs_f64(timeout / 1000.0));
        return Ok("timed-out".into());
    }
    Err(vm.type_error("Atomics.wait would block forever on the main thread"))
}

fn atomics_pause(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let n = arg(args, 0);
    if !n.is_undefined() {
        let ok = matches!(n, Value::Number(x) if x.fract() == 0.0 && x.is_finite());
        if !ok {
            return Err(vm.type_error("Atomics.pause expects an integer or undefined"));
        }
    }
    std::hint::spin_loop();
    Ok(Value::Undefined)
}
