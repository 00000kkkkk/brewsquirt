use super::{arg, method};
use crate::compiler::Mode;
use crate::object::{Obj, ObjKind};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use wasmi::{AsContext, AsContextMut};

type R = Result<Value, Value>;

pub const LAZY_GLOBALS: [&str; 1] = ["WebAssembly"];

pub struct Host {
    vm: *mut Vm,
}

struct MemEntry {
    memory: wasmi::Memory,
    buffer: Option<Rc<Obj>>,
    ptr: usize,
    len: usize,
}

pub struct WasmState {
    engine: wasmi::Engine,
    store: wasmi::Store<Host>,
    modules: Vec<wasmi::Module>,
    export_order: Vec<Vec<(String, u32)>>,
    funcs: Vec<(wasmi::Func, Option<Value>)>,
    memories: Vec<MemEntry>,
    tables: Vec<wasmi::Table>,
    globals: Vec<wasmi::Global>,
    externs: Vec<Value>,
    host_fns: Vec<Value>,
    active: Vec<*mut wasmi::Caller<'static, Host>>,
    pending: Option<Value>,
    errors: Option<[Value; 3]>,
    invoke: Option<Value>,
    handles: FxHashMap<String, usize>,
}

impl WasmState {
    fn new(vm: *mut Vm) -> WasmState {
        let mut config = wasmi::Config::default();
        config.compilation_mode(wasmi::CompilationMode::LazyTranslation);
        let engine = wasmi::Engine::new(&config);
        let store = wasmi::Store::new(&engine, Host { vm });
        WasmState {
            engine,
            store,
            modules: Vec::new(),
            export_order: Vec::new(),
            funcs: Vec::new(),
            memories: Vec::new(),
            tables: Vec::new(),
            globals: Vec::new(),
            externs: Vec::new(),
            host_fns: Vec::new(),
            active: Vec::new(),
            pending: None,
            errors: None,
            invoke: None,
            handles: FxHashMap::default(),
        }
    }

    fn ctx(&mut self) -> wasmi::StoreContextMut<'_, Host> {
        match self.active.last() {
            Some(&caller) => unsafe { (&mut *caller).as_context_mut() },
            None => self.store.as_context_mut(),
        }
    }

    fn view(&self) -> wasmi::StoreContext<'_, Host> {
        match self.active.last() {
            Some(&caller) => unsafe { (&*caller).as_context() },
            None => self.store.as_context(),
        }
    }
}

fn state(vm: &mut Vm) -> *mut WasmState {
    let ptr = vm as *mut Vm;
    let st = vm.wasm.get_or_insert_with(|| Box::new(WasmState::new(ptr)));
    if st.active.is_empty() {
        st.store.data_mut().vm = ptr;
    }
    &mut **st as *mut WasmState
}

#[derive(Clone, Copy)]
enum WasmError {
    Compile,
    Link,
    Runtime,
}

fn wasm_error(vm: &mut Vm, kind: WasmError, msg: &str) -> Value {
    let st = state(vm);
    let ctor = unsafe { (&mut *st).errors.as_ref() }.map(|e| e[kind as usize].clone());
    if let Some(ctor) = ctor {
        if let Ok(e) = vm.construct(&ctor, &[msg.into()]) {
            return e;
        }
    }
    vm.make_error(ErrorKind::Error, msg)
}

fn trap_message(code: wasmi::core::TrapCode) -> &'static str {
    use wasmi::core::TrapCode as T;
    match code {
        T::UnreachableCodeReached => "unreachable",
        T::MemoryOutOfBounds => "memory access out of bounds",
        T::TableOutOfBounds => "table index is out of bounds",
        T::IndirectCallToNull => "null function or function signature mismatch",
        T::IntegerDivisionByZero => "divide by zero",
        T::IntegerOverflow => "divide result unrepresentable",
        T::BadConversionToInteger => "float unrepresentable in integer range",
        T::StackOverflow => "Maximum call stack size exceeded",
        T::BadSignature => "null function or function signature mismatch",
        T::OutOfFuel => "out of fuel",
        T::GrowthOperationLimited => "memory or table growth limited",
    }
}

fn map_error(vm: &mut Vm, e: wasmi::Error, fallback: WasmError) -> Value {
    let st = state(vm);
    if let Some(pending) = unsafe { (&mut *st).pending.take() } {
        return pending;
    }
    if let Some(code) = e.as_trap_code() {
        if matches!(code, wasmi::core::TrapCode::StackOverflow) {
            return vm.range_error("Maximum call stack size exceeded");
        }
        return wasm_error(vm, WasmError::Runtime, trap_message(code));
    }
    let kind = match e.kind() {
        wasmi::errors::ErrorKind::Linker(_) | wasmi::errors::ErrorKind::Instantiation(_) => {
            WasmError::Link
        }
        wasmi::errors::ErrorKind::Wasm(_) | wasmi::errors::ErrorKind::Read(_) => WasmError::Compile,
        _ => fallback,
    };
    let text = e.to_string();
    wasm_error(vm, kind, &text)
}

fn sync_memories(vm: &mut Vm) {
    let Some(st) = vm.wasm.as_deref_mut() else {
        return;
    };
    let st = st as *mut WasmState;
    unsafe {
        for i in 0..(&mut *st).memories.len() {
            if (&mut *st).memories[i].buffer.is_none() {
                continue;
            }
            let memory = (&mut *st).memories[i].memory;
            let ctx = (&mut *st).ctx();
            let ptr = memory.data_ptr(&ctx) as usize;
            let len = memory.data_size(&ctx);
            let entry = &mut (&mut *st).memories[i];
            if entry.ptr != ptr || entry.len != len {
                if let Some(old) = entry.buffer.take() {
                    super::typed::detach(&old);
                }
            }
        }
    }
}

fn bytes_of(vm: &mut Vm, v: &Value) -> Result<Vec<u8>, Value> {
    match super::typed::buffer_source_bytes(v) {
        Some(b) => Ok(b),
        None => {
            Err(vm
                .type_error("WebAssembly expects a BufferSource (an ArrayBuffer or a typed array)"))
        }
    }
}

fn index_arg(vm: &mut Vm, args: &[Value], i: usize) -> Result<usize, Value> {
    Ok(vm.to_number(&arg(args, i))? as usize)
}

fn val_type_name(t: &wasmi::core::ValType) -> &'static str {
    use wasmi::core::ValType as V;
    match t {
        V::I32 => "i32",
        V::I64 => "i64",
        V::F32 => "f32",
        V::F64 => "f64",
        V::V128 => "v128",
        V::FuncRef => "anyfunc",
        V::ExternRef => "externref",
    }
}

fn parse_val_type(name: &str) -> Option<wasmi::core::ValType> {
    use wasmi::core::ValType as V;
    Some(match name {
        "i32" => V::I32,
        "i64" => V::I64,
        "f32" => V::F32,
        "f64" => V::F64,
        "anyfunc" | "funcref" => V::FuncRef,
        "externref" => V::ExternRef,
        _ => return None,
    })
}

fn func_wrapper(vm: &mut Vm, index: usize, name: Option<u32>) -> Value {
    let st = state(vm);
    if let Some(v) = unsafe { (&mut *st).funcs[index].1.clone() } {
        return v;
    }
    let invoke = unsafe { (&mut *st).invoke.clone() }.unwrap_or_default();
    let func = unsafe { (&mut *st).funcs[index].0 };
    let params = unsafe { func.ty((&mut *st).ctx()).params().len() };
    let bound = Obj::new(
        ObjKind::Bound(Box::new(crate::object::Bound {
            target: invoke,
            this: Value::Undefined,
            args: vec![Value::Number(index as f64)],
        })),
        Some(vm.realm.function_proto.clone()),
    );
    let attrs = crate::object::NON_ENUM | crate::object::READ_ONLY;
    let label = name.map_or(index as u64, u64::from).to_string();
    bound.set_with_attrs("name", label.into(), attrs);
    bound.set_with_attrs("length", Value::Number(params as f64), attrs);
    let v = Value::Object(bound);
    unsafe {
        (&mut *st).funcs[index].1 = Some(v.clone());
    }
    v
}

fn wrapper_index(vm: &mut Vm, v: &Value) -> Option<usize> {
    let Value::Object(o) = v else {
        return None;
    };
    let ObjKind::Bound(b) = &o.kind else {
        return None;
    };
    let st = state(vm);
    let invoke = unsafe { (&mut *st).invoke.clone() }?;
    if !b.target.strict_eq(&invoke) {
        return None;
    }
    match b.args.first() {
        Some(Value::Number(n)) => Some(*n as usize),
        _ => None,
    }
}

fn handle_key<T: std::fmt::Debug>(kind: char, h: &T) -> String {
    format!("{kind}{h:?}")
}

fn register_func(vm: &mut Vm, func: wasmi::Func) -> usize {
    let st = state(vm);
    let key = handle_key('f', &func);
    unsafe {
        if let Some(&i) = (&mut *st).handles.get(&key) {
            return i;
        }
        (&mut *st).funcs.push((func, None));
        let i = (&mut *st).funcs.len() - 1;
        (&mut *st).handles.insert(key, i);
        i
    }
}

fn register_memory(st: *mut WasmState, memory: wasmi::Memory) -> usize {
    let key = handle_key('m', &memory);
    unsafe {
        if let Some(&i) = (&mut *st).handles.get(&key) {
            return i;
        }
        (&mut *st).memories.push(MemEntry {
            memory,
            buffer: None,
            ptr: 0,
            len: 0,
        });
        let i = (&mut *st).memories.len() - 1;
        (&mut *st).handles.insert(key, i);
        i
    }
}

fn register_table(st: *mut WasmState, table: wasmi::Table) -> usize {
    let key = handle_key('t', &table);
    unsafe {
        if let Some(&i) = (&mut *st).handles.get(&key) {
            return i;
        }
        (&mut *st).tables.push(table);
        let i = (&mut *st).tables.len() - 1;
        (&mut *st).handles.insert(key, i);
        i
    }
}

fn register_global(st: *mut WasmState, global: wasmi::Global) -> usize {
    let key = handle_key('g', &global);
    unsafe {
        if let Some(&i) = (&mut *st).handles.get(&key) {
            return i;
        }
        (&mut *st).globals.push(global);
        let i = (&mut *st).globals.len() - 1;
        (&mut *st).handles.insert(key, i);
        i
    }
}

fn from_wasm(vm: &mut Vm, v: &wasmi::Val) -> R {
    Ok(match v {
        wasmi::Val::I32(n) => Value::Number(*n as f64),
        wasmi::Val::I64(n) => Value::BigInt(Rc::new(num_bigint::BigInt::from(*n))),
        wasmi::Val::F32(f) => Value::Number(f32::from(*f) as f64),
        wasmi::Val::F64(f) => Value::Number(f64::from(*f)),
        wasmi::Val::FuncRef(r) => match r.func() {
            Some(f) => {
                let i = register_func(vm, *f);
                func_wrapper(vm, i, None)
            }
            None => Value::Null,
        },
        wasmi::Val::ExternRef(r) => {
            let st = state(vm);
            let data = unsafe {
                r.data((&*st).view())
                    .and_then(|d| d.downcast_ref::<u32>().copied())
            };
            match data {
                Some(i) => unsafe {
                    (&mut *st)
                        .externs
                        .get(i as usize)
                        .cloned()
                        .unwrap_or_default()
                },
                None => Value::Null,
            }
        }
        wasmi::Val::V128(_) => {
            return Err(vm.type_error("type incompatibility when transforming from/to JS"));
        }
    })
}

fn to_wasm(vm: &mut Vm, v: &Value, t: &wasmi::core::ValType) -> Result<wasmi::Val, Value> {
    use wasmi::core::ValType as V;
    Ok(match t {
        V::I32 => wasmi::Val::I32(crate::number::to_int32(vm.to_number(v)?)),
        V::I64 => {
            let b = super::number::to_bigint(vm, v)?;
            let n = crate::bigint::as_int_n(64, &b);
            wasmi::Val::I64(num_traits::ToPrimitive::to_i64(&n).unwrap_or(0))
        }
        V::F32 => wasmi::Val::F32((vm.to_number(v)? as f32).into()),
        V::F64 => wasmi::Val::F64(vm.to_number(v)?.into()),
        V::FuncRef => {
            if matches!(v, Value::Null) {
                wasmi::Val::FuncRef(wasmi::FuncRef::null())
            } else if let Some(i) = wrapper_index(vm, v) {
                let st = state(vm);
                wasmi::Val::FuncRef(wasmi::FuncRef::new(unsafe { (&mut *st).funcs[i].0 }))
            } else {
                return Err(vm.type_error(
                    "type incompatibility when transforming from/to JS: an exported WebAssembly function or null is required",
                ));
            }
        }
        V::ExternRef => {
            if matches!(v, Value::Null) {
                wasmi::Val::ExternRef(wasmi::ExternRef::null())
            } else {
                let st = state(vm);
                unsafe {
                    (&mut *st).externs.push(v.clone());
                    let idx = ((&mut *st).externs.len() - 1) as u32;
                    wasmi::Val::ExternRef(wasmi::ExternRef::new::<u32>((&mut *st).ctx(), idx))
                }
            }
        }
        V::V128 => return Err(vm.type_error("type incompatibility when transforming from/to JS")),
    })
}

fn read_uleb(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let b = *bytes.get(*pos)?;
        *pos += 1;
        result |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return u32::try_from(result).ok();
        }
        shift += 7;
        if shift > 35 {
            return None;
        }
    }
}

fn parse_exports(bytes: &[u8]) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let mut pos = 8;
    while pos < bytes.len() {
        let id = bytes[pos];
        pos += 1;
        let Some(size) = read_uleb(bytes, &mut pos) else {
            break;
        };
        let end = pos + size as usize;
        if id == 7 {
            let mut p = pos;
            let count = read_uleb(bytes, &mut p).unwrap_or(0);
            for _ in 0..count {
                let Some(len) = read_uleb(bytes, &mut p) else {
                    break;
                };
                let Some(name) = bytes.get(p..p + len as usize) else {
                    break;
                };
                let name = String::from_utf8_lossy(name).into_owned();
                p += len as usize + 1;
                let Some(index) = read_uleb(bytes, &mut p) else {
                    break;
                };
                out.push((name, index));
            }
            break;
        }
        pos = end;
    }
    out
}

fn wasm_validate(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bytes = bytes_of(vm, &arg(args, 0))?;
    let st = state(vm);
    let ok = unsafe { wasmi::Module::validate(&(&mut *st).engine, &bytes).is_ok() };
    Ok(Value::Bool(ok))
}

fn wasm_compile(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bytes = bytes_of(vm, &arg(args, 0))?;
    let st = state(vm);
    let module = unsafe { wasmi::Module::new(&(&mut *st).engine, &bytes) };
    match module {
        Ok(m) => unsafe {
            (&mut *st).modules.push(m);
            (&mut *st).export_order.push(parse_exports(&bytes));
            Ok(Value::Number(((&mut *st).modules.len() - 1) as f64))
        },
        Err(e) => {
            let text = e.to_string();
            Err(wasm_error(
                vm,
                WasmError::Compile,
                &format!("WebAssembly.Module(): {text}"),
            ))
        }
    }
}

fn kind_name(t: &wasmi::ExternType) -> &'static str {
    match t {
        wasmi::ExternType::Func(_) => "function",
        wasmi::ExternType::Table(_) => "table",
        wasmi::ExternType::Memory(_) => "memory",
        wasmi::ExternType::Global(_) => "global",
    }
}

fn wasm_module_imports(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = index_arg(vm, args, 0)?;
    let st = state(vm);
    let items: Vec<(String, String, &'static str, String)> = unsafe {
        let Some(m) = (&mut *st).modules.get(id) else {
            return Err(vm.type_error("Argument 0 must be a WebAssembly.Module"));
        };
        m.imports()
            .map(|i| {
                let detail = match i.ty() {
                    wasmi::ExternType::Global(g) => val_type_name(&g.content()).to_string(),
                    wasmi::ExternType::Table(t) => val_type_name(&t.element()).to_string(),
                    _ => String::new(),
                };
                (
                    i.module().to_string(),
                    i.name().to_string(),
                    kind_name(i.ty()),
                    detail,
                )
            })
            .collect()
    };
    let list = items
        .into_iter()
        .map(|(m, n, k, d)| vm.new_array(vec![m.into(), n.into(), k.into(), d.into()]))
        .collect();
    Ok(vm.new_array(list))
}

fn wasm_module_exports(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = index_arg(vm, args, 0)?;
    let st = state(vm);
    let items: Vec<(String, &'static str)> = unsafe {
        let Some(m) = (&mut *st).modules.get(id) else {
            return Err(vm.type_error("Argument 0 must be a WebAssembly.Module"));
        };
        (&*st).export_order[id]
            .iter()
            .filter_map(|(name, _)| m.get_export(name).map(|t| (name.clone(), kind_name(&t))))
            .collect()
    };
    let list = items
        .into_iter()
        .map(|(n, k)| vm.new_array(vec![n.into(), k.into()]))
        .collect();
    Ok(vm.new_array(list))
}

fn wasm_custom_sections(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = index_arg(vm, args, 0)?;
    let name = vm.to_string(&arg(args, 1))?;
    let st = state(vm);
    let sections: Vec<Vec<u8>> = unsafe {
        let Some(m) = (&mut *st).modules.get(id) else {
            return Err(vm.type_error("Argument 0 must be a WebAssembly.Module"));
        };
        m.custom_sections()
            .filter(|s| s.name() == &*name)
            .map(|s| s.data().to_vec())
            .collect()
    };
    let list = sections
        .into_iter()
        .map(|b| Value::Object(super::typed::new_buffer(vm, b)))
        .collect();
    Ok(vm.new_array(list))
}

fn host_func(vm: &mut Vm, js: Value, ty: wasmi::FuncType) -> wasmi::Func {
    let st = state(vm);
    let slot = unsafe {
        (&mut *st).host_fns.push(js);
        (&mut *st).host_fns.len() - 1
    };
    let ctx = unsafe { (&mut *st).ctx() };
    wasmi::Func::new(
        ctx,
        ty,
        move |mut caller: wasmi::Caller<'_, Host>,
              params: &[wasmi::Val],
              results: &mut [wasmi::Val]|
              -> Result<(), wasmi::Error> {
            let vm = unsafe { &mut *caller.data().vm };
            let st = state(vm);
            unsafe {
                (&mut *st)
                    .active
                    .push(std::ptr::from_mut(&mut caller).cast());
            }
            sync_memories(vm);
            let outcome = (|| -> Result<(), Value> {
                let mut js_args = Vec::with_capacity(params.len());
                for p in params {
                    js_args.push(from_wasm(vm, p)?);
                }
                let f = unsafe { (&mut *st).host_fns[slot].clone() };
                let ret = vm.call_value(&f, Value::Undefined, &js_args)?;
                match results.len() {
                    0 => {}
                    1 => {
                        let t = results[0].ty();
                        results[0] = to_wasm(vm, &ret, &t)?;
                    }
                    n => {
                        let items = vm.iterate(&ret)?;
                        if items.len() != n {
                            return Err(vm.type_error("multi-return length mismatch"));
                        }
                        for i in 0..n {
                            let t = results[i].ty();
                            results[i] = to_wasm(vm, &items[i], &t)?;
                        }
                    }
                }
                Ok(())
            })();
            unsafe {
                (&mut *st).active.pop();
            }
            match outcome {
                Ok(()) => Ok(()),
                Err(e) => {
                    unsafe {
                        (&mut *st).pending = Some(e);
                    }
                    Err(wasmi::Error::new("JavaScript exception"))
                }
            }
        },
    )
}

fn wasm_instantiate(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = index_arg(vm, args, 0)?;
    let values = vm.iterate(&arg(args, 1))?;
    let st = state(vm);
    let module = match unsafe { (&mut *st).modules.get(id).cloned() } {
        Some(m) => m,
        None => return Err(vm.type_error("Argument 0 must be a WebAssembly.Module")),
    };
    let imports: Vec<wasmi::ExternType> = module.imports().map(|i| i.ty().clone()).collect();
    let mut externs = Vec::with_capacity(imports.len());
    for (i, ty) in imports.iter().enumerate() {
        let v = values.get(i).cloned().unwrap_or_default();
        let ext = match ty {
            wasmi::ExternType::Func(fty) => {
                if let Some(idx) = wrapper_index(vm, &v) {
                    wasmi::Extern::Func(unsafe { (&mut *st).funcs[idx].0 })
                } else if v.is_callable() {
                    wasmi::Extern::Func(host_func(vm, v, fty.clone()))
                } else {
                    return Err(wasm_error(
                        vm,
                        WasmError::Link,
                        &format!(
                            "WebAssembly.Instance(): Import #{i}: function import requires a callable"
                        ),
                    ));
                }
            }
            wasmi::ExternType::Memory(_) => {
                let idx = vm.to_number(&v)?;
                match unsafe { (&mut *st).memories.get(idx as usize) } {
                    Some(m) if idx >= 0.0 => wasmi::Extern::Memory(m.memory),
                    _ => {
                        return Err(wasm_error(
                            vm,
                            WasmError::Link,
                            &format!(
                                "WebAssembly.Instance(): Import #{i}: memory import must be a WebAssembly.Memory object"
                            ),
                        ));
                    }
                }
            }
            wasmi::ExternType::Table(_) => {
                let idx = vm.to_number(&v)?;
                match unsafe { (&mut *st).tables.get(idx as usize) } {
                    Some(t) if idx >= 0.0 => wasmi::Extern::Table(*t),
                    _ => {
                        return Err(wasm_error(
                            vm,
                            WasmError::Link,
                            &format!(
                                "WebAssembly.Instance(): Import #{i}: table import requires a WebAssembly.Table"
                            ),
                        ));
                    }
                }
            }
            wasmi::ExternType::Global(gty) => {
                let idx = vm.to_number(&v)?;
                match unsafe { (&mut *st).globals.get(idx as usize) } {
                    Some(g) if idx >= 0.0 => wasmi::Extern::Global(*g),
                    _ => {
                        return Err(wasm_error(
                            vm,
                            WasmError::Link,
                            &format!(
                                "WebAssembly.Instance(): Import #{i}: global import must be a number, valid Wasm reference, or WebAssembly.Global object ({})",
                                val_type_name(&gty.content())
                            ),
                        ));
                    }
                }
            }
        };
        externs.push(ext);
    }
    let instance = unsafe { wasmi::Instance::new((&mut *st).ctx(), &module, &externs) };
    sync_memories(vm);
    let instance = match instance {
        Ok(i) => i,
        Err(e) => return Err(map_error(vm, e, WasmError::Link)),
    };
    let exports: Vec<(String, u32, wasmi::Extern)> = unsafe {
        let order = (&*st).export_order[id].clone();
        order
            .into_iter()
            .filter_map(|(name, index)| {
                instance
                    .get_export((&*st).view(), &name)
                    .map(|e| (name, index, e))
            })
            .collect()
    };
    let mut list = Vec::with_capacity(exports.len());
    for (name, index, ext) in exports {
        let (kind, value) = match ext {
            wasmi::Extern::Func(f) => {
                let i = register_func(vm, f);
                ("function", func_wrapper(vm, i, Some(index)))
            }
            wasmi::Extern::Memory(m) => ("memory", Value::Number(register_memory(st, m) as f64)),
            wasmi::Extern::Table(t) => ("table", Value::Number(register_table(st, t) as f64)),
            wasmi::Extern::Global(g) => ("global", Value::Number(register_global(st, g) as f64)),
        };
        list.push(vm.new_array(vec![name.into(), kind.into(), value]));
    }
    Ok(vm.new_array(list))
}

fn wasm_invoke(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let index = index_arg(vm, args, 0)?;
    let rest = args.get(1..).unwrap_or(&[]);
    let st = state(vm);
    let func = match unsafe { (&mut *st).funcs.get(index) } {
        Some((f, _)) => *f,
        None => return Err(vm.type_error("not a WebAssembly function")),
    };
    let ty = unsafe { func.ty((&mut *st).ctx()) };
    let mut params = Vec::with_capacity(ty.params().len());
    for (i, t) in ty.params().iter().enumerate() {
        let v = rest.get(i).cloned().unwrap_or_default();
        params.push(to_wasm(vm, &v, t)?);
    }
    let mut results: Vec<wasmi::Val> = ty
        .results()
        .iter()
        .map(|t| wasmi::Val::default(*t))
        .collect();
    let r = unsafe { func.call((&mut *st).ctx(), &params, &mut results) };
    sync_memories(vm);
    if let Err(e) = r {
        return Err(map_error(vm, e, WasmError::Runtime));
    }
    match results.len() {
        0 => Ok(Value::Undefined),
        1 => from_wasm(vm, &results[0]),
        _ => {
            let mut out = Vec::with_capacity(results.len());
            for v in &results {
                out.push(from_wasm(vm, v)?);
            }
            Ok(vm.new_array(out))
        }
    }
}

fn limits(vm: &mut Vm, args: &[Value]) -> Result<(u32, Option<u32>), Value> {
    let initial = vm.to_number(&arg(args, 0))?;
    let maximum = match arg(args, 1) {
        Value::Undefined | Value::Null => None,
        v => Some(vm.to_number(&v)?),
    };
    if !(0.0..=4294967295.0).contains(&initial) || maximum.is_some_and(|m| m < initial) {
        return Err(vm.range_error("WebAssembly: invalid initial or maximum size"));
    }
    Ok((initial as u32, maximum.map(|m| m.min(4294967295.0) as u32)))
}

fn wasm_memory_new(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (initial, maximum) = limits(vm, args)?;
    if initial > 65536 || maximum.is_some_and(|m| m > 65536) {
        return Err(vm.range_error(
            "WebAssembly.Memory(): Property 'initial': value is above the upper bound 65536",
        ));
    }
    let st = state(vm);
    let memory =
        unsafe { wasmi::Memory::new((&mut *st).ctx(), wasmi::MemoryType::new(initial, maximum)) };
    match memory {
        Ok(m) => Ok(Value::Number(register_memory(st, m) as f64)),
        Err(e) => {
            let text = e.to_string();
            Err(vm.range_error(format!(
                "WebAssembly.Memory(): could not allocate memory: {text}"
            )))
        }
    }
}

fn wasm_memory_buffer(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = index_arg(vm, args, 0)?;
    let st = state(vm);
    let (ptr, len, cached) = unsafe {
        let Some(entry) = (&mut *st).memories.get(id) else {
            return Err(vm.type_error("Receiver is not a WebAssembly.Memory"));
        };
        let memory = entry.memory;
        let ctx = (&mut *st).ctx();
        let ptr = memory.data_ptr(&ctx);
        let len = memory.data_size(&ctx);
        let entry = &(&mut *st).memories[id];
        let cached = entry
            .buffer
            .clone()
            .filter(|_| entry.ptr == ptr as usize && entry.len == len);
        (ptr, len, cached)
    };
    if let Some(b) = cached {
        return Ok(Value::Object(b));
    }
    unsafe {
        if let Some(old) = (&mut *st).memories[id].buffer.take() {
            super::typed::detach(&old);
        }
    }
    let buffer = unsafe { super::typed::new_external_buffer(vm, ptr, len) };
    unsafe {
        let entry = &mut (&mut *st).memories[id];
        entry.buffer = Some(buffer.clone());
        entry.ptr = ptr as usize;
        entry.len = len;
    }
    Ok(Value::Object(buffer))
}

fn wasm_memory_grow(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = index_arg(vm, args, 0)?;
    let delta = vm.to_number(&arg(args, 1))?;
    if !(0.0..=65536.0).contains(&delta) {
        return Err(vm.range_error("WebAssembly.Memory.grow(): Maximum memory size exceeded"));
    }
    let st = state(vm);
    let r = unsafe {
        let Some(entry) = (&mut *st).memories.get(id) else {
            return Err(vm.type_error("Receiver is not a WebAssembly.Memory"));
        };
        let memory = entry.memory;
        memory.grow((&mut *st).ctx(), delta as u64)
    };
    unsafe {
        if let Some(old) = (&mut *st).memories[id].buffer.take() {
            super::typed::detach(&old);
        }
    }
    match r {
        Ok(old) => Ok(Value::Number(old as f64)),
        Err(_) => Err(vm.range_error("WebAssembly.Memory.grow(): Maximum memory size exceeded")),
    }
}

fn wasm_memory_pages(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let id = index_arg(vm, args, 0)?;
    let st = state(vm);
    unsafe {
        let Some(entry) = (&mut *st).memories.get(id) else {
            return Err(vm.type_error("Receiver is not a WebAssembly.Memory"));
        };
        let memory = entry.memory;
        let ctx = (&mut *st).ctx();
        let ty = memory.ty(&ctx);
        let size = memory.size(&ctx);
        let max = match ty.maximum() {
            Some(m) => Value::Number(m as f64),
            None => Value::Undefined,
        };
        Ok(vm.new_array(vec![
            Value::Number(size as f64),
            Value::Number(ty.minimum() as f64),
            max,
        ]))
    }
}

fn element_type(vm: &mut Vm, v: &Value) -> Result<wasmi::core::ValType, Value> {
    let name = vm.to_string(v)?;
    match parse_val_type(&name) {
        Some(t @ (wasmi::core::ValType::FuncRef | wasmi::core::ValType::ExternRef)) => Ok(t),
        _ => Err(vm.type_error(
            "WebAssembly.Table(): Descriptor property 'element' must be a WebAssembly reference type",
        )),
    }
}

fn wasm_table_new(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let element = element_type(vm, &arg(args, 0))?;
    let (initial, maximum) = limits(vm, &args[1.min(args.len())..])?;
    let init = to_wasm(vm, &arg(args, 3), &element)?;
    let st = state(vm);
    let table = unsafe {
        wasmi::Table::new(
            (&mut *st).ctx(),
            wasmi::TableType::new(element, initial, maximum),
            init,
        )
    };
    match table {
        Ok(t) => Ok(Value::Number(register_table(st, t) as f64)),
        Err(e) => {
            let text = e.to_string();
            Err(vm.range_error(format!("WebAssembly.Table(): {text}")))
        }
    }
}

fn table_of(vm: &mut Vm, args: &[Value]) -> Result<(*mut WasmState, wasmi::Table), Value> {
    let id = index_arg(vm, args, 0)?;
    let st = state(vm);
    match unsafe { (&mut *st).tables.get(id) } {
        Some(t) => Ok((st, *t)),
        None => Err(vm.type_error("Receiver is not a WebAssembly.Table")),
    }
}

fn wasm_table_get(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (st, table) = table_of(vm, args)?;
    let index = vm.to_number(&arg(args, 1))?;
    let v = unsafe { table.get((&mut *st).ctx(), index as u64) };
    match v {
        Some(v) if index >= 0.0 => from_wasm(vm, &v),
        _ => Err(vm.range_error("WebAssembly.Table.get(): invalid address")),
    }
}

fn wasm_table_set(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (st, table) = table_of(vm, args)?;
    let index = vm.to_number(&arg(args, 1))?;
    let element = unsafe { table.ty((&mut *st).ctx()).element() };
    let v = to_wasm(vm, &arg(args, 2), &element)?;
    let r = unsafe { table.set((&mut *st).ctx(), index as u64, v) };
    match r {
        Ok(()) if index >= 0.0 => Ok(Value::Undefined),
        _ => Err(vm.range_error("WebAssembly.Table.set(): invalid address")),
    }
}

fn wasm_table_grow(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (st, table) = table_of(vm, args)?;
    let delta = vm.to_number(&arg(args, 1))?;
    let element = unsafe { table.ty((&mut *st).ctx()).element() };
    let init = to_wasm(vm, &arg(args, 2), &element)?;
    let r = unsafe { table.grow((&mut *st).ctx(), delta.max(0.0) as u64, init) };
    match r {
        Ok(old) => Ok(Value::Number(old as f64)),
        Err(_) => Err(vm.range_error("WebAssembly.Table.grow(): failed to grow table")),
    }
}

fn wasm_table_size(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (st, table) = table_of(vm, args)?;
    Ok(Value::Number(unsafe { table.size((&mut *st).ctx()) } as f64))
}

fn wasm_global_new(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let name = vm.to_string(&arg(args, 0))?;
    let Some(t) = parse_val_type(&name) else {
        return Err(vm.type_error(
            "WebAssembly.Global(): Descriptor property 'value' must be a WebAssembly type",
        ));
    };
    let mutable = arg(args, 1).truthy();
    let init = match arg(args, 2) {
        Value::Undefined => wasmi::Val::default(t),
        v => to_wasm(vm, &v, &t)?,
    };
    let st = state(vm);
    let mutability = if mutable {
        wasmi::Mutability::Var
    } else {
        wasmi::Mutability::Const
    };
    let g = unsafe { wasmi::Global::new((&mut *st).ctx(), init, mutability) };
    Ok(Value::Number(register_global(st, g) as f64))
}

fn global_of(vm: &mut Vm, args: &[Value]) -> Result<(*mut WasmState, wasmi::Global), Value> {
    let id = index_arg(vm, args, 0)?;
    let st = state(vm);
    match unsafe { (&mut *st).globals.get(id) } {
        Some(g) => Ok((st, *g)),
        None => Err(vm.type_error("Receiver is not a WebAssembly.Global")),
    }
}

fn wasm_global_get(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (st, global) = global_of(vm, args)?;
    let v = unsafe { global.get((&mut *st).ctx()) };
    from_wasm(vm, &v)
}

fn wasm_global_set(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (st, global) = global_of(vm, args)?;
    let ty = unsafe { global.ty((&mut *st).ctx()) };
    if matches!(ty.mutability(), wasmi::Mutability::Const) {
        return Err(
            vm.type_error("WebAssembly.Global.value: Can't set the value of an immutable global.")
        );
    }
    let v = to_wasm(vm, &arg(args, 1), &ty.content())?;
    let r = unsafe { global.set((&mut *st).ctx(), v) };
    match r {
        Ok(()) => Ok(Value::Undefined),
        Err(e) => {
            let text = e.to_string();
            Err(vm.type_error(text))
        }
    }
}

fn wasm_global_type(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let (st, global) = global_of(vm, args)?;
    let ty = unsafe { global.ty((&mut *st).ctx()) };
    Ok(vm.new_array(vec![
        val_type_name(&ty.content()).into(),
        Value::Bool(matches!(ty.mutability(), wasmi::Mutability::Var)),
    ]))
}

fn wasm_register(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let st = state(vm);
    unsafe {
        (&mut *st).errors = Some([arg(args, 0), arg(args, 1), arg(args, 2)]);
        (&mut *st).invoke = Some(arg(args, 3));
    }
    Ok(Value::Undefined)
}

const WASM_SRC: &str = r##"
(native) => {
  class CompileError extends Error {
    constructor(message) {
      super(message)
      this.name = "CompileError"
    }
  }
  class LinkError extends Error {
    constructor(message) {
      super(message)
      this.name = "LinkError"
    }
  }
  class RuntimeError extends Error {
    constructor(message) {
      super(message)
      this.name = "RuntimeError"
    }
  }
  native.register(CompileError, LinkError, RuntimeError, native.invoke)

  const kId = Symbol("wasmId")
  const isBufferSource = (v) => v instanceof ArrayBuffer || ArrayBuffer.isView(v) || (typeof SharedArrayBuffer === "function" && v instanceof SharedArrayBuffer)
  const requireBytes = (v, what) => {
    if (!isBufferSource(v)) throw new TypeError(`${what}: Argument 0 must be a buffer source`)
    return v
  }

  class Module {
    constructor(bytes) {
      if (new.target === undefined) throw new TypeError("WebAssembly.Module must be invoked with 'new'")
      this[kId] = native.compile(requireBytes(bytes, "WebAssembly.Module()"))
    }
    static imports(module) {
      if (!(module instanceof Module)) throw new TypeError("WebAssembly.Module.imports(): Argument 0 must be a WebAssembly.Module")
      return native.moduleImports(module[kId]).map(([module, name, kind]) => ({ module, name, kind }))
    }
    static exports(module) {
      if (!(module instanceof Module)) throw new TypeError("WebAssembly.Module.exports(): Argument 0 must be a WebAssembly.Module")
      return native.moduleExports(module[kId]).map(([name, kind]) => ({ name, kind }))
    }
    static customSections(module, name) {
      if (!(module instanceof Module)) throw new TypeError("WebAssembly.Module.customSections(): Argument 0 must be a WebAssembly.Module")
      if (name === undefined) throw new TypeError("WebAssembly.Module.customSections(): Argument 1 is required")
      return native.customSections(module[kId], String(name))
    }
  }
  Object.defineProperty(Module.prototype, Symbol.toStringTag, { value: "WebAssembly.Module", configurable: true })

  const memories = new Map()
  class Memory {
    constructor(descriptor) {
      if (new.target === undefined) throw new TypeError("WebAssembly.Memory must be invoked with 'new'")
      if (descriptor === null || typeof descriptor !== "object") throw new TypeError("WebAssembly.Memory(): Argument 0 must be a memory descriptor")
      const initial = descriptor.initial ?? descriptor.minimum
      if (initial === undefined) throw new TypeError("WebAssembly.Memory(): Property 'initial' is required")
      if (descriptor.shared) throw new TypeError("WebAssembly.Memory(): shared memories are not supported in this runtime")
      this[kId] = native.memoryNew(initial, descriptor.maximum)
      memories.set(this[kId], this)
    }
    get buffer() {
      return native.memoryBuffer(this[kId])
    }
    grow(delta) {
      return native.memoryGrow(this[kId], delta)
    }
    static [Symbol.for("jsq.fromId")](id) {
      let m = memories.get(id)
      if (!m) {
        m = Object.create(Memory.prototype)
        m[kId] = id
        memories.set(id, m)
      }
      return m
    }
  }
  Object.defineProperty(Memory.prototype, Symbol.toStringTag, { value: "WebAssembly.Memory", configurable: true })

  const tables = new Map()
  class Table {
    constructor(descriptor, value) {
      if (new.target === undefined) throw new TypeError("WebAssembly.Table must be invoked with 'new'")
      if (descriptor === null || typeof descriptor !== "object") throw new TypeError("WebAssembly.Table(): Argument 0 must be a table descriptor")
      const initial = descriptor.initial ?? descriptor.minimum
      if (initial === undefined) throw new TypeError("WebAssembly.Table(): Property 'initial' is required")
      this[kId] = native.tableNew(String(descriptor.element), initial, descriptor.maximum, value === undefined ? null : value)
      tables.set(this[kId], this)
    }
    get length() {
      return native.tableSize(this[kId])
    }
    get(index) {
      return native.tableGet(this[kId], index)
    }
    set(index, value = null) {
      native.tableSet(this[kId], index, value)
    }
    grow(delta, value = null) {
      return native.tableGrow(this[kId], delta, value)
    }
    static [Symbol.for("jsq.fromId")](id) {
      let t = tables.get(id)
      if (!t) {
        t = Object.create(Table.prototype)
        t[kId] = id
        tables.set(id, t)
      }
      return t
    }
  }
  Object.defineProperty(Table.prototype, Symbol.toStringTag, { value: "WebAssembly.Table", configurable: true })

  const globals = new Map()
  class Global {
    constructor(descriptor, value) {
      if (new.target === undefined) throw new TypeError("WebAssembly.Global must be invoked with 'new'")
      if (descriptor === null || typeof descriptor !== "object") throw new TypeError("WebAssembly.Global(): Argument 0 must be a global descriptor")
      this[kId] = native.globalNew(String(descriptor.value), !!descriptor.mutable, value)
      globals.set(this[kId], this)
    }
    get value() {
      return native.globalGet(this[kId])
    }
    set value(v) {
      native.globalSet(this[kId], v)
    }
    valueOf() {
      return native.globalGet(this[kId])
    }
    static [Symbol.for("jsq.fromId")](id) {
      let g = globals.get(id)
      if (!g) {
        g = Object.create(Global.prototype)
        g[kId] = id
        globals.set(id, g)
      }
      return g
    }
  }
  Object.defineProperty(Global.prototype, Symbol.toStringTag, { value: "WebAssembly.Global", configurable: true })

  const fromId = Symbol.for("jsq.fromId")
  const importValue = (descriptor, value) => {
    const [module, name, kind, detail] = descriptor
    if (kind === "function") {
      if (typeof value !== "function") throw new LinkError(`WebAssembly.Instance(): Import #${module}.${name}: function import requires a callable`)
      return value
    }
    if (kind === "memory") {
      if (!(value instanceof Memory)) throw new LinkError(`WebAssembly.Instance(): Import ${module}.${name}: memory import must be a WebAssembly.Memory object`)
      return value[kId]
    }
    if (kind === "table") {
      if (!(value instanceof Table)) throw new LinkError(`WebAssembly.Instance(): Import ${module}.${name}: table import requires a WebAssembly.Table`)
      return value[kId]
    }
    if (value instanceof Global) return value[kId]
    if (typeof value === "number" || typeof value === "bigint" || value === null || typeof value === "function" || detail === "externref") {
      return native.globalNew(detail, false, value)
    }
    throw new LinkError(`WebAssembly.Instance(): Import ${module}.${name}: global import must be a number, valid Wasm reference, or WebAssembly.Global object`)
  }

  class Instance {
    constructor(module, importObject) {
      if (new.target === undefined) throw new TypeError("WebAssembly.Instance must be invoked with 'new'")
      if (!(module instanceof Module)) throw new TypeError("WebAssembly.Instance(): Argument 0 must be a WebAssembly.Module")
      const descriptors = native.moduleImports(module[kId])
      if (descriptors.length > 0 && (importObject === undefined || importObject === null || typeof importObject !== "object")) {
        throw new TypeError("WebAssembly.Instance(): Imports argument must be present and must be an object")
      }
      const values = descriptors.map((d) => {
        const ns = importObject[d[0]]
        if (ns === null || (typeof ns !== "object" && typeof ns !== "function")) throw new TypeError(`WebAssembly.Instance(): Import #0 "${d[0]}": module is not an object or function`)
        return importValue(d, ns[d[1]])
      })
      const exports = Object.create(null)
      for (const [name, kind, value] of native.instantiate(module[kId], values)) {
        if (kind === "function") exports[name] = value
        else if (kind === "memory") exports[name] = Memory[fromId](value)
        else if (kind === "table") exports[name] = Table[fromId](value)
        else exports[name] = Global[fromId](value)
      }
      Object.freeze(exports)
      Object.defineProperty(this, "exports", { value: exports, enumerable: true, configurable: true })
    }
  }
  Object.defineProperty(Instance.prototype, Symbol.toStringTag, { value: "WebAssembly.Instance", configurable: true })

  const toBytes = async (source) => {
    const resolved = await source
    if (resolved && typeof resolved.arrayBuffer === "function" && !isBufferSource(resolved)) {
      const type = resolved.headers?.get?.("content-type")
      if (type && !type.startsWith("application/wasm")) throw new TypeError(`WebAssembly.instantiateStreaming(): Incorrect response MIME type. Expected 'application/wasm'.`)
      return new Uint8Array(await resolved.arrayBuffer())
    }
    return requireBytes(resolved, "WebAssembly.compileStreaming()")
  }

  const WebAssembly = {
    async compile(bytes) {
      return new Module(requireBytes(bytes, "WebAssembly.compile()"))
    },
    validate(bytes) {
      return native.validate(requireBytes(bytes, "WebAssembly.validate()"))
    },
    async instantiate(source, importObject) {
      if (source instanceof Module) return new Instance(source, importObject)
      const module = new Module(requireBytes(source, "WebAssembly.instantiate()"))
      return { module, instance: new Instance(module, importObject) }
    },
    async compileStreaming(source) {
      return new Module(await toBytes(source))
    },
    async instantiateStreaming(source, importObject) {
      const module = new Module(await toBytes(source))
      return { module, instance: new Instance(module, importObject) }
    },
    promising(fn) {
      if (typeof fn !== "function") throw new TypeError("WebAssembly.promising(): Argument 0 must be a WebAssembly exported function")
      return async (...args) => fn(...args)
    },
  }
  for (const [key, value] of Object.entries({ Module, Instance, Table, Memory, Global, CompileError, LinkError, RuntimeError })) {
    Object.defineProperty(WebAssembly, key, { value, writable: true, configurable: true, enumerable: false })
  }
  Object.defineProperty(WebAssembly, Symbol.toStringTag, { value: "WebAssembly", configurable: true })
  return WebAssembly
}
"##;

pub fn load(vm: &mut Vm) -> Result<Value, Value> {
    if let Some(v) = vm.globals.get("WebAssembly").cloned() {
        return Ok(v);
    }
    let dir = std::env::current_dir().unwrap_or_default();
    let proto = match vm.compile(WASM_SRC, "<squirt:wasm>", dir, Mode::Repl) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<squirt:wasm>")),
    };
    let factory = vm.run_proto(proto)?;
    let n = vm.new_object();
    let natives: [(&str, u32, crate::object::NativeFn); 21] = [
        ("register", 4, wasm_register),
        ("invoke", 1, wasm_invoke),
        ("validate", 1, wasm_validate),
        ("compile", 1, wasm_compile),
        ("moduleImports", 1, wasm_module_imports),
        ("moduleExports", 1, wasm_module_exports),
        ("customSections", 2, wasm_custom_sections),
        ("instantiate", 2, wasm_instantiate),
        ("memoryNew", 2, wasm_memory_new),
        ("memoryBuffer", 1, wasm_memory_buffer),
        ("memoryGrow", 2, wasm_memory_grow),
        ("memoryPages", 1, wasm_memory_pages),
        ("tableNew", 4, wasm_table_new),
        ("tableGet", 2, wasm_table_get),
        ("tableSet", 3, wasm_table_set),
        ("tableGrow", 3, wasm_table_grow),
        ("tableSize", 1, wasm_table_size),
        ("globalNew", 3, wasm_global_new),
        ("globalGet", 1, wasm_global_get),
        ("globalSet", 2, wasm_global_set),
        ("globalType", 1, wasm_global_type),
    ];
    for (name, arity, f) in natives {
        method(vm, &n, name, arity, f);
    }
    let wasm = vm.call_value(&factory, Value::Undefined, &[Value::Object(n)])?;
    vm.define_global("WebAssembly", wasm.clone());
    vm.globals
        .attrs
        .insert("WebAssembly".into(), crate::object::NON_ENUM);
    Ok(wasm)
}
