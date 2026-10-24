use crate::bytecode::{Cmp, FunctionProto, Op};
use crate::value::Value;
use crate::vm::Vm;
use cranelift_codegen::Context;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, InstBuilder, JumpTableData, MemFlagsData, SigRef, Signature, types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Module, default_libcall_names};
use std::cell::{Cell, OnceCell};

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
}

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

unsafe extern "C" fn jit_upval(ctx: *mut JitCtx, index: u32, slot: u32) -> u64 {
    unsafe {
        let ctx = &mut *ctx;
        let vm = &mut *ctx.vm;
        vm.jit_upval(ctx, index as usize, slot as usize)
    }
}

unsafe extern "C" fn jit_pop_to(ctx: *mut JitCtx, to: u32, depth: u32) -> u64 {
    unsafe {
        let ctx = &mut *ctx;
        let vm = &mut *ctx.vm;
        vm.jit_pop_to(ctx, to as usize, depth as usize)
    }
}

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

pub struct Jit {
    module: JITModule,
    ctx: Context,
    fctx: FunctionBuilderContext,
}

struct Sigs {
    upval: SigRef,
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
    )
}

type V = cranelift_codegen::ir::Value;

struct Gen<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    blocks: &'a [Block],
    depths: &'a [u16],
    ret: Block,
    ctx_var: Variable,
    bp_var: Variable,
    head: Vec<Variable>,
    pay: Vec<Variable>,
    sigs: &'a Sigs,
    pending: Vec<(Block, usize, usize, Vec<usize>)>,
}

impl Gen<'_, '_> {
    fn bp(&mut self) -> V {
        self.b.use_var(self.bp_var)
    }

    fn tag(&mut self, slot: usize) -> V {
        let h = self.b.use_var(self.head[slot]);
        self.b.ins().band_imm_u(h, 0xff)
    }

    fn is(&mut self, slot: usize, tag: i64) -> V {
        let t = self.tag(slot);
        self.b.ins().icmp_imm_s(IntCC::Equal, t, tag)
    }

    fn num(&mut self, slot: usize) -> V {
        self.b.use_var(self.pay[slot])
    }

    fn set_payload(&mut self, slot: usize, v: V) {
        self.b.def_var(self.pay[slot], v);
    }

    fn put_tag(&mut self, slot: usize, tag: i64) {
        let t = self.b.ins().iconst(types::I64, tag);
        self.b.def_var(self.head[slot], t);
    }

    fn put_num(&mut self, slot: usize, v: V) {
        self.put_tag(slot, T_NUMBER);
        self.set_payload(slot, v);
    }

    fn put_bool(&mut self, slot: usize, v: V) {
        let w = self.b.ins().uextend(types::I64, v);
        let sh = self.b.ins().ishl_imm_u(w, 8);
        let h = self.b.ins().bor_imm_u(sh, T_BOOL);
        self.b.def_var(self.head[slot], h);
    }

    fn copy_slot(&mut self, from: usize, to: usize) {
        let h = self.b.use_var(self.head[from]);
        let p = self.b.use_var(self.pay[from]);
        self.b.def_var(self.head[to], h);
        self.b.def_var(self.pay[to], p);
    }

    fn spill(&mut self, d: usize) {
        let bp = self.bp();
        for k in 0..d {
            let h = self.b.use_var(self.head[k]);
            let p = self.b.use_var(self.pay[k]);
            self.b.ins().store(flags(), h, bp, k as i32 * SLOT);
            self.b.ins().store(flags(), p, bp, k as i32 * SLOT + 8);
        }
    }

    fn fill(&mut self, d: usize) {
        let bp = self.bp();
        for k in 0..d {
            let h = self.b.ins().load(types::I64, flags(), bp, k as i32 * SLOT);
            let p = self
                .b
                .ins()
                .load(types::F64, flags(), bp, k as i32 * SLOT + 8);
            self.b.def_var(self.head[k], h);
            self.b.def_var(self.pay[k], p);
        }
    }

    fn depth(&self, ip: usize) -> usize {
        self.depths[ip] as usize
    }

    fn ret_value(&mut self, v: V) {
        self.b.ins().jump(self.ret, &[BlockArg::Value(v)]);
    }

    fn bail(&mut self, ip: usize, d: usize) {
        self.spill(d);
        let v = self.b.ins().iconst(types::I64, (NEXT | ip as u64) as i64);
        self.ret_value(v);
    }

    fn goto(&mut self, ip: usize) {
        self.b.ins().jump(self.blocks[ip], &[]);
    }

    fn reload_bp(&mut self) {
        let ctx = self.b.use_var(self.ctx_var);
        let bp = self.b.ins().load(types::I64, flags(), ctx, 8);
        self.b.def_var(self.bp_var, bp);
    }

    fn guard(&mut self, cond: V, slow: Block) {
        let ok = self.b.create_block();
        self.b.ins().brif(cond, ok, &[], slow, &[]);
        self.b.switch_to_block(ok);
    }

    fn both_numbers(&mut self, a: usize, b: usize) -> V {
        let x = self.is(a, T_NUMBER);
        let y = self.is(b, T_NUMBER);
        self.b.ins().band(x, y)
    }

    fn resume(&mut self, r: V, succs: &[usize]) {
        for &s in succs {
            let hit = self.b.ins().icmp_imm_s(IntCC::Equal, r, s as i64);
            let yes = self.b.create_block();
            let miss = self.b.create_block();
            self.b.ins().brif(hit, yes, &[], miss, &[]);
            self.b.switch_to_block(yes);
            let d = self.depth(s);
            self.fill(d);
            self.goto(s);
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

    fn step_call(&mut self, ip: usize, d: usize, succs: &[usize]) {
        self.spill(d);
        let r = self.helper(
            self.sigs.step,
            jit_step as *const () as usize,
            &[ip as i64, d as i64],
        );
        self.resume(r, succs);
    }

    fn slow_block(&mut self, ip: usize, d: usize, succs: &[usize]) -> Block {
        let slow = self.b.create_block();
        self.pending.push((slow, ip, d, succs.to_vec()));
        slow
    }

    fn flush(&mut self) {
        while let Some((blk, ip, d, succs)) = self.pending.pop() {
            self.b.switch_to_block(blk);
            self.step_call(ip, d, &succs);
        }
    }

    fn truthy(&mut self, slot: usize, slow: Block) -> V {
        let t = self.tag(slot);
        let done = self.b.create_block();
        self.b.append_block_param(done, types::I8);
        let not_bool = self.b.create_block();
        let not_num = self.b.create_block();
        let bool_block = self.b.create_block();
        let num_block = self.b.create_block();
        let falsy = self.b.create_block();
        let is_bool = self.b.ins().icmp_imm_s(IntCC::Equal, t, T_BOOL);
        self.b.ins().brif(is_bool, bool_block, &[], not_bool, &[]);
        self.b.switch_to_block(bool_block);
        let h = self.b.use_var(self.head[slot]);
        let bv = self.b.ins().ushr_imm_u(h, 8);
        let bv = self.b.ins().band_imm_u(bv, 0xff);
        let bt = self.b.ins().icmp_imm_s(IntCC::NotEqual, bv, 0);
        self.b.ins().jump(done, &[BlockArg::Value(bt)]);
        self.b.switch_to_block(not_bool);
        let is_num = self.b.ins().icmp_imm_s(IntCC::Equal, t, T_NUMBER);
        self.b.ins().brif(is_num, num_block, &[], not_num, &[]);
        self.b.switch_to_block(num_block);
        let x = self.num(slot);
        let zero = self.b.ins().f64const(0.0);
        let nt = self.b.ins().fcmp(FloatCC::OrderedNotEqual, x, zero);
        self.b.ins().jump(done, &[BlockArg::Value(nt)]);
        self.b.switch_to_block(not_num);
        let nullish = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThanOrEqual, t, T_NULL);
        self.b.ins().brif(nullish, falsy, &[], slow, &[]);
        self.b.switch_to_block(falsy);
        let f = self.b.ins().iconst(types::I8, 0);
        self.b.ins().jump(done, &[BlockArg::Value(f)]);
        self.b.switch_to_block(done);
        self.b.block_params(done)[0]
    }

    fn op(&mut self, proto: &FunctionProto, ip: usize, d: usize, op: Op, succs: &[usize]) {
        let next = ip + 1;
        match op {
            Op::Const(c) => match &proto.consts[c as usize] {
                Value::Number(x) => {
                    let v = self.b.ins().f64const(*x);
                    self.put_num(d, v);
                    self.goto(next);
                }
                _ => self.step_call(ip, d, succs),
            },
            Op::Undefined | Op::Null | Op::Uninit => {
                let t = match op {
                    Op::Undefined => T_UNDEFINED,
                    Op::Null => T_NULL,
                    _ => T_UNINIT,
                };
                self.put_tag(d, t);
                self.goto(next);
            }
            Op::True | Op::False => {
                let v = self
                    .b
                    .ins()
                    .iconst(types::I8, matches!(op, Op::True) as i64);
                self.put_bool(d, v);
                self.goto(next);
            }
            Op::UndefinedN(n) | Op::UninitN(n) => {
                let t = if matches!(op, Op::UninitN(_)) {
                    T_UNINIT
                } else {
                    T_UNDEFINED
                };
                for k in 0..n as usize {
                    self.put_tag(d + k, t);
                }
                self.goto(next);
            }
            Op::Pop => {
                let slow = self.slow_block(ip, d, succs);
                let t = self.tag(d - 1);
                let off = self.b.ins().iadd_imm_s(t, -4);
                let heap = self
                    .b
                    .ins()
                    .icmp_imm_s(IntCC::UnsignedLessThanOrEqual, off, 3);
                let ok = self.b.ins().icmp_imm_s(IntCC::Equal, heap, 0);
                self.guard(ok, slow);
                self.goto(next);
            }
            Op::Dup => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.is(d - 1, T_NUMBER);
                self.guard(c, slow);
                self.copy_slot(d - 1, d);
                self.goto(next);
            }
            Op::Swap => {
                let (h1, p1) = (self.head[d - 1], self.pay[d - 1]);
                let (h2, p2) = (self.head[d - 2], self.pay[d - 2]);
                let a0 = self.b.use_var(h1);
                let a1 = self.b.use_var(p1);
                let b0 = self.b.use_var(h2);
                let b1 = self.b.use_var(p2);
                self.b.def_var(h1, b0);
                self.b.def_var(p1, b1);
                self.b.def_var(h2, a0);
                self.b.def_var(p2, a1);
                self.goto(next);
            }
            Op::GetLocal(s) | Op::GetLocalUndef(s) | Op::GetLocalChecked(s, _) => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.is(s as usize, T_NUMBER);
                self.guard(c, slow);
                self.copy_slot(s as usize, d);
                if matches!(op, Op::GetLocalUndef(_)) {
                    self.put_tag(d + 1, T_UNDEFINED);
                }
                self.goto(next);
            }
            Op::SetLocal(s) | Op::StoreLocal(s) => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.both_numbers(d - 1, s as usize);
                self.guard(c, slow);
                let x = self.num(d - 1);
                self.set_payload(s as usize, x);
                self.goto(next);
            }
            Op::IncLocal(s) | Op::DecLocal(s) => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.is(s as usize, T_NUMBER);
                self.guard(c, slow);
                let x = self.num(s as usize);
                let one = self.b.ins().f64const(if matches!(op, Op::IncLocal(_)) {
                    1.0
                } else {
                    -1.0
                });
                let r = self.b.ins().fadd(x, one);
                self.set_payload(s as usize, r);
                self.goto(next);
            }
            Op::AppendLocal(s) => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.both_numbers(d - 1, s as usize);
                self.guard(c, slow);
                let x = self.num(s as usize);
                let y = self.num(d - 1);
                let r = self.b.ins().fadd(x, y);
                self.set_payload(s as usize, r);
                self.goto(next);
            }
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.both_numbers(d - 2, d - 1);
                self.guard(c, slow);
                let x = self.num(d - 2);
                let y = self.num(d - 1);
                let r = match op {
                    Op::Add => self.b.ins().fadd(x, y),
                    Op::Sub => self.b.ins().fsub(x, y),
                    Op::Mul => self.b.ins().fmul(x, y),
                    Op::Div => self.b.ins().fdiv(x, y),
                    _ => {
                        let f = self
                            .b
                            .ins()
                            .iconst(types::I64, jit_rem as *const () as usize as i64);
                        let call = self.b.ins().call_indirect(self.sigs.rem, f, &[x, y]);
                        self.b.inst_results(call)[0]
                    }
                };
                self.set_payload(d - 2, r);
                self.goto(next);
            }
            Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr | Op::UShr => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.both_numbers(d - 2, d - 1);
                self.guard(c, slow);
                let x = self.num(d - 2);
                let y = self.num(d - 1);
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
                self.set_payload(d - 2, r);
                self.goto(next);
            }
            Op::AddConst(k) | Op::SubConst(k) => {
                let Value::Number(y) = proto.consts[k as usize] else {
                    self.step_call(ip, d, succs);
                    return;
                };
                let slow = self.slow_block(ip, d, succs);
                let c = self.is(d - 1, T_NUMBER);
                self.guard(c, slow);
                let x = self.num(d - 1);
                let yv = self.b.ins().f64const(y);
                let r = if matches!(op, Op::AddConst(_)) {
                    self.b.ins().fadd(x, yv)
                } else {
                    self.b.ins().fsub(x, yv)
                };
                self.set_payload(d - 1, r);
                self.goto(next);
            }
            Op::Neg | Op::Inc | Op::Dec | Op::ToNumeric | Op::ToNumber => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.is(d - 1, T_NUMBER);
                self.guard(c, slow);
                let x = self.num(d - 1);
                let r = match op {
                    Op::Neg => Some(self.b.ins().fneg(x)),
                    Op::Inc | Op::Dec => {
                        let one = self
                            .b
                            .ins()
                            .f64const(if op == Op::Inc { 1.0 } else { -1.0 });
                        Some(self.b.ins().fadd(x, one))
                    }
                    _ => None,
                };
                if let Some(r) = r {
                    self.set_payload(d - 1, r);
                }
                self.goto(next);
            }
            Op::Lt | Op::Gt | Op::Le | Op::Ge | Op::StrictEq | Op::StrictNe => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.both_numbers(d - 2, d - 1);
                self.guard(c, slow);
                let x = self.num(d - 2);
                let y = self.num(d - 1);
                let cc = match op {
                    Op::Lt => FloatCC::LessThan,
                    Op::Gt => FloatCC::GreaterThan,
                    Op::Le => FloatCC::LessThanOrEqual,
                    Op::Ge => FloatCC::GreaterThanOrEqual,
                    Op::StrictEq => FloatCC::Equal,
                    _ => FloatCC::NotEqual,
                };
                let r = self.b.ins().fcmp(cc, x, y);
                self.put_bool(d - 2, r);
                self.goto(next);
            }
            Op::Not => {
                let slow = self.slow_block(ip, d, succs);
                let t = self.truthy(d - 1, slow);
                let r = self.b.ins().bxor_imm_u(t, 1);
                self.put_bool(d - 1, r);
                self.goto(next);
            }
            Op::Jump(t) => {
                if (t as usize) <= ip {
                    let ctx = self.b.use_var(self.ctx_var);
                    let p = self.b.ins().load(types::I64, flags(), ctx, 32);
                    let flag = self.b.ins().uload8(types::I32, flags(), p, 0);
                    let stop = self.b.create_block();
                    self.b
                        .ins()
                        .brif(flag, stop, &[], self.blocks[t as usize], &[]);
                    self.b.switch_to_block(stop);
                    self.bail(ip, d);
                } else {
                    self.goto(t as usize);
                }
            }
            Op::JumpIfFalse(t)
            | Op::JumpIfTrue(t)
            | Op::JumpIfFalseKeep(t)
            | Op::JumpIfTrueKeep(t) => {
                let slow = self.slow_block(ip, d, succs);
                let v = self.truthy(d - 1, slow);
                let (yes, no) = if matches!(op, Op::JumpIfTrue(_) | Op::JumpIfTrueKeep(_)) {
                    (self.blocks[t as usize], self.blocks[next])
                } else {
                    (self.blocks[next], self.blocks[t as usize])
                };
                self.b.ins().brif(v, yes, &[], no, &[]);
            }
            Op::JumpIfNotNullishKeep(t) | Op::JumpIfNotUndef(t) => {
                let tag = self.tag(d - 1);
                let keep = if matches!(op, Op::JumpIfNotUndef(_)) {
                    self.b.ins().icmp_imm_s(IntCC::NotEqual, tag, T_UNDEFINED)
                } else {
                    self.b
                        .ins()
                        .icmp_imm_s(IntCC::UnsignedGreaterThan, tag, T_NULL)
                };
                self.b
                    .ins()
                    .brif(keep, self.blocks[t as usize], &[], self.blocks[next], &[]);
            }
            Op::JumpIfNullish(t) => {
                let tag = self.tag(d - 1);
                let nullish = self
                    .b
                    .ins()
                    .icmp_imm_s(IntCC::UnsignedLessThanOrEqual, tag, T_NULL);
                let yes = self.b.create_block();
                self.b.ins().brif(nullish, yes, &[], self.blocks[next], &[]);
                self.b.switch_to_block(yes);
                self.put_tag(d - 1, T_UNDEFINED);
                self.goto(t as usize);
            }
            Op::CmpJump(cmp, t) => {
                let slow = self.slow_block(ip, d, succs);
                let c = self.both_numbers(d - 2, d - 1);
                self.guard(c, slow);
                let x = self.num(d - 2);
                let y = self.num(d - 1);
                let ok = self.b.ins().fcmp(float_cc(cmp), x, y);
                self.b
                    .ins()
                    .brif(ok, self.blocks[next], &[], self.blocks[t as usize], &[]);
            }
            Op::CmpConstJump(cmp, k, t) => {
                let Value::Number(y) = proto.consts[k as usize] else {
                    self.step_call(ip, d, succs);
                    return;
                };
                let slow = self.slow_block(ip, d, succs);
                let c = self.is(d - 1, T_NUMBER);
                self.guard(c, slow);
                let x = self.num(d - 1);
                let yv = self.b.ins().f64const(y);
                let ok = self.b.ins().fcmp(float_cc(cmp), x, yv);
                self.b
                    .ins()
                    .brif(ok, self.blocks[next], &[], self.blocks[t as usize], &[]);
            }
            Op::PopTo(n) => {
                self.spill(d);
                self.helper(
                    self.sigs.pop_to,
                    jit_pop_to as *const () as usize,
                    &[n as i64, d as i64],
                );
                self.fill(n as usize);
                self.goto(next);
            }
            Op::Call(argc, _) => {
                self.spill(d);
                let r = self.helper(
                    self.sigs.call,
                    jit_call as *const () as usize,
                    &[ip as i64, d as i64, argc as i64],
                );
                let hit = self.b.ins().icmp_imm_s(IntCC::Equal, r, next as i64);
                let yes = self.b.create_block();
                let miss = self.b.create_block();
                self.b.ins().brif(hit, yes, &[], miss, &[]);
                self.b.switch_to_block(yes);
                let depth_after = self.depth(next);
                self.fill(depth_after);
                self.goto(next);
                self.b.switch_to_block(miss);
                self.ret_value(r);
            }
            Op::GetUpval(i) | Op::GetUpvalChecked(i, _) | Op::GetUpvalCheckedUndef(i, _) => {
                let r = self.helper(
                    self.sigs.upval,
                    jit_upval as *const () as usize,
                    &[i as i64, d as i64],
                );
                let bp = self.bp();
                let h = self.b.ins().load(types::I64, flags(), bp, d as i32 * SLOT);
                let p = self
                    .b
                    .ins()
                    .load(types::F64, flags(), bp, d as i32 * SLOT + 8);
                self.b.def_var(self.head[d], h);
                self.b.def_var(self.pay[d], p);
                if matches!(op, Op::GetUpvalCheckedUndef(..)) {
                    self.put_tag(d + 1, T_UNDEFINED);
                }
                if matches!(op, Op::GetUpval(_)) {
                    self.goto(next);
                } else {
                    let slow = self.slow_block(ip, d, succs);
                    let ok = self.b.ins().icmp_imm_s(IntCC::Equal, r, 0);
                    self.b.ins().brif(ok, self.blocks[next], &[], slow, &[]);
                }
            }
            Op::Return | Op::New(..) | Op::SuperCall(_) => self.bail(ip, d),
            _ => self.step_call(ip, d, succs),
        }
    }
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
    pub fn new() -> Option<Jit> {
        let mut fb = settings::builder();
        fb.set("use_colocated_libcalls", "false").ok()?;
        fb.set("is_pic", "false").ok()?;
        fb.set("opt_level", "speed").ok()?;
        let isa = cranelift_native::builder()
            .ok()?
            .finish(settings::Flags::new(fb))
            .ok()?;
        let module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
        let ctx = module.make_context();
        Some(Jit {
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
        if proto.is_coroutine() || proto.code.len() > 20_000 {
            return false;
        }
        if proto.code.iter().any(|op| matches!(op, Op::DirectEval(..))) {
            return false;
        }
        let Some((depths, max)) = analyze(proto) else {
            return false;
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
            return false;
        }
        let entries = entry_points(proto, &depths);
        let tc = self.module.target_config();
        let main = self.sig(&[types::I64, types::I32], types::I64);
        let step = self.sig(&[types::I64, types::I32, types::I32], types::I64);
        let upval = self.sig(&[types::I64, types::I32, types::I32], types::I64);
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
                upval: b.import_signature(upval),
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
            b.switch_to_block(entry);
            let ctx_v = b.block_params(entry)[0];
            let ip_v = b.block_params(entry)[1];
            b.def_var(ctx_var, ctx_v);
            let bp0 = b.ins().load(types::I64, flags(), ctx_v, 8);
            b.def_var(bp_var, bp0);
            let zero_h = b.ins().iconst(types::I64, 0);
            let zero_p = b.ins().f64const(0.0);
            for k in 0..slots {
                b.def_var(head[k], zero_h);
                b.def_var(pay[k], zero_p);
            }
            let mut starts = Vec::new();
            let def = b.func.dfg.block_call(bail, &[BlockArg::Value(ip_v)]);
            let table: Vec<_> = (0..n)
                .map(|i| {
                    if entries[i] != UNKNOWN {
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
                    blocks: &blocks,
                    depths: &depths,
                    ret,
                    ctx_var,
                    bp_var,
                    head,
                    pay,
                    sigs: &sigs,
                    pending: Vec::new(),
                };
                for (s, i) in starts {
                    g.b.switch_to_block(s);
                    g.fill(depths[i] as usize);
                    g.goto(i);
                }
                for (ip, &op) in proto.code.iter().enumerate() {
                    if depths[ip] == UNKNOWN {
                        continue;
                    }
                    g.b.switch_to_block(blocks[ip]);
                    let d = depths[ip] as usize;
                    let succs: Vec<usize> = flow(op, ip, d as i32)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(t, _)| t)
                        .collect();
                    g.op(proto, ip, d, op, &succs);
                    g.flush();
                }
            }
            b.seal_all_blocks();
            b.finalize(tc);
        }
        let ok = self.module.define_function(id, &mut self.ctx).is_ok();
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
