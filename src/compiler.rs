use crate::ast::*;
use crate::bytecode::Cmp;
use crate::bytecode::*;
use crate::token::SyntaxError;
use crate::value::Value;
use rustc_hash::FxHashMap;
use std::rc::Rc;

type R<T> = Result<T, SyntaxError>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Module,
    Repl,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BindMode {
    Init,
    Assign,
}

enum Var {
    Local(u32, bool),
    Upval(u32, bool),
    LocalTdz(u32, bool),
    UpvalTdz(u32, bool),
    Global,
}

enum ExportEntry {
    Local(Rc<str>, Rc<str>),
    All(Rc<str>),
    Named(Rc<str>, Rc<str>, Rc<str>),
}

struct Local {
    name: Rc<str>,
    depth: u32,
    captured: bool,
    is_const: bool,
    tdz: bool,
    sticky: bool,
}

struct LoopCtx {
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
    break_locals: usize,
    continue_locals: usize,
    try_depth: usize,
    is_switch: bool,
    is_block: bool,
    labels: Vec<Rc<str>>,
    iter_slot: Option<u32>,
}

#[derive(Clone)]
struct TryCtx {
    has_handler: bool,
    finally: Option<Rc<Vec<Stmt>>>,
}

struct FnState {
    name: Rc<str>,
    kind: ProtoKind,
    is_static: bool,
    line: u32,
    code: Vec<Op>,
    lines: Vec<u32>,
    consts: Vec<Value>,
    const_strs: FxHashMap<Rc<str>, u32>,
    functions: Vec<Rc<FunctionProto>>,
    upvalues: Vec<UpvalDesc>,
    locals: Vec<Local>,
    depth: u32,
    loops: Vec<LoopCtx>,
    tries: Vec<TryCtx>,
    num_params: u32,
    arity: u32,
    has_rest: bool,
    uses_arguments: bool,
    is_async: bool,
    is_generator: bool,
    fields: Option<Rc<Vec<ClassMember>>>,
    vars: Vec<Rc<str>>,
    span: (u32, u32),
    template_sites: u32,
    eval_upvals: Option<EvalUpvals>,
    evals: Vec<Rc<EvalScope>>,
}

impl FnState {
    fn new(name: Rc<str>, kind: ProtoKind, line: u32, is_static: bool) -> Self {
        FnState {
            name,
            kind,
            is_static,
            line,
            code: Vec::new(),
            lines: Vec::new(),
            consts: Vec::new(),
            const_strs: FxHashMap::default(),
            functions: Vec::new(),
            upvalues: Vec::new(),
            locals: Vec::new(),
            depth: 0,
            loops: Vec::new(),
            tries: Vec::new(),
            num_params: 0,
            arity: 0,
            has_rest: false,
            uses_arguments: false,
            is_async: false,
            is_generator: false,
            fields: None,
            vars: Vec::new(),
            span: (0, 0),
            template_sites: 0,
            eval_upvals: None,
            evals: Vec::new(),
        }
    }
}

pub struct Compiler {
    states: Vec<FnState>,
    module: Rc<ModuleInfo>,
    mode: Mode,
    line: u32,
    chain: Vec<usize>,
    exports: Vec<ExportEntry>,
    hidden: u32,
    self_bind: Option<Rc<str>>,
    repl_consts: Vec<Rc<str>>,
    var_scope_next: bool,
    pending_labels: Vec<Rc<str>>,
    import_aliases: Vec<(Rc<str>, Rc<str>, Rc<str>)>,
    export_names: Vec<(Rc<str>, Rc<str>)>,
    completion: Option<(usize, u32)>,
    script_eval: u8,
}

fn collect_var_names(s: &Stmt, out: &mut Vec<Rc<str>>) {
    let push = |p: &Pattern, out: &mut Vec<Rc<str>>| {
        let mut v = Vec::new();
        p.bound_names(&mut v);
        for n in v {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    };
    match &s.kind {
        StmtKind::Decl(DeclKind::Var, decls) => {
            for (p, _) in decls {
                push(p, out);
            }
        }
        StmtKind::If(_, a, b) => {
            collect_var_names(a, out);
            if let Some(b) = b {
                collect_var_names(b, out);
            }
        }
        StmtKind::Block(list) => {
            for x in list {
                collect_var_names(x, out);
            }
        }
        StmtKind::While(_, b) | StmtKind::DoWhile(b, _) | StmtKind::Labeled(_, b) => {
            collect_var_names(b, out)
        }
        StmtKind::For { init, body, .. } => {
            if let Some(i) = init {
                collect_var_names(i, out);
            }
            collect_var_names(body, out);
        }
        StmtKind::ForOf {
            decl,
            pattern,
            body,
            ..
        }
        | StmtKind::ForIn {
            decl,
            pattern,
            body,
            ..
        } => {
            if *decl == Some(DeclKind::Var) {
                push(pattern, out);
            }
            collect_var_names(body, out);
        }
        StmtKind::Try {
            block,
            handler,
            finalizer,
            ..
        } => {
            for x in block {
                collect_var_names(x, out);
            }
            for x in handler.iter().flatten() {
                collect_var_names(x, out);
            }
            for x in finalizer.iter().flatten() {
                collect_var_names(x, out);
            }
        }
        StmtKind::Switch(_, cases) => {
            for c in cases {
                for x in &c.body {
                    collect_var_names(x, out);
                }
            }
        }
        StmtKind::Export(ExportDecl::Decl(d)) => collect_var_names(d, out),
        _ => {}
    }
}

fn new_compiler(module: Rc<ModuleInfo>, mode: Mode) -> Compiler {
    Compiler {
        states: Vec::new(),
        module,
        mode,
        line: 1,
        chain: Vec::new(),
        exports: Vec::new(),
        hidden: 0,
        self_bind: None,
        repl_consts: Vec::new(),
        var_scope_next: false,
        pending_labels: Vec::new(),
        import_aliases: Vec::new(),
        export_names: Vec::new(),
        completion: None,
        script_eval: 0,
    }
}

pub fn compile_eval(
    stmts: &[Stmt],
    module: Rc<ModuleInfo>,
    scope: &EvalScope,
) -> R<Rc<FunctionProto>> {
    let mut c = new_compiler(module, Mode::Module);
    let mut outer = FnState::new("<scope>".into(), scope.kind, 1, scope.is_static);
    outer.depth = 1;
    for (name, is_const) in &scope.locals {
        outer.locals.push(Local {
            name: name.clone(),
            depth: 1,
            captured: true,
            is_const: *is_const,
            tdz: true,
            sticky: true,
        });
    }
    outer.eval_upvals = Some(scope.upvals.clone());
    c.states.push(outer);
    c.states
        .push(FnState::new("eval".into(), ProtoKind::Arrow, 1, false));
    c.begin_scope();
    c.add_local("".into(), true);
    c.emit(Op::Undefined);
    let slot = c.add_local("%completion".into(), false);
    c.completion = Some((1, slot));
    c.declare_vars(stmts);
    c.var_scope_next = true;
    c.hoist(stmts)?;
    for s in stmts {
        c.stmt(s)?;
    }
    c.emit(Op::GetLocal(slot));
    c.emit(Op::Return);
    let st = c.states.pop().expect("eval state");
    Ok(Rc::new(c.finish(st)))
}

pub fn compile_indirect_eval(
    stmts: &[Stmt],
    module: Rc<ModuleInfo>,
    strict: bool,
) -> R<Rc<FunctionProto>> {
    compile_unit(stmts, module, Mode::Repl, if strict { 2 } else { 1 })
}

pub fn compile_module(stmts: &[Stmt], module: Rc<ModuleInfo>, mode: Mode) -> R<Rc<FunctionProto>> {
    compile_unit(stmts, module, mode, 0)
}

fn compile_unit(
    stmts: &[Stmt],
    module: Rc<ModuleInfo>,
    mode: Mode,
    script_eval: u8,
) -> R<Rc<FunctionProto>> {
    let mut c = new_compiler(module, mode);
    c.script_eval = script_eval;
    c.states
        .push(FnState::new("<main>".into(), ProtoKind::Module, 1, false));
    c.begin_scope();
    c.add_local("this".into(), true);
    if mode == Mode::Module {
        c.emit(Op::ModuleNamespace);
        c.add_local("%ns".into(), true);
    } else {
        c.emit(Op::Undefined);
        let slot = c.add_local("%completion".into(), false);
        c.completion = Some((0, slot));
    }
    c.declare_vars(stmts);
    let refs: Vec<&Stmt> = stmts.iter().collect();
    c.hoist_list(&refs, true)?;
    for s in stmts {
        c.stmt(s)?;
    }
    if let Some((_, slot)) = c.completion {
        c.emit(Op::GetLocal(slot));
        c.emit(Op::Return);
        let st = c.states.pop().expect("module state");
        return Ok(Rc::new(c.finish(st)));
    }
    let exports = std::mem::take(&mut c.exports);
    let mut exported: Vec<&Rc<str>> = Vec::new();
    for entry in &exports {
        let name = match entry {
            ExportEntry::Local(local, x) => {
                if c.resolve_local(0, local).is_none() && c.alias(local).is_none() {
                    return Err(c.err(format!("Export '{local}' is not defined in module")));
                }
                x
            }
            ExportEntry::Named(_, _, x) => x,
            ExportEntry::All(_) => continue,
        };
        if exported.contains(&name) {
            return Err(c.err(format!("Duplicate export of '{name}'")));
        }
        exported.push(name);
    }
    c.load_var(&"%ns".into());
    for entry in &exports {
        match entry {
            ExportEntry::Local(local, exported) => {
                c.load_var(local);
                let k = c.str_const(exported);
                c.emit(Op::InitProp(k));
            }
            ExportEntry::All(hidden) => {
                c.load_var(hidden);
                c.emit(Op::ExportAll);
            }
            ExportEntry::Named(hidden, imported, exported) => {
                c.load_var(hidden);
                if &**imported != "*" {
                    let i = c.str_const(imported);
                    c.emit(Op::GetProp(i));
                }
                let k = c.str_const(exported);
                c.emit(Op::InitProp(k));
            }
        }
    }
    c.emit(Op::Return);
    let st = c.states.pop().expect("module state");
    Ok(Rc::new(c.finish(st)))
}

fn expr_text(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(n) => Some(n.to_string()),
        ExprKind::This => Some("this".to_string()),
        ExprKind::Member { obj, name, .. } => Some(format!("{}.{}", expr_text(obj)?, name)),
        ExprKind::Index { obj, .. } => Some(format!("{}[...]", expr_text(obj)?)),
        ExprKind::Call { callee, .. } => Some(format!("{}(...)", expr_text(callee)?)),
        ExprKind::SuperMember(n) => Some(format!("super.{n}")),
        ExprKind::OptChain(inner) => expr_text(inner),
        _ => None,
    }
}

fn bin_op(op: BinOp) -> Op {
    match op {
        BinOp::Add => Op::Add,
        BinOp::Sub => Op::Sub,
        BinOp::Mul => Op::Mul,
        BinOp::Div => Op::Div,
        BinOp::Mod => Op::Mod,
        BinOp::Pow => Op::Pow,
        BinOp::Eq => Op::Eq,
        BinOp::Ne => Op::Ne,
        BinOp::StrictEq => Op::StrictEq,
        BinOp::StrictNe => Op::StrictNe,
        BinOp::Lt => Op::Lt,
        BinOp::Gt => Op::Gt,
        BinOp::Le => Op::Le,
        BinOp::Ge => Op::Ge,
        BinOp::BitAnd => Op::BitAnd,
        BinOp::BitOr => Op::BitOr,
        BinOp::BitXor => Op::BitXor,
        BinOp::Shl => Op::Shl,
        BinOp::Shr => Op::Shr,
        BinOp::UShr => Op::UShr,
        BinOp::In => Op::In,
        BinOp::InstanceOf => Op::InstanceOf,
    }
}

fn logical_jump(op: LogicalOp) -> Op {
    match op {
        LogicalOp::And => Op::JumpIfFalseKeep(0),
        LogicalOp::Or => Op::JumpIfTrueKeep(0),
        LogicalOp::Nullish => Op::JumpIfNotNullishKeep(0),
    }
}

impl Compiler {
    fn err(&self, msg: impl Into<String>) -> SyntaxError {
        SyntaxError::new(msg, self.line, 1)
    }

    fn st(&mut self) -> &mut FnState {
        self.states.last_mut().expect("function state")
    }

    fn st_ref(&self) -> &FnState {
        self.states.last().expect("function state")
    }

    fn finish(&self, st: FnState) -> FunctionProto {
        let cache_len = st.consts.len();
        FunctionProto {
            name: st.name,
            kind: st.kind,
            num_params: st.num_params,
            arity: st.arity,
            has_rest: st.has_rest,
            uses_arguments: st.uses_arguments,
            is_async: st.is_async,
            is_generator: st.is_generator,
            sloppy: self.module.sloppy && st.kind != ProtoKind::Arrow,
            code: st.code,
            lines: st.lines,
            consts: st.consts,
            functions: st.functions,
            upvalues: st.upvalues,
            module: self.module.clone(),
            line: st.line,
            span: st.span,
            templates: (0..st.template_sites)
                .map(|_| std::cell::RefCell::new(None))
                .collect(),
            global_cache: (0..cache_len)
                .map(|_| std::cell::Cell::new(u32::MAX))
                .collect(),
            evals: st.evals,
        }
    }

    fn emit(&mut self, op: Op) -> usize {
        let private = |c: &Compiler, k: u32| matches!(c.st_ref().consts.get(k as usize), Some(Value::Str(s)) if s.starts_with("\u{0}#"));
        match op {
            Op::GetProp(k) if private(self, k) => return self.emit_raw(Op::GetPrivate(k)),
            Op::GetLocalProp(s, k) if private(self, k) => {
                self.emit_raw(Op::GetLocal(s));
                return self.emit_raw(Op::GetPrivate(k));
            }
            Op::GetMethod(k) if private(self, k) => {
                self.emit_raw(Op::Dup);
                self.emit_raw(Op::GetPrivate(k));
                return self.emit_raw(Op::Swap);
            }
            Op::GetLocalMethod(s, k) if private(self, k) => {
                self.emit_raw(Op::GetLocal(s));
                self.emit_raw(Op::Dup);
                self.emit_raw(Op::GetPrivate(k));
                return self.emit_raw(Op::Swap);
            }
            Op::SetProp(k) if private(self, k) => return self.emit_raw(Op::SetPrivate(k)),
            Op::StoreProp(k) if private(self, k) => {
                self.emit_raw(Op::SetPrivate(k));
                return self.emit_raw(Op::Pop);
            }
            Op::StoreLocalProp(s, k) if private(self, k) => {
                self.emit_raw(Op::GetLocal(s));
                self.emit_raw(Op::Swap);
                self.emit_raw(Op::SetPrivate(k));
                return self.emit_raw(Op::Pop);
            }
            _ => {}
        }
        self.emit_raw(op)
    }

    fn emit_raw(&mut self, op: Op) -> usize {
        let line = self.line;
        let st = self.st();
        st.code.push(op);
        st.lines.push(line);
        st.code.len() - 1
    }

    fn here(&self) -> u32 {
        self.st_ref().code.len() as u32
    }

    fn patch_to(&mut self, at: usize, t: u32) {
        let op = &mut self.st().code[at];
        *op = match *op {
            Op::Jump(_) => Op::Jump(t),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(t),
            Op::JumpIfTrue(_) => Op::JumpIfTrue(t),
            Op::JumpIfFalseKeep(_) => Op::JumpIfFalseKeep(t),
            Op::JumpIfTrueKeep(_) => Op::JumpIfTrueKeep(t),
            Op::JumpIfNotNullishKeep(_) => Op::JumpIfNotNullishKeep(t),
            Op::JumpIfNullish(_) => Op::JumpIfNullish(t),
            Op::JumpIfNullishCall(_) => Op::JumpIfNullishCall(t),
            Op::JumpIfNotUndef(_) => Op::JumpIfNotUndef(t),
            Op::IterNext(_) => Op::IterNext(t),
            Op::TryBegin(_, s) => Op::TryBegin(t, s),
            Op::YieldReturn(_) => Op::YieldReturn(t),
            Op::TemplateCache(s, _) => Op::TemplateCache(s, t),
            Op::CmpJump(c, _) => Op::CmpJump(c, t),
            other => other,
        };
    }

    fn patch(&mut self, at: usize) {
        let t = self.here();
        self.patch_to(at, t);
    }

    fn constant(&mut self, v: Value) -> u32 {
        let st = self.st();
        st.consts.push(v);
        (st.consts.len() - 1) as u32
    }

    fn str_const(&mut self, s: &Rc<str>) -> u32 {
        if let Some(&i) = self.st_ref().const_strs.get(s) {
            return i;
        }
        let i = self.constant(Value::Str(s.clone()));
        self.st().const_strs.insert(s.clone(), i);
        i
    }

    fn hidden_name(&mut self, prefix: &str) -> Rc<str> {
        self.hidden += 1;
        format!("%{prefix}{}", self.hidden).into()
    }

    fn locals_len(&self) -> usize {
        self.st_ref().locals.len()
    }

    fn add_local(&mut self, name: Rc<str>, is_const: bool) -> u32 {
        let st = self.st();
        let depth = st.depth;
        st.locals.push(Local {
            name,
            depth,
            captured: false,
            is_const,
            tdz: false,
            sticky: false,
        });
        (st.locals.len() - 1) as u32
    }

    fn add_lexical(&mut self, name: Rc<str>, is_const: bool) -> u32 {
        let slot = self.add_local(name, is_const);
        self.st().locals[slot as usize].tdz = true;
        slot
    }

    fn mark_init(&mut self, name: &str) {
        let st = self.st();
        if let Some(l) = st.locals.iter_mut().rev().find(|l| &*l.name == name) {
            if !l.sticky {
                l.tdz = false;
            }
        }
    }

    fn mark_init_pattern(&mut self, p: &Pattern) {
        let mut names = Vec::new();
        p.bound_names(&mut names);
        for n in &names {
            self.mark_init(n);
        }
    }

    fn local_tdz(&self, si: usize, slot: u32) -> bool {
        self.states[si].locals[slot as usize].tdz
    }

    fn upvalue_tdz(&self, si: usize, name: &str) -> bool {
        let mut i = si;
        while i > 0 {
            i -= 1;
            if let Some((slot, _)) = self.resolve_local(i, name) {
                return self.local_tdz(i, slot);
            }
            if self.states[i]
                .eval_upvals
                .as_ref()
                .is_some_and(|u| u.iter().any(|(n, _, _)| &**n == name))
            {
                return true;
            }
        }
        false
    }

    fn completion_slot(&self) -> Option<u32> {
        match self.completion {
            Some((si, slot)) if si + 1 == self.states.len() => Some(slot),
            _ => None,
        }
    }

    fn finally_block(&mut self, f: &[Stmt]) -> R<()> {
        let saved = self.completion.take();
        let r = self.block(f);
        self.completion = saved;
        r
    }

    fn eval_scope(&mut self, ctx: Rc<EvalCtx>) -> u32 {
        let si = self.states.len() - 1;
        let mut outer: Vec<Rc<str>> = Vec::new();
        for i in (0..si).rev() {
            let st = &self.states[i];
            let names = st.locals.iter().rev().map(|l| l.name.clone()).chain(
                st.eval_upvals
                    .iter()
                    .flat_map(|u| u.iter().map(|(n, _, _)| n.clone())),
            );
            for n in names {
                if !n.is_empty() && !n.starts_with('%') && !outer.contains(&n) {
                    outer.push(n);
                }
            }
        }
        let mut upvals = Vec::new();
        for n in outer {
            if self.resolve_local(si, &n).is_some() {
                continue;
            }
            if let Some((idx, c)) = self.resolve_upvalue(si, &n) {
                upvals.push((n, idx, c));
            }
        }
        let (kind, is_static) = match self.method_state() {
            Some(s) => (s.kind, s.is_static),
            None => (ProtoKind::Module, false),
        };
        let kind = if ctx.field_init && kind == ProtoKind::DerivedConstructor {
            ProtoKind::Method
        } else {
            kind
        };
        let st = self.st();
        for l in st.locals.iter_mut() {
            l.captured = true;
        }
        let locals = st
            .locals
            .iter()
            .map(|l| (l.name.clone(), l.is_const))
            .collect();
        st.evals.push(Rc::new(EvalScope {
            locals,
            upvals: Rc::new(upvals),
            kind,
            is_static,
            ctx,
        }));
        (st.evals.len() - 1) as u32
    }

    fn begin_scope(&mut self) {
        self.st().depth += 1;
    }

    fn scope_cut(&mut self) -> Option<usize> {
        let st = self.st();
        st.depth -= 1;
        let d = st.depth;
        let mut n = st.locals.len();
        while n > 0 && st.locals[n - 1].depth > d {
            n -= 1;
        }
        if n < st.locals.len() {
            st.locals.truncate(n);
            Some(n)
        } else {
            None
        }
    }

    fn end_scope(&mut self) {
        if let Some(n) = self.scope_cut() {
            self.emit(Op::PopTo(n as u32));
        }
    }

    fn is_repl_top(&self) -> bool {
        self.mode == Mode::Repl && self.states.len() == 1 && self.st_ref().depth == 1
    }

    fn resolve_local(&self, si: usize, name: &str) -> Option<(u32, bool)> {
        let st = &self.states[si];
        st.locals
            .iter()
            .rposition(|l| &*l.name == name)
            .map(|i| (i as u32, st.locals[i].is_const))
    }

    fn add_upvalue(&mut self, si: usize, is_local: bool, index: u32) -> u32 {
        let ups = &mut self.states[si].upvalues;
        if let Some(i) = ups
            .iter()
            .position(|u| u.is_local == is_local && u.index == index)
        {
            return i as u32;
        }
        ups.push(UpvalDesc { is_local, index });
        (ups.len() - 1) as u32
    }

    fn resolve_upvalue(&mut self, si: usize, name: &str) -> Option<(u32, bool)> {
        if si == 0 {
            return None;
        }
        if let Some((slot, c)) = self.resolve_local(si - 1, name) {
            self.states[si - 1].locals[slot as usize].captured = true;
            return Some((self.add_upvalue(si, true, slot), c));
        }
        if let Some(ups) = self.states[si - 1].eval_upvals.clone() {
            let (_, idx, c) = ups.iter().find(|(n, _, _)| &**n == name)?;
            return Some((self.add_upvalue(si, false, *idx), *c));
        }
        if let Some((idx, c)) = self.resolve_upvalue(si - 1, name) {
            return Some((self.add_upvalue(si, false, idx), c));
        }
        None
    }

    fn resolve(&mut self, name: &str) -> Var {
        let si = self.states.len() - 1;
        if let Some((s, c)) = self.resolve_local(si, name) {
            if self.local_tdz(si, s) {
                return Var::LocalTdz(s, c);
            }
            return Var::Local(s, c);
        }
        if let Some((i, c)) = self.resolve_upvalue(si, name) {
            if self.upvalue_tdz(si, name) {
                return Var::UpvalTdz(i, c);
            }
            return Var::Upval(i, c);
        }
        Var::Global
    }

    fn load_var(&mut self, name: &Rc<str>) {
        match self.resolve(name) {
            Var::Local(s, _) => {
                self.emit(Op::GetLocal(s));
            }
            Var::Upval(i, _) => {
                self.emit(Op::GetUpval(i));
            }
            Var::LocalTdz(s, _) => {
                let k = self.str_const(name);
                self.emit(Op::GetLocalChecked(s, k));
            }
            Var::UpvalTdz(i, _) => {
                let k = self.str_const(name);
                self.emit(Op::GetUpvalChecked(i, k));
            }
            Var::Global if self.alias(name).is_some() => {
                let (h, key) = self.alias(name).expect("alias");
                self.load_var(&h);
                let k = self.str_const(&key);
                self.emit(Op::GetProp(k));
            }
            Var::Global => {
                if &**name == "undefined" {
                    self.emit(Op::Undefined);
                } else {
                    let k = self.str_const(name);
                    self.emit(Op::GetGlobal(k));
                }
            }
        }
    }

    fn store_var(&mut self, name: &Rc<str>, init: bool) {
        match self.resolve(name) {
            Var::Local(s, c) => {
                if c && !init {
                    let k = self.str_const(name);
                    self.emit(Op::ThrowConstAssign(k));
                } else {
                    self.emit(Op::SetLocal(s));
                    self.export_hook(name);
                }
            }
            Var::Upval(i, c) => {
                if c && !init {
                    let k = self.str_const(name);
                    self.emit(Op::ThrowConstAssign(k));
                } else {
                    self.emit(Op::SetUpval(i));
                    self.export_hook(name);
                }
            }
            Var::LocalTdz(s, c) => {
                let k = self.str_const(name);
                if init {
                    self.emit(Op::SetLocal(s));
                    self.export_hook(name);
                } else if c {
                    self.emit(Op::GetLocalChecked(s, k));
                    self.emit(Op::Pop);
                    self.emit(Op::ThrowConstAssign(k));
                } else {
                    self.emit(Op::SetLocalChecked(s, k));
                    self.export_hook(name);
                }
            }
            Var::UpvalTdz(i, c) => {
                let k = self.str_const(name);
                if init {
                    self.emit(Op::SetUpval(i));
                    self.export_hook(name);
                } else if c {
                    self.emit(Op::GetUpvalChecked(i, k));
                    self.emit(Op::Pop);
                    self.emit(Op::ThrowConstAssign(k));
                } else {
                    self.emit(Op::SetUpvalChecked(i, k));
                    self.export_hook(name);
                }
            }
            Var::Global if self.alias(name).is_some() => {
                let k = self.str_const(name);
                self.emit(Op::ThrowConstAssign(k));
            }
            Var::Global => {
                let k = self.str_const(name);
                if init && self.is_repl_top() {
                    let c = self.repl_consts.contains(name);
                    self.emit(Op::DefineGlobal(k, c));
                } else {
                    self.emit(Op::SetGlobal(k));
                }
            }
        }
    }

    fn collect_decls<'a>(
        &mut self,
        s: &'a Stmt,
        top: bool,
        names: &mut Vec<(Rc<str>, bool)>,
        funcs: &mut Vec<&'a Rc<FuncDef>>,
        imports: &mut Vec<&'a ImportDecl>,
    ) -> R<()> {
        match &s.kind {
            StmtKind::Decl(DeclKind::Var, _) => {}
            StmtKind::Decl(kind, decls) => {
                for (p, _) in decls {
                    let mut v = Vec::new();
                    p.bound_names(&mut v);
                    for n in v {
                        names.push((n, *kind == DeclKind::Const));
                    }
                }
            }
            StmtKind::Function(f) => {
                if let Some(n) = &f.name {
                    names.push((n.clone(), false));
                }
                funcs.push(f);
            }
            StmtKind::Class(c) => {
                if let Some(n) = &c.name {
                    names.push((n.clone(), false));
                }
            }
            StmtKind::Import(i) => {
                if !top {
                    return Err(SyntaxError::new(
                        "Cannot use import statement outside the module top level",
                        s.line,
                        1,
                    ));
                }
                let hidden: Rc<str> = format!("%import{}", imports.len()).into();
                names.push((hidden.clone(), true));
                if let Some(n) = &i.default {
                    self.import_aliases
                        .push((n.clone(), hidden.clone(), "default".into()));
                }
                if let Some(n) = &i.namespace {
                    names.push((n.clone(), true));
                }
                for (imported, local) in &i.named {
                    self.import_aliases
                        .push((local.clone(), hidden.clone(), imported.clone()));
                }
                imports.push(i);
            }
            StmtKind::Export(e) => {
                if !top {
                    return Err(SyntaxError::new("Unexpected token 'export'", s.line, 1));
                }
                match e {
                    ExportDecl::Decl(inner) => {
                        if let StmtKind::Decl(DeclKind::Var, decls) = &inner.kind {
                            for (p, _) in decls {
                                let mut v = Vec::new();
                                p.bound_names(&mut v);
                                for n in v {
                                    self.exports.push(ExportEntry::Local(n.clone(), n));
                                }
                            }
                        }
                        let before = names.len();
                        self.collect_decls(inner, top, names, funcs, imports)?;
                        for (n, _) in names[before..].iter() {
                            self.exports.push(ExportEntry::Local(n.clone(), n.clone()));
                        }
                    }
                    ExportDecl::DefaultDecl(inner) => {
                        let before = names.len();
                        self.collect_decls(inner, top, names, funcs, imports)?;
                        if let Some((n, _)) = names.get(before) {
                            self.exports
                                .push(ExportEntry::Local(n.clone(), "default".into()));
                        }
                    }
                    ExportDecl::Default(_) => {
                        names.push(("%default".into(), false));
                        self.exports
                            .push(ExportEntry::Local("%default".into(), "default".into()));
                    }
                    ExportDecl::Names(list) => {
                        for (local, exported) in list {
                            self.exports
                                .push(ExportEntry::Local(local.clone(), exported.clone()));
                        }
                    }
                    ExportDecl::From { .. } => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn declare_vars(&mut self, stmts: &[Stmt]) {
        let mut names = Vec::new();
        for s in stmts {
            collect_var_names(s, &mut names);
        }
        let repl = self.is_repl_top() && self.script_eval != 2;
        for n in names {
            if self.st_ref().vars.contains(&n) {
                continue;
            }
            let exists = self
                .st_ref()
                .locals
                .iter()
                .any(|l| l.name == n && l.depth <= self.st_ref().depth);
            self.st().vars.push(n.clone());
            if exists {
                continue;
            }
            if repl {
                self.emit(Op::Undefined);
                let k = self.str_const(&n);
                self.emit(Op::DefineGlobal(k, false));
                self.emit(Op::Pop);
            } else {
                self.emit(Op::Undefined);
                self.add_local(n, false);
            }
        }
    }

    fn hoist(&mut self, stmts: &[Stmt]) -> R<()> {
        let refs: Vec<&Stmt> = stmts.iter().collect();
        self.hoist_list(&refs, false)
    }

    fn hoist_list(&mut self, stmts: &[&Stmt], top: bool) -> R<()> {
        let mut names = Vec::new();
        let mut funcs = Vec::new();
        let mut imports = Vec::new();
        let mut reexports = Vec::new();
        for s in stmts {
            self.collect_decls(s, top, &mut names, &mut funcs, &mut imports)?;
            if let StmtKind::Export(ExportDecl::From {
                source,
                names: list,
            }) = &s.kind
            {
                reexports.push((source.clone(), list.clone()));
            }
        }
        let var_scope = top || std::mem::take(&mut self.var_scope_next);
        let mut var_names = Vec::new();
        for s in stmts {
            collect_var_names(s, &mut var_names);
        }
        for (n, _) in &names {
            let is_fn = funcs.iter().any(|f| f.name.as_ref() == Some(n));
            if var_scope && is_fn && !(top && self.mode == Mode::Module) {
                continue;
            }
            if var_names.contains(n) {
                return Err(self.err(format!("Identifier '{n}' has already been declared")));
            }
        }
        let depth = self.st_ref().depth;
        let mut fresh = Vec::new();
        for (i, (n, c)) in names.iter().enumerate() {
            let is_fn = funcs.iter().any(|f| f.name.as_ref() == Some(n));
            let dup_new = names[..i].iter().any(|(m, _)| m == n);
            let var_clash = self.st_ref().vars.contains(n)
                && self
                    .st_ref()
                    .locals
                    .iter()
                    .any(|l| l.depth == depth && &l.name == n);
            if var_clash && is_fn {
                continue;
            }
            let dup_old = self
                .st_ref()
                .locals
                .iter()
                .any(|l| l.depth == depth && &l.name == n);
            if dup_new || dup_old {
                return Err(self.err(format!("Identifier '{n}' has already been declared")));
            }
            fresh.push((n.clone(), *c));
        }
        let names = fresh;
        let repl = self.is_repl_top() && self.script_eval != 2;
        let import_names: Vec<Rc<str>> =
            imports.iter().filter_map(|i| i.namespace.clone()).collect();
        for (n, c) in &names {
            let lexical_kind = !funcs.iter().any(|f| f.name.as_ref() == Some(n))
                && !n.starts_with("%import")
                && !import_names.contains(n);
            let lexical = (!repl || self.script_eval == 1) && lexical_kind;
            if lexical {
                self.emit(Op::Uninit);
                self.add_lexical(n.clone(), *c);
                continue;
            }
            if repl && lexical_kind {
                self.emit(Op::Uninit);
            } else {
                self.emit(Op::Undefined);
            }
            if repl {
                if *c {
                    self.repl_consts.push(n.clone());
                } else {
                    self.repl_consts.retain(|m| m != n);
                }
                let k = self.str_const(n);
                self.emit(Op::DefineGlobal(k, *c));
                self.emit(Op::Pop);
            } else {
                self.add_local(n.clone(), *c);
            }
        }
        if top && self.mode == Mode::Module {
            let mut pre: Vec<(Rc<str>, Rc<str>)> = self
                .exports
                .iter()
                .filter_map(|e| match e {
                    ExportEntry::Local(l, x) => Some((x.clone(), l.clone())),
                    _ => None,
                })
                .collect();
            pre.sort();
            self.export_names = pre.iter().map(|(x, l)| (l.clone(), x.clone())).collect();
            for f in &funcs {
                let idx = self.function(f, None, None)?;
                self.emit(Op::Closure(idx));
                if let Some(n) = &f.name {
                    self.store_var(n, true);
                }
                self.emit(Op::Pop);
            }
            for (exported, local) in &pre {
                self.load_var(&"%ns".into());
                if funcs.iter().any(|f| f.name.as_ref() == Some(local)) {
                    self.load_var(local);
                } else {
                    self.emit(Op::Undefined);
                }
                let k = self.str_const(exported);
                self.emit(Op::SetProp(k));
                self.emit(Op::Pop);
            }
        }
        for (n, imp) in imports.iter().enumerate() {
            let src = self.str_const(&imp.source);
            self.emit(Op::Import(src));
            if let Some(ns) = &imp.namespace {
                self.store_var(ns, true);
            }
            let hidden: Rc<str> = format!("%import{n}").into();
            self.store_var(&hidden, true);
            self.emit(Op::Pop);
        }
        for (source, list) in reexports {
            let hidden = self.hidden_name("reexport");
            let src = self.str_const(&source);
            self.emit(Op::Import(src));
            self.add_local(hidden.clone(), true);
            match list {
                None => self.exports.push(ExportEntry::All(hidden)),
                Some(list) => {
                    for (imported, exported) in list {
                        self.exports
                            .push(ExportEntry::Named(hidden.clone(), imported, exported));
                    }
                }
            }
        }
        if !(top && self.mode == Mode::Module) {
            for f in funcs {
                let idx = self.function(f, None, None)?;
                self.emit(Op::Closure(idx));
                if let Some(n) = &f.name {
                    self.store_var(n, true);
                }
                self.emit(Op::Pop);
            }
        }
        Ok(())
    }

    fn alias(&self, name: &str) -> Option<(Rc<str>, Rc<str>)> {
        self.import_aliases
            .iter()
            .find(|(l, _, _)| &**l == name)
            .map(|(_, h, k)| (h.clone(), k.clone()))
    }

    fn is_export_binding(&self, name: &str) -> bool {
        if self.export_names.is_empty() || !self.export_names.iter().any(|(l, _)| &**l == name) {
            return false;
        }
        if self.states[1..]
            .iter()
            .any(|st| st.locals.iter().any(|l| &*l.name == name))
        {
            return false;
        }
        match self.states[0].locals.iter().rposition(|l| &*l.name == name) {
            Some(i) => self.states[0].locals[i].depth == 1,
            None => false,
        }
    }

    fn export_hook(&mut self, name: &Rc<str>) {
        if !self.is_export_binding(name) {
            return;
        }
        let targets: Vec<Rc<str>> = self
            .export_names
            .iter()
            .filter(|(l, _)| l == name)
            .map(|(_, x)| x.clone())
            .collect();
        for x in targets {
            self.load_var(&"%ns".into());
            self.emit(Op::Swap);
            let k = self.str_const(&x);
            self.emit(Op::SetProp(k));
        }
    }

    fn block(&mut self, stmts: &[Stmt]) -> R<()> {
        self.begin_scope();
        self.hoist(stmts)?;
        for s in stmts {
            self.stmt(s)?;
        }
        self.end_scope();
        Ok(())
    }

    fn sub_stmt(&mut self, s: &Stmt) -> R<()> {
        match &s.kind {
            StmtKind::Decl(..) | StmtKind::Class(_) | StmtKind::Function(_) => {
                Err(SyntaxError::new(
                    "Lexical declaration cannot appear in a single-statement context",
                    s.line,
                    1,
                ))
            }
            _ => self.stmt(s),
        }
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        self.line = s.line;
        if let Some(slot) = self.completion_slot() {
            if matches!(
                s.kind,
                StmtKind::If(..)
                    | StmtKind::While(..)
                    | StmtKind::DoWhile(..)
                    | StmtKind::For { .. }
                    | StmtKind::ForOf { .. }
                    | StmtKind::ForIn { .. }
                    | StmtKind::Switch(..)
                    | StmtKind::Try { .. }
            ) {
                self.emit(Op::Undefined);
                self.emit(Op::SetLocal(slot));
                self.emit(Op::Pop);
            }
        }
        match &s.kind {
            StmtKind::Expr(e) => match self.completion_slot() {
                Some(slot) => {
                    self.expr(e)?;
                    self.emit(Op::SetLocal(slot));
                    self.emit(Op::Pop);
                }
                None => self.expr_discard(e)?,
            },
            StmtKind::Decl(DeclKind::Var, decls) => {
                for (pattern, init) in decls {
                    if let Some(e) = init {
                        let name = match pattern {
                            Pattern::Ident(n) => Some(n.clone()),
                            _ => None,
                        };
                        self.expr_named(e, name.as_ref())?;
                        self.bind_pattern(pattern, BindMode::Assign)?;
                    }
                }
            }
            StmtKind::Decl(_, decls) => {
                for (pattern, init) in decls {
                    match init {
                        Some(e) => {
                            let name = match pattern {
                                Pattern::Ident(n) => Some(n.clone()),
                                _ => None,
                            };
                            self.expr_named(e, name.as_ref())?;
                        }
                        None => {
                            self.emit(Op::Undefined);
                        }
                    }
                    self.bind_pattern(pattern, BindMode::Init)?;
                    self.mark_init_pattern(pattern);
                }
            }
            StmtKind::Function(_) | StmtKind::Import(_) | StmtKind::Empty => {}
            StmtKind::Class(c) => {
                self.class(c, None)?;
                if let Some(n) = &c.name {
                    self.store_var(n, true);
                    self.mark_init(n);
                }
                self.emit(Op::Pop);
            }
            StmtKind::Return(e) => self.return_stmt(e.as_ref())?,
            StmtKind::If(test, cons, alt) => {
                let j = self.jump_if_false(test)?;
                self.sub_stmt(cons)?;
                if let Some(alt) = alt {
                    let k = self.emit(Op::Jump(0));
                    self.patch(j);
                    self.sub_stmt(alt)?;
                    self.patch(k);
                } else {
                    self.patch(j);
                }
            }
            StmtKind::Block(stmts) => self.block(stmts)?,
            StmtKind::While(test, body) => {
                let start = self.here();
                let exit = self.jump_if_false(test)?;
                self.push_loop(false);
                self.sub_stmt(body)?;
                let ctx = self.st().loops.pop().expect("loop");
                for j in ctx.continue_jumps {
                    self.patch_to(j, start);
                }
                self.emit(Op::Jump(start));
                self.patch(exit);
                for j in ctx.break_jumps {
                    self.patch(j);
                }
            }
            StmtKind::DoWhile(body, test) => {
                let start = self.here();
                self.push_loop(false);
                self.sub_stmt(body)?;
                let ctx = self.st().loops.pop().expect("loop");
                for j in ctx.continue_jumps {
                    self.patch(j);
                }
                self.expr(test)?;
                self.emit(Op::JumpIfTrue(start));
                for j in ctx.break_jumps {
                    self.patch(j);
                }
            }
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                self.begin_scope();
                let mut loop_base = None;
                if let Some(init) = init {
                    match &init.kind {
                        StmtKind::Decl(..) => {
                            let base = self.locals_len();
                            self.hoist(std::slice::from_ref(init.as_ref()))?;
                            self.stmt(init)?;
                            if self.locals_len() > base {
                                loop_base = Some(base);
                            }
                        }
                        StmtKind::Expr(e) => {
                            self.expr(e)?;
                            self.emit(Op::Pop);
                        }
                        _ => return Err(self.err("Invalid for loop initializer")),
                    }
                }
                let start = self.here();
                let exit = match test {
                    Some(t) => Some(self.jump_if_false(t)?),
                    None => None,
                };
                self.push_loop(false);
                self.sub_stmt(body)?;
                let ctx = self.st().loops.pop().expect("loop");
                for j in ctx.continue_jumps {
                    self.patch(j);
                }
                if let Some(b) = loop_base {
                    if self.st_ref().locals[b..].iter().any(|l| l.captured) {
                        self.emit(Op::CloseUpvalues(b as u32));
                    }
                }
                if let Some(u) = update {
                    self.expr_discard(u)?;
                }
                self.emit(Op::Jump(start));
                if let Some(exit) = exit {
                    self.patch(exit);
                }
                for j in ctx.break_jumps {
                    self.patch(j);
                }
                self.end_scope();
            }
            StmtKind::ForOf {
                decl,
                pattern,
                iter,
                body,
                is_await: false,
            } => self.for_each(*decl, pattern, iter, body, false)?,
            StmtKind::ForOf {
                decl,
                pattern,
                iter,
                body,
                is_await: true,
            } => self.for_await(*decl, pattern, iter, body)?,
            StmtKind::ForIn {
                decl,
                pattern,
                obj,
                body,
            } => self.for_each(*decl, pattern, obj, body, true)?,
            StmtKind::Break(label) => {
                let found = match label {
                    None => self.st_ref().loops.iter().rposition(|l| !l.is_block),
                    Some(l) => self
                        .st_ref()
                        .loops
                        .iter()
                        .rposition(|c| c.labels.contains(l)),
                };
                let Some(idx) = found else {
                    return Err(match label {
                        Some(l) => self.err(format!("Undefined label '{l}'")),
                        None => self.err("Illegal break statement"),
                    });
                };
                let ctx = &self.st_ref().loops[idx];
                let (target, try_depth) = (ctx.break_locals, ctx.try_depth);
                self.unwind_tries(try_depth)?;
                self.close_iterators(idx);
                if self.locals_len() > target {
                    self.emit(Op::PopTo(target as u32));
                }
                let j = self.emit(Op::Jump(0));
                self.st().loops[idx].break_jumps.push(j);
            }
            StmtKind::Labeled(..) => self.labeled(s)?,
            StmtKind::Continue(label) => {
                let found = match label {
                    None => self
                        .st_ref()
                        .loops
                        .iter()
                        .rposition(|l| !l.is_switch && !l.is_block),
                    Some(l) => match self
                        .st_ref()
                        .loops
                        .iter()
                        .rposition(|c| c.labels.contains(l))
                    {
                        Some(i)
                            if self.st_ref().loops[i].is_switch
                                || self.st_ref().loops[i].is_block =>
                        {
                            return Err(self.err(format!(
                                "Illegal continue statement: '{l}' does not denote an iteration statement"
                            )));
                        }
                        other => other,
                    },
                };
                let Some(idx) = found else {
                    return Err(match label {
                        Some(l) => self.err(format!("Undefined label '{l}'")),
                        None => self
                            .err("Illegal continue statement: no surrounding iteration statement"),
                    });
                };
                let ctx = &self.st_ref().loops[idx];
                let (target, try_depth) = (ctx.continue_locals, ctx.try_depth);
                self.unwind_tries(try_depth)?;
                self.close_iterators(idx + 1);
                if self.locals_len() > target {
                    self.emit(Op::PopTo(target as u32));
                }
                let j = self.emit(Op::Jump(0));
                self.st().loops[idx].continue_jumps.push(j);
            }
            StmtKind::Throw(e) => {
                self.expr(e)?;
                self.emit(Op::Throw);
            }
            StmtKind::Try {
                block,
                param,
                handler,
                finalizer,
            } => self.try_stmt(block, param.as_ref(), handler.as_ref(), finalizer.as_ref())?,
            StmtKind::Switch(disc, cases) => self.switch(disc, cases)?,
            StmtKind::Export(e) => match e {
                ExportDecl::Decl(inner) | ExportDecl::DefaultDecl(inner) => self.stmt(inner)?,
                ExportDecl::Default(expr) => {
                    self.expr_named(expr, Some(&"default".into()))?;
                    self.store_var(&"%default".into(), true);
                    self.emit(Op::Pop);
                }
                ExportDecl::Names(_) | ExportDecl::From { .. } => {}
            },
        }
        Ok(())
    }

    fn push_loop(&mut self, is_switch: bool) {
        let n = self.locals_len();
        let try_depth = self.st_ref().tries.len();
        let labels = std::mem::take(&mut self.pending_labels);
        self.st().loops.push(LoopCtx {
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
            break_locals: n,
            continue_locals: n,
            try_depth,
            is_switch,
            is_block: false,
            labels,
            iter_slot: None,
        });
    }

    fn labeled(&mut self, s: &Stmt) -> R<()> {
        let mut labels = Vec::new();
        let mut cur = s;
        while let StmtKind::Labeled(l, body) = &cur.kind {
            if labels.contains(l) || self.st_ref().loops.iter().any(|c| c.labels.contains(l)) {
                return Err(self.err(format!("Label '{l}' has already been declared")));
            }
            labels.push(l.clone());
            cur = body;
        }
        let is_loop = matches!(
            cur.kind,
            StmtKind::While(..)
                | StmtKind::DoWhile(..)
                | StmtKind::For { .. }
                | StmtKind::ForOf { .. }
                | StmtKind::ForIn { .. }
        );
        if is_loop {
            self.pending_labels = labels;
            return self.stmt(cur);
        }
        let n = self.locals_len();
        let try_depth = self.st_ref().tries.len();
        self.st().loops.push(LoopCtx {
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
            break_locals: n,
            continue_locals: n,
            try_depth,
            is_switch: false,
            is_block: true,
            labels,
            iter_slot: None,
        });
        let r = self.sub_stmt(cur);
        let ctx = self.st().loops.pop().expect("label");
        r?;
        for j in ctx.break_jumps {
            self.patch(j);
        }
        Ok(())
    }

    fn for_each(
        &mut self,
        decl: Option<DeclKind>,
        pattern: &Pattern,
        source: &Expr,
        body: &Stmt,
        keys: bool,
    ) -> R<()> {
        let labels = std::mem::take(&mut self.pending_labels);
        self.begin_scope();
        self.expr(source)?;
        self.pending_labels = labels;
        self.emit(if keys { Op::ForInIter } else { Op::GetIter });
        let iter_name = self.hidden_name("iter");
        let iter_slot = self.add_local(iter_name, true);
        let start = self.here();
        self.begin_scope();
        let decl = decl.filter(|k| *k != DeclKind::Var);
        if let Some(kind) = decl {
            let mut names = Vec::new();
            pattern.bound_names(&mut names);
            for n in names {
                self.emit(Op::Undefined);
                self.add_local(n, kind == DeclKind::Const);
            }
        }
        self.emit(Op::GetLocal(iter_slot));
        let next = self.emit(Op::IterNext(0));
        self.bind_pattern(
            pattern,
            if decl.is_some() {
                BindMode::Init
            } else {
                BindMode::Assign
            },
        )?;
        let try_depth = self.st_ref().tries.len();
        let continue_locals = iter_slot as usize + 1;
        let labels = std::mem::take(&mut self.pending_labels);
        self.st().loops.push(LoopCtx {
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
            break_locals: iter_slot as usize,
            continue_locals,
            try_depth,
            is_switch: false,
            is_block: false,
            labels,
            iter_slot: if keys { None } else { Some(iter_slot) },
        });
        self.sub_stmt(body)?;
        let ctx = self.st().loops.pop().expect("loop");
        for j in ctx.continue_jumps {
            self.patch_to(j, start);
        }
        self.end_scope();
        self.emit(Op::Jump(start));
        self.patch(next);
        self.end_scope();
        for j in ctx.break_jumps {
            self.patch(j);
        }
        Ok(())
    }

    fn require_await(&mut self) -> R<()> {
        let module_top = self.states.len() == 1 && self.st_ref().kind == ProtoKind::Module;
        if module_top {
            self.st().is_async = true;
            Ok(())
        } else if self.st_ref().is_async {
            Ok(())
        } else {
            Err(self.err("await is only valid in async functions and the top level of modules"))
        }
    }

    fn emit_async_next(&mut self) {
        let next: Rc<str> = "next".into();
        let k = self.str_const(&next);
        self.emit(Op::GetMethod(k));
        self.emit(Op::Call(0, NO_NAME));
        self.emit(Op::Await);
    }

    fn for_await(
        &mut self,
        decl: Option<DeclKind>,
        pattern: &Pattern,
        source: &Expr,
        body: &Stmt,
    ) -> R<()> {
        self.require_await()?;
        let labels = std::mem::take(&mut self.pending_labels);
        self.begin_scope();
        self.expr(source)?;
        self.pending_labels = labels;
        self.emit(Op::GetAsyncIter);
        let iter_name = self.hidden_name("aiter");
        let iter_slot = self.add_local(iter_name, true);
        let start = self.here();
        self.begin_scope();
        if let Some(kind) = decl {
            let mut names = Vec::new();
            pattern.bound_names(&mut names);
            for n in names {
                self.emit(Op::Undefined);
                self.add_local(n, kind == DeclKind::Const);
            }
        }
        self.emit(Op::GetLocal(iter_slot));
        self.emit_async_next();
        self.emit(Op::Dup);
        let done: Rc<str> = "done".into();
        let k = self.str_const(&done);
        self.emit(Op::GetProp(k));
        let exit = self.emit(Op::JumpIfTrue(0));
        let value: Rc<str> = "value".into();
        let k = self.str_const(&value);
        self.emit(Op::GetProp(k));
        self.bind_pattern(
            pattern,
            if decl.is_some() {
                BindMode::Init
            } else {
                BindMode::Assign
            },
        )?;
        let try_depth = self.st_ref().tries.len();
        let labels = std::mem::take(&mut self.pending_labels);
        self.st().loops.push(LoopCtx {
            break_jumps: Vec::new(),
            continue_jumps: Vec::new(),
            break_locals: iter_slot as usize,
            continue_locals: iter_slot as usize + 1,
            try_depth,
            is_switch: false,
            is_block: false,
            labels,
            iter_slot: None,
        });
        self.sub_stmt(body)?;
        let ctx = self.st().loops.pop().expect("loop");
        for j in ctx.continue_jumps {
            self.patch_to(j, start);
        }
        self.end_scope();
        self.emit(Op::Jump(start));
        self.patch(exit);
        self.end_scope();
        for j in ctx.break_jumps {
            self.patch(j);
        }
        Ok(())
    }

    fn unwind_tries(&mut self, to_depth: usize) -> R<()> {
        let tries: Vec<TryCtx> = self.st_ref().tries[to_depth..].to_vec();
        for (i, t) in tries.iter().enumerate().rev() {
            if t.has_handler {
                self.emit(Op::TryEnd);
            }
            if let Some(f) = &t.finally {
                let saved = self.st().tries.split_off(to_depth + i);
                let r = self.finally_block(f);
                self.st().tries.extend(saved);
                r?;
            }
        }
        Ok(())
    }

    fn return_stmt(&mut self, e: Option<&Expr>) -> R<()> {
        match e {
            Some(e) => {
                self.expr(e)?;
                if self.st_ref().is_async && self.st_ref().is_generator {
                    self.emit(Op::Await);
                }
            }
            None => {
                self.emit(Op::Undefined);
            }
        }
        self.return_top()
    }

    fn emit_yield(&mut self) -> R<()> {
        self.emit(Op::Yield);
        let check = self.emit(Op::YieldReturn(0));
        self.return_top()?;
        self.patch(check);
        Ok(())
    }

    fn close_iterators(&mut self, from: usize) {
        let slots: Vec<u32> = self.st_ref().loops[from..]
            .iter()
            .rev()
            .filter_map(|l| l.iter_slot)
            .collect();
        for slot in slots {
            self.emit(Op::IterClose(slot));
        }
    }

    fn return_top(&mut self) -> R<()> {
        self.close_iterators(0);
        if self.st_ref().tries.iter().any(|t| t.finally.is_some()) {
            let name = self.hidden_name("ret");
            let slot = self.add_local(name, true);
            self.unwind_tries(0)?;
            self.emit(Op::GetLocal(slot));
            self.st().locals.pop();
        } else {
            let n = self.st_ref().tries.iter().filter(|t| t.has_handler).count();
            for _ in 0..n {
                self.emit(Op::TryEnd);
            }
        }
        self.emit(Op::Return);
        Ok(())
    }

    fn try_stmt(
        &mut self,
        block: &[Stmt],
        param: Option<&Pattern>,
        handler: Option<&Vec<Stmt>>,
        finalizer: Option<&Vec<Stmt>>,
    ) -> R<()> {
        let finally = finalizer.map(|f| Rc::new(f.clone()));
        let base = self.locals_len() as u32;
        let tb = self.emit(Op::TryBegin(0, base));
        self.st().tries.push(TryCtx {
            has_handler: true,
            finally: finally.clone(),
        });
        let r = self.block(block);
        self.st().tries.pop();
        r?;
        self.emit(Op::TryEnd);
        let mut normal = vec![self.emit(Op::Jump(0))];
        if let Some(h) = handler {
            self.patch(tb);
            self.begin_scope();
            let exc_name = match param {
                Some(Pattern::Ident(n)) => n.clone(),
                _ => self.hidden_name("exc"),
            };
            let exc_slot = self.add_local(exc_name, false);
            if let Some(p) = param {
                if !matches!(p, Pattern::Ident(_)) {
                    let mut names = Vec::new();
                    p.bound_names(&mut names);
                    for n in names {
                        self.emit(Op::Undefined);
                        self.add_local(n, false);
                    }
                    self.emit(Op::GetLocal(exc_slot));
                    self.bind_pattern(p, BindMode::Init)?;
                }
            }
            let tb2 = if finally.is_some() {
                Some(self.emit(Op::TryBegin(0, base)))
            } else {
                None
            };
            self.st().tries.push(TryCtx {
                has_handler: finally.is_some(),
                finally: finally.clone(),
            });
            let r = (|| {
                self.hoist(h)?;
                for s in h {
                    self.stmt(s)?;
                }
                Ok(())
            })();
            self.st().tries.pop();
            r?;
            if finally.is_some() {
                self.emit(Op::TryEnd);
            }
            self.end_scope();
            normal.push(self.emit(Op::Jump(0)));
            if let Some(tb2) = tb2 {
                self.patch(tb2);
            }
        } else {
            self.patch(tb);
        }
        if let Some(f) = finalizer {
            self.begin_scope();
            let exc = self.hidden_name("exc");
            let slot = self.add_local(exc, true);
            self.finally_block(f)?;
            self.emit(Op::GetLocal(slot));
            self.emit(Op::Throw);
            self.scope_cut();
            for j in normal {
                self.patch(j);
            }
            self.finally_block(f)?;
        } else {
            for j in normal {
                self.patch(j);
            }
        }
        Ok(())
    }

    fn switch(&mut self, disc: &Expr, cases: &[SwitchCase]) -> R<()> {
        self.begin_scope();
        self.expr(disc)?;
        let name = self.hidden_name("disc");
        let d = self.add_local(name, true);
        let all: Vec<&Stmt> = cases.iter().flat_map(|c| c.body.iter()).collect();
        let before = self.locals_len();
        self.hoist_list(&all, false)?;
        let after = self.locals_len();
        for l in &mut self.st().locals[before..after] {
            l.sticky = true;
        }
        let mut tests = Vec::new();
        for (i, c) in cases.iter().enumerate() {
            if let Some(t) = &c.test {
                self.emit(Op::GetLocal(d));
                self.expr(t)?;
                self.emit(Op::StrictEq);
                tests.push((i, self.emit(Op::JumpIfTrue(0))));
            }
        }
        let default_jump = self.emit(Op::Jump(0));
        self.push_loop(true);
        self.st().loops.last_mut().expect("switch").break_locals = d as usize;
        let mut has_default = false;
        for (i, c) in cases.iter().enumerate() {
            for (ci, j) in &tests {
                if *ci == i {
                    self.patch(*j);
                }
            }
            if c.test.is_none() {
                has_default = true;
                self.patch(default_jump);
            }
            for s in &c.body {
                self.stmt(s)?;
            }
        }
        if !has_default {
            self.patch(default_jump);
        }
        let ctx = self.st().loops.pop().expect("switch");
        self.end_scope();
        for j in ctx.break_jumps {
            self.patch(j);
        }
        Ok(())
    }

    fn bind_pattern(&mut self, p: &Pattern, mode: BindMode) -> R<()> {
        match p {
            Pattern::Ident(n) => {
                self.store_var(n, mode == BindMode::Init);
                self.emit(Op::Pop);
            }
            Pattern::Target(e) => match &e.kind {
                ExprKind::Member { obj, name, .. } => {
                    self.expr(obj)?;
                    self.emit(Op::Swap);
                    let k = self.str_const(name);
                    self.emit(Op::SetProp(k));
                    self.emit(Op::Pop);
                }
                ExprKind::Index { obj, index, .. } => {
                    self.expr(obj)?;
                    self.expr(index)?;
                    self.emit(Op::Rot3);
                    self.emit(Op::Rot3);
                    self.emit(Op::SetIndex);
                    self.emit(Op::Pop);
                }
                _ => return Err(self.err("Invalid destructuring assignment target")),
            },
            Pattern::Object { props, rest } => {
                self.emit(Op::RequireObject);
                for prop in props {
                    self.emit(Op::Dup);
                    match &prop.key {
                        PropKey::Named(k) => {
                            let k = self.str_const(k);
                            self.emit(Op::GetProp(k));
                        }
                        PropKey::Computed(e) => {
                            self.expr(e)?;
                            self.emit(Op::GetIndex);
                        }
                    }
                    if let Some(d) = &prop.default {
                        let j = self.emit(Op::JumpIfNotUndef(0));
                        let name = match &prop.value {
                            Pattern::Ident(n) => Some(n.clone()),
                            _ => None,
                        };
                        self.expr_named(d, name.as_ref())?;
                        self.patch(j);
                    }
                    self.bind_pattern(&prop.value, mode)?;
                }
                if let Some(r) = rest {
                    self.emit(Op::Dup);
                    let mut n = 0;
                    for prop in props {
                        if let PropKey::Named(k) = &prop.key {
                            let k = self.str_const(k);
                            self.emit(Op::Const(k));
                            n += 1;
                        }
                    }
                    self.emit(Op::ObjRest(n));
                    self.bind_pattern(r, mode)?;
                }
                self.emit(Op::Pop);
            }
            Pattern::Array { elems, rest } => {
                self.emit(Op::GetIter);
                for el in elems {
                    self.emit(Op::Dup);
                    self.emit(Op::IterNextOrUndef);
                    match el {
                        None => {
                            self.emit(Op::Pop);
                        }
                        Some(PatElem { pattern, default }) => {
                            if let Some(d) = default {
                                let j = self.emit(Op::JumpIfNotUndef(0));
                                let name = match pattern {
                                    Pattern::Ident(n) => Some(n.clone()),
                                    _ => None,
                                };
                                self.expr_named(d, name.as_ref())?;
                                self.patch(j);
                            }
                            self.bind_pattern(pattern, mode)?;
                        }
                    }
                }
                if let Some(r) = rest {
                    self.emit(Op::Dup);
                    self.emit(Op::IterRest);
                    self.bind_pattern(r, mode)?;
                    self.emit(Op::Pop);
                } else {
                    self.emit(Op::IterCloseTop);
                }
            }
        }
        Ok(())
    }

    fn function(
        &mut self,
        f: &FuncDef,
        name_hint: Option<Rc<str>>,
        fields: Option<Rc<Vec<ClassMember>>>,
    ) -> R<u32> {
        let kind = match f.kind {
            FuncKind::Normal => ProtoKind::Normal,
            FuncKind::Arrow => ProtoKind::Arrow,
            FuncKind::Method | FuncKind::StaticMethod => ProtoKind::Method,
            FuncKind::ClassConstructor => ProtoKind::ClassConstructor,
            FuncKind::DerivedConstructor => ProtoKind::DerivedConstructor,
        };
        let name = f.name.clone().or(name_hint).unwrap_or_else(|| "".into());
        let self_name = self.self_bind.take();
        let saved_line = self.line;
        self.line = f.line;
        let mut st = FnState::new(name, kind, f.line, f.kind == FuncKind::StaticMethod);
        st.span = f.span;
        st.fields = fields;
        st.is_async = f.is_async;
        st.is_generator = f.is_generator;
        self.states.push(st);
        let saved_labels = std::mem::take(&mut self.pending_labels);
        let r = self.function_body(f, kind, self_name);
        self.pending_labels = saved_labels;
        let st = self.states.pop().expect("function state");
        self.line = saved_line;
        r?;
        let proto = Rc::new(self.finish(st));
        let parent = self.st();
        parent.functions.push(proto);
        Ok((parent.functions.len() - 1) as u32)
    }

    fn param_counts(&mut self, f: &FuncDef) {
        let arity = f
            .params
            .iter()
            .position(|p| p.default.is_some())
            .unwrap_or(f.params.len());
        let st = self.st();
        st.num_params = f.params.len() as u32;
        st.arity = arity as u32;
        st.has_rest = f.rest.is_some();
    }

    fn arguments_local(&mut self, f: &FuncDef) {
        if f.uses_arguments && f.kind != FuncKind::Arrow {
            self.add_local("arguments".into(), false);
            self.st().uses_arguments = true;
        }
    }

    fn simple_params(&mut self, f: &FuncDef) -> R<()> {
        for p in &f.params {
            if let Pattern::Ident(n) = &p.pattern {
                if self.resolve_local(self.states.len() - 1, n).is_some() && &**n != "this" {
                    return Err(self.err(format!("Duplicate parameter name '{n}'")));
                }
                self.add_local(n.clone(), false);
            }
        }
        self.param_counts(f);
        if let Some(Pattern::Ident(n)) = &f.rest {
            if self.resolve_local(self.states.len() - 1, n).is_some() {
                return Err(self.err(format!("Duplicate parameter name '{n}'")));
            }
            self.add_local(n.clone(), false);
        }
        self.arguments_local(f);
        Ok(())
    }

    fn complex_params(&mut self, f: &FuncDef) -> R<()> {
        let mut raw = Vec::with_capacity(f.params.len());
        for _ in &f.params {
            let h = self.hidden_name("param");
            raw.push(self.add_local(h, false));
        }
        self.param_counts(f);
        let rest_raw = match &f.rest {
            Some(_) => {
                let h = self.hidden_name("rest");
                Some(self.add_local(h, false))
            }
            None => None,
        };
        self.arguments_local(f);
        let mut names = Vec::new();
        for p in &f.params {
            p.pattern.bound_names(&mut names);
        }
        if let Some(r) = &f.rest {
            r.bound_names(&mut names);
        }
        for (i, n) in names.iter().enumerate() {
            if names[..i].contains(n) {
                return Err(self.err(format!("Duplicate parameter name '{n}'")));
            }
        }
        for n in &names {
            self.emit(Op::Uninit);
            self.add_lexical(n.clone(), false);
        }
        for (i, p) in f.params.iter().enumerate() {
            self.emit(Op::GetLocal(raw[i]));
            if let Some(d) = &p.default {
                let j = self.emit(Op::JumpIfNotUndef(0));
                let name = match &p.pattern {
                    Pattern::Ident(n) => Some(n.clone()),
                    _ => None,
                };
                self.expr_named(d, name.as_ref())?;
                self.patch(j);
            }
            self.bind_pattern(&p.pattern, BindMode::Init)?;
            self.mark_init_pattern(&p.pattern);
        }
        if let (Some(r), Some(slot)) = (&f.rest, rest_raw) {
            self.emit(Op::GetLocal(slot));
            self.bind_pattern(r, BindMode::Init)?;
            self.mark_init_pattern(r);
        }
        Ok(())
    }

    fn function_body(&mut self, f: &FuncDef, kind: ProtoKind, self_name: Option<Rc<str>>) -> R<()> {
        self.begin_scope();
        if kind == ProtoKind::Arrow {
            self.add_local("".into(), true);
        } else {
            self.add_local("this".into(), true);
        }
        let simple = f
            .params
            .iter()
            .all(|p| p.default.is_none() && matches!(p.pattern, Pattern::Ident(_)))
            && f.rest
                .as_ref()
                .is_none_or(|r| matches!(r, Pattern::Ident(_)));
        if simple {
            self.simple_params(f)?;
        } else {
            self.complex_params(f)?;
        }
        if f.is_generator {
            self.emit(Op::Undefined);
            self.emit_yield()?;
            self.emit(Op::Pop);
        }
        if let Some(n) = self_name {
            if self.resolve_local(self.states.len() - 1, &n).is_none() {
                self.emit(Op::GetCallee);
                self.add_local(n, false);
            }
        }
        if kind == ProtoKind::ClassConstructor {
            self.emit_field_inits()?;
        }
        match &f.body {
            FuncBody::Block(stmts) => {
                self.declare_vars(stmts);
                self.var_scope_next = true;
                self.hoist(stmts)?;
                for s in stmts {
                    self.stmt(s)?;
                }
                self.emit(Op::Undefined);
                self.emit(Op::Return);
            }
            FuncBody::Expr(e) => {
                self.expr(e)?;
                self.emit(Op::Return);
            }
        }
        Ok(())
    }

    fn method_state(&self) -> Option<&FnState> {
        self.states
            .iter()
            .rev()
            .find(|s| s.kind != ProtoKind::Arrow)
    }

    fn emit_field_inits(&mut self) -> R<()> {
        let Some(fields) = self.method_state().and_then(|s| s.fields.clone()) else {
            return Ok(());
        };
        self.load_var(&"this".into());
        self.emit(Op::GetCallee);
        self.emit(Op::CopyPrivates);
        self.emit(Op::Pop);
        for m in fields.iter() {
            if let ClassMember::Field {
                key,
                value,
                is_static: false,
            } = m
            {
                self.load_var(&"this".into());
                match key {
                    PropKey::Named(k) => {
                        match value {
                            Some(v) => self.expr_named(v, Some(k))?,
                            None => {
                                self.emit(Op::Undefined);
                            }
                        }
                        let private = k.starts_with("\u{0}#");
                        let k = self.str_const(k);
                        if private {
                            self.emit_raw(Op::InitPrivate(k));
                        } else {
                            self.emit_raw(Op::DefineField(k));
                        }
                    }
                    PropKey::Computed(e) => {
                        self.expr(e)?;
                        match value {
                            Some(v) => self.expr(v)?,
                            None => {
                                self.emit(Op::Undefined);
                            }
                        }
                        self.emit(Op::SetIndex);
                    }
                }
                self.emit(Op::Pop);
            }
        }
        Ok(())
    }

    fn class(&mut self, c: &ClassDef, name_hint: Option<&Rc<str>>) -> R<()> {
        let saved_line = self.line;
        self.line = c.line;
        let name: Rc<str> = c
            .name
            .clone()
            .or_else(|| name_hint.cloned())
            .unwrap_or_else(|| "".into());
        let has_super = c.superclass.is_some();
        if let Some(sup) = &c.superclass {
            self.expr(sup)?;
        }
        let ctor = match &c.constructor {
            Some(f) => f.clone(),
            None => {
                let body = if has_super {
                    vec![Stmt {
                        kind: StmtKind::Expr(Expr {
                            kind: ExprKind::SuperCall(vec![Arg::Spread(Expr {
                                kind: ExprKind::Ident("args".into()),
                                line: c.line,
                            })]),
                            line: c.line,
                        }),
                        line: c.line,
                    }]
                } else {
                    Vec::new()
                };
                Rc::new(FuncDef {
                    name: Some(name.clone()),
                    params: Vec::new(),
                    rest: if has_super {
                        Some(Pattern::Ident("args".into()))
                    } else {
                        None
                    },
                    body: FuncBody::Block(body),
                    kind: if has_super {
                        FuncKind::DerivedConstructor
                    } else {
                        FuncKind::ClassConstructor
                    },
                    is_async: false,
                    is_generator: false,
                    uses_arguments: false,
                    line: c.line,
                    span: c.span,
                })
            }
        };
        let fields: Vec<ClassMember> = c
            .members
            .iter()
            .filter(|m| match m {
                ClassMember::Field {
                    is_static: false, ..
                } => true,
                ClassMember::Method {
                    key: PropKey::Named(k),
                    is_static: false,
                    ..
                }
                | ClassMember::Accessor {
                    key: PropKey::Named(k),
                    is_static: false,
                    ..
                } => k.starts_with("\u{0}#"),
                _ => false,
            })
            .cloned()
            .collect();
        let fields = if fields.is_empty() {
            None
        } else {
            Some(Rc::new(fields))
        };
        let mut ctor_def = (*ctor).clone();
        ctor_def.name = Some(name.clone());
        ctor_def.span = c.span;
        let idx = self.function(&ctor_def, Some(name.clone()), fields)?;
        self.emit(Op::Closure(idx));
        let k = self.str_const(&name);
        self.emit(Op::MakeClass(k, has_super));
        if let Some(n) = &c.name {
            match self.resolve(n) {
                Var::Local(s, _) | Var::LocalTdz(s, _) => {
                    self.emit(Op::SetLocal(s));
                }
                Var::Upval(i, _) | Var::UpvalTdz(i, _) => {
                    self.emit(Op::SetUpval(i));
                }
                Var::Global => {}
            }
            self.mark_init(n);
        }
        for m in &c.members {
            if let ClassMember::Method {
                key,
                func,
                is_static,
            } = m
            {
                match key {
                    PropKey::Named(n) => {
                        let idx = self.function(func, Some(n.clone()), None)?;
                        self.emit(Op::Closure(idx));
                        let k = self.str_const(n);
                        self.emit(Op::DefineMethod(k, *is_static));
                    }
                    PropKey::Computed(e) => {
                        self.expr(e)?;
                        let idx = self.function(func, None, None)?;
                        self.emit(Op::Closure(idx));
                        self.emit(Op::DefineMethodComputed(*is_static));
                    }
                }
            }
            if let ClassMember::Accessor {
                key,
                func,
                is_static,
                setter,
            } = m
            {
                match key {
                    PropKey::Named(n) => {
                        let mut g = (**func).clone();
                        g.name =
                            Some(format!("{} {n}", if *setter { "set" } else { "get" }).into());
                        let idx = self.function(&g, None, None)?;
                        self.emit(Op::Closure(idx));
                        let k = self.str_const(n);
                        self.emit(Op::DefineAccessor(k, *setter, *is_static));
                    }
                    PropKey::Computed(e) => {
                        self.expr(e)?;
                        let idx = self.function(func, None, None)?;
                        self.emit(Op::Closure(idx));
                        self.emit(Op::DefineAccessorComputed(*setter, *is_static));
                    }
                }
            }
        }
        for m in &c.members {
            match m {
                ClassMember::Field {
                    key,
                    value,
                    is_static: true,
                } => match key {
                    PropKey::Named(n) => {
                        self.emit(Op::Dup);
                        match value {
                            Some(v) => {
                                self.emit(Op::Dup);
                                self.static_init_value(v, Some(n), c.line)?;
                            }
                            None => {
                                self.emit(Op::Undefined);
                            }
                        }
                        let private = n.starts_with("\u{0}#");
                        let k = self.str_const(n);
                        if private {
                            self.emit_raw(Op::InitPrivate(k));
                        } else {
                            self.emit_raw(Op::DefineField(k));
                        }
                        self.emit(Op::Pop);
                    }
                    PropKey::Computed(e) => {
                        self.emit(Op::Dup);
                        self.expr(e)?;
                        match value {
                            Some(v) => {
                                self.emit(Op::Dup2);
                                self.emit(Op::Pop);
                                self.static_init_value(v, None, c.line)?;
                            }
                            None => {
                                self.emit(Op::Undefined);
                            }
                        }
                        self.emit(Op::SetIndex);
                        self.emit(Op::Pop);
                    }
                },
                ClassMember::StaticBlock(f) => {
                    let idx = self.function(f, Some("static".into()), None)?;
                    self.emit(Op::Dup);
                    self.emit(Op::Closure(idx));
                    self.emit(Op::SetHome);
                    self.emit(Op::Swap);
                    self.emit(Op::Call(0, NO_NAME));
                    self.emit(Op::Pop);
                }
                _ => {}
            }
        }
        self.line = saved_line;
        Ok(())
    }

    fn static_init_value(&mut self, v: &Expr, name: Option<&Rc<str>>, line: u32) -> R<()> {
        let f = FuncDef {
            name: Some("static".into()),
            params: Vec::new(),
            rest: None,
            body: FuncBody::Block(vec![Stmt {
                kind: StmtKind::Return(Some(v.clone())),
                line,
            }]),
            kind: FuncKind::StaticMethod,
            is_async: false,
            is_generator: false,
            uses_arguments: false,
            line,
            span: (0, 0),
        };
        let idx = self.function(&f, name.cloned(), None)?;
        self.emit(Op::Closure(idx));
        self.emit(Op::SetHome);
        self.emit(Op::Swap);
        self.emit(Op::Call(0, NO_NAME));
        Ok(())
    }

    fn has_spread(args: &[Arg]) -> bool {
        args.iter().any(|a| matches!(a, Arg::Spread(_)))
    }

    fn args_array(&mut self, args: &[Arg]) -> R<()> {
        self.emit(Op::NewArray(0));
        for a in args {
            match a {
                Arg::Expr(e) => {
                    self.expr(e)?;
                    self.emit(Op::ArrayPush);
                }
                Arg::Spread(e) => {
                    self.expr(e)?;
                    self.emit(Op::ArraySpread);
                }
            }
        }
        Ok(())
    }

    fn plain_args(&mut self, args: &[Arg]) -> R<u32> {
        for a in args {
            if let Arg::Expr(e) = a {
                self.expr(e)?;
            }
        }
        Ok(args.len() as u32)
    }

    fn name_const(&mut self, e: &Expr) -> u32 {
        match expr_text(e) {
            Some(t) => {
                let t: Rc<str> = t.into();
                self.str_const(&t)
            }
            None => NO_NAME,
        }
    }

    fn super_info(&self, call: bool) -> R<bool> {
        let Some(st) = self.method_state() else {
            return Err(self.err("'super' keyword unexpected here"));
        };
        let ok = if call {
            st.kind == ProtoKind::DerivedConstructor
        } else {
            matches!(
                st.kind,
                ProtoKind::Method | ProtoKind::ClassConstructor | ProtoKind::DerivedConstructor
            )
        };
        if !ok {
            return Err(self.err("'super' keyword unexpected here"));
        }
        Ok(st.is_static)
    }

    fn call(&mut self, callee: &Expr, args: &[Arg], optional: bool) -> R<()> {
        let name = self.name_const(callee);
        match &callee.kind {
            ExprKind::Member {
                obj,
                name: m,
                optional: false,
            } if self.local_slot(obj).is_some() => {
                let slot = self.local_slot(obj).unwrap_or(0);
                let k = self.str_const(m);
                self.emit(Op::GetLocalMethod(slot, k));
            }
            ExprKind::Member {
                obj,
                name: m,
                optional: mopt,
            } => {
                self.expr(obj)?;
                if *mopt {
                    let j = self.emit(Op::JumpIfNullish(0));
                    self.chain.push(j);
                }
                let k = self.str_const(m);
                self.emit(Op::GetMethod(k));
            }
            ExprKind::Index {
                obj,
                index,
                optional: iopt,
            } => {
                self.expr(obj)?;
                if *iopt {
                    let j = self.emit(Op::JumpIfNullish(0));
                    self.chain.push(j);
                }
                self.expr(index)?;
                self.emit(Op::GetIndexMethod);
            }
            ExprKind::SuperMember(m) => {
                let is_static = self.super_info(false)?;
                self.load_var(&"this".into());
                let k = self.str_const(m);
                self.emit(Op::SuperGetMethod(k, is_static));
            }
            ExprKind::SuperIndex(key) => {
                let is_static = self.super_info(false)?;
                self.load_var(&"this".into());
                self.expr(key)?;
                self.emit(Op::SuperGetIndexMethod(is_static));
            }
            _ => {
                self.expr(callee)?;
                self.emit(Op::Undefined);
            }
        }
        if optional {
            let j = self.emit(Op::JumpIfNullishCall(0));
            self.chain.push(j);
        }
        if Self::has_spread(args) {
            self.args_array(args)?;
            self.emit(Op::CallSpread(name));
        } else {
            let n = self.plain_args(args)?;
            self.emit(Op::Call(n, name));
        }
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> R<()> {
        self.expr_named(e, None)
    }

    fn jump_if_false(&mut self, test: &Expr) -> R<usize> {
        let saved_line = self.line;
        self.line = test.line;
        let r = self.jump_if_false_inner(test);
        self.line = saved_line;
        r
    }

    fn jump_if_false_inner(&mut self, test: &Expr) -> R<usize> {
        if let ExprKind::Binary(op, a, b) = &test.kind {
            let cmp = match op {
                BinOp::Lt => Some(Cmp::Lt),
                BinOp::Le => Some(Cmp::Le),
                BinOp::Gt => Some(Cmp::Gt),
                BinOp::Ge => Some(Cmp::Ge),
                BinOp::StrictEq => Some(Cmp::StrictEq),
                BinOp::StrictNe => Some(Cmp::StrictNe),
                BinOp::Eq => Some(Cmp::Eq),
                BinOp::Ne => Some(Cmp::Ne),
                _ => None,
            };
            if let Some(cmp) = cmp {
                self.expr(a)?;
                self.expr(b)?;
                return Ok(self.emit(Op::CmpJump(cmp, 0)));
            }
        }
        if let ExprKind::Unary(UnaryOp::Not, x) = &test.kind {
            self.expr(x)?;
            return Ok(self.emit(Op::JumpIfTrue(0)));
        }
        self.expr(test)?;
        Ok(self.emit(Op::JumpIfFalse(0)))
    }

    fn local_slot(&mut self, e: &Expr) -> Option<u32> {
        match &e.kind {
            ExprKind::Ident(n) => match self.resolve(n) {
                Var::Local(s, _) => Some(s),
                _ => None,
            },
            ExprKind::This => match self.resolve("this") {
                Var::Local(s, _) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    fn store_discard(&mut self, name: &Rc<str>) {
        if self.is_export_binding(name) {
            self.store_var(name, false);
            self.emit(Op::Pop);
            return;
        }
        match self.resolve(name) {
            Var::Local(s, false) => {
                self.emit(Op::StoreLocal(s));
            }
            Var::Upval(i, false) => {
                self.emit(Op::StoreUpval(i));
            }
            _ => {
                self.store_var(name, false);
                self.emit(Op::Pop);
            }
        }
    }

    fn expr_discard(&mut self, e: &Expr) -> R<()> {
        let saved_line = self.line;
        self.line = e.line;
        let r = self.expr_discard_inner(e);
        self.line = saved_line;
        r
    }

    fn expr_discard_inner(&mut self, e: &Expr) -> R<()> {
        match &e.kind {
            ExprKind::Update { inc, target, .. } => {
                if let ExprKind::Ident(n) = &target.kind {
                    if let (Var::Local(s, false), false) =
                        (self.resolve(n), self.is_export_binding(n))
                    {
                        self.emit(if *inc {
                            Op::IncLocal(s)
                        } else {
                            Op::DecLocal(s)
                        });
                        return Ok(());
                    }
                }
                let as_prefix = Expr {
                    kind: ExprKind::Update {
                        inc: *inc,
                        prefix: true,
                        target: target.clone(),
                    },
                    line: e.line,
                };
                self.expr(&as_prefix)?;
                self.emit(Op::Pop);
            }
            ExprKind::Assign {
                op,
                target: AssignTarget::Simple(t),
                value,
            } => match (&t.kind, op) {
                (ExprKind::Ident(n), None) => {
                    self.expr_named(value, Some(n))?;
                    self.store_discard(n);
                }
                (ExprKind::Ident(n), Some(AssignOp::Bin(BinOp::Add)))
                    if matches!(self.resolve(n), Var::Local(_, false))
                        && !self.is_export_binding(n) =>
                {
                    let Var::Local(slot, _) = self.resolve(n) else {
                        unreachable!()
                    };
                    self.expr(value)?;
                    self.emit(Op::AppendLocal(slot));
                }
                (ExprKind::Ident(n), Some(AssignOp::Bin(b))) => {
                    self.load_var(n);
                    self.expr(value)?;
                    self.emit(bin_op(*b));
                    self.store_discard(n);
                }
                (ExprKind::Member { obj, name, .. }, None) => {
                    let k = self.str_const(name);
                    if let Some(slot) = self.local_slot(obj) {
                        self.expr(value)?;
                        self.emit(Op::StoreLocalProp(slot, k));
                    } else {
                        self.expr(obj)?;
                        self.expr(value)?;
                        self.emit(Op::StoreProp(k));
                    }
                }
                _ => {
                    self.expr(e)?;
                    self.emit(Op::Pop);
                }
            },
            _ => {
                self.expr(e)?;
                self.emit(Op::Pop);
            }
        }
        Ok(())
    }

    fn expr_named(&mut self, e: &Expr, name: Option<&Rc<str>>) -> R<()> {
        let saved_line = self.line;
        self.line = e.line;
        let r = self.expr_inner(e, name);
        self.line = saved_line;
        r
    }

    fn expr_inner(&mut self, e: &Expr, name: Option<&Rc<str>>) -> R<()> {
        match &e.kind {
            ExprKind::Number(n) => {
                let k = self.constant(Value::Number(*n));
                self.emit(Op::Const(k));
            }
            ExprKind::BigInt(text) => {
                let Some(b) = crate::bigint::parse_literal(text) else {
                    return Err(self.err("Invalid BigInt literal"));
                };
                let k = self.constant(Value::BigInt(Rc::new(b)));
                self.emit(Op::Const(k));
            }
            ExprKind::Str(s) => {
                let k = self.str_const(s);
                self.emit(Op::Const(k));
            }
            ExprKind::Regex(pattern, flags) => {
                if let Err(msg) = crate::regexp::compile(pattern, flags) {
                    return Err(self.err(msg));
                }
                let p = self.str_const(pattern);
                let f = self.str_const(flags);
                self.emit(Op::NewRegex(p, f));
            }
            ExprKind::Template(quasis, exprs) => {
                let mut n = 0;
                for (i, q) in quasis.iter().enumerate() {
                    if !q.is_empty() {
                        let k = self.str_const(q);
                        self.emit(Op::Const(k));
                        n += 1;
                    }
                    if let Some(x) = exprs.get(i) {
                        self.expr(x)?;
                        n += 1;
                    }
                }
                if n == 0 {
                    let k = self.str_const(&"".into());
                    self.emit(Op::Const(k));
                } else {
                    self.emit(Op::Template(n));
                }
            }
            ExprKind::TemplateStrings(cooked, raw) => {
                let site = self.st_ref().template_sites;
                self.st().template_sites += 1;
                let jump = self.emit(Op::TemplateCache(site, 0));
                for q in cooked {
                    match q {
                        Some(q) => {
                            let k = self.str_const(q);
                            self.emit(Op::Const(k));
                        }
                        None => {
                            self.emit(Op::Undefined);
                        }
                    }
                }
                self.emit(Op::NewArray(cooked.len() as u32));
                for q in raw {
                    let k = self.str_const(q);
                    self.emit(Op::Const(k));
                }
                self.emit(Op::NewArray(raw.len() as u32));
                let k = self.str_const(&"raw".into());
                self.emit(Op::InitHidden(k));
                self.emit(Op::TemplateStore(site));
                self.patch(jump);
            }
            ExprKind::Bool(true) => {
                self.emit(Op::True);
            }
            ExprKind::Bool(false) => {
                self.emit(Op::False);
            }
            ExprKind::Null => {
                self.emit(Op::Null);
            }
            ExprKind::This => self.load_var(&"this".into()),
            ExprKind::Ident(n) => self.load_var(n),
            ExprKind::Array(elems) => {
                let simple = elems.iter().all(|e| matches!(e, ArrayElem::Expr(_)));
                if simple {
                    for el in elems {
                        if let ArrayElem::Expr(x) = el {
                            self.expr(x)?;
                        }
                    }
                    self.emit(Op::NewArray(elems.len() as u32));
                } else {
                    self.emit(Op::NewArray(0));
                    for el in elems {
                        match el {
                            ArrayElem::Expr(x) => {
                                self.expr(x)?;
                                self.emit(Op::ArrayPush);
                            }
                            ArrayElem::Spread(x) => {
                                self.expr(x)?;
                                self.emit(Op::ArraySpread);
                            }
                            ArrayElem::Hole => {
                                self.emit(Op::ArrayHole);
                            }
                        }
                    }
                }
            }
            ExprKind::Object(props) => {
                self.emit(Op::NewObject);
                for p in props {
                    match p {
                        PropDef::KeyValue(PropKey::Named(k), v) if &**k == "__proto__" => {
                            self.expr(v)?;
                            self.emit(Op::InitProto);
                        }
                        PropDef::KeyValue(PropKey::Named(k), v) => {
                            self.expr_named(v, Some(k))?;
                            let k = self.str_const(k);
                            self.emit(Op::InitProp(k));
                        }
                        PropDef::KeyValue(PropKey::Computed(ke), v) => {
                            self.expr(ke)?;
                            self.expr(v)?;
                            self.emit(Op::InitIndex);
                        }
                        PropDef::Shorthand(n) => {
                            self.load_var(n);
                            let k = self.str_const(n);
                            self.emit(Op::InitProp(k));
                        }
                        PropDef::ShorthandDefault(..) => {
                            return Err(self.err("Invalid shorthand property initializer"));
                        }
                        PropDef::Spread(x) => {
                            self.expr(x)?;
                            self.emit(Op::ObjSpread);
                        }
                        PropDef::Method(PropKey::Named(k), f) => {
                            let idx = self.function(f, Some(k.clone()), None)?;
                            self.emit(Op::Closure(idx));
                            let k = self.str_const(k);
                            self.emit(Op::InitMethod(k));
                        }
                        PropDef::Method(PropKey::Computed(ke), f) => {
                            self.expr(ke)?;
                            let idx = self.function(f, None, None)?;
                            self.emit(Op::Closure(idx));
                            self.emit(Op::InitMethodComputed);
                        }
                        PropDef::Getter(key, f) | PropDef::Setter(key, f) => {
                            let setter = matches!(p, PropDef::Setter(..));
                            match key {
                                PropKey::Named(k) => {
                                    let mut g = (**f).clone();
                                    g.name = Some(
                                        format!("{} {k}", if setter { "set" } else { "get" })
                                            .into(),
                                    );
                                    let idx = self.function(&g, None, None)?;
                                    self.emit(Op::Closure(idx));
                                    let k = self.str_const(k);
                                    self.emit(Op::InitAccessor(k, setter));
                                }
                                PropKey::Computed(ke) => {
                                    self.expr(ke)?;
                                    let idx = self.function(f, None, None)?;
                                    self.emit(Op::Closure(idx));
                                    self.emit(Op::InitAccessorComputed(setter));
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::Function(f) => {
                if f.kind == FuncKind::Normal {
                    self.self_bind = f.name.clone();
                }
                let idx = self.function(f, name.cloned(), None)?;
                self.emit(Op::Closure(idx));
            }
            ExprKind::Class(c) => match &c.name {
                Some(n) => {
                    let wrapper = FuncDef {
                        name: None,
                        params: Vec::new(),
                        rest: None,
                        body: FuncBody::Block(vec![
                            Stmt {
                                kind: StmtKind::Class(c.clone()),
                                line: c.line,
                            },
                            Stmt {
                                kind: StmtKind::Return(Some(Expr {
                                    kind: ExprKind::Ident(n.clone()),
                                    line: c.line,
                                })),
                                line: c.line,
                            },
                        ]),
                        kind: FuncKind::Arrow,
                        is_async: false,
                        is_generator: false,
                        uses_arguments: false,
                        line: c.line,
                        span: (0, 0),
                    };
                    let idx = self.function(&wrapper, None, None)?;
                    self.emit(Op::Closure(idx));
                    self.emit(Op::Undefined);
                    self.emit(Op::Call(0, NO_NAME));
                }
                None => self.class(c, name)?,
            },
            ExprKind::NewTarget => {
                self.emit(Op::NewTarget);
            }
            ExprKind::ImportMeta => {
                self.emit(Op::ImportMeta);
            }
            ExprKind::DynImport(spec) => {
                self.expr(spec)?;
                self.emit(Op::DynImport);
            }
            ExprKind::Unary(op, x) => match op {
                UnaryOp::Typeof => {
                    if let ExprKind::Ident(n) = &x.kind {
                        if matches!(self.resolve(n), Var::Global) && self.alias(n).is_none() {
                            let k = self.str_const(n);
                            self.emit(Op::TypeofGlobal(k));
                            return Ok(());
                        }
                    }
                    self.expr(x)?;
                    self.emit(Op::Typeof);
                }
                UnaryOp::Delete => match &x.kind {
                    ExprKind::Member { obj, name, .. } => {
                        self.expr(obj)?;
                        let k = self.str_const(name);
                        self.emit(Op::DeleteProp(k));
                    }
                    ExprKind::Index { obj, index, .. } => {
                        self.expr(obj)?;
                        self.expr(index)?;
                        self.emit(Op::DeleteIndex);
                    }
                    _ => {
                        self.expr(x)?;
                        self.emit(Op::Pop);
                        self.emit(Op::True);
                    }
                },
                UnaryOp::Void => {
                    self.expr(x)?;
                    self.emit(Op::Pop);
                    self.emit(Op::Undefined);
                }
                UnaryOp::Not => {
                    self.expr(x)?;
                    self.emit(Op::Not);
                }
                UnaryOp::Neg => {
                    self.expr(x)?;
                    self.emit(Op::Neg);
                }
                UnaryOp::Plus => {
                    self.expr(x)?;
                    self.emit(Op::ToNumber);
                }
                UnaryOp::BitNot => {
                    self.expr(x)?;
                    self.emit(Op::BitNot);
                }
            },
            ExprKind::Update {
                inc,
                prefix,
                target,
            } => {
                let step = if *inc { Op::Inc } else { Op::Dec };
                match &target.kind {
                    ExprKind::Ident(n) => {
                        self.load_var(n);
                        if *prefix {
                            self.emit(step);
                            self.store_var(n, false);
                        } else {
                            self.emit(Op::ToNumeric);
                            self.emit(Op::Dup);
                            self.emit(step);
                            self.store_var(n, false);
                            self.emit(Op::Pop);
                        }
                    }
                    ExprKind::Member { obj, name, .. } => {
                        self.expr(obj)?;
                        self.emit(Op::Dup);
                        let k = self.str_const(name);
                        self.emit(Op::GetProp(k));
                        if *prefix {
                            self.emit(step);
                            self.emit(Op::SetProp(k));
                        } else {
                            self.emit(Op::ToNumeric);
                            self.emit(Op::Dup);
                            self.emit(Op::Rot3);
                            self.emit(step);
                            self.emit(Op::SetProp(k));
                            self.emit(Op::Pop);
                        }
                    }
                    ExprKind::Index { obj, index, .. } => {
                        self.expr(obj)?;
                        self.expr(index)?;
                        self.emit(Op::Dup2);
                        self.emit(Op::GetIndex);
                        if *prefix {
                            self.emit(step);
                            self.emit(Op::SetIndex);
                        } else {
                            self.emit(Op::ToNumeric);
                            self.emit(Op::Dup);
                            self.emit(Op::Rot4);
                            self.emit(step);
                            self.emit(Op::SetIndex);
                            self.emit(Op::Pop);
                        }
                    }
                    _ => {
                        return Err(
                            self.err("Invalid left-hand side expression in update operation")
                        );
                    }
                }
            }
            ExprKind::Binary(op, a, b) => {
                self.expr(a)?;
                self.expr(b)?;
                self.emit(bin_op(*op));
            }
            ExprKind::Logical(op, a, b) => {
                self.expr(a)?;
                let j = self.emit(logical_jump(*op));
                self.expr(b)?;
                self.patch(j);
            }
            ExprKind::Assign { op, target, value } => self.assign(*op, target, value)?,
            ExprKind::Cond(t, c, a) => {
                let j = self.jump_if_false(t)?;
                self.expr_named(c, name)?;
                let k = self.emit(Op::Jump(0));
                self.patch(j);
                self.expr_named(a, name)?;
                self.patch(k);
            }
            ExprKind::Member {
                obj,
                name: m,
                optional: false,
            } if self.local_slot(obj).is_some() => {
                let slot = self.local_slot(obj).unwrap_or(0);
                let k = self.str_const(m);
                self.emit(Op::GetLocalProp(slot, k));
            }
            ExprKind::Member {
                obj,
                name: m,
                optional,
            } => {
                self.expr(obj)?;
                if *optional {
                    let j = self.emit(Op::JumpIfNullish(0));
                    self.chain.push(j);
                }
                let k = self.str_const(m);
                self.emit(Op::GetProp(k));
            }
            ExprKind::Index {
                obj,
                index,
                optional,
            } => {
                self.expr(obj)?;
                if *optional {
                    let j = self.emit(Op::JumpIfNullish(0));
                    self.chain.push(j);
                }
                self.expr(index)?;
                self.emit(Op::GetIndex);
            }
            ExprKind::Call {
                callee,
                args,
                optional,
            } => self.call(callee, args, *optional)?,
            ExprKind::DirectEval(args, ctx) => {
                self.load_var(&"eval".into());
                self.emit(Op::Undefined);
                let idx = self.eval_scope(ctx.clone());
                let argc = self.plain_args(args)?;
                self.emit(Op::DirectEval(argc, idx));
            }
            ExprKind::New { callee, args } => {
                let name = self.name_const(callee);
                self.expr(callee)?;
                if Self::has_spread(args) {
                    self.args_array(args)?;
                    self.emit(Op::NewSpread(name));
                } else {
                    let n = self.plain_args(args)?;
                    self.emit(Op::New(n, name));
                }
            }
            ExprKind::SuperCall(args) => {
                self.super_info(true)?;
                self.load_var(&"this".into());
                if Self::has_spread(args) {
                    self.args_array(args)?;
                    self.emit(Op::SuperCall(u32::MAX));
                } else {
                    let n = self.plain_args(args)?;
                    self.emit(Op::SuperCall(n));
                }
                self.store_var(&"this".into(), true);
                self.emit_field_inits()?;
            }
            ExprKind::SuperMember(m) => {
                let is_static = self.super_info(false)?;
                self.load_var(&"this".into());
                let k = self.str_const(m);
                self.emit(Op::SuperGet(k, is_static));
            }
            ExprKind::SuperIndex(key) => {
                let is_static = self.super_info(false)?;
                self.load_var(&"this".into());
                self.expr(key)?;
                self.emit(Op::SuperGetIndex(is_static));
            }
            ExprKind::OptChain(inner) => {
                let saved = std::mem::take(&mut self.chain);
                let r = self.expr(inner);
                let jumps = std::mem::replace(&mut self.chain, saved);
                r?;
                for j in jumps {
                    self.patch(j);
                }
            }
            ExprKind::Seq(items) => {
                for (i, x) in items.iter().enumerate() {
                    self.expr(x)?;
                    if i + 1 < items.len() {
                        self.emit(Op::Pop);
                    }
                }
            }
            ExprKind::Await(x) => {
                self.require_await()?;
                self.expr(x)?;
                self.emit(Op::Await);
            }
            ExprKind::Yield { arg, delegate } => {
                if !self.st_ref().is_generator {
                    return Err(self.err("yield is only valid in generator functions"));
                }
                match arg {
                    Some(a) => self.expr(a)?,
                    None => {
                        self.emit(Op::Undefined);
                    }
                }
                let async_gen = self.st_ref().is_async;
                if *delegate {
                    self.emit(Op::DelegateInit(async_gen));
                    let start = self.here();
                    self.emit(Op::DelegateCall);
                    if async_gen {
                        self.emit(Op::Await);
                    }
                    let done = self.emit(Op::DelegateDone(0, 0));
                    self.emit(Op::YieldDelegate);
                    self.emit(Op::Jump(start));
                    let ret = self.here();
                    self.emit(Op::DelegateEnd);
                    self.return_top()?;
                    let exit = self.here();
                    self.emit(Op::DelegateEnd);
                    self.st().code[done] = Op::DelegateDone(exit, ret);
                } else if async_gen {
                    self.emit(Op::Await);
                    self.emit_yield()?;
                } else {
                    self.emit_yield()?;
                }
            }
        }
        Ok(())
    }

    fn assign(&mut self, op: Option<AssignOp>, target: &AssignTarget, value: &Expr) -> R<()> {
        match target {
            AssignTarget::Pattern(p) => {
                self.expr(value)?;
                self.emit(Op::Dup);
                self.bind_pattern(p, BindMode::Assign)?;
            }
            AssignTarget::Simple(t) => match &t.kind {
                ExprKind::Ident(n) => match op {
                    None => {
                        self.expr_named(value, Some(n))?;
                        self.store_var(n, false);
                    }
                    Some(AssignOp::Bin(b)) => {
                        self.load_var(n);
                        self.expr(value)?;
                        self.emit(bin_op(b));
                        self.store_var(n, false);
                    }
                    Some(AssignOp::Logical(l)) => {
                        self.load_var(n);
                        let j = self.emit(logical_jump(l));
                        self.expr_named(value, Some(n))?;
                        self.store_var(n, false);
                        self.patch(j);
                    }
                },
                ExprKind::Member { obj, name, .. } => {
                    let k = self.str_const(name);
                    self.expr(obj)?;
                    match op {
                        None => {
                            self.expr(value)?;
                            self.emit(Op::SetProp(k));
                        }
                        Some(AssignOp::Bin(b)) => {
                            self.emit(Op::Dup);
                            self.emit(Op::GetProp(k));
                            self.expr(value)?;
                            self.emit(bin_op(b));
                            self.emit(Op::SetProp(k));
                        }
                        Some(AssignOp::Logical(l)) => {
                            self.emit(Op::Dup);
                            self.emit(Op::GetProp(k));
                            let j = self.emit(logical_jump(l));
                            self.expr(value)?;
                            self.emit(Op::SetProp(k));
                            let end = self.emit(Op::Jump(0));
                            self.patch(j);
                            self.emit(Op::Swap);
                            self.emit(Op::Pop);
                            self.patch(end);
                        }
                    }
                }
                ExprKind::Index { obj, index, .. } => {
                    self.expr(obj)?;
                    self.expr(index)?;
                    match op {
                        None => {
                            self.expr(value)?;
                            self.emit(Op::SetIndex);
                        }
                        Some(AssignOp::Bin(b)) => {
                            self.emit(Op::Dup2);
                            self.emit(Op::GetIndex);
                            self.expr(value)?;
                            self.emit(bin_op(b));
                            self.emit(Op::SetIndex);
                        }
                        Some(AssignOp::Logical(l)) => {
                            self.emit(Op::Dup2);
                            self.emit(Op::GetIndex);
                            let j = self.emit(logical_jump(l));
                            self.expr(value)?;
                            self.emit(Op::SetIndex);
                            let end = self.emit(Op::Jump(0));
                            self.patch(j);
                            self.emit(Op::Rot3);
                            self.emit(Op::Pop);
                            self.emit(Op::Pop);
                            self.patch(end);
                        }
                    }
                }
                ExprKind::SuperMember(m) => {
                    let is_static = self.super_info(false)?;
                    let k = self.str_const(m);
                    self.load_var(&"this".into());
                    match op {
                        None => {
                            self.expr(value)?;
                            self.emit(Op::SuperSet(k, is_static));
                        }
                        Some(AssignOp::Bin(b)) => {
                            self.emit(Op::Dup);
                            self.emit(Op::SuperGet(k, is_static));
                            self.expr(value)?;
                            self.emit(bin_op(b));
                            self.emit(Op::SuperSet(k, is_static));
                        }
                        Some(AssignOp::Logical(l)) => {
                            self.emit(Op::Dup);
                            self.emit(Op::SuperGet(k, is_static));
                            let j = self.emit(logical_jump(l));
                            self.expr(value)?;
                            self.emit(Op::SuperSet(k, is_static));
                            let end = self.emit(Op::Jump(0));
                            self.patch(j);
                            self.emit(Op::Swap);
                            self.emit(Op::Pop);
                            self.patch(end);
                        }
                    }
                }
                ExprKind::SuperIndex(key) => {
                    let is_static = self.super_info(false)?;
                    self.load_var(&"this".into());
                    self.expr(key)?;
                    self.emit(Op::ToPropertyKey);
                    match op {
                        None => {
                            self.expr(value)?;
                            self.emit(Op::SuperSetIndex(is_static));
                        }
                        Some(AssignOp::Bin(b)) => {
                            self.emit(Op::Dup2);
                            self.emit(Op::SuperGetIndex(is_static));
                            self.expr(value)?;
                            self.emit(bin_op(b));
                            self.emit(Op::SuperSetIndex(is_static));
                        }
                        Some(AssignOp::Logical(l)) => {
                            self.emit(Op::Dup2);
                            self.emit(Op::SuperGetIndex(is_static));
                            let j = self.emit(logical_jump(l));
                            self.expr(value)?;
                            self.emit(Op::SuperSetIndex(is_static));
                            let end = self.emit(Op::Jump(0));
                            self.patch(j);
                            self.emit(Op::Rot3);
                            self.emit(Op::Pop);
                            self.emit(Op::Pop);
                            self.patch(end);
                        }
                    }
                }
                _ => return Err(self.err("Invalid left-hand side in assignment")),
            },
        }
        Ok(())
    }
}
