use crate::bytecode::{Cmp, FunctionProto, Op};
use crate::value::Value;
use crate::vm::Vm;
use cranelift_codegen::Context;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, InstBuilder, JumpTableData, MemFlagsData, SigRef, Signature,
    StackSlotData, StackSlotKind, types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Module, default_libcall_names};
use std::cell::{Cell, OnceCell};
use std::rc::Rc;

const T_UNDEFINED: i64 = 0;
const T_NULL: i64 = 1;
const T_BOOL: i64 = 2;
const T_NUMBER: i64 = 3;
const T_UNINIT: i64 = 8;
const SLOT: i32 = 16;
const UNKNOWN: u16 = u16::MAX;

pub const NEXT: u64 = 0;
pub const RELOAD: u64 = 1 << 32;
pub const ERR: u64 = 2 << 32;
pub const RETURNED: u64 = 3 << 32;
pub const DEOPT_OVERFLOW: u64 = 4 << 32;
pub const DEOPT_TYPE: u64 = 5 << 32;
pub const POPPED: u64 = 6 << 32;

pub const COLD: u8 = 0;
pub const READY: u8 = 1;
pub const REJECTED: u8 = 2;

#[derive(Default)]
pub struct JitInfo {
    pub hot: Cell<u32>,
    pub state: Cell<u8>,
    pub code: Cell<usize>,
    pub depths: OnceCell<Box<[u16]>>,
    pub entries: OnceCell<Box<[u16]>>,
    pub max: Cell<u32>,
    pub banned: std::cell::RefCell<Vec<u32>>,
    pub no_int: Cell<bool>,
    pub compiles: Cell<u32>,
    pub direct: Cell<u32>,
    pub need: Cell<u32>,
}

impl std::fmt::Debug for JitInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JitInfo({})", self.state.get())
    }
}

impl JitInfo {
    pub fn depth_at(&self, ip: usize) -> Option<usize> {
        let d = *self.depths.get()?.get(ip)?;
        (d != UNKNOWN).then_some(d as usize)
    }

    pub fn entry_at(&self, ip: usize) -> Option<usize> {
        let d = *self.entries.get()?.get(ip)?;
        (d != UNKNOWN).then_some(d as usize)
    }
}

#[repr(C)]
pub struct JitCtx {
    pub vm: *mut Vm,
    pub bp: *mut Value,
    pub base: usize,
    pub need: usize,
    pub interrupt: *const u8,
    pub upvals: *const u8,
    pub frame: usize,
    pub gc_allocs: *const usize,
    pub gc_threshold: *const usize,
}

#[cfg(target_arch = "x86_64")]
pub type JitFn = unsafe extern "sysv64" fn(*mut JitCtx, u32) -> u64;
#[cfg(not(target_arch = "x86_64"))]
pub type JitFn = unsafe extern "C" fn(*mut JitCtx, u32) -> u64;

unsafe extern "C" fn jit_step(ctx: *mut JitCtx, ip: u32, depth: u32) -> u64 {
    unsafe {
        let ctx = &mut *ctx;
        let vm = &mut *ctx.vm;
        vm.jit_step(ctx, ip as usize, depth as usize)
    }
}

unsafe extern "C" fn jit_call(ctx: *mut JitCtx, ip: u32, depth: u32, argc: u32) -> u64 {
    unsafe {
        let ctx = &mut *ctx;
        let vm = &mut *ctx.vm;
        vm.jit_call(ctx, ip as usize, depth as usize, argc as usize)
    }
}

unsafe extern "C" fn jit_return(ctx: *mut JitCtx, ip: u32, depth: u32) -> u64 {
    unsafe {
        let ctx = &mut *ctx;
        let vm = &mut *ctx.vm;
        vm.jit_return(ctx, ip as usize, depth as usize)
    }
}

unsafe extern "C" fn jit_pop_to(ctx: *mut JitCtx, to: u32, depth: u32) -> u64 {
    unsafe {
        let ctx = &mut *ctx;
        let vm = &mut *ctx.vm;
        vm.jit_pop_to(ctx, to as usize, depth as usize)
    }
}

unsafe extern "C" fn jit_clone(h: u64, p: u64) -> u64 {
    let v = unsafe { crate::vm::value_bits(h, p) };
    std::mem::forget((*v).clone());
    0
}

unsafe extern "C" fn jit_drop(h: u64, p: u64) -> u64 {
    drop(std::mem::ManuallyDrop::into_inner(unsafe {
        crate::vm::value_bits(h, p)
    }));
    0
}

macro_rules! six {
    ($name:ident, $method:ident) => {
        unsafe extern "C" fn $name(
            ctx: *mut JitCtx,
            a0: u64,
            a1: u64,
            a2: u64,
            a3: u64,
            a4: u64,
            a5: u64,
        ) -> u64 {
            unsafe {
                let ctx = &mut *ctx;
                let vm = &mut *ctx.vm;
                vm.$method(ctx, &[a0, a1, a2, a3, a4, a5])
            }
        }
    };
}

six!(jit_index, jit_index);
six!(jit_set_index, jit_set_index);
six!(jit_prop, jit_prop);
six!(jit_global, jit_global);
six!(jit_new_object, jit_new_object);
six!(jit_init_prop, jit_init_prop);
six!(jit_frame, jit_frame);
six!(jit_call_done, jit_call_done);

extern "C" fn jit_rem(x: f64, y: f64) -> f64 {
    crate::number::js_rem(x, y)
}

extern "C" fn jit_bits(op: u32, x: f64, y: f64) -> f64 {
    use crate::number::{to_int32, to_uint32};
    match op {
        0 => (to_int32(x) & to_int32(y)) as f64,
        1 => (to_int32(x) | to_int32(y)) as f64,
        2 => (to_int32(x) ^ to_int32(y)) as f64,
        3 => to_int32(x).wrapping_shl(to_uint32(y) & 31) as f64,
        4 => (to_int32(x) >> (to_uint32(y) & 31)) as f64,
        _ => (to_uint32(x) >> (to_uint32(y) & 31)) as f64,
    }
}

fn flow(op: Op, ip: usize, d: i32) -> Option<Vec<(usize, i32)>> {
    let next = ip + 1;
    let step = |delta: i32| Some(vec![(next, d + delta)]);
    match op {
        Op::Const(_)
        | Op::Undefined
        | Op::Null
        | Op::True
        | Op::False
        | Op::Uninit
        | Op::GetLocal(_)
        | Op::GetLocalChecked(..)
        | Op::GetUpval(_)
        | Op::GetUpvalChecked(..)
        | Op::GetGlobal(_)
        | Op::Dup
        | Op::Pick(_)
        | Op::NewObject
        | Op::Closure(_)
        | Op::GetCallee
        | Op::NewTarget
        | Op::GlobalRef(_)
        | Op::TypeofGlobal(_)
        | Op::GetMethod(_)
        | Op::GetLocalProp(..) => step(1),
        Op::GetLocalUndef(_) | Op::GetUpvalCheckedUndef(..) | Op::Dup2 | Op::GetLocalMethod(..) => {
            step(2)
        }
        Op::UndefinedN(n) | Op::UninitN(n) => step(n as i32),
        Op::Pop
        | Op::StoreLocal(_)
        | Op::StoreUpval(_)
        | Op::AppendLocal(_)
        | Op::InitProp(_)
        | Op::InitHidden(_)
        | Op::InitMethod(_)
        | Op::ArrayPush
        | Op::ArraySpread
        | Op::ObjSpread
        | Op::SetProp(_)
        | Op::StoreLocalProp(..)
        | Op::GetIndex
        | Op::DefineField(_)
        | Op::DeleteIndex
        | Op::IterCloseTop
        | Op::IterCloseUnder
        | Op::SetGlobalStrict(_)
        | Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Mod
        | Op::Pow
        | Op::BitAnd
        | Op::BitOr
        | Op::BitXor
        | Op::Shl
        | Op::Shr
        | Op::UShr
        | Op::Eq
        | Op::Ne
        | Op::StrictEq
        | Op::StrictNe
        | Op::Lt
        | Op::Gt
        | Op::Le
        | Op::Ge
        | Op::In
        | Op::InstanceOf => step(-1),
        Op::StoreProp(_) | Op::InitIndex | Op::DefineIndex | Op::SetIndex => step(-2),
        Op::Swap
        | Op::Rot3
        | Op::Rot4
        | Op::SetLocal(_)
        | Op::SetUpval(_)
        | Op::SetLocalChecked(..)
        | Op::SetUpvalChecked(..)
        | Op::SetGlobal(_)
        | Op::IncLocal(_)
        | Op::DecLocal(_)
        | Op::AddConst(_)
        | Op::SubConst(_)
        | Op::MulConst(_)
        | Op::ModConst(_)
        | Op::Neg
        | Op::Not
        | Op::BitNot
        | Op::Typeof
        | Op::ToNumber
        | Op::ToNumeric
        | Op::Inc
        | Op::Dec
        | Op::GetProp(_)
        | Op::GetIter
        | Op::ToObject
        | Op::ToPropertyKey
        | Op::RequireObject(_)
        | Op::CloseUpvalues(_)
        | Op::TryEnd
        | Op::CheckIterResult
        | Op::ArrayHole
        | Op::SetHome
        | Op::DeleteProp(_)
        | Op::IterClose(_)
        | Op::IterCloseQuiet(_)
        | Op::GetIndexMethod => step(0),
        Op::Jump(t) => Some(vec![(t as usize, d)]),
        Op::JumpIfFalse(t) | Op::JumpIfTrue(t) => Some(vec![(t as usize, d - 1), (next, d - 1)]),
        Op::JumpIfFalseKeep(t)
        | Op::JumpIfTrueKeep(t)
        | Op::JumpIfNotNullishKeep(t)
        | Op::JumpIfNotUndef(t) => Some(vec![(t as usize, d), (next, d - 1)]),
        Op::JumpIfNullish(t) => Some(vec![(t as usize, d), (next, d)]),
        Op::CmpJump(_, t) => Some(vec![(t as usize, d - 2), (next, d - 2)]),
        Op::CmpConstJump(_, _, t) => Some(vec![(t as usize, d - 1), (next, d - 1)]),
        Op::IterNext(t) => Some(vec![(t as usize, d - 1), (next, d)]),
        Op::Call(argc, _) => step(-(argc as i32) - 1),
        Op::New(argc, _) => step(-(argc as i32)),
        Op::CallSpread(_) => step(-2),
        Op::NewSpread(_) => step(-1),
        Op::PopTo(n) => Some(vec![(next, n as i32)]),
        Op::Collapse(n) => Some(vec![(next, n as i32 + 1)]),
        Op::NewArray(n) | Op::Template(n) => step(1 - n as i32),
        Op::TryBegin(t, slot) => Some(vec![(next, d), (t as usize, slot as i32 + 1)]),
        Op::Return | Op::Throw => Some(Vec::new()),
        _ => None,
    }
}

fn analyze(proto: &FunctionProto) -> Option<(Vec<u16>, u32)> {
    let code = &proto.code;
    let mut depth = vec![UNKNOWN; code.len()];
    let entry = 1 + proto.num_params as i32 + proto.has_rest as i32 + proto.uses_arguments as i32;
    let mut work = vec![(0usize, entry)];
    let mut max = 0i32;
    while let Some((ip, d)) = work.pop() {
        if ip >= code.len() || !(0..60000).contains(&d) {
            return None;
        }
        if depth[ip] != UNKNOWN {
            if depth[ip] as i32 != d {
                return None;
            }
            continue;
        }
        depth[ip] = d as u16;
        max = max.max(d);
        for (to, depth_after) in flow(code[ip], ip, d)? {
            if depth_after < 0 {
                return None;
            }
            max = max.max(depth_after);
            work.push((to, depth_after));
        }
    }
    Some((depth, max as u32 + 4))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intrinsic {
    Floor,
    Ceil,
    Trunc,
    Abs,
    Sqrt,
    Min,
    Max,
}

impl Intrinsic {
    fn arity(self) -> u32 {
        match self {
            Intrinsic::Min | Intrinsic::Max => 2,
            _ => 1,
        }
    }
}

pub struct Jit {
    lay: crate::object::JitLayout,
    vl: crate::vm::VmLayout,
    intrinsics: Vec<(Rc<crate::object::Obj>, Intrinsic)>,
    module: JITModule,
    ctx: Context,
    fctx: FunctionBuilderContext,
}

struct Sigs {
    main: SigRef,
    six: SigRef,
    two: SigRef,
    step: SigRef,
    call: SigRef,
    pop_to: SigRef,
    rem: SigRef,
    bits: SigRef,
}

fn flags() -> MemFlagsData {
    MemFlagsData::trusted()
}

fn float_cc(c: Cmp) -> FloatCC {
    match c {
        Cmp::Lt => FloatCC::LessThan,
        Cmp::Le => FloatCC::LessThanOrEqual,
        Cmp::Gt => FloatCC::GreaterThan,
        Cmp::Ge => FloatCC::GreaterThanOrEqual,
        Cmp::StrictEq | Cmp::Eq => FloatCC::Equal,
        Cmp::StrictNe | Cmp::Ne => FloatCC::NotEqual,
    }
}

fn local_of(op: Op) -> Option<usize> {
    match op {
        Op::GetLocal(l)
        | Op::GetLocalUndef(l)
        | Op::GetLocalChecked(l, _)
        | Op::SetLocal(l)
        | Op::SetLocalChecked(l, _)
        | Op::StoreLocal(l)
        | Op::IncLocal(l)
        | Op::DecLocal(l)
        | Op::AppendLocal(l)
        | Op::GetLocalProp(l, _)
        | Op::GetLocalMethod(l, _)
        | Op::StoreLocalProp(l, _) => Some(l as usize),
        _ => None,
    }
}

fn touched_from(op: Op, d: usize, da: usize) -> usize {
    let outs = match op {
        Op::UndefinedN(n) | Op::UninitN(n) => n as usize,
        _ => 2,
    };
    da.saturating_sub(outs).min(d)
}

fn dirty_sets(proto: &FunctionProto, depths: &[u16], max: usize) -> Vec<Vec<u64>> {
    let code = &proto.code;
    let words = max.div_ceil(64).max(1);
    let mut sets: Vec<Option<Vec<u64>>> = vec![None; code.len()];
    sets[0] = Some(vec![0; words]);
    let mut work = vec![0usize];
    while let Some(ip) = work.pop() {
        let Some(inn) = sets[ip].clone() else {
            continue;
        };
        let op = code[ip];
        let d = depths[ip] as usize;
        let Some(edges) = flow(op, ip, d as i32) else {
            continue;
        };
        for (t, da) in edges {
            let da = da as usize;
            let mut out = vec![0u64; words];
            if inline_op(op) {
                for k in 0..da.min(words * 64) {
                    if inn[k / 64] >> (k % 64) & 1 == 1 {
                        out[k / 64] |= 1 << (k % 64);
                    }
                }
                let mut mark = |k: usize| {
                    if k < words * 64 {
                        out[k / 64] |= 1 << (k % 64);
                    }
                };
                for k in touched_from(op, d, da)..da {
                    mark(k);
                }
                if let Some(l) = local_of(op) {
                    mark(l);
                }
            }
            let Some(slot) = sets.get_mut(t) else {
                continue;
            };
            match slot {
                Some(have) => {
                    let mut grew = false;
                    for (h, o) in have.iter_mut().zip(&out) {
                        if *h | o != *h {
                            *h |= o;
                            grew = true;
                        }
                    }
                    if grew {
                        work.push(t);
                    }
                }
                None => {
                    *slot = Some(out);
                    work.push(t);
                }
            }
        }
    }
    sets.into_iter()
        .map(|s| s.unwrap_or_else(|| vec![0; words]))
        .collect()
}

fn inline_op(op: Op) -> bool {
    matches!(
        op,
        Op::Const(_)
            | Op::Undefined
            | Op::Null
            | Op::True
            | Op::False
            | Op::Uninit
            | Op::UndefinedN(_)
            | Op::UninitN(_)
            | Op::Pop
            | Op::Dup
            | Op::Swap
            | Op::GetLocal(_)
            | Op::GetLocalUndef(_)
            | Op::GetLocalChecked(..)
            | Op::SetLocal(_)
            | Op::StoreLocal(_)
            | Op::IncLocal(_)
            | Op::DecLocal(_)
            | Op::AppendLocal(_)
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::BitAnd
            | Op::BitOr
            | Op::BitXor
            | Op::Shl
            | Op::Shr
            | Op::UShr
            | Op::AddConst(_)
            | Op::SubConst(_)
            | Op::MulConst(_)
            | Op::ModConst(_)
            | Op::Neg
            | Op::Inc
            | Op::Dec
            | Op::ToNumeric
            | Op::ToNumber
            | Op::Lt
            | Op::Gt
            | Op::Le
            | Op::Ge
            | Op::StrictEq
            | Op::StrictNe
            | Op::Not
            | Op::Jump(_)
            | Op::JumpIfFalse(_)
            | Op::JumpIfTrue(_)
            | Op::JumpIfFalseKeep(_)
            | Op::JumpIfTrueKeep(_)
            | Op::JumpIfNotNullishKeep(_)
            | Op::JumpIfNotUndef(_)
            | Op::JumpIfNullish(_)
            | Op::CmpJump(..)
            | Op::CmpConstJump(..)
            | Op::PopTo(_)
            | Op::Return
            | Op::Call(..)
            | Op::GetUpval(_)
            | Op::GetUpvalChecked(..)
            | Op::GetUpvalCheckedUndef(..)
            | Op::SetUpval(_)
            | Op::StoreUpval(_)
            | Op::GetIndex
            | Op::SetIndex
            | Op::GetProp(_)
            | Op::GetLocalProp(..)
            | Op::GetMethod(_)
            | Op::GetLocalMethod(..)
            | Op::GetGlobal(_)
            | Op::NewObject
            | Op::InitProp(_)
    )
}

type V = cranelift_codegen::ir::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ty {
    Int,
    Num,
    Prim,
    Any,
}

fn join(a: Ty, b: Ty) -> Ty {
    match (a, b) {
        _ if a == b => a,
        (Ty::Int, Ty::Num) | (Ty::Num, Ty::Int) => Ty::Num,
        _ => Ty::Any,
    }
}

fn is_num(t: Ty) -> bool {
    matches!(t, Ty::Int | Ty::Num)
}

fn int_value(x: f64) -> Option<i32> {
    let i = x as i32;
    (i as f64 == x && (i != 0 || x.is_sign_positive())).then_some(i)
}

struct Infer<'a> {
    proto: &'a FunctionProto,
    banned: &'a [bool],
    ints: bool,
    captured: &'a [bool],
}

impl Infer<'_> {
    fn const_ty(&self, k: u32) -> Ty {
        match &self.proto.consts[k as usize] {
            Value::Number(x) if self.ints && int_value(*x).is_some() => Ty::Int,
            Value::Number(_) => Ty::Num,
            Value::Undefined | Value::Null | Value::Bool(_) | Value::Uninit => Ty::Prim,
            _ => Ty::Any,
        }
    }

    fn arith(&self, ip: usize, a: Ty, b: Ty) -> Ty {
        if self.ints && !self.banned[ip] && a == Ty::Int && b == Ty::Int {
            Ty::Int
        } else {
            Ty::Num
        }
    }

    fn int_const(&self, k: u32) -> Option<i32> {
        match &self.proto.consts[k as usize] {
            Value::Number(x) if self.ints => int_value(*x),
            _ => None,
        }
    }

    fn next(&self, op: Op, ip: usize, inn: &[Ty]) -> Option<Vec<(usize, Vec<Ty>)>> {
        let d = inn.len();
        let next = ip + 1;
        let mut s = inn.to_vec();
        let outs = match op {
            Op::GetMethod(_) | Op::GetLocalMethod(..) | Op::Dup2 | Op::GetIndexMethod => 2,
            Op::AppendLocal(_)
            | Op::IncLocal(_)
            | Op::DecLocal(_)
            | Op::StoreUpval(_)
            | Op::StoreProp(_)
            | Op::StoreLocalProp(..)
            | Op::ArrayPush
            | Op::InitHidden(_)
            | Op::InitMethod(_)
            | Op::TryEnd
            | Op::CloseUpvalues(_)
            | Op::Pop => 0,
            Op::Rot3 => 3,
            Op::Rot4 => 4,
            _ => 1,
        };
        let impure = || -> Option<Vec<(usize, Vec<Ty>)>> {
            Some(
                flow(op, ip, d as i32)?
                    .into_iter()
                    .map(|(t, depth_after)| {
                        let da = depth_after as usize;
                        let keep = da.saturating_sub(outs).min(d);
                        let st = (0..da)
                            .map(|k| {
                                if k < keep && !self.captured.get(k).copied().unwrap_or(true) {
                                    inn[k]
                                } else {
                                    Ty::Any
                                }
                            })
                            .collect();
                        (t, st)
                    })
                    .collect(),
            )
        };
        match op {
            Op::Const(k) => s.push(self.const_ty(k)),
            Op::Undefined | Op::Null | Op::True | Op::False | Op::Uninit => s.push(Ty::Prim),
            Op::UndefinedN(n) | Op::UninitN(n) => {
                s.extend(std::iter::repeat_n(Ty::Prim, n as usize))
            }
            Op::Pop => {
                s.pop();
            }
            Op::Dup => s.push(s[d - 1]),
            Op::Swap => s.swap(d - 1, d - 2),
            Op::GetLocal(l) | Op::GetLocalChecked(l, _) => s.push(s[l as usize]),
            Op::GetLocalUndef(l) => {
                s.push(s[l as usize]);
                s.push(Ty::Prim);
            }
            Op::SetLocal(l) | Op::SetLocalChecked(l, _) => s[l as usize] = s[d - 1],
            Op::StoreLocal(l) => {
                let t = s.pop()?;
                s[l as usize] = t;
            }
            Op::IncLocal(l) | Op::DecLocal(l) => {
                let t = s[l as usize];
                if !is_num(t) {
                    return impure().map(|v| {
                        v.into_iter()
                            .map(|(t, mut st)| {
                                st[l as usize] = Ty::Any;
                                (t, st)
                            })
                            .collect()
                    });
                }
                s[l as usize] = self.arith(ip, t, Ty::Int);
            }
            Op::AppendLocal(l) => {
                let (a, b) = (s[l as usize], s[d - 1]);
                if !is_num(a) || !is_num(b) {
                    return impure().map(|v| {
                        v.into_iter()
                            .map(|(t, mut st)| {
                                st[l as usize] = Ty::Any;
                                (t, st)
                            })
                            .collect()
                    });
                }
                s[l as usize] = self.arith(ip, a, b);
                s.pop();
            }
            Op::Add | Op::Sub | Op::Mul | Op::Mod => {
                let (a, b) = (s[d - 2], s[d - 1]);
                if !is_num(a) || !is_num(b) {
                    return impure();
                }
                s.truncate(d - 2);
                s.push(self.arith(ip, a, b));
            }
            Op::Div => {
                if !is_num(s[d - 2]) || !is_num(s[d - 1]) {
                    return impure();
                }
                s.truncate(d - 2);
                s.push(Ty::Num);
            }
            Op::AddConst(k) | Op::SubConst(k) | Op::MulConst(k) | Op::ModConst(k) => {
                let a = s[d - 1];
                if !is_num(a) {
                    return impure();
                }
                let c = self.int_const(k);
                let int = c.is_some_and(|c| !matches!(op, Op::ModConst(_)) || c != 0);
                s[d - 1] = if int {
                    self.arith(ip, a, Ty::Int)
                } else {
                    Ty::Num
                };
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr | Op::UShr => {
                if !is_num(s[d - 2]) || !is_num(s[d - 1]) {
                    return impure();
                }
                s.truncate(d - 2);
                s.push(if self.ints && op != Op::UShr {
                    Ty::Int
                } else {
                    Ty::Num
                });
            }
            Op::Neg | Op::Inc | Op::Dec => {
                let a = s[d - 1];
                if !is_num(a) {
                    return impure();
                }
                s[d - 1] = self.arith(ip, a, Ty::Int);
            }
            Op::ToNumeric | Op::ToNumber => {
                if !is_num(s[d - 1]) {
                    return impure();
                }
            }
            Op::Lt | Op::Gt | Op::Le | Op::Ge => {
                if !is_num(s[d - 2]) || !is_num(s[d - 1]) {
                    return impure();
                }
                s.truncate(d - 2);
                s.push(Ty::Prim);
            }
            Op::StrictEq | Op::StrictNe => {
                s.truncate(d - 2);
                s.push(Ty::Prim);
            }
            Op::Not => s[d - 1] = Ty::Prim,
            Op::Jump(t) => return Some(vec![(t as usize, s)]),
            Op::JumpIfFalse(t) | Op::JumpIfTrue(t) => {
                s.pop();
                return Some(vec![(t as usize, s.clone()), (next, s)]);
            }
            Op::JumpIfFalseKeep(t)
            | Op::JumpIfTrueKeep(t)
            | Op::JumpIfNotNullishKeep(t)
            | Op::JumpIfNotUndef(t) => {
                let kept = s.clone();
                s.pop();
                return Some(vec![(t as usize, kept), (next, s)]);
            }
            Op::JumpIfNullish(t) => {
                let mut taken = s.clone();
                taken[d - 1] = Ty::Prim;
                return Some(vec![(t as usize, taken), (next, s)]);
            }
            Op::CmpJump(c, t) => {
                let strict = matches!(c, Cmp::StrictEq | Cmp::StrictNe);
                if !strict && (!is_num(s[d - 2]) || !is_num(s[d - 1])) {
                    return impure();
                }
                s.truncate(d - 2);
                return Some(vec![(t as usize, s.clone()), (next, s)]);
            }
            Op::CmpConstJump(c, _, t) => {
                let strict = matches!(c, Cmp::StrictEq | Cmp::StrictNe);
                if !strict && !is_num(s[d - 1]) {
                    return impure();
                }
                s.pop();
                return Some(vec![(t as usize, s.clone()), (next, s)]);
            }
            Op::PopTo(n) => s.truncate(n as usize),
            Op::NewObject => s.push(Ty::Any),
            Op::InitProp(_) => {
                s.pop();
            }
            Op::GetUpval(_) | Op::GetUpvalChecked(..) => s.push(Ty::Any),
            Op::SetUpval(_) => {}
            Op::StoreUpval(_) => {
                s.pop();
            }
            Op::GetUpvalCheckedUndef(..) => {
                s.push(Ty::Any);
                s.push(Ty::Prim);
            }
            Op::Return | Op::Throw => return Some(Vec::new()),
            Op::TryBegin(t, slot) => {
                return Some(vec![
                    (next, s),
                    (t as usize, vec![Ty::Any; slot as usize + 1]),
                ]);
            }
            _ => return impure(),
        }
        Some(vec![(next, s)])
    }
}

fn infer(
    proto: &FunctionProto,
    depths: &[u16],
    banned: &[bool],
    ints: bool,
    captured: &[bool],
) -> Option<Vec<Option<Vec<Ty>>>> {
    let inf = Infer {
        proto,
        banned,
        ints,
        captured,
    };
    let n = proto.code.len();
    let mut types: Vec<Option<Vec<Ty>>> = vec![None; n];
    let entry = depths[0] as usize;
    let mut work = vec![(0usize, vec![Ty::Any; entry])];
    let mut steps = 0usize;
    while let Some((ip, st)) = work.pop() {
        steps += 1;
        if steps > n * 64 + 1000 || ip >= n || st.len() != depths[ip] as usize {
            return None;
        }
        let merged = match &types[ip] {
            None => st,
            Some(old) => {
                let m: Vec<Ty> = old.iter().zip(&st).map(|(&a, &b)| join(a, b)).collect();
                if &m == old {
                    continue;
                }
                m
            }
        };
        types[ip] = Some(merged.clone());
        for (to, s) in inf.next(proto.code[ip], ip, &merged)? {
            work.push((to, s));
        }
    }
    Some(types)
}

struct Gen<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    proto: &'a FunctionProto,
    intrinsics: &'a [(Rc<crate::object::Obj>, Intrinsic)],
    lay: crate::object::JitLayout,
    vl: crate::vm::VmLayout,
    blocks: &'a [Block],
    types: &'a [Option<Vec<Ty>>],
    ints: bool,
    banned: &'a [bool],
    ret: Block,
    ctx_var: Variable,
    bp_var: Variable,
    head: Vec<Variable>,
    pay: Vec<Variable>,
    ival: Vec<Variable>,
    sigs: &'a Sigs,
    volatile: &'a [bool],
    dirty: &'a [Vec<u64>],
    spill_at: usize,
    slow_memo: Option<(usize, Block)>,
    slow: Vec<(Block, usize, Vec<Ty>, Vec<usize>)>,
    deopts: Vec<(Block, usize, Vec<Ty>)>,
    narrows: Vec<(Block, usize, Vec<Ty>)>,
    exits: Vec<(Block, u64)>,
}

impl Gen<'_, '_> {
    fn bp(&mut self) -> V {
        self.b.use_var(self.bp_var)
    }

    fn ty_at(&self, ip: usize) -> Vec<Ty> {
        self.types[ip].clone().unwrap_or_default()
    }

    fn tag(&mut self, k: usize) -> V {
        let h = self.b.use_var(self.head[k]);
        self.b.ins().band_imm_u(h, 0xff)
    }

    fn tag_is(&mut self, k: usize, tag: i64) -> V {
        let t = self.tag(k);
        self.b.ins().icmp_imm_s(IntCC::Equal, t, tag)
    }

    fn f64_of(&mut self, k: usize, t: Ty) -> V {
        if t == Ty::Int {
            let i = self.b.use_var(self.ival[k]);
            self.b.ins().fcvt_from_sint(types::F64, i)
        } else {
            self.b.use_var(self.pay[k])
        }
    }

    fn bits(&mut self, k: usize, t: Ty) -> (V, V) {
        let (h, p) = match t {
            Ty::Int | Ty::Num => (self.b.ins().iconst(types::I64, T_NUMBER), self.f64_of(k, t)),
            _ => (self.b.use_var(self.head[k]), self.b.use_var(self.pay[k])),
        };
        let p = self.b.ins().bitcast(types::I64, MemFlagsData::new(), p);
        (h, p)
    }

    fn call_six(&mut self, f: usize, args: [V; 6]) -> V {
        let ctx = self.b.use_var(self.ctx_var);
        let fv = self.b.ins().iconst(types::I64, f as i64);
        let mut vals = vec![ctx];
        vals.extend(args);
        let call = self.b.ins().call_indirect(self.sigs.six, fv, &vals);
        self.b.inst_results(call)[0]
    }

    fn konst(&mut self, v: i64) -> V {
        self.b.ins().iconst(types::I64, v)
    }

    fn load_slot(&mut self, k: usize) {
        let bp = self.bp();
        let h = self.b.ins().load(types::I64, flags(), bp, k as i32 * SLOT);
        let p = self
            .b
            .ins()
            .load(types::F64, flags(), bp, k as i32 * SLOT + 8);
        self.b.def_var(self.head[k], h);
        self.b.def_var(self.pay[k], p);
    }

    fn refcount(&mut self, k: usize, t: Ty, f: usize) {
        if t != Ty::Any {
            return;
        }
        let (h, p) = self.bits(k, t);
        let inc = f == jit_clone as *const () as usize;
        self.ref_change(h, p, inc);
    }

    fn ref_change(&mut self, h: V, p: V, inc: bool) {
        let tag = self.b.ins().band_imm_u(h, 0xff);
        let off = self.b.ins().iadd_imm_s(tag, -4);
        let heap = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThanOrEqual, off, 3);
        let yes = self.b.create_block();
        let done = self.b.create_block();
        self.b.ins().brif(heap, yes, &[], done, &[]);
        self.b.switch_to_block(yes);
        let c = self.b.ins().load(types::I64, flags(), p, 0);
        if inc {
            let c1 = self.b.ins().iadd_imm_s(c, 1);
            self.b.ins().store(flags(), c1, p, 0);
            self.b.ins().jump(done, &[]);
        } else {
            let last = self.b.ins().icmp_imm_s(IntCC::Equal, c, 1);
            let free = self.b.create_block();
            let keep = self.b.create_block();
            self.b.ins().brif(last, free, &[], keep, &[]);
            self.b.switch_to_block(free);
            let fv = self
                .b
                .ins()
                .iconst(types::I64, jit_drop as *const () as usize as i64);
            self.b.ins().call_indirect(self.sigs.two, fv, &[h, p]);
            self.b.ins().jump(done, &[]);
            self.b.switch_to_block(keep);
            let c1 = self.b.ins().iadd_imm_s(c, -1);
            self.b.ins().store(flags(), c1, p, 0);
            self.b.ins().jump(done, &[]);
        }
        self.b.switch_to_block(done);
    }

    #[allow(clippy::too_many_arguments)]
    fn inline_frame(
        &mut self,
        ci: usize,
        ip: usize,
        argc: u32,
        nctx: V,
        direct: Block,
        miss: Block,
        other: Block,
    ) {
        let (lay, vl) = (self.lay, self.vl);
        let tag = self.tag(ci);
        let is_obj = self.b.ins().icmp_imm_s(IntCC::Equal, tag, 5);
        self.guard(is_obj, other);
        let p = self.b.use_var(self.pay[ci]);
        let bits = self.b.ins().bitcast(types::I64, MemFlagsData::new(), p);
        let objp = self.b.ins().iadd_imm_s(bits, lay.rc_value as i64);
        let k = self
            .b
            .ins()
            .uload8(types::I64, flags(), objp, lay.obj_kind as i32);
        let is_fn = self
            .b
            .ins()
            .icmp_imm_s(IntCC::Equal, k, crate::object::FUNCTION_TAG as i64);
        self.guard(is_fn, other);
        let crc = self.b.ins().load(
            types::I64,
            flags(),
            objp,
            (lay.obj_kind + lay.kind_field) as i32,
        );
        let clo = self.b.ins().iadd_imm_s(crc, lay.rc_value as i64);
        let prc = self
            .b
            .ins()
            .load(types::I64, flags(), clo, vl.closure_proto as i32);
        let proto = self.b.ins().iadd_imm_s(prc, lay.rc_value as i64);
        let dir = self.b.ins().uload32(flags(), proto, vl.proto_direct as i32);
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, dir, argc as i64 + 1);
        self.guard(ok, other);
        let ctx = self.b.use_var(self.ctx_var);
        let vm = self.b.ins().load(types::I64, flags(), ctx, 0);
        let flen = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.frames_len as i32);
        let fcap = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.frames_cap as i32);
        let room = self.b.ins().icmp(IntCC::UnsignedLessThan, flen, fcap);
        self.guard(room, miss);
        let low = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, flen, vl.max_frames as i64);
        self.guard(low, miss);
        let nesting = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.native_depth as i32);
        let shallow = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, nesting, 1000);
        self.guard(shallow, miss);
        let intr = self.b.ins().load(types::I64, flags(), ctx, 32);
        let flag = self.b.ins().uload8(types::I64, flags(), intr, 0);
        let calm = self.b.ins().icmp_imm_s(IntCC::Equal, flag, 0);
        self.guard(calm, miss);
        let ga = self.b.ins().load(types::I64, flags(), ctx, 56);
        let gt = self.b.ins().load(types::I64, flags(), ctx, 64);
        let allocs = self.b.ins().load(types::I64, flags(), ga, 0);
        let limit = self.b.ins().load(types::I64, flags(), gt, 0);
        let fresh = self.b.ins().icmp(IntCC::UnsignedLessThan, allocs, limit);
        self.guard(fresh, miss);
        let base = self.b.ins().load(types::I64, flags(), ctx, 16);
        let cbase = self.b.ins().iadd_imm_s(base, ci as i64 + 1);
        let max = self.b.ins().uload32(flags(), proto, vl.proto_max as i32);
        let need = self.b.ins().iadd(cbase, max);
        let need = self.b.ins().iadd_imm_s(need, 8);
        let capacity = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.stack_cap as i32);
        let fits = self
            .b
            .ins()
            .icmp(IntCC::UnsignedLessThanOrEqual, need, capacity);
        self.guard(fits, miss);
        let fptr = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.frames_ptr as i32);
        let fi = self.b.ins().load(types::I64, flags(), ctx, 48);
        let off = self.b.ins().imul_imm_s(fi, vl.frame_size as i64);
        let fr = self.b.ins().iadd(fptr, off);
        let resume_ip = self.konst(ip as i64 + 1);
        self.b
            .ins()
            .store(flags(), resume_ip, fr, vl.frame_ip as i32);
        let off = self.b.ins().imul_imm_s(flen, vl.frame_size as i64);
        let nf = self.b.ins().iadd(fptr, off);
        let strong = self.b.ins().load(types::I64, flags(), crc, 0);
        let strong = self.b.ins().iadd_imm_s(strong, 1);
        self.b.ins().store(flags(), strong, crc, 0);
        let zero = self.konst(0);
        self.b
            .ins()
            .store(flags(), crc, nf, vl.frame_closure as i32);
        self.b.ins().store(flags(), zero, nf, vl.frame_ip as i32);
        self.b.ins().store(flags(), cbase, nf, vl.frame_base as i32);
        self.b
            .ins()
            .istore8(flags(), zero, nf, vl.frame_construct as i32);
        self.b.ins().store(flags(), zero, nf, vl.frame_extra as i32);
        let flen1 = self.b.ins().iadd_imm_s(flen, 1);
        self.b.ins().store(flags(), flen1, vm, vl.frames_len as i32);
        let deeper = self.b.ins().iadd_imm_s(nesting, 1);
        self.b
            .ins()
            .store(flags(), deeper, vm, vl.native_depth as i32);
        let bp = self.bp();
        let cbp = self.b.ins().iadd_imm_s(bp, (ci as i64 + 1) * SLOT as i64);
        let ups = self
            .b
            .ins()
            .load(types::I64, flags(), clo, vl.closure_upvalues as i32);
        for (v, at) in [
            (vm, 0),
            (cbp, 8),
            (cbase, 16),
            (need, 24),
            (intr, 32),
            (ups, 40),
            (flen, 48),
            (ga, 56),
            (gt, 64),
        ] {
            self.b.ins().store(flags(), v, nctx, at);
        }
        let code = self
            .b
            .ins()
            .load(types::I64, flags(), proto, vl.proto_code as i32);
        self.b.ins().jump(direct, &[BlockArg::Value(code)]);
    }

    fn inline_return(&mut self, inn: &[Ty], d: usize, slow: Block) {
        let vl = self.vl;
        let ctx = self.b.use_var(self.ctx_var);
        let vm = self.b.ins().load(types::I64, flags(), ctx, 0);
        let base = self.b.ins().load(types::I64, flags(), ctx, 16);
        let fi = self.b.ins().load(types::I64, flags(), ctx, 48);
        let flen = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.frames_len as i32);
        let top = self.b.ins().iadd_imm_s(fi, 1);
        let ok = self.b.ins().icmp(IntCC::Equal, flen, top);
        self.guard(ok, slow);
        let fptr = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.frames_ptr as i32);
        let off = self.b.ins().imul_imm_s(fi, vl.frame_size as i64);
        let fr = self.b.ins().iadd(fptr, off);
        let cons = self
            .b
            .ins()
            .uload8(types::I64, flags(), fr, vl.frame_construct as i32);
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, cons, 0);
        self.guard(ok, slow);
        let extra = self
            .b
            .ins()
            .load(types::I64, flags(), fr, vl.frame_extra as i32);
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, extra, 0);
        self.guard(ok, slow);
        let crc = self
            .b
            .ins()
            .load(types::I64, flags(), fr, vl.frame_closure as i32);
        let strong = self.b.ins().load(types::I64, flags(), crc, 0);
        let ok = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedGreaterThan, strong, 1);
        self.guard(ok, slow);
        let open = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.highest_open as i32);
        let ok = self.b.ins().icmp(IntCC::UnsignedLessThan, open, base);
        self.guard(ok, slow);
        let bl = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.builders_len as i32);
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, bl, 0);
        self.guard(ok, slow);
        let hl = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.handlers_len as i32);
        let none = self.b.ins().icmp_imm_s(IntCC::Equal, hl, 0);
        let check = self.b.create_block();
        let clear = self.b.create_block();
        self.b.ins().brif(none, clear, &[], check, &[]);
        self.b.switch_to_block(check);
        let hptr = self
            .b
            .ins()
            .load(types::I64, flags(), vm, vl.handlers_ptr as i32);
        let last = self.b.ins().iadd_imm_s(hl, -1);
        let off = self.b.ins().imul_imm_s(last, vl.handler_size as i64);
        let h = self.b.ins().iadd(hptr, off);
        let hf = self
            .b
            .ins()
            .load(types::I64, flags(), h, vl.handler_frame as i32);
        let ok = self.b.ins().icmp(IntCC::UnsignedLessThan, hf, fi);
        self.b.ins().brif(ok, clear, &[], slow, &[]);
        self.b.switch_to_block(clear);
        let strong = self.b.ins().iadd_imm_s(strong, -1);
        self.b.ins().store(flags(), strong, crc, 0);
        self.b.ins().store(flags(), fi, vm, vl.frames_len as i32);
        for (k, &t) in inn.iter().enumerate().take(d - 1) {
            if t == Ty::Any {
                let (h, p) = self.bits(k, t);
                self.ref_change(h, p, false);
            }
        }
        let bp = self.bp();
        let h = self.b.ins().load(types::I64, flags(), bp, -SLOT);
        let p = self.b.ins().load(types::I64, flags(), bp, -SLOT + 8);
        self.ref_change(h, p, false);
        let (rh, rp) = self.bits(d - 1, inn[d - 1]);
        let bp = self.bp();
        self.b.ins().store(flags(), rh, bp, -SLOT);
        self.b.ins().store(flags(), rp, bp, -SLOT + 8);
        let ctx = self.b.use_var(self.ctx_var);
        let vm = self.b.ins().load(types::I64, flags(), ctx, 0);
        let base = self.b.ins().load(types::I64, flags(), ctx, 16);
        self.b.ins().store(flags(), base, vm, vl.stack_len as i32);
        let r = self.konst(POPPED as i64);
        self.ret_value(r);
    }

    fn upval_addr(&mut self, i: u32) -> V {
        let ctx = self.b.use_var(self.ctx_var);
        let ups = self.b.ins().load(types::I64, flags(), ctx, 40);
        let cell = self.b.ins().load(types::I64, flags(), ups, i as i32 * 8);
        let uv = self.b.ins().iadd_imm_s(cell, self.lay.upvalue as i64);
        let tag = self.b.ins().uload8(types::I32, flags(), uv, 0);
        let open = self
            .b
            .ins()
            .icmp_imm_s(IntCC::Equal, tag, self.lay.open_tag as i64);
        let ob = self.b.create_block();
        let cb = self.b.create_block();
        let join_b = self.b.create_block();
        self.b.append_block_param(join_b, types::I64);
        self.b.ins().brif(open, ob, &[], cb, &[]);
        self.b.switch_to_block(ob);
        let idx = self
            .b
            .ins()
            .load(types::I64, flags(), uv, self.lay.open_index as i32);
        let base = self.b.ins().load(types::I64, flags(), ctx, 16);
        let bp = self.bp();
        let rel = self.b.ins().isub(idx, base);
        let off = self.b.ins().ishl_imm_u(rel, 4);
        let addr = self.b.ins().iadd(bp, off);
        self.b.ins().jump(join_b, &[BlockArg::Value(addr)]);
        self.b.switch_to_block(cb);
        let addr = self.b.ins().iadd_imm_s(uv, self.lay.closed_value as i64);
        self.b.ins().jump(join_b, &[BlockArg::Value(addr)]);
        self.b.switch_to_block(join_b);
        self.b.block_params(join_b)[0]
    }

    fn object_ptr(&mut self, h: V, p: V, fail: Block) -> V {
        let tag = self.b.ins().band_imm_u(h, 0xff);
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, tag, 5);
        self.guard(ok, fail);
        self.b.ins().iadd_imm_s(p, self.lay.rc_value as i64)
    }

    fn kind_is(&mut self, objp: V, tag: u8) -> V {
        let k = self
            .b
            .ins()
            .uload8(types::I32, flags(), objp, self.lay.obj_kind as i32);
        self.b.ins().icmp_imm_s(IntCC::Equal, k, tag as i64)
    }

    fn array_vec(&mut self, objp: V, mask: i64, fail: Block) -> (V, V) {
        let is_array = self.kind_is(objp, self.lay.array_tag);
        self.guard(is_array, fail);
        let fl = self
            .b
            .ins()
            .uload8(types::I32, flags(), objp, self.lay.obj_flags as i32);
        let bad = self.b.ins().band_imm_u(fl, mask);
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, bad, 0);
        self.guard(ok, fail);
        let vec = (self.lay.obj_kind + self.lay.array_vec) as i32;
        let ptr = self
            .b
            .ins()
            .load(types::I64, flags(), objp, vec + self.lay.vec_ptr as i32);
        let len = self
            .b
            .ins()
            .load(types::I64, flags(), objp, vec + self.lay.vec_len as i32);
        (ptr, len)
    }

    fn index_of(&mut self, k: usize, t: Ty, fail: Block) -> V {
        if t == Ty::Int {
            let i = self.i32_of(k);
            return self.b.ins().sextend(types::I64, i);
        }
        if t != Ty::Num {
            let ok = self.tag_is(k, T_NUMBER);
            self.guard(ok, fail);
        }
        let x = self.b.use_var(self.pay[k]);
        let i = self.b.ins().fcvt_to_sint_sat(types::I64, x);
        let back = self.b.ins().fcvt_from_sint(types::F64, i);
        let ok = self.b.ins().fcmp(FloatCC::Equal, back, x);
        self.guard(ok, fail);
        i
    }

    fn own_prop(&mut self, objp: V, ip: usize, fail: Block) -> (V, V) {
        let ord = self.kind_is(objp, self.lay.ordinary_tag);
        self.guard(ord, fail);
        let pm = self.b.ins().iadd_imm_s(objp, self.lay.obj_props as i64);
        let shp = self
            .b
            .ins()
            .load(types::I64, flags(), pm, self.lay.pm_shape as i32);
        let sid = self
            .b
            .ins()
            .load(types::I64, flags(), shp, self.lay.shape_id as i32);
        let ic = &self.proto.ics[self.proto.ic_index[ip] as usize] as *const _ as i64
            + std::mem::offset_of!(crate::bytecode::PropIc, entries) as i64;
        let e = self.b.ins().iconst(types::I64, ic);
        let esh = self.b.ins().load(
            types::I64,
            flags(),
            e,
            std::mem::offset_of!(crate::bytecode::IcEntry, shape) as i32,
        );
        let same = self.b.ins().icmp(IntCC::Equal, sid, esh);
        self.guard(same, fail);
        let holder = self.b.ins().load(
            types::I64,
            flags(),
            e,
            std::mem::offset_of!(crate::bytecode::IcEntry, holder) as i32,
        );
        let own = self.b.ins().icmp_imm_s(IntCC::Equal, holder, 0);
        self.guard(own, fail);
        let acc = self
            .b
            .ins()
            .uload8(types::I32, flags(), pm, self.lay.pm_accessors as i32);
        let plain = self.b.ins().icmp_imm_s(IntCC::Equal, acc, 0);
        self.guard(plain, fail);
        let idx = self.b.ins().uload32(
            flags(),
            e,
            std::mem::offset_of!(crate::bytecode::IcEntry, index) as i32,
        );
        let vals = self
            .b
            .ins()
            .load(types::I64, flags(), pm, self.lay.pm_values as i32);
        let off = self.b.ins().ishl_imm_u(idx, 4);
        let addr = self.b.ins().iadd(vals, off);
        let vh = self.b.ins().load(types::I64, flags(), addr, 0);
        let vp = self.b.ins().load(types::F64, flags(), addr, 8);
        let tag = self.b.ins().band_imm_u(vh, 0xff);
        let ok = self.b.ins().icmp_imm_s(IntCC::NotEqual, tag, T_UNINIT);
        self.guard(ok, fail);
        (vh, vp)
    }

    fn not_builder(&mut self, k: usize, t: Ty, slow: Block) {
        if t == Ty::Any || t == Ty::Prim {
            let tg = self.tag(k);
            let ok = self.b.ins().icmp_imm_s(IntCC::NotEqual, tg, T_UNDEFINED);
            self.guard(ok, slow);
        }
    }

    fn i32_of(&mut self, k: usize) -> V {
        self.b.use_var(self.ival[k])
    }

    fn set_int(&mut self, k: usize, v: V) {
        self.b.def_var(self.ival[k], v);
    }

    fn set_f64(&mut self, k: usize, t: Ty, v: V) {
        if t != Ty::Num {
            let h = self.b.ins().iconst(types::I64, T_NUMBER);
            self.b.def_var(self.head[k], h);
        }
        self.b.def_var(self.pay[k], v);
    }

    fn set_head(&mut self, k: usize, tag: i64) {
        let h = self.b.ins().iconst(types::I64, tag);
        self.b.def_var(self.head[k], h);
    }

    fn set_bool(&mut self, k: usize, v: V) {
        let w = self.b.ins().uextend(types::I64, v);
        let sh = self.b.ins().ishl_imm_u(w, 8);
        let h = self.b.ins().bor_imm_u(sh, T_BOOL);
        self.b.def_var(self.head[k], h);
    }

    fn copy(&mut self, from: usize, to: usize, t: Ty) {
        if t == Ty::Int {
            let v = self.b.use_var(self.ival[from]);
            self.b.def_var(self.ival[to], v);
            return;
        }
        if t != Ty::Num {
            let h = self.b.use_var(self.head[from]);
            self.b.def_var(self.head[to], h);
        }
        let p = self.b.use_var(self.pay[from]);
        self.b.def_var(self.pay[to], p);
    }

    fn convert(&mut self, k: usize, from: Ty, to: Ty) {
        match (from, to) {
            (Ty::Int, Ty::Num) | (Ty::Int, Ty::Any) => {
                let f = self.f64_of(k, Ty::Int);
                self.set_f64(k, to, f);
            }
            (Ty::Num, Ty::Any) => self.set_head(k, T_NUMBER),
            _ => {}
        }
    }

    fn goto_from(&mut self, cur: &[Ty], target: usize) {
        let want = self.ty_at(target);
        let mut fail = None;
        for k in 0..want.len() {
            if join(cur[k], want[k]) != want[k] {
                let f = *fail.get_or_insert_with(|| {
                    let blk = self.b.create_block();
                    self.narrows.push((blk, target, cur.to_vec()));
                    blk
                });
                self.narrow(k, cur[k], want[k], f);
            } else {
                self.convert(k, cur[k], want[k]);
            }
        }
        self.b.ins().jump(self.blocks[target], &[]);
    }

    fn narrow(&mut self, k: usize, from: Ty, to: Ty, fail: Block) {
        if from == Ty::Any || from == Ty::Prim {
            let tag = self.tag(k);
            let ok = match to {
                Ty::Prim => {
                    let off = self.b.ins().iadd_imm_s(tag, -3);
                    self.b.ins().icmp_imm_s(IntCC::UnsignedGreaterThan, off, 4)
                }
                _ => self.b.ins().icmp_imm_s(IntCC::Equal, tag, T_NUMBER),
            };
            self.guard(ok, fail);
        }
        if to == Ty::Int {
            let p = self.b.use_var(self.pay[k]);
            let i = self.b.ins().fcvt_to_sint_sat(types::I32, p);
            let back = self.b.ins().fcvt_from_sint(types::F64, i);
            let pb = self.b.ins().bitcast(types::I64, MemFlagsData::new(), p);
            let bb = self.b.ins().bitcast(types::I64, MemFlagsData::new(), back);
            let ok = self.b.ins().icmp(IntCC::Equal, pb, bb);
            self.guard(ok, fail);
            self.b.def_var(self.ival[k], i);
        }
    }

    fn branch(&mut self, cond: V, cur_yes: &[Ty], yes: usize, cur_no: &[Ty], no: usize) {
        let by = self.b.create_block();
        let bn = self.b.create_block();
        self.b.ins().brif(cond, by, &[], bn, &[]);
        self.b.switch_to_block(by);
        self.goto_from(cur_yes, yes);
        self.b.switch_to_block(bn);
        self.goto_from(cur_no, no);
    }

    fn spill(&mut self, cur: &[Ty], from: usize, to: usize) {
        let bp = self.bp();
        let dirty = &self.dirty[self.spill_at];
        let marked: Vec<bool> = (0..to)
            .map(|k| dirty.get(k / 64).is_none_or(|w| w >> (k % 64) & 1 == 1))
            .collect();
        for (k, &t) in cur.iter().enumerate().take(to).skip(from) {
            if !marked[k] {
                continue;
            }
            let off = k as i32 * SLOT;
            let (h, p) = match t {
                Ty::Int | Ty::Num => (self.b.ins().iconst(types::I64, T_NUMBER), self.f64_of(k, t)),
                _ => (self.b.use_var(self.head[k]), self.b.use_var(self.pay[k])),
            };
            self.b.ins().store(flags(), h, bp, off);
            self.b.ins().store(flags(), p, bp, off + 8);
        }
    }

    fn deopt_exit(&mut self, kind: u64, ip: usize) -> Block {
        let blk = self.b.create_block();
        self.exits.push((blk, kind | ip as u64));
        blk
    }

    fn guard(&mut self, ok: V, fail: Block) {
        let cont = self.b.create_block();
        self.b.ins().brif(ok, cont, &[], fail, &[]);
        self.b.switch_to_block(cont);
    }

    fn fill(&mut self, tys: &[Ty], ip: usize) {
        let mut fail = None;
        let bp = self.bp();
        for (k, &t) in tys.iter().enumerate() {
            let off = k as i32 * SLOT;
            let h = self.b.ins().load(types::I64, flags(), bp, off);
            let p = self.b.ins().load(types::F64, flags(), bp, off + 8);
            match t {
                Ty::Prim | Ty::Any => {
                    self.b.def_var(self.head[k], h);
                    self.b.def_var(self.pay[k], p);
                }
                Ty::Num | Ty::Int => {
                    let tag = self.b.ins().band_imm_u(h, 0xff);
                    let ok = self.b.ins().icmp_imm_s(IntCC::Equal, tag, T_NUMBER);
                    let f = *fail.get_or_insert_with(|| self.deopt_exit(DEOPT_TYPE, ip));
                    self.guard(ok, f);
                    if t == Ty::Num {
                        self.b.def_var(self.pay[k], p);
                    } else {
                        let i = self.b.ins().fcvt_to_sint_sat(types::I32, p);
                        let back = self.b.ins().fcvt_from_sint(types::F64, i);
                        let pb = self.b.ins().bitcast(types::I64, MemFlagsData::new(), p);
                        let bb = self.b.ins().bitcast(types::I64, MemFlagsData::new(), back);
                        let ok = self.b.ins().icmp(IntCC::Equal, pb, bb);
                        self.guard(ok, f);
                        self.b.def_var(self.ival[k], i);
                    }
                }
            }
        }
    }

    fn ret_value(&mut self, v: V) {
        self.b.ins().jump(self.ret, &[BlockArg::Value(v)]);
    }

    fn bail(&mut self, ip: usize, cur: &[Ty]) {
        self.spill(cur, 0, cur.len());
        let v = self.b.ins().iconst(types::I64, (NEXT | ip as u64) as i64);
        self.ret_value(v);
    }

    fn reload_bp(&mut self) {
        let ctx = self.b.use_var(self.ctx_var);
        let bp = self.b.ins().load(types::I64, flags(), ctx, 8);
        self.b.def_var(self.bp_var, bp);
    }

    fn helper(&mut self, sig: SigRef, f: usize, args: &[i64]) -> V {
        let ctx = self.b.use_var(self.ctx_var);
        let fv = self.b.ins().iconst(types::I64, f as i64);
        let mut vals = vec![ctx];
        for &a in args {
            vals.push(self.b.ins().iconst(types::I32, a));
        }
        let call = self.b.ins().call_indirect(sig, fv, &vals);
        let r = self.b.inst_results(call)[0];
        self.reload_bp();
        r
    }

    fn refill(&mut self, s: usize, inn: &[Ty], lo: usize, local: Option<usize>) {
        let want = self.ty_at(s);
        let mut cur = Vec::with_capacity(want.len());
        for k in 0..want.len() {
            if k < lo
                && k < inn.len()
                && !self.volatile.get(k).copied().unwrap_or(true)
                && local != Some(k)
            {
                cur.push(inn[k]);
            } else {
                self.load_slot(k);
                cur.push(Ty::Any);
            }
        }
        self.goto_from(&cur, s);
    }

    fn resume(&mut self, r: V, succs: &[usize], ip: usize, inn: &[Ty]) {
        let op = self.proto.code[ip];
        for &s in succs {
            let hit = self.b.ins().icmp_imm_s(IntCC::Equal, r, s as i64);
            let yes = self.b.create_block();
            let miss = self.b.create_block();
            self.b.ins().brif(hit, yes, &[], miss, &[]);
            self.b.switch_to_block(yes);
            let (lo, local) = if inline_op(op) {
                let da = self.ty_at(s).len();
                (touched_from(op, inn.len(), da), local_of(op))
            } else {
                (0, None)
            };
            self.refill(s, inn, lo, local);
            self.b.switch_to_block(miss);
        }
        let kind = self.b.ins().ushr_imm_u(r, 32);
        let err = self.b.ins().icmp_imm_s(IntCC::Equal, kind, 2);
        let done = self.b.ins().icmp_imm_s(IntCC::Equal, kind, 3);
        let pass = self.b.ins().bor(err, done);
        let reload = self.b.ins().iconst(types::I64, RELOAD as i64);
        let out = self.b.ins().select(pass, r, reload);
        self.ret_value(out);
    }

    fn step(&mut self, ip: usize, inn: &[Ty], succs: &[usize]) {
        self.spill(inn, 0, inn.len());
        let r = self.helper(
            self.sigs.step,
            jit_step as *const () as usize,
            &[ip as i64, inn.len() as i64],
        );
        self.resume(r, succs, ip, inn);
    }

    fn slow_block(&mut self, ip: usize, inn: &[Ty], succs: &[usize]) -> Block {
        if let Some((at, blk)) = self.slow_memo
            && at == ip
        {
            return blk;
        }
        let blk = self.b.create_block();
        self.slow.push((blk, ip, inn.to_vec(), succs.to_vec()));
        self.slow_memo = Some((ip, blk));
        blk
    }

    fn overflow_block(&mut self, ip: usize, inn: &[Ty]) -> Block {
        let blk = self.b.create_block();
        self.deopts.push((blk, ip, inn.to_vec()));
        blk
    }

    fn flush(&mut self) {
        self.slow_memo = None;
        while let Some((blk, ip, inn, succs)) = self.slow.pop() {
            self.b.switch_to_block(blk);
            self.step(ip, &inn, &succs);
        }
        while let Some((blk, status)) = self.exits.pop() {
            self.b.switch_to_block(blk);
            let v = self.b.ins().iconst(types::I64, status as i64);
            self.ret_value(v);
        }
        while let Some((blk, ip, cur)) = self.narrows.pop() {
            self.b.switch_to_block(blk);
            let at = std::mem::replace(&mut self.spill_at, ip);
            self.spill(&cur, 0, cur.len());
            self.spill_at = at;
            let v = self
                .b
                .ins()
                .iconst(types::I64, (DEOPT_TYPE | ip as u64) as i64);
            self.ret_value(v);
        }
        while let Some((blk, ip, inn)) = self.deopts.pop() {
            self.b.switch_to_block(blk);
            self.spill(&inn, 0, inn.len());
            let v = self
                .b
                .ins()
                .iconst(types::I64, (DEOPT_OVERFLOW | ip as u64) as i64);
            self.ret_value(v);
        }
    }

    fn number_guard(&mut self, k: usize, t: Ty, slow: Block) {
        if t == Ty::Prim || t == Ty::Any {
            let ok = self.tag_is(k, T_NUMBER);
            self.guard(ok, slow);
        }
    }

    fn truthy(&mut self, k: usize, t: Ty, slow: Block) -> V {
        match t {
            Ty::Int => {
                let i = self.i32_of(k);
                return self.b.ins().icmp_imm_s(IntCC::NotEqual, i, 0);
            }
            Ty::Num => {
                let x = self.f64_of(k, t);
                let zero = self.b.ins().f64const(0.0);
                return self.b.ins().fcmp(FloatCC::OrderedNotEqual, x, zero);
            }
            _ => {}
        }
        let tg = self.tag(k);
        let done = self.b.create_block();
        self.b.append_block_param(done, types::I8);
        let not_bool = self.b.create_block();
        let not_num = self.b.create_block();
        let bool_block = self.b.create_block();
        let num_block = self.b.create_block();
        let falsy = self.b.create_block();
        let is_bool = self.b.ins().icmp_imm_s(IntCC::Equal, tg, T_BOOL);
        self.b.ins().brif(is_bool, bool_block, &[], not_bool, &[]);
        self.b.switch_to_block(bool_block);
        let h = self.b.use_var(self.head[k]);
        let bv = self.b.ins().ushr_imm_u(h, 8);
        let bv = self.b.ins().band_imm_u(bv, 0xff);
        let bt = self.b.ins().icmp_imm_s(IntCC::NotEqual, bv, 0);
        self.b.ins().jump(done, &[BlockArg::Value(bt)]);
        self.b.switch_to_block(not_bool);
        let is_n = self.b.ins().icmp_imm_s(IntCC::Equal, tg, T_NUMBER);
        self.b.ins().brif(is_n, num_block, &[], not_num, &[]);
        self.b.switch_to_block(num_block);
        let x = self.b.use_var(self.pay[k]);
        let zero = self.b.ins().f64const(0.0);
        let nt = self.b.ins().fcmp(FloatCC::OrderedNotEqual, x, zero);
        self.b.ins().jump(done, &[BlockArg::Value(nt)]);
        self.b.switch_to_block(not_num);
        let nullish = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThanOrEqual, tg, T_NULL);
        self.b.ins().brif(nullish, falsy, &[], slow, &[]);
        self.b.switch_to_block(falsy);
        let f = self.b.ins().iconst(types::I8, 0);
        self.b.ins().jump(done, &[BlockArg::Value(f)]);
        self.b.switch_to_block(done);
        self.b.block_params(done)[0]
    }

    fn int_mode(&self, ip: usize) -> bool {
        self.ints && !self.banned[ip]
    }

    fn int_arith(&mut self, op: Op, a: V, b: V, ovf: Block) -> V {
        let (r, of) = match op {
            Op::Sub | Op::SubConst(_) => self.b.ins().ssub_overflow(a, b),
            Op::Mul | Op::MulConst(_) => self.b.ins().smul_overflow(a, b),
            _ => self.b.ins().sadd_overflow(a, b),
        };
        let mut bad = of;
        if matches!(op, Op::Mul | Op::MulConst(_)) {
            let z = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
            let na = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, a, 0);
            let nb = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, b, 0);
            let neg = self.b.ins().bor(na, nb);
            let negz = self.b.ins().band(z, neg);
            bad = self.b.ins().bor(bad, negz);
        }
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, bad, 0);
        self.guard(ok, ovf);
        r
    }

    fn int_rem(&mut self, a: V, b: V, ovf: Block) -> V {
        let nz = self.b.ins().icmp_imm_s(IntCC::NotEqual, b, 0);
        self.guard(nz, ovf);
        let r = self.b.ins().srem(a, b);
        let z = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
        let na = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, a, 0);
        let negz = self.b.ins().band(z, na);
        let ok = self.b.ins().icmp_imm_s(IntCC::Equal, negz, 0);
        self.guard(ok, ovf);
        r
    }

    fn f64_rem(&mut self, x: V, y: V) -> V {
        let f = self
            .b
            .ins()
            .iconst(types::I64, jit_rem as *const () as usize as i64);
        let call = self.b.ins().call_indirect(self.sigs.rem, f, &[x, y]);
        self.b.inst_results(call)[0]
    }

    fn op(&mut self, proto: &FunctionProto, ip: usize, op: Op, succs: &[usize]) {
        let inn = self.ty_at(ip);
        let d = inn.len();
        let next = ip + 1;
        let mut cur = inn.clone();
        match op {
            Op::Const(k) => match &proto.consts[k as usize] {
                Value::Number(x) => {
                    match int_value(*x).filter(|_| self.ints) {
                        Some(i) => {
                            let v = self.b.ins().iconst(types::I32, i as i64);
                            self.set_int(d, v);
                            cur.push(Ty::Int);
                        }
                        None => {
                            let v = self.b.ins().f64const(*x);
                            self.set_f64(d, Ty::Num, v);
                            cur.push(Ty::Num);
                        }
                    }
                    self.goto_from(&cur, next);
                }
                _ => self.step(ip, &inn, succs),
            },
            Op::Undefined | Op::Null | Op::Uninit | Op::True | Op::False => {
                match op {
                    Op::True | Op::False => {
                        let v = self.b.ins().iconst(types::I8, (op == Op::True) as i64);
                        self.set_bool(d, v);
                    }
                    Op::Undefined => self.set_head(d, T_UNDEFINED),
                    Op::Null => self.set_head(d, T_NULL),
                    _ => self.set_head(d, T_UNINIT),
                }
                cur.push(Ty::Prim);
                self.goto_from(&cur, next);
            }
            Op::UndefinedN(n) | Op::UninitN(n) => {
                let t = if matches!(op, Op::UninitN(_)) {
                    T_UNINIT
                } else {
                    T_UNDEFINED
                };
                for k in 0..n as usize {
                    self.set_head(d + k, t);
                    cur.push(Ty::Prim);
                }
                self.goto_from(&cur, next);
            }
            Op::Pop => {
                self.refcount(d - 1, inn[d - 1], jit_drop as *const () as usize);
                cur.pop();
                self.goto_from(&cur, next);
            }
            Op::Dup => {
                let t = inn[d - 1];
                self.copy(d - 1, d, t);
                self.refcount(d, t, jit_clone as *const () as usize);
                cur.push(t);
                self.goto_from(&cur, next);
            }
            Op::Swap => {
                let pairs = [
                    (self.head[d - 1], self.head[d - 2]),
                    (self.pay[d - 1], self.pay[d - 2]),
                    (self.ival[d - 1], self.ival[d - 2]),
                ];
                for (x, y) in pairs {
                    let a = self.b.use_var(x);
                    let c = self.b.use_var(y);
                    self.b.def_var(x, c);
                    self.b.def_var(y, a);
                }
                cur.swap(d - 1, d - 2);
                self.goto_from(&cur, next);
            }
            Op::GetLocal(l) | Op::GetLocalUndef(l) | Op::GetLocalChecked(l, _) => {
                let l = l as usize;
                let t = inn[l];
                let checked = matches!(op, Op::GetLocalChecked(..));
                if t == Ty::Any || t == Ty::Prim {
                    let slow = self.slow_block(ip, &inn, succs);
                    self.not_builder(l, t, slow);
                    if checked {
                        let tg = self.tag(l);
                        let ok = self.b.ins().icmp_imm_s(IntCC::NotEqual, tg, T_UNINIT);
                        self.guard(ok, slow);
                    }
                }
                self.copy(l, d, t);
                self.refcount(d, t, jit_clone as *const () as usize);
                cur.push(t);
                if matches!(op, Op::GetLocalUndef(_)) {
                    self.set_head(d + 1, T_UNDEFINED);
                    cur.push(Ty::Prim);
                }
                self.goto_from(&cur, next);
            }
            Op::SetLocal(l) | Op::StoreLocal(l) => {
                let l = l as usize;
                let (top, loc) = (inn[d - 1], inn[l]);
                let store = matches!(op, Op::StoreLocal(_));
                if loc == Ty::Any || loc == Ty::Prim {
                    let slow = self.slow_block(ip, &inn, succs);
                    self.not_builder(l, loc, slow);
                }
                self.refcount(l, loc, jit_drop as *const () as usize);
                self.copy(d - 1, l, top);
                if !store {
                    self.refcount(l, top, jit_clone as *const () as usize);
                }
                cur[l] = top;
                if store {
                    cur.pop();
                }
                self.goto_from(&cur, next);
            }
            Op::GetIndex => {
                let (ta, tk) = (inn[d - 2], inn[d - 1]);
                if !matches!(ta, Ty::Any) {
                    self.step(ip, &inn, succs);
                    return;
                }
                let slow = self.slow_block(ip, &inn, succs);
                let help = self.b.create_block();
                let done = self.b.create_block();
                let (oh, op_) = self.bits(d - 2, ta);
                let objp = self.object_ptr(oh, op_, help);
                let (ptr, len) = self.array_vec(objp, 72, help);
                let i = self.index_of(d - 1, tk, help);
                let inb = self.b.ins().icmp(IntCC::UnsignedLessThan, i, len);
                self.guard(inb, help);
                let off = self.b.ins().ishl_imm_u(i, 4);
                let addr = self.b.ins().iadd(ptr, off);
                let vh = self.b.ins().load(types::I64, flags(), addr, 0);
                let vp = self.b.ins().load(types::F64, flags(), addr, 8);
                let vpb = self.b.ins().bitcast(types::I64, MemFlagsData::new(), vp);
                self.ref_change(vh, vpb, true);
                self.ref_change(oh, op_, false);
                self.b.def_var(self.head[d - 2], vh);
                self.b.def_var(self.pay[d - 2], vp);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(help);
                let (kh, kp) = self.bits(d - 1, tk);
                let (oh2, op2) = self.bits(d - 2, ta);
                let dst = self.konst((d - 2) as i64);
                let zero = self.konst(0);
                let r = self.call_six(
                    jit_index as *const () as usize,
                    [oh2, op2, kh, kp, dst, zero],
                );
                let ok = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
                self.guard(ok, slow);
                self.load_slot(d - 2);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
                cur.truncate(d - 1);
                cur[d - 2] = Ty::Any;
                self.goto_from(&cur, next);
            }
            Op::SetIndex => {
                let (ta, tk, tv) = (inn[d - 3], inn[d - 2], inn[d - 1]);
                if !matches!(ta, Ty::Any) {
                    self.step(ip, &inn, succs);
                    return;
                }
                let slow = self.slow_block(ip, &inn, succs);
                let help = self.b.create_block();
                let done = self.b.create_block();
                let (oh, op_) = self.bits(d - 3, ta);
                let objp = self.object_ptr(oh, op_, help);
                let (ptr, len) = self.array_vec(objp, 74, help);
                let i = self.index_of(d - 2, tk, help);
                let inb = self.b.ins().icmp(IntCC::UnsignedLessThan, i, len);
                self.guard(inb, help);
                let off = self.b.ins().ishl_imm_u(i, 4);
                let addr = self.b.ins().iadd(ptr, off);
                let old_h = self.b.ins().load(types::I64, flags(), addr, 0);
                let old_p = self.b.ins().load(types::I64, flags(), addr, 8);
                let (vh, vp) = self.bits(d - 1, tv);
                if tv == Ty::Any {
                    self.ref_change(vh, vp, true);
                }
                self.b.ins().store(flags(), vh, addr, 0);
                self.b.ins().store(flags(), vp, addr, 8);
                self.ref_change(old_h, old_p, false);
                self.ref_change(oh, op_, false);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(help);
                let (oh2, op2) = self.bits(d - 3, ta);
                let (kh, kp) = self.bits(d - 2, tk);
                let (vh2, vp2) = self.bits(d - 1, tv);
                let r = self.call_six(
                    jit_set_index as *const () as usize,
                    [oh2, op2, kh, kp, vh2, vp2],
                );
                let ok = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
                self.guard(ok, slow);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
                self.copy(d - 1, d - 3, tv);
                cur.truncate(d - 2);
                cur[d - 3] = tv;
                self.goto_from(&cur, next);
            }
            Op::GetProp(k)
            | Op::GetMethod(k)
            | Op::GetLocalProp(_, k)
            | Op::GetLocalMethod(_, k) => {
                let (src, mode, dst) = match op {
                    Op::GetLocalProp(s, _) => (s as usize, 0, d),
                    Op::GetProp(_) => (d - 1, 1, d - 1),
                    Op::GetLocalMethod(s, _) => (s as usize, 2, d),
                    _ => (d - 1, 3, d - 1),
                };
                let ts = inn[src];
                let slow = self.slow_block(ip, &inn, succs);
                if matches!(op, Op::GetLocalProp(..) | Op::GetLocalMethod(..)) {
                    self.not_builder(src, ts, slow);
                }
                let help = self.b.create_block();
                let done = self.b.create_block();
                let length =
                    matches!(&self.proto.consts[k as usize], Value::Str(s) if &**s == "length");
                if mode < 2 && ts == Ty::Any {
                    let (oh, op_) = self.bits(src, ts);
                    let objp = self.object_ptr(oh, op_, help);
                    if length {
                        let (_, len) = self.array_vec(objp, 64, help);
                        let f = self.b.ins().fcvt_from_uint(types::F64, len);
                        if mode == 1 {
                            self.ref_change(oh, op_, false);
                        }
                        self.set_f64(dst, Ty::Any, f);
                    } else {
                        let (vh, vp) = self.own_prop(objp, ip, help);
                        let vpb = self.b.ins().bitcast(types::I64, MemFlagsData::new(), vp);
                        self.ref_change(vh, vpb, true);
                        if mode == 1 {
                            self.ref_change(oh, op_, false);
                        }
                        self.b.def_var(self.head[dst], vh);
                        self.b.def_var(self.pay[dst], vp);
                    }
                    self.b.ins().jump(done, &[]);
                } else {
                    self.b.ins().jump(help, &[]);
                }
                self.b.switch_to_block(help);
                let (oh, op_) = self.bits(src, ts);
                let modev = self.konst(mode);
                let dstv = self.konst(dst as i64);
                let ic = &self.proto.ics[self.proto.ic_index[ip] as usize] as *const _ as i64;
                let icv = self.konst(ic);
                let key = &self.proto.consts[k as usize] as *const Value as i64;
                let keyv = self.konst(key);
                let r = self.call_six(
                    jit_prop as *const () as usize,
                    [oh, op_, modev, dstv, icv, keyv],
                );
                let ok = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
                self.guard(ok, slow);
                self.load_slot(dst);
                if mode >= 2 {
                    self.load_slot(dst + 1);
                }
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
                match op {
                    Op::GetLocalProp(..) => cur.push(Ty::Any),
                    Op::GetProp(_) => cur[d - 1] = Ty::Any,
                    Op::GetLocalMethod(..) => {
                        cur.push(Ty::Any);
                        cur.push(Ty::Any);
                    }
                    _ => {
                        cur[d - 1] = Ty::Any;
                        cur.push(Ty::Any);
                    }
                }
                self.goto_from(&cur, next);
            }
            Op::NewObject => {
                let dst = self.konst(d as i64);
                let zero = self.konst(0);
                let r = self.call_six(
                    jit_new_object as *const () as usize,
                    [dst, zero, zero, zero, zero, zero],
                );
                let slow = self.slow_block(ip, &inn, succs);
                let ok = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
                self.guard(ok, slow);
                self.load_slot(d);
                cur.push(Ty::Any);
                self.goto_from(&cur, next);
            }
            Op::InitProp(k) => {
                let (oh, op_) = self.bits(d - 2, inn[d - 2]);
                let (vh, vp) = self.bits(d - 1, inn[d - 1]);
                let key = &self.proto.consts[k as usize] as *const Value as i64;
                let keyv = self.konst(key);
                let zero = self.konst(0);
                let r = self.call_six(
                    jit_init_prop as *const () as usize,
                    [oh, op_, vh, vp, keyv, zero],
                );
                let slow = self.slow_block(ip, &inn, succs);
                let ok = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
                self.guard(ok, slow);
                cur.pop();
                self.goto_from(&cur, next);
            }
            Op::GetGlobal(k) => {
                let cache = &self.proto.global_cache[k as usize] as *const _ as i64;
                let cv = self.konst(cache);
                let dst = self.konst(d as i64);
                let zero = self.konst(0);
                let r = self.call_six(
                    jit_global as *const () as usize,
                    [cv, dst, zero, zero, zero, zero],
                );
                let slow = self.slow_block(ip, &inn, succs);
                let ok = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
                self.guard(ok, slow);
                self.load_slot(d);
                cur.push(Ty::Any);
                self.goto_from(&cur, next);
            }
            Op::IncLocal(l) | Op::DecLocal(l) => {
                let l = l as usize;
                let t = inn[l];
                let delta = if matches!(op, Op::IncLocal(_)) { 1 } else { -1 };
                if t == Ty::Int && self.int_mode(ip) {
                    let ovf = self.overflow_block(ip, &inn);
                    let a = self.i32_of(l);
                    let c = self.b.ins().iconst(types::I32, delta);
                    let r = self.int_arith(Op::Add, a, c, ovf);
                    self.set_int(l, r);
                } else {
                    let slow = self.slow_block(ip, &inn, succs);
                    self.number_guard(l, t, slow);
                    let x = self.f64_of(l, t);
                    let c = self.b.ins().f64const(delta as f64);
                    let r = self.b.ins().fadd(x, c);
                    let nt = if is_num(t) { Ty::Num } else { Ty::Any };
                    self.set_f64(l, nt, r);
                    cur[l] = nt;
                }
                self.goto_from(&cur, next);
            }
            Op::AppendLocal(_) | Op::Add | Op::Sub | Op::Mul | Op::Mod | Op::Div => {
                let (dst, a_k, b_k) = match op {
                    Op::AppendLocal(l) => (l as usize, l as usize, d - 1),
                    _ => (d - 2, d - 2, d - 1),
                };
                let (ta, tb) = (inn[a_k], inn[b_k]);
                let arith = match op {
                    Op::AppendLocal(_) => Op::Add,
                    o => o,
                };
                if ta == Ty::Int && tb == Ty::Int && self.int_mode(ip) && arith != Op::Div {
                    let ovf = self.overflow_block(ip, &inn);
                    let a = self.i32_of(a_k);
                    let b = self.i32_of(b_k);
                    let r = if arith == Op::Mod {
                        self.int_rem(a, b, ovf)
                    } else {
                        self.int_arith(arith, a, b, ovf)
                    };
                    self.set_int(dst, r);
                    cur.truncate(d - 1);
                    if dst != d - 2 {
                        cur[dst] = Ty::Int;
                    } else {
                        cur[d - 2] = Ty::Int;
                    }
                } else {
                    let slow = self.slow_block(ip, &inn, succs);
                    self.number_guard(a_k, ta, slow);
                    self.number_guard(b_k, tb, slow);
                    let x = self.f64_of(a_k, ta);
                    let y = self.f64_of(b_k, tb);
                    let r = match arith {
                        Op::Add => self.b.ins().fadd(x, y),
                        Op::Sub => self.b.ins().fsub(x, y),
                        Op::Mul => self.b.ins().fmul(x, y),
                        Op::Div => self.b.ins().fdiv(x, y),
                        _ => self.f64_rem(x, y),
                    };
                    let nt = if is_num(ta) && is_num(tb) {
                        Ty::Num
                    } else {
                        Ty::Any
                    };
                    self.set_f64(dst, nt, r);
                    cur.truncate(d - 1);
                    cur[dst] = nt;
                }
                self.goto_from(&cur, next);
            }
            Op::AddConst(k) | Op::SubConst(k) | Op::MulConst(k) | Op::ModConst(k) => {
                let Value::Number(c) = proto.consts[k as usize] else {
                    self.step(ip, &inn, succs);
                    return;
                };
                let t = inn[d - 1];
                let ci = int_value(c).filter(|&c| !matches!(op, Op::ModConst(_)) || c != 0);
                match ci {
                    Some(c) if t == Ty::Int && self.int_mode(ip) => {
                        let ovf = self.overflow_block(ip, &inn);
                        let a = self.i32_of(d - 1);
                        let cv = self.b.ins().iconst(types::I32, c as i64);
                        let r = if matches!(op, Op::ModConst(_)) {
                            self.int_rem(a, cv, ovf)
                        } else {
                            self.int_arith(op, a, cv, ovf)
                        };
                        self.set_int(d - 1, r);
                    }
                    _ => {
                        let slow = self.slow_block(ip, &inn, succs);
                        self.number_guard(d - 1, t, slow);
                        let x = self.f64_of(d - 1, t);
                        let cv = self.b.ins().f64const(c);
                        let r = match op {
                            Op::AddConst(_) => self.b.ins().fadd(x, cv),
                            Op::SubConst(_) => self.b.ins().fsub(x, cv),
                            Op::MulConst(_) => self.b.ins().fmul(x, cv),
                            _ => self.f64_rem(x, cv),
                        };
                        let nt = if is_num(t) { Ty::Num } else { Ty::Any };
                        self.set_f64(d - 1, nt, r);
                        cur[d - 1] = nt;
                    }
                }
                self.goto_from(&cur, next);
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr | Op::UShr => {
                let (ta, tb) = (inn[d - 2], inn[d - 1]);
                if ta == Ty::Int && tb == Ty::Int && self.ints {
                    let a = self.i32_of(d - 2);
                    let b = self.i32_of(d - 1);
                    let r = match op {
                        Op::BitAnd => self.b.ins().band(a, b),
                        Op::BitOr => self.b.ins().bor(a, b),
                        Op::BitXor => self.b.ins().bxor(a, b),
                        Op::Shl => self.b.ins().ishl(a, b),
                        Op::Shr => self.b.ins().sshr(a, b),
                        _ => self.b.ins().ushr(a, b),
                    };
                    cur.truncate(d - 1);
                    if op == Op::UShr {
                        let f = self.b.ins().fcvt_from_uint(types::F64, r);
                        self.set_f64(d - 2, Ty::Num, f);
                        cur[d - 2] = Ty::Num;
                    } else {
                        self.set_int(d - 2, r);
                        cur[d - 2] = Ty::Int;
                    }
                } else {
                    let slow = self.slow_block(ip, &inn, succs);
                    self.number_guard(d - 2, ta, slow);
                    self.number_guard(d - 1, tb, slow);
                    let x = self.f64_of(d - 2, ta);
                    let y = self.f64_of(d - 1, tb);
                    let code = match op {
                        Op::BitAnd => 0,
                        Op::BitOr => 1,
                        Op::BitXor => 2,
                        Op::Shl => 3,
                        Op::Shr => 4,
                        _ => 5,
                    };
                    let f = self
                        .b
                        .ins()
                        .iconst(types::I64, jit_bits as *const () as usize as i64);
                    let cv = self.b.ins().iconst(types::I32, code);
                    let call = self.b.ins().call_indirect(self.sigs.bits, f, &[cv, x, y]);
                    let r = self.b.inst_results(call)[0];
                    cur.truncate(d - 1);
                    let both = is_num(ta) && is_num(tb);
                    if both && self.ints && op != Op::UShr {
                        let i = self.b.ins().fcvt_to_sint_sat(types::I32, r);
                        self.set_int(d - 2, i);
                        cur[d - 2] = Ty::Int;
                    } else {
                        let nt = if both { Ty::Num } else { Ty::Any };
                        self.set_f64(d - 2, nt, r);
                        cur[d - 2] = nt;
                    }
                }
                self.goto_from(&cur, next);
            }
            Op::Neg | Op::Inc | Op::Dec | Op::ToNumeric | Op::ToNumber => {
                let t = inn[d - 1];
                if t == Ty::Int && (!matches!(op, Op::Neg | Op::Inc | Op::Dec) || self.int_mode(ip))
                {
                    if matches!(op, Op::Neg | Op::Inc | Op::Dec) {
                        let ovf = self.overflow_block(ip, &inn);
                        let a = self.i32_of(d - 1);
                        let r = if op == Op::Neg {
                            let z = self.b.ins().icmp_imm_s(IntCC::Equal, a, 0);
                            let m = self.b.ins().icmp_imm_s(IntCC::Equal, a, i32::MIN as i64);
                            let bad = self.b.ins().bor(z, m);
                            let ok = self.b.ins().icmp_imm_s(IntCC::Equal, bad, 0);
                            self.guard(ok, ovf);
                            self.b.ins().ineg(a)
                        } else {
                            let c = self
                                .b
                                .ins()
                                .iconst(types::I32, if op == Op::Inc { 1 } else { -1 });
                            self.int_arith(Op::Add, a, c, ovf)
                        };
                        self.set_int(d - 1, r);
                    }
                } else {
                    let slow = self.slow_block(ip, &inn, succs);
                    self.number_guard(d - 1, t, slow);
                    let x = self.f64_of(d - 1, t);
                    let r = match op {
                        Op::Neg => self.b.ins().fneg(x),
                        Op::Inc | Op::Dec => {
                            let one = self
                                .b
                                .ins()
                                .f64const(if op == Op::Inc { 1.0 } else { -1.0 });
                            self.b.ins().fadd(x, one)
                        }
                        _ => x,
                    };
                    let nt = if is_num(t) { Ty::Num } else { Ty::Any };
                    self.set_f64(d - 1, nt, r);
                    cur[d - 1] = nt;
                }
                self.goto_from(&cur, next);
            }
            Op::Lt | Op::Gt | Op::Le | Op::Ge | Op::StrictEq | Op::StrictNe => {
                let cmp = match op {
                    Op::Lt => Cmp::Lt,
                    Op::Gt => Cmp::Gt,
                    Op::Le => Cmp::Le,
                    Op::Ge => Cmp::Ge,
                    Op::StrictEq => Cmp::StrictEq,
                    _ => Cmp::StrictNe,
                };
                let r = self.compare(
                    ip,
                    &inn,
                    succs,
                    cmp,
                    d - 2,
                    inn[d - 2],
                    d - 1,
                    inn[d - 1],
                    None,
                );
                self.set_bool(d - 2, r);
                cur.truncate(d - 1);
                cur[d - 2] = Ty::Prim;
                self.goto_from(&cur, next);
            }
            Op::Not => {
                let slow = self.slow_block(ip, &inn, succs);
                let t = self.truthy(d - 1, inn[d - 1], slow);
                let r = self.b.ins().bxor_imm_u(t, 1);
                self.set_bool(d - 1, r);
                cur[d - 1] = Ty::Prim;
                self.goto_from(&cur, next);
            }
            Op::Jump(t) => {
                if (t as usize) <= ip {
                    let ctx = self.b.use_var(self.ctx_var);
                    let p = self.b.ins().load(types::I64, flags(), ctx, 32);
                    let flag = self.b.ins().uload8(types::I32, flags(), p, 0);
                    let stop = self.b.create_block();
                    let go = self.b.create_block();
                    self.b.ins().brif(flag, stop, &[], go, &[]);
                    self.b.switch_to_block(stop);
                    self.bail(ip, &inn);
                    self.b.switch_to_block(go);
                }
                self.goto_from(&cur, t as usize);
            }
            Op::JumpIfFalse(t)
            | Op::JumpIfTrue(t)
            | Op::JumpIfFalseKeep(t)
            | Op::JumpIfTrueKeep(t) => {
                let slow = self.slow_block(ip, &inn, succs);
                let v = self.truthy(d - 1, inn[d - 1], slow);
                let keep = matches!(op, Op::JumpIfFalseKeep(_) | Op::JumpIfTrueKeep(_));
                let mut popped = cur.clone();
                popped.pop();
                let taken = if keep { cur.clone() } else { popped.clone() };
                if matches!(op, Op::JumpIfTrue(_) | Op::JumpIfTrueKeep(_)) {
                    self.branch(v, &taken, t as usize, &popped, next);
                } else {
                    self.branch(v, &popped, next, &taken, t as usize);
                }
            }
            Op::JumpIfNotNullishKeep(t) | Op::JumpIfNotUndef(t) => {
                let mut popped = cur.clone();
                popped.pop();
                let ty = inn[d - 1];
                if is_num(ty) {
                    self.goto_from(&cur, t as usize);
                } else {
                    let tag = self.tag(d - 1);
                    let keep = if matches!(op, Op::JumpIfNotUndef(_)) {
                        self.b.ins().icmp_imm_s(IntCC::NotEqual, tag, T_UNDEFINED)
                    } else {
                        self.b
                            .ins()
                            .icmp_imm_s(IntCC::UnsignedGreaterThan, tag, T_NULL)
                    };
                    self.branch(keep, &cur, t as usize, &popped, next);
                }
            }
            Op::JumpIfNullish(t) => {
                if is_num(inn[d - 1]) {
                    self.goto_from(&cur, next);
                } else {
                    let tag = self.tag(d - 1);
                    let nullish =
                        self.b
                            .ins()
                            .icmp_imm_s(IntCC::UnsignedLessThanOrEqual, tag, T_NULL);
                    let yes = self.b.create_block();
                    let no = self.b.create_block();
                    self.b.ins().brif(nullish, yes, &[], no, &[]);
                    self.b.switch_to_block(yes);
                    self.set_head(d - 1, T_UNDEFINED);
                    let mut taken = cur.clone();
                    taken[d - 1] = Ty::Prim;
                    self.goto_from(&taken, t as usize);
                    self.b.switch_to_block(no);
                    self.goto_from(&cur, next);
                }
            }
            Op::CmpJump(cmp, t) => {
                let ok = self.compare(
                    ip,
                    &inn,
                    succs,
                    cmp,
                    d - 2,
                    inn[d - 2],
                    d - 1,
                    inn[d - 1],
                    None,
                );
                cur.truncate(d - 2);
                self.branch(ok, &cur.clone(), next, &cur, t as usize);
            }
            Op::CmpConstJump(cmp, k, t) => {
                let Value::Number(c) = proto.consts[k as usize] else {
                    self.step(ip, &inn, succs);
                    return;
                };
                let ok = self.compare(
                    ip,
                    &inn,
                    succs,
                    cmp,
                    d - 1,
                    inn[d - 1],
                    usize::MAX,
                    Ty::Num,
                    Some(c),
                );
                cur.truncate(d - 1);
                self.branch(ok, &cur.clone(), next, &cur, t as usize);
            }
            Op::PopTo(n) => {
                let n = n as usize;
                self.spill(&inn, n, d);
                self.helper(
                    self.sigs.pop_to,
                    jit_pop_to as *const () as usize,
                    &[n as i64, d as i64],
                );
                cur.truncate(n);
                self.goto_from(&cur, next);
            }
            Op::Call(argc, _) => {
                let ci = d - argc as usize - 2;
                let cands: Vec<(usize, Intrinsic)> = self
                    .intrinsics
                    .iter()
                    .filter(|(_, k)| k.arity() == argc)
                    .map(|(o, k)| (Rc::as_ptr(o) as usize, *k))
                    .collect();
                let generic = self.b.create_block();
                if !cands.is_empty() && inn[ci] == Ty::Any {
                    let tag = self.tag(ci);
                    let is_obj = self.b.ins().icmp_imm_s(IntCC::Equal, tag, 5);
                    self.guard(is_obj, generic);
                    for (k, &t) in inn.iter().enumerate().take(d).skip(ci + 2) {
                        if !is_num(t) {
                            let ok = self.tag_is(k, T_NUMBER);
                            self.guard(ok, generic);
                        }
                    }
                    let p = self.b.use_var(self.pay[ci]);
                    let bits = self.b.ins().bitcast(types::I64, MemFlagsData::new(), p);
                    let objp = self.b.ins().iadd_imm_s(bits, self.lay.rc_value as i64);
                    let done = self.b.create_block();
                    self.b.append_block_param(done, types::F64);
                    for (ptr, kind) in cands {
                        let hit = self.b.ins().icmp_imm_s(IntCC::Equal, objp, ptr as i64);
                        let yes = self.b.create_block();
                        let no = self.b.create_block();
                        self.b.ins().brif(hit, yes, &[], no, &[]);
                        self.b.switch_to_block(yes);
                        let x = self.f64_of(ci + 2, inn[ci + 2]);
                        let r = match kind {
                            Intrinsic::Floor => self.b.ins().floor(x),
                            Intrinsic::Ceil => self.b.ins().ceil(x),
                            Intrinsic::Trunc => self.b.ins().trunc(x),
                            Intrinsic::Abs => self.b.ins().fabs(x),
                            Intrinsic::Sqrt => self.b.ins().sqrt(x),
                            Intrinsic::Min | Intrinsic::Max => {
                                let y = self.f64_of(ci + 3, inn[ci + 3]);
                                if kind == Intrinsic::Min {
                                    self.b.ins().fmin(x, y)
                                } else {
                                    self.b.ins().fmax(x, y)
                                }
                            }
                        };
                        self.b.ins().jump(done, &[BlockArg::Value(r)]);
                        self.b.switch_to_block(no);
                    }
                    self.b.ins().jump(generic, &[]);
                    self.b.switch_to_block(done);
                    let r = self.b.block_params(done)[0];
                    let (fh, fp) = self.bits(ci, Ty::Any);
                    self.ref_change(fh, fp, false);
                    let (th, tp) = self.bits(ci + 1, inn[ci + 1]);
                    if inn[ci + 1] == Ty::Any {
                        self.ref_change(th, tp, false);
                    }
                    self.set_f64(ci, Ty::Any, r);
                    let mut out = inn[..ci].to_vec();
                    out.push(Ty::Any);
                    self.goto_from(&out, next);
                } else {
                    self.b.ins().jump(generic, &[]);
                }
                self.b.switch_to_block(generic);
                self.spill(&inn, 0, d);
                let ss = self.b.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    80,
                    3,
                ));
                let nctx = self.b.ins().stack_addr(types::I64, ss, 0);
                let civ = self.konst(ci as i64);
                let ipv = self.konst(ip as i64);
                let dv = self.konst(d as i64);
                let av = self.konst(argc as i64);
                let zero = self.konst(0);
                let direct = self.b.create_block();
                self.b.append_block_param(direct, types::I64);
                let framed = self.b.create_block();
                let fallback = self.b.create_block();
                if inn[ci] == Ty::Any {
                    self.inline_frame(ci, ip, argc, nctx, direct, framed, fallback);
                } else {
                    self.b.ins().jump(fallback, &[]);
                }
                self.b.switch_to_block(framed);
                let code = self.call_six(
                    jit_frame as *const () as usize,
                    [civ, ipv, nctx, dv, av, zero],
                );
                self.reload_bp();
                let join_b = self.b.create_block();
                self.b.append_block_param(join_b, types::I64);
                self.b
                    .ins()
                    .brif(code, direct, &[BlockArg::Value(code)], fallback, &[]);
                self.b.switch_to_block(direct);
                let code = self.b.block_params(direct)[0];
                let entry_ip = self.b.ins().iconst(types::I32, 0);
                let call = self
                    .b
                    .ins()
                    .call_indirect(self.sigs.main, code, &[nctx, entry_ip]);
                let r = self.b.inst_results(call)[0];
                let popped = self.b.ins().icmp_imm_s(IntCC::Equal, r, POPPED as i64);
                let fast = self.b.create_block();
                let other = self.b.create_block();
                self.b.ins().brif(popped, fast, &[], other, &[]);
                self.b.switch_to_block(fast);
                let nbp = self.b.ins().load(types::I64, flags(), nctx, 8);
                let bp = self
                    .b
                    .ins()
                    .iadd_imm_s(nbp, -((ci as i64 + 1) * SLOT as i64));
                let ctx = self.b.use_var(self.ctx_var);
                self.b.ins().store(flags(), bp, ctx, 8);
                self.b.def_var(self.bp_var, bp);
                let vm = self.b.ins().load(types::I64, flags(), ctx, 0);
                let off = self.vl.native_depth as i32;
                let nesting = self.b.ins().load(types::I64, flags(), vm, off);
                let nesting = self.b.ins().iadd_imm_s(nesting, -1);
                self.b.ins().store(flags(), nesting, vm, off);
                self.refill(next, &inn, ci, None);
                self.b.switch_to_block(other);
                let r = self.call_six(
                    jit_call_done as *const () as usize,
                    [r, ipv, dv, av, zero, zero],
                );
                self.reload_bp();
                self.b.ins().jump(join_b, &[BlockArg::Value(r)]);
                self.b.switch_to_block(fallback);
                let r = self.helper(
                    self.sigs.call,
                    jit_call as *const () as usize,
                    &[ip as i64, d as i64, argc as i64],
                );
                self.b.ins().jump(join_b, &[BlockArg::Value(r)]);
                self.b.switch_to_block(join_b);
                let r = self.b.block_params(join_b)[0];
                let hit = self.b.ins().icmp_imm_s(IntCC::Equal, r, next as i64);
                let yes = self.b.create_block();
                let miss = self.b.create_block();
                self.b.ins().brif(hit, yes, &[], miss, &[]);
                self.b.switch_to_block(yes);
                self.refill(next, &inn, ci, None);
                self.b.switch_to_block(miss);
                self.ret_value(r);
            }
            Op::SetUpval(i) | Op::StoreUpval(i) => {
                let addr = self.upval_addr(i);
                let oh = self.b.ins().load(types::I64, flags(), addr, 0);
                let op_ = self.b.ins().load(types::I64, flags(), addr, 8);
                let (h, p) = self.bits(d - 1, inn[d - 1]);
                if matches!(op, Op::SetUpval(_)) && inn[d - 1] == Ty::Any {
                    self.ref_change(h, p, true);
                }
                self.b.ins().store(flags(), h, addr, 0);
                self.b.ins().store(flags(), p, addr, 8);
                self.ref_change(oh, op_, false);
                if matches!(op, Op::StoreUpval(_)) {
                    cur.pop();
                }
                self.goto_from(&cur, next);
            }
            Op::GetUpval(i) | Op::GetUpvalChecked(i, _) | Op::GetUpvalCheckedUndef(i, _) => {
                let addr = self.upval_addr(i);
                let h = self.b.ins().load(types::I64, flags(), addr, 0);
                let p = self.b.ins().load(types::F64, flags(), addr, 8);
                if !matches!(op, Op::GetUpval(_)) {
                    let slow = self.slow_block(ip, &inn, succs);
                    let tg = self.b.ins().band_imm_u(h, 0xff);
                    let ok = self.b.ins().icmp_imm_s(IntCC::NotEqual, tg, T_UNINIT);
                    self.guard(ok, slow);
                }
                let pb = self.b.ins().bitcast(types::I64, MemFlagsData::new(), p);
                self.ref_change(h, pb, true);
                self.b.def_var(self.head[d], h);
                self.b.def_var(self.pay[d], p);
                cur.push(Ty::Any);
                if matches!(op, Op::GetUpvalCheckedUndef(..)) {
                    self.set_head(d + 1, T_UNDEFINED);
                    cur.push(Ty::Prim);
                }
                self.goto_from(&cur, next);
            }
            Op::Return => {
                let slow = self.b.create_block();
                if inn[..d - 1].iter().filter(|&&t| t == Ty::Any).count() <= 6 {
                    self.inline_return(&inn, d, slow);
                } else {
                    self.b.ins().jump(slow, &[]);
                }
                self.b.switch_to_block(slow);
                self.spill(&inn, 0, d);
                let r = self.helper(
                    self.sigs.step,
                    jit_return as *const () as usize,
                    &[ip as i64, d as i64],
                );
                self.ret_value(r);
            }
            Op::New(..) | Op::SuperCall(_) => self.bail(ip, &inn),
            _ => self.step(ip, &inn, succs),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compare(
        &mut self,
        ip: usize,
        inn: &[Ty],
        succs: &[usize],
        cmp: Cmp,
        a: usize,
        ta: Ty,
        b: usize,
        tb: Ty,
        constant: Option<f64>,
    ) -> V {
        let ci = constant.and_then(int_value);
        if ta == Ty::Int && (tb == Ty::Int || ci.is_some()) {
            let x = self.i32_of(a);
            let y = match ci {
                Some(c) => self.b.ins().iconst(types::I32, c as i64),
                None => self.i32_of(b),
            };
            let cc = match cmp {
                Cmp::Lt => IntCC::SignedLessThan,
                Cmp::Le => IntCC::SignedLessThanOrEqual,
                Cmp::Gt => IntCC::SignedGreaterThan,
                Cmp::Ge => IntCC::SignedGreaterThanOrEqual,
                Cmp::StrictEq | Cmp::Eq => IntCC::Equal,
                Cmp::StrictNe | Cmp::Ne => IntCC::NotEqual,
            };
            return self.b.ins().icmp(cc, x, y);
        }
        if !is_num(ta) || (constant.is_none() && !is_num(tb)) {
            let slow = self.slow_block(ip, inn, succs);
            self.number_guard(a, ta, slow);
            if constant.is_none() {
                self.number_guard(b, tb, slow);
            }
        }
        let x = self.f64_of(a, ta);
        let y = match constant {
            Some(c) => self.b.ins().f64const(c),
            None => self.f64_of(b, tb),
        };
        self.b.ins().fcmp(float_cc(cmp), x, y)
    }
}

fn numeric(op: Op, inn: &[Ty]) -> bool {
    let d = inn.len();
    let num = |k: usize| inn.get(k).is_some_and(|&t| is_num(t));
    match op {
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Mod
        | Op::BitAnd
        | Op::BitOr
        | Op::BitXor
        | Op::Shl
        | Op::Shr
        | Op::UShr
        | Op::Lt
        | Op::Gt
        | Op::Le
        | Op::Ge
        | Op::StrictEq
        | Op::StrictNe
        | Op::CmpJump(..) => d >= 2 && num(d - 1) && num(d - 2),
        Op::AddConst(_)
        | Op::SubConst(_)
        | Op::MulConst(_)
        | Op::ModConst(_)
        | Op::Neg
        | Op::Inc
        | Op::Dec
        | Op::ToNumber
        | Op::ToNumeric
        | Op::CmpConstJump(..)
        | Op::GetIndex
        | Op::StoreLocal(_) => d >= 1 && num(d - 1),
        Op::SetIndex => d >= 2 && num(d - 2),
        Op::GetLocal(l)
        | Op::GetLocalUndef(l)
        | Op::GetLocalChecked(l, _)
        | Op::SetLocal(l)
        | Op::IncLocal(l)
        | Op::DecLocal(l) => num(l as usize),
        _ => false,
    }
}

fn helpful(op: Op) -> bool {
    matches!(
        op,
        Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::BitAnd
            | Op::BitOr
            | Op::BitXor
            | Op::Shl
            | Op::Shr
            | Op::UShr
            | Op::Lt
            | Op::Gt
            | Op::Le
            | Op::Ge
            | Op::StrictEq
            | Op::StrictNe
            | Op::CmpJump(..)
            | Op::CmpConstJump(..)
            | Op::AddConst(_)
            | Op::SubConst(_)
            | Op::MulConst(_)
            | Op::ModConst(_)
            | Op::Neg
            | Op::Inc
            | Op::Dec
            | Op::IncLocal(_)
            | Op::DecLocal(_)
            | Op::GetIndex
            | Op::SetIndex
            | Op::GetProp(_)
            | Op::GetLocalProp(..)
    )
}

fn costly(op: Op) -> bool {
    matches!(
        op,
        Op::Call(..)
            | Op::GetMethod(_)
            | Op::GetLocalMethod(..)
            | Op::GetGlobal(_)
            | Op::NewObject
            | Op::InitProp(_)
            | Op::Return
            | Op::AppendLocal(_)
            | Op::PopTo(_)
    )
}

struct Plan {
    depths: Vec<u16>,
    max: u32,
    hot: Vec<bool>,
    looped: bool,
}

fn plan(proto: &FunctionProto) -> Option<Plan> {
    let log = std::env::var_os("JSQ_JIT_LOG").is_some();
    if proto.is_coroutine()
        || proto.code.len() > 20_000
        || proto.code.iter().any(|op| matches!(op, Op::DirectEval(..)))
    {
        return None;
    }
    let Some((depths, max)) = analyze(proto) else {
        if log {
            eprintln!("jit analyze failed {}", proto.name);
        }
        return None;
    };
    let mut hot = vec![false; proto.code.len()];
    let mut looped = false;
    for (ip, op) in proto.code.iter().enumerate() {
        if let Op::Jump(t) = *op
            && (t as usize) <= ip
            && depths[ip] != UNKNOWN
        {
            looped = true;
            for h in &mut hot[t as usize..=ip] {
                *h = true;
            }
        }
    }
    let cold = proto
        .code
        .iter()
        .zip(&depths)
        .zip(&hot)
        .any(|((op, &d), &h)| d != UNKNOWN && (h || !looped) && !inline_op(*op));
    if cold || max > 400 {
        if log {
            eprintln!("jit not eligible {} cold={cold} max={max}", proto.name);
        }
        return None;
    }
    Some(Plan {
        depths,
        max,
        hot,
        looped,
    })
}

pub fn cost(proto: &FunctionProto) -> Option<u32> {
    let p = plan(proto)?;
    let ops = p
        .hot
        .iter()
        .zip(&p.depths)
        .filter(|&(&h, &d)| d != UNKNOWN && (h || !p.looped))
        .count() as u32;
    Some(ops * p.max)
}

fn entry_points(proto: &FunctionProto, depths: &[u16]) -> Vec<u16> {
    let mut entries = vec![UNKNOWN; depths.len()];
    let mut mark = |i: usize| {
        if i < depths.len() {
            entries[i] = depths[i];
        }
    };
    mark(0);
    for (ip, op) in proto.code.iter().enumerate() {
        if depths[ip] == UNKNOWN {
            continue;
        }
        match *op {
            Op::Jump(t)
            | Op::JumpIfFalse(t)
            | Op::JumpIfTrue(t)
            | Op::CmpJump(_, t)
            | Op::CmpConstJump(_, _, t)
                if (t as usize) <= ip =>
            {
                mark(t as usize)
            }
            Op::TryBegin(t, _) => mark(t as usize),
            Op::Call(..)
            | Op::New(..)
            | Op::SuperCall(_)
            | Op::CallSpread(_)
            | Op::NewSpread(_) => mark(ip + 1),
            _ => {}
        }
    }
    entries
}

impl Jit {
    pub fn new(intrinsics: Vec<(Rc<crate::object::Obj>, Intrinsic)>) -> Option<Jit> {
        let mut fb = settings::builder();
        fb.set("use_colocated_libcalls", "false").ok()?;
        fb.set("is_pic", "false").ok()?;
        fb.set("opt_level", "speed").ok()?;
        fb.set("enable_verifier", "false").ok()?;
        let isa = cranelift_native::builder()
            .ok()?
            .finish(settings::Flags::new(fb))
            .ok()?;
        let module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
        let ctx = module.make_context();
        Some(Jit {
            lay: crate::object::jit_layout(),
            vl: crate::vm::vm_layout(),
            intrinsics,
            module,
            ctx,
            fctx: FunctionBuilderContext::new(),
        })
    }

    fn sig(&self, params: &[types::Type], ret: types::Type) -> Signature {
        let mut s = self.module.make_signature();
        for &p in params {
            s.params.push(AbiParam::new(p));
        }
        s.returns.push(AbiParam::new(ret));
        s
    }

    pub fn compile(&mut self, proto: &FunctionProto) -> bool {
        let info = &proto.jit;
        let Some(Plan {
            depths,
            max,
            hot,
            looped,
        }) = plan(proto)
        else {
            return false;
        };
        let mut banned = vec![false; proto.code.len()];
        for &ip in info.banned.borrow().iter() {
            if let Some(b) = banned.get_mut(ip as usize) {
                *b = true;
            }
        }
        let ints = !info.no_int.get();
        let mut captured = vec![false; max as usize + 8];
        for f in &proto.functions {
            for u in &f.upvalues {
                if u.is_local
                    && let Some(c) = captured.get_mut(u.index as usize)
                {
                    *c = true;
                }
            }
        }
        if proto.uses_arguments {
            captured.iter_mut().for_each(|c| *c = true);
        }
        let dirty = dirty_sets(proto, &depths, max as usize);
        let mut volatile = captured.clone();
        for op in &proto.code {
            if let Op::AppendLocal(l) = *op
                && let Some(v) = volatile.get_mut(l as usize)
            {
                *v = true;
            }
        }
        let Some(tys) = infer(proto, &depths, &banned, ints, &captured) else {
            if std::env::var_os("JSQ_JIT_LOG").is_some() {
                eprintln!("jit infer failed {}", proto.name);
            }
            return false;
        };
        let region: Vec<bool> = hot.iter().map(|&h| h || !looped).collect();
        let (mut ops, mut fast) = (0usize, 0usize);
        for ((op, t), &r) in proto.code.iter().zip(&tys).zip(&region) {
            if let (Some(t), true) = (t, r) {
                ops += 1;
                fast += numeric(*op, t) as usize;
            }
        }
        let (mut good, mut bad) = (0, 0);
        for ((op, t), &r) in proto.code.iter().zip(&tys).zip(&region) {
            if t.is_some() && r {
                good += helpful(*op) as usize;
                bad += costly(*op) as usize;
            }
        }
        if fast + good < bad {
            if std::env::var_os("JSQ_JIT_LOG").is_some() {
                eprintln!(
                    "jit unprofitable {} fast={fast} good={good} bad={bad} ops={ops}",
                    proto.name
                );
            }
            return false;
        }
        let mut entries = entry_points(proto, &depths);
        for (e, &r) in entries.iter_mut().zip(&region) {
            if !r {
                *e = UNKNOWN;
            }
        }
        let tc = self.module.target_config();
        let mut main = self.sig(&[types::I64, types::I32], types::I64);
        if cfg!(target_arch = "x86_64") {
            main.call_conv = cranelift_codegen::isa::CallConv::SystemV;
        }
        let step = self.sig(&[types::I64, types::I32, types::I32], types::I64);
        let six = self.sig(&[types::I64; 7], types::I64);
        let two = self.sig(&[types::I64, types::I64], types::I64);
        let call = self.sig(
            &[types::I64, types::I32, types::I32, types::I32],
            types::I64,
        );
        let pop_to = self.sig(&[types::I64, types::I32, types::I32], types::I64);
        let rem = self.sig(&[types::F64, types::F64], types::F64);
        let bits = self.sig(&[types::I32, types::F64, types::F64], types::F64);
        let Ok(id) = self.module.declare_anonymous_function(&main) else {
            return false;
        };
        self.ctx.func.signature = main;
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            let sigs = Sigs {
                main: {
                    let s = b.func.signature.clone();
                    b.import_signature(s)
                },
                six: b.import_signature(six),
                two: b.import_signature(two),
                step: b.import_signature(step),
                call: b.import_signature(call),
                pop_to: b.import_signature(pop_to),
                rem: b.import_signature(rem),
                bits: b.import_signature(bits),
            };
            let n = proto.code.len();
            let slots = max as usize;
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            let blocks: Vec<Block> = (0..n).map(|_| b.create_block()).collect();
            let ret = b.create_block();
            b.append_block_param(ret, types::I64);
            let bail = b.create_block();
            b.append_block_param(bail, types::I32);
            let ctx_var = b.declare_var(types::I64);
            let bp_var = b.declare_var(types::I64);
            let head: Vec<Variable> = (0..slots).map(|_| b.declare_var(types::I64)).collect();
            let pay: Vec<Variable> = (0..slots).map(|_| b.declare_var(types::F64)).collect();
            let ival: Vec<Variable> = (0..slots).map(|_| b.declare_var(types::I32)).collect();
            b.switch_to_block(entry);
            let ctx_v = b.block_params(entry)[0];
            let ip_v = b.block_params(entry)[1];
            b.def_var(ctx_var, ctx_v);
            let bp0 = b.ins().load(types::I64, flags(), ctx_v, 8);
            b.def_var(bp_var, bp0);
            let zero_h = b.ins().iconst(types::I64, 0);
            let zero_p = b.ins().f64const(0.0);
            let zero_i = b.ins().iconst(types::I32, 0);
            for k in 0..slots {
                b.def_var(head[k], zero_h);
                b.def_var(pay[k], zero_p);
                b.def_var(ival[k], zero_i);
            }
            let mut starts = Vec::new();
            let def = b.func.dfg.block_call(bail, &[BlockArg::Value(ip_v)]);
            let table: Vec<_> = (0..n)
                .map(|i| {
                    if entries[i] != UNKNOWN && tys[i].is_some() {
                        let s = b.create_block();
                        starts.push((s, i));
                        b.func.dfg.block_call(s, &[])
                    } else {
                        b.func.dfg.block_call(bail, &[BlockArg::Value(ip_v)])
                    }
                })
                .collect();
            let jt = b.create_jump_table(JumpTableData::new(def, &table));
            b.ins().br_table(ip_v, jt);
            b.switch_to_block(bail);
            let bip = b.block_params(bail)[0];
            let r = b.ins().uextend(types::I64, bip);
            b.ins().return_(&[r]);
            b.switch_to_block(ret);
            let r = b.block_params(ret)[0];
            b.ins().return_(&[r]);
            {
                let mut g = Gen {
                    b: &mut b,
                    proto,
                    intrinsics: &self.intrinsics,
                    lay: self.lay,
                    vl: self.vl,
                    blocks: &blocks,

                    types: &tys,
                    ints,
                    banned: &banned,
                    ret,
                    ctx_var,
                    bp_var,
                    head,
                    pay,
                    ival,
                    sigs: &sigs,
                    volatile: &volatile,
                    dirty: &dirty,
                    spill_at: 0,
                    slow_memo: None,
                    slow: Vec::new(),
                    deopts: Vec::new(),
                    narrows: Vec::new(),
                    exits: Vec::new(),
                };
                for (s, i) in starts {
                    g.b.switch_to_block(s);
                    let want = g.ty_at(i);
                    g.fill(&want, i);
                    g.b.ins().jump(g.blocks[i], &[]);
                    g.flush();
                }
                for (ip, &op) in proto.code.iter().enumerate() {
                    if tys[ip].is_none() {
                        continue;
                    }
                    g.b.switch_to_block(blocks[ip]);
                    let d = depths[ip] as usize;
                    let succs: Vec<usize> = flow(op, ip, d as i32)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(t, _)| t)
                        .collect();
                    g.spill_at = ip;
                    if region[ip] {
                        g.op(proto, ip, op, &succs);
                    } else if let Some(inn) = &tys[ip] {
                        g.bail(ip, inn);
                    }
                    g.flush();
                }
            }
            b.seal_all_blocks();
            b.finalize(tc);
        }
        if std::env::var_os("JSQ_JIT_LOG").is_some() {
            eprintln!(
                "jit ir {} insts={} blocks={}",
                proto.name,
                self.ctx.func.dfg.num_insts(),
                self.ctx.func.dfg.num_blocks()
            );
        }
        if std::env::var_os("JSQ_JIT_CLIF").is_some() {
            eprintln!("{}", self.ctx.func.display());
        }
        let dump = std::env::var_os("JSQ_JIT_DUMP").is_some();
        self.ctx.set_disasm(dump);
        let defined = self.module.define_function(id, &mut self.ctx);
        if let Err(e) = &defined
            && std::env::var_os("JSQ_JIT_LOG").is_some()
        {
            eprintln!("jit codegen failed {}: {e:?}", proto.name);
        }
        let ok = defined.is_ok();
        if dump && let Some(text) = self.ctx.compiled_code().and_then(|c| c.vcode.clone()) {
            eprintln!("== jit {}", proto.name);
            eprintln!("{text}");
        }
        self.module.clear_context(&mut self.ctx);
        if !ok || self.module.finalize_definitions().is_err() {
            return false;
        }
        let code = self.module.get_finalized_function(id);
        let _ = info.depths.set(depths.into());
        let _ = info.entries.set(entries.into());
        info.max.set(max);
        info.code.set(code as usize);
        true
    }
}
