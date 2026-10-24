use crate::ast::*;
use crate::bytecode::Cmp;
use crate::bytecode::*;
use crate::jsstr::JsStr;
use crate::token::SyntaxError;
use crate::value::Value;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

type R<T> = Result<T, SyntaxError>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Module,
    Repl,
}

enum TargetRef {
    None,
    Member(JsStr),
    Index,
}

impl TargetRef {
    fn width(&self) -> u32 {
        match self {
            TargetRef::None => 0,
            TargetRef::Member(_) => 1,
            TargetRef::Index => 2,
        }
    }
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
    Local(JsStr, JsStr),
    All(JsStr),
    Named(JsStr, JsStr, JsStr),
}

struct Local {
    name: JsStr,
    depth: u32,
    captured: bool,
    is_const: bool,
    tdz: bool,
    sticky: bool,
    lexical: bool,
}

struct LoopCtx {
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
    break_locals: usize,
    continue_locals: usize,
    try_depth: usize,
    is_switch: bool,
    is_block: bool,
    labels: Vec<JsStr>,
    iter_slot: Option<u32>,
    async_iter: Option<u32>,
}

#[derive(Clone)]
struct TryCtx {
    has_handler: bool,
    finally: Option<Rc<Vec<Stmt>>>,
}

struct FnState {
    name: JsStr,
    kind: ProtoKind,
    is_static: bool,
    line: u32,
    code: Vec<Op>,
    lines: Vec<u32>,
    cols: Vec<u32>,
    target: std::cell::Cell<usize>,
    consts: Vec<Value>,
    const_strs: FxHashMap<JsStr, u32>,
    functions: Vec<Rc<FunctionProto>>,
    upvalues: Vec<UpvalDesc>,
    locals: Vec<Local>,
    index: FxHashMap<JsStr, Vec<u32>>,
    depth: u32,
    loops: Vec<LoopCtx>,
    tries: Vec<TryCtx>,
    num_params: u32,
    arity: u32,
    has_rest: bool,
    uses_arguments: bool,
    mapped: Vec<bool>,
    param_exprs: bool,
    param_names: Vec<JsStr>,
    is_async: bool,
    is_generator: bool,
    fields: Option<Rc<Vec<ClassMember>>>,
    vars: FxHashSet<JsStr>,
    span: (u32, u32),
    template_sites: u32,
    eval_upvals: Option<EvalUpvals>,
    evals: Vec<Rc<EvalScope>>,
    strict: bool,
    self_slot: Option<u32>,
    annexb: Vec<*const FuncDef>,
    reads_this: bool,
}

impl FnState {
    fn new(name: JsStr, kind: ProtoKind, line: u32, is_static: bool) -> Self {
        FnState {
            name,
            kind,
            is_static,
            line,
            code: Vec::new(),
            lines: Vec::new(),
            cols: Vec::new(),
            target: std::cell::Cell::new(usize::MAX),
            consts: Vec::new(),
            const_strs: FxHashMap::default(),
            functions: Vec::new(),
            upvalues: Vec::new(),
            locals: Vec::new(),
            index: FxHashMap::default(),
            depth: 0,
            loops: Vec::new(),
            tries: Vec::new(),
            num_params: 0,
            arity: 0,
            has_rest: false,
            uses_arguments: false,
            mapped: Vec::new(),
            param_exprs: false,
            param_names: Vec::new(),
            is_async: false,
            is_generator: false,
            fields: None,
            vars: FxHashSet::default(),
            span: (0, 0),
            template_sites: 0,
            eval_upvals: None,
            evals: Vec::new(),
            strict: true,
            self_slot: None,
            annexb: Vec::new(),
            reads_this: false,
        }
    }
}

pub struct Compiler {
    states: Vec<FnState>,
    module: Rc<ModuleInfo>,
    mode: Mode,
    line: u32,
    col: u32,
    chain: Vec<usize>,
    exports: Vec<ExportEntry>,
    hidden: u32,
    next_with: u32,
    dynamic_locals: u32,
    self_bind: Option<JsStr>,
    repl_consts: Vec<JsStr>,
    repl_lex: Vec<JsStr>,
    eval_silent: Vec<JsStr>,
    eval_at_global: bool,
    var_scope_next: bool,
    pending_labels: Vec<JsStr>,
    import_aliases: Vec<(JsStr, JsStr, JsStr)>,
    export_names: Vec<(JsStr, JsStr)>,
    completion: Option<(usize, u32)>,
    script_eval: u8,
    in_eval: bool,
    eval_leak_vars: bool,
    defining_fn: bool,
    link_imports: Vec<(JsStr, Vec<JsStr>)>,
    reexport_sources: Vec<(JsStr, JsStr)>,
    ns_imports: Vec<(JsStr, JsStr)>,
}

type AnnexB = Vec<(JsStr, *const FuncDef)>;

fn annexb_list(list: &[&Stmt], enclosing: &mut Vec<JsStr>, nested: bool, out: &mut AnnexB) {
    let mut lex = Vec::new();
    let mut fns = Vec::new();
    for s in list {
        match &s.kind {
            StmtKind::Decl(k, decls) if *k != DeclKind::Var => {
                for (p, _) in decls {
                    p.bound_names(&mut lex);
                }
            }
            StmtKind::Class(c) => {
                if let Some(n) = &c.name {
                    lex.push(n.clone());
                }
            }
            StmtKind::Function(f) if nested => {
                if let Some(n) = &f.name {
                    fns.push(n.clone());
                }
            }
            _ => {}
        }
    }
    let mark = enclosing.len();
    for s in list {
        if let StmtKind::Function(f) = &s.kind {
            if nested && !f.is_async && !f.is_generator {
                if let Some(n) = &f.name {
                    let conflict = enclosing[..mark].contains(n)
                        || lex.contains(n)
                        || fns.iter().filter(|m| *m == n).count() > 1;
                    if !conflict {
                        out.push((n.clone(), Rc::as_ptr(f)));
                    }
                }
            }
        }
    }
    enclosing.extend(lex);
    enclosing.extend(fns);
    for s in list {
        annexb_stmt(s, enclosing, out);
    }
    enclosing.truncate(mark);
}

fn annexb_block(list: &[Stmt], enclosing: &mut Vec<JsStr>, out: &mut AnnexB) {
    let refs: Vec<&Stmt> = list.iter().collect();
    annexb_list(&refs, enclosing, true, out);
}

fn annexb_stmt(s: &Stmt, enclosing: &mut Vec<JsStr>, out: &mut AnnexB) {
    match &s.kind {
        StmtKind::Block(list) => annexb_block(list, enclosing, out),
        StmtKind::If(_, a, b) => {
            annexb_stmt(a, enclosing, out);
            if let Some(b) = b {
                annexb_stmt(b, enclosing, out);
            }
        }
        StmtKind::While(_, b)
        | StmtKind::DoWhile(b, _)
        | StmtKind::With(_, b)
        | StmtKind::Labeled(_, b) => annexb_stmt(b, enclosing, out),
        StmtKind::For { init, body, .. } => {
            let mark = enclosing.len();
            if let Some(i) = init {
                if let StmtKind::Decl(k, decls) = &i.kind {
                    if *k != DeclKind::Var {
                        for (p, _) in decls {
                            p.bound_names(enclosing);
                        }
                    }
                }
            }
            annexb_stmt(body, enclosing, out);
            enclosing.truncate(mark);
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
            let mark = enclosing.len();
            if decl.is_some_and(|k| k != DeclKind::Var) {
                pattern.bound_names(enclosing);
            }
            annexb_stmt(body, enclosing, out);
            enclosing.truncate(mark);
        }
        StmtKind::Try {
            block,
            param,
            handler,
            finalizer,
        } => {
            annexb_block(block, enclosing, out);
            if let Some(h) = handler {
                let mark = enclosing.len();
                if let Some(p) = param {
                    if !matches!(p, Pattern::Ident(_)) {
                        p.bound_names(enclosing);
                    }
                }
                annexb_block(h, enclosing, out);
                enclosing.truncate(mark);
            }
            if let Some(f) = finalizer {
                annexb_block(f, enclosing, out);
            }
        }
        StmtKind::Switch(_, cases) => {
            let refs: Vec<&Stmt> = cases.iter().flat_map(|c| c.body.iter()).collect();
            annexb_list(&refs, enclosing, true, out);
        }
        _ => {}
    }
}
fn collect_var_names(s: &Stmt, out: &mut Vec<JsStr>) {
    let push = |p: &Pattern, out: &mut Vec<JsStr>| {
        p.bound_names(out);
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
        StmtKind::While(_, b)
        | StmtKind::DoWhile(b, _)
        | StmtKind::Labeled(_, b)
        | StmtKind::With(_, b) => collect_var_names(b, out),
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
        col: 0,
        chain: Vec::new(),
        exports: Vec::new(),
        hidden: 0,
        next_with: 0,
        dynamic_locals: 0,
        self_bind: None,
        repl_consts: Vec::new(),
        repl_lex: Vec::new(),
        eval_silent: Vec::new(),
        eval_at_global: false,
        var_scope_next: false,
        pending_labels: Vec::new(),
        import_aliases: Vec::new(),
        export_names: Vec::new(),
        completion: None,
        script_eval: 0,
        in_eval: false,
        eval_leak_vars: false,
        defining_fn: false,
        link_imports: Vec::new(),
        reexport_sources: Vec::new(),
        ns_imports: Vec::new(),
    }
}

pub fn compile_eval(
    stmts: &[Stmt],
    module: Rc<ModuleInfo>,
    scope: &EvalScope,
) -> R<Rc<FunctionProto>> {
    let mut c = new_compiler(module, Mode::Module);
    let strict = scope.ctx.strict || crate::parser::body_use_strict(stmts);
    let mut outer = FnState::new("<scope>".into(), scope.kind, 1, scope.is_static);
    outer.strict = strict;
    outer.depth = 1;
    for (name, is_const, lexical) in &scope.locals {
        let slot = outer.locals.len() as u32;
        outer.index.entry(name.clone()).or_default().push(slot);
        outer.locals.push(Local {
            name: name.clone(),
            depth: 1,
            captured: true,
            is_const: *is_const,
            tdz: true,
            sticky: true,
            lexical: *lexical,
        });
    }
    outer.eval_upvals = Some(scope.upvals.clone());
    let dynamic = |n: &str| n.starts_with("%with") || n == "%evalvars";
    if scope.locals.iter().any(|(n, _, _)| dynamic(n))
        || scope.upvals.iter().any(|(n, _, _)| dynamic(n))
    {
        c.dynamic_locals = 1;
    }
    c.states.push(outer);
    let mut inner = FnState::new("eval".into(), ProtoKind::Arrow, 1, false);
    inner.strict = strict;
    c.states.push(inner);
    c.begin_scope();
    c.add_local("".into(), true);
    c.emit(Op::Undefined);
    let slot = c.add_local("%completion".into(), false);
    c.completion = Some((1, slot));
    c.in_eval = true;
    c.eval_silent = scope.self_names.clone();
    let in_function = !strict
        && (scope.locals.iter().any(|(n, _, _)| &**n == "%evalvars")
            || scope.upvals.iter().any(|(n, _, _)| &**n == "%evalvars"));
    let at_global = !strict && !in_function && scope.kind == ProtoKind::Module;
    c.eval_at_global = at_global;
    if in_function || at_global {
        let mut vars = Vec::new();
        for s in stmts {
            collect_var_names(s, &mut vars);
        }
        let refs: Vec<&Stmt> = stmts.iter().collect();
        let mut enclosing = Vec::new();
        let mut cands = Vec::new();
        annexb_list(&refs, &mut enclosing, false, &mut cands);
        for (n, ptr) in cands {
            c.st().annexb.push(ptr);
            if !vars.contains(&n) {
                vars.push(n);
            }
        }
        for s in stmts {
            if let StmtKind::Function(f) = &s.kind {
                if let Some(n) = &f.name {
                    if !vars.contains(n) {
                        vars.push(n.clone());
                    }
                }
            }
        }
        if scope.ctx.in_params && !scope.in_arrow && vars.iter().any(|n| &**n == "arguments") {
            return Err(SyntaxError::new(
                "Identifier 'arguments' cannot be declared by eval in parameter expressions",
                1,
                1,
            ));
        }
        for n in &vars {
            if scope
                .locals
                .iter()
                .any(|(m, _, lexical)| m == n && *lexical)
            {
                return Err(SyntaxError::new(
                    format!("Identifier '{n}' has already been declared"),
                    1,
                    1,
                ));
            }
        }
        c.eval_leak_vars = true;
        if at_global {
            for n in &vars {
                let k = c.str_const(n);
                c.emit(Op::CheckGlobalVar(k));
            }
            for s in stmts {
                if let StmtKind::Function(f) = &s.kind {
                    if let Some(n) = &f.name {
                        let k = c.str_const(n);
                        if !vars.contains(n) {
                            c.emit(Op::CheckGlobalVar(k));
                        }
                        c.emit(Op::CanDeclareGlobalFn(k));
                    }
                }
            }
        }
        for n in &vars {
            let k = c.str_const(n);
            if in_function {
                if scope.locals.iter().any(|(m, _, _)| m == n) {
                    continue;
                }
                c.load_var_static(&"%evalvars".into());
                c.emit(Op::EvalVar(k));
            } else {
                c.emit(Op::Undefined);
                let flags = c.define_flags(false, true, false);
                c.emit(Op::DefineGlobal(k, flags));
                c.emit(Op::Pop);
            }
        }
    } else {
        c.declare_vars(stmts);
    }
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
    let mut main = FnState::new("<main>".into(), ProtoKind::Module, 1, false);
    main.strict = mode == Mode::Module || script_eval == 2 || crate::parser::body_use_strict(stmts);
    c.states.push(main);
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
    if mode == Mode::Repl && script_eval != 2 {
        c.global_checks(stmts, script_eval == 0);
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
    let mut exported: Vec<&JsStr> = Vec::new();
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
                if c.import_binding(local).is_some() {
                    continue;
                }
                c.load_var(local);
                let k = c.str_const(exported);
                c.emit(Op::InitProp(k));
            }
            ExportEntry::All(_) | ExportEntry::Named(..) => {}
        }
    }
    c.emit(Op::Return);
    let source_of = |c: &Compiler, hidden: &JsStr| {
        c.reexport_sources
            .iter()
            .find(|(h, _)| h == hidden)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| "".into())
    };
    let src = &c.module.source;
    let cjs =
        src.contains("module.exports") || src.contains("exports.") || src.contains("require(");
    let mut link = ModuleLink {
        esm: !exports.is_empty() || !c.link_imports.is_empty() || !cjs,
        imports: c.link_imports.clone(),
        ..ModuleLink::default()
    };
    for entry in &exports {
        match entry {
            ExportEntry::Local(local, exported) => match c.import_binding(local) {
                Some((source, imported)) => {
                    link.indirect.push((exported.clone(), source, imported))
                }
                None => link.local_exports.push(exported.clone()),
            },
            ExportEntry::All(hidden) => link.stars.push(source_of(&c, hidden)),
            ExportEntry::Named(hidden, imported, exported) => {
                link.indirect
                    .push((exported.clone(), source_of(&c, hidden), imported.clone()));
            }
        }
    }
    let st = c.states.pop().expect("module state");
    let mut proto = c.finish(st);
    proto.link = Some(Rc::new(link));
    Ok(Rc::new(proto))
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

fn may_make_closure(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Null
        | ExprKind::Ident(_)
        | ExprKind::This
        | ExprKind::BigInt(_)
        | ExprKind::Regex(..) => false,
        ExprKind::Unary(_, a) => may_make_closure(a),
        ExprKind::Update { target, .. } => may_make_closure(target),
        ExprKind::Binary(_, a, b) | ExprKind::Logical(_, a, b) => {
            may_make_closure(a) || may_make_closure(b)
        }
        ExprKind::Cond(a, b, c) => {
            may_make_closure(a) || may_make_closure(b) || may_make_closure(c)
        }
        ExprKind::Assign { target, value, .. } => {
            let t = match target {
                AssignTarget::Simple(t) => may_make_closure(t),
                AssignTarget::Pattern(_) => true,
            };
            t || may_make_closure(value)
        }
        ExprKind::Seq(items) => items.iter().any(may_make_closure),
        ExprKind::Member { obj, .. } => may_make_closure(obj),
        ExprKind::Index { obj, index, .. } => may_make_closure(obj) || may_make_closure(index),
        ExprKind::Call { callee, args, .. } => {
            may_make_closure(callee)
                || args.iter().any(|a| match a {
                    Arg::Expr(e) | Arg::Spread(e) => may_make_closure(e),
                })
        }
        _ => true,
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
        let mut ic_count = 0u32;
        let ic_index: Box<[u32]> = st
            .code
            .iter()
            .map(|op| {
                if op.uses_prop_ic() {
                    ic_count += 1;
                    ic_count - 1
                } else {
                    u32::MAX
                }
            })
            .collect();
        FunctionProto {
            name: st.name,
            kind: st.kind,
            num_params: st.num_params,
            arity: st.arity,
            has_rest: st.has_rest,
            uses_arguments: st.uses_arguments,
            mapped: st.mapped.into_boxed_slice(),
            is_async: st.is_async,
            is_generator: st.is_generator,
            sloppy: !st.strict,
            sloppy_this: !st.strict && st.reads_this,
            code: st.code,
            lines: st.lines,
            cols: st.cols,
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
            ic_index,
            ics: (0..ic_count).map(|_| PropIc::default()).collect(),
            evals: st.evals,
            link: None,
            private_keys: Default::default(),
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
        let (line, col) = (self.line, self.col);
        let st = self.st();
        let at = st.code.len();
        if at > 0 && st.target.get() != at {
            let merged = match (st.code[at - 1], op) {
                (Op::Undefined, Op::Undefined) => Some(Op::UndefinedN(2)),
                (Op::UndefinedN(n), Op::Undefined) => Some(Op::UndefinedN(n + 1)),
                (Op::Uninit, Op::Uninit) => Some(Op::UninitN(2)),
                (Op::UninitN(n), Op::Uninit) => Some(Op::UninitN(n + 1)),
                _ => None,
            };
            if let Some(m) = merged {
                st.code[at - 1] = m;
                return at - 1;
            }
        }
        st.code.push(op);
        st.lines.push(line);
        st.cols.push(col);
        st.code.len() - 1
    }

    fn here(&self) -> u32 {
        let st = self.st_ref();
        st.target.set(st.code.len());
        st.code.len() as u32
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
            Op::TryBeginTop(_) => Op::TryBeginTop(t),
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

    fn str_const(&mut self, s: &JsStr) -> u32 {
        if let Some(&i) = self.st_ref().const_strs.get(s) {
            return i;
        }
        let i = self.constant(Value::Str(s.clone()));
        self.st().const_strs.insert(s.clone(), i);
        i
    }

    fn hidden_name(&mut self, prefix: &str) -> JsStr {
        self.hidden += 1;
        format!("%{prefix}{}", self.hidden).into()
    }

    fn locals_len(&self) -> usize {
        self.st_ref().locals.len()
    }

    fn add_local(&mut self, name: JsStr, is_const: bool) -> u32 {
        let st = self.st();
        let depth = st.depth;
        let slot = st.locals.len() as u32;
        st.index.entry(name.clone()).or_default().push(slot);
        st.locals.push(Local {
            name,
            depth,
            captured: false,
            is_const,
            tdz: false,
            sticky: false,
            lexical: false,
        });
        slot
    }

    fn truncate_locals(&mut self, n: usize) {
        let st = self.st();
        while st.locals.len() > n {
            let local = st.locals.pop().expect("local");
            if let Some(slots) = st.index.get_mut(&local.name) {
                slots.pop();
                if slots.is_empty() {
                    st.index.remove(&local.name);
                }
            }
        }
    }

    fn local_slots(&self, name: &str) -> &[u32] {
        self.st_ref().index.get(name).map_or(&[], |v| v.as_slice())
    }

    fn local_at_depth(&self, name: &str, depth: u32) -> bool {
        let st = self.st_ref();
        self.local_slots(name)
            .iter()
            .any(|&i| st.locals[i as usize].depth == depth)
    }

    fn add_lexical(&mut self, name: JsStr, is_const: bool) -> u32 {
        let slot = self.add_local(name, is_const);
        self.st().locals[slot as usize].tdz = true;
        self.st().locals[slot as usize].lexical = true;
        slot
    }

    fn mark_init(&mut self, name: &str) {
        let st = self.st();
        if let Some(&slot) = st.index.get(name).and_then(|v| v.last()) {
            let l = &mut st.locals[slot as usize];
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
        let mut outer: Vec<JsStr> = Vec::new();
        for i in (0..si).rev() {
            let st = &self.states[i];
            let names = st.locals.iter().rev().map(|l| l.name.clone()).chain(
                st.eval_upvals
                    .iter()
                    .flat_map(|u| u.iter().map(|(n, _, _)| n.clone())),
            );
            for n in names {
                let hidden = n.starts_with('%') && !n.starts_with("%with") && &*n != "%evalvars";
                if !n.is_empty() && !hidden && !outer.contains(&n) {
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
            .map(|l| (l.name.clone(), l.is_const, l.lexical))
            .collect();
        let in_arrow = st.kind == ProtoKind::Arrow;
        let self_names: Vec<JsStr> = self
            .states
            .iter()
            .filter(|s| !s.strict)
            .filter_map(|s| s.self_slot.map(|i| s.locals[i as usize].name.clone()))
            .collect();
        let st = self.st();
        st.evals.push(Rc::new(EvalScope {
            locals,
            upvals: Rc::new(upvals),
            kind,
            is_static,
            in_arrow,
            self_names,
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
            self.truncate_locals(n);
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
        st.index
            .get(name)
            .and_then(|v| v.last())
            .map(|&i| (i, st.locals[i as usize].is_const))
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
            if name == "this" {
                self.states[si - 1].reads_this = true;
            }
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
            if name == "this" {
                self.states[si].reads_this = true;
            }
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

    fn dynamic_scopes(&self, name: &str) -> Vec<JsStr> {
        if self.dynamic_locals == 0 || name.starts_with('%') || name == "this" {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut si = self.states.len() - 1;
        loop {
            let st = &self.states[si];
            let bound = st.locals.iter().rposition(|l| &*l.name == name);
            let from = bound.map_or(0, |i| i + 1);
            for l in st.locals[from..].iter().rev() {
                if l.name.starts_with("%with") || &*l.name == "%evalvars" {
                    out.push(l.name.clone());
                }
            }
            if bound.is_some() {
                break;
            }
            if let Some(ups) = &st.eval_upvals {
                let mut stop = false;
                for (n, _, _) in ups.iter() {
                    if &**n == name {
                        stop = true;
                        break;
                    }
                    if n.starts_with("%with") || &**n == "%evalvars" {
                        out.push(n.clone());
                    }
                }
                if stop {
                    break;
                }
            }
            if si == 0 {
                break;
            }
            si -= 1;
        }
        out
    }

    fn has_dynamic(&self, name: &str) -> bool {
        !self.dynamic_scopes(name).is_empty()
    }

    fn load_var(&mut self, name: &JsStr) {
        let scopes = self.dynamic_scopes(name);
        if scopes.is_empty() {
            self.load_var_static(name);
            return;
        }
        let k = self.str_const(name);
        let mut ends = Vec::new();
        for w in &scopes {
            self.load_var_static(w);
            self.emit(Op::WithHas(k));
            let j = self.emit(Op::JumpIfFalse(0));
            self.load_var_static(w);
            self.emit(Op::WithGet(k));
            ends.push(self.emit(Op::Jump(0)));
            self.patch(j);
        }
        self.load_var_static(name);
        for e in ends {
            self.patch(e);
        }
    }

    fn load_var_static(&mut self, name: &JsStr) {
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

    fn load_this_raw(&mut self) {
        match self.resolve("this") {
            Var::Local(s, _) | Var::LocalTdz(s, _) => {
                self.emit(Op::GetLocal(s));
            }
            Var::Upval(i, _) | Var::UpvalTdz(i, _) => {
                self.emit(Op::GetUpval(i));
            }
            Var::Global => {
                self.emit(Op::Undefined);
            }
        }
    }

    fn store_var(&mut self, name: &JsStr, init: bool) {
        let scopes = self.dynamic_scopes(name);
        if scopes.is_empty() {
            self.store_var_static(name, init);
            return;
        }
        let k = self.str_const(name);
        let mut ends = Vec::new();
        for w in &scopes {
            self.load_var_static(w);
            self.emit(Op::WithHas(k));
            let j = self.emit(Op::JumpIfFalse(0));
            self.load_var_static(w);
            self.emit(Op::WithSet(k));
            ends.push(self.emit(Op::Jump(0)));
            self.patch(j);
        }
        self.store_var_static(name, init);
        for e in ends {
            self.patch(e);
        }
    }

    fn store_var_static(&mut self, name: &JsStr, init: bool) {
        match self.resolve(name) {
            Var::Local(s, c) => {
                let silent =
                    c && !init && Some(s) == self.st_ref().self_slot && !self.st_ref().strict;
                if silent {
                    return;
                }
                if c && !init {
                    let k = self.str_const(name);
                    self.emit(Op::ThrowConstAssign(k));
                } else {
                    self.emit(Op::SetLocal(s));
                    self.export_hook(name);
                }
            }
            Var::Upval(i, c) => {
                let silent = c
                    && !init
                    && (self.states.iter().rev().skip(1).any(|st| {
                        !st.strict
                            && st
                                .self_slot
                                .is_some_and(|s| *st.locals[s as usize].name == **name)
                    }) || (self.in_eval
                        && !self.st_ref().strict
                        && self.eval_silent.contains(name)));
                if silent {
                    return;
                }
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
                if c && !init
                    && self.in_eval
                    && !self.st_ref().strict
                    && self.eval_silent.contains(name)
                {
                    return;
                }
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
                if init && (self.is_repl_top() || (self.eval_at_global && self.defining_fn)) {
                    let c = self.repl_consts.contains(name);
                    let lex = self.repl_lex.contains(name);
                    self.emit(Op::DefineGlobal(k, self.define_flags(c, false, lex)));
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
        names: &mut Vec<(JsStr, bool)>,
        fn_idx: &mut Vec<(usize, bool)>,
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
                    fn_idx.push((names.len(), !f.is_async && !f.is_generator));
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
                let hidden: JsStr = format!("%import{}", imports.len()).into();
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
                        self.collect_decls(inner, top, names, fn_idx, funcs, imports)?;
                        for (n, _) in names[before..].iter() {
                            self.exports.push(ExportEntry::Local(n.clone(), n.clone()));
                        }
                    }
                    ExportDecl::DefaultDecl(inner) => {
                        let before = names.len();
                        self.collect_decls(inner, top, names, fn_idx, funcs, imports)?;
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

    fn global_checks(&mut self, stmts: &[Stmt], lexical: bool) {
        let mut vars = Vec::new();
        for s in stmts {
            collect_var_names(s, &mut vars);
        }
        let mut funcs = Vec::new();
        let mut lex = Vec::new();
        for s in stmts {
            match &s.kind {
                StmtKind::Function(f) => {
                    if let Some(n) = &f.name {
                        funcs.push(n.clone());
                    }
                }
                StmtKind::Decl(kind, decls) if *kind != DeclKind::Var => {
                    for (p, _) in decls {
                        p.bound_names(&mut lex);
                    }
                }
                StmtKind::Class(c) => {
                    if let Some(n) = &c.name {
                        lex.push(n.clone());
                    }
                }
                _ => {}
            }
        }
        if lexical {
            for n in &lex {
                let k = self.str_const(n);
                self.emit(Op::CheckGlobalLex(k));
            }
        }
        for n in vars.iter().chain(funcs.iter()) {
            let k = self.str_const(n);
            self.emit(Op::CheckGlobalVar(k));
        }
        for n in &funcs {
            let k = self.str_const(n);
            self.emit(Op::CanDeclareGlobalFn(k));
        }
    }

    fn define_flags(&self, is_const: bool, keep: bool, lexical: bool) -> u8 {
        let mut flags = 0;
        if is_const {
            flags |= crate::bytecode::DG_CONST;
        }
        if lexical {
            flags |= crate::bytecode::DG_LEXICAL;
        }
        if keep {
            flags |= crate::bytecode::DG_KEEP;
        }
        if self.script_eval != 0 || self.in_eval {
            flags |= crate::bytecode::DG_CONFIGURABLE;
        }
        if self.defining_fn {
            flags |= crate::bytecode::DG_FUNCTION;
        }
        flags
    }

    fn declare_vars(&mut self, stmts: &[Stmt]) {
        let mut names = Vec::new();
        for s in stmts {
            collect_var_names(s, &mut names);
        }
        if !self.st_ref().strict {
            let refs: Vec<&Stmt> = stmts.iter().collect();
            let mut enclosing = Vec::new();
            let mut cands = Vec::new();
            annexb_list(&refs, &mut enclosing, false, &mut cands);
            for (n, ptr) in cands {
                let bound = !self.local_slots(&n).is_empty();
                if bound {
                    continue;
                }
                self.st().annexb.push(ptr);
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        let repl = self.is_repl_top() && self.script_eval != 2;
        for n in names {
            if self.st_ref().vars.contains(&n) {
                continue;
            }
            let cur_depth = self.st_ref().depth;
            let exists = {
                let st = self.st_ref();
                let self_slot = st.self_slot;
                self.local_slots(&n)
                    .iter()
                    .any(|&i| st.locals[i as usize].depth <= cur_depth && Some(i) != self_slot)
            };
            self.st().vars.insert(n.clone());
            if exists {
                if self.st_ref().param_exprs && self.st_ref().param_names.contains(&n) {
                    if let Some((slot, _)) = self.resolve_local(self.states.len() - 1, &n) {
                        self.emit(Op::GetLocal(slot));
                        self.add_local(n, false);
                    }
                }
                continue;
            }
            if repl {
                self.emit(Op::Undefined);
                let k = self.str_const(&n);
                self.emit(Op::DefineGlobal(k, self.define_flags(false, true, false)));
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
        let mut fn_idx = Vec::new();
        let mut funcs = Vec::new();
        let mut imports = Vec::new();
        let mut reexports = Vec::new();
        for s in stmts {
            self.collect_decls(s, top, &mut names, &mut fn_idx, &mut funcs, &mut imports)?;
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
        let var_set: FxHashSet<JsStr> = var_names.iter().cloned().collect();
        let fn_names: FxHashSet<JsStr> = funcs.iter().filter_map(|f| f.name.clone()).collect();
        let fn_map: FxHashMap<usize, bool> = fn_idx.iter().copied().collect();
        for (n, _) in &names {
            let is_fn = fn_names.contains(n);
            if var_scope && is_fn && !(top && self.mode == Mode::Module) {
                continue;
            }
            if var_set.contains(n) {
                return Err(self.err(format!("Identifier '{n}' has already been declared")));
            }
        }
        let depth = self.st_ref().depth;
        let mut fresh = Vec::new();
        let mut positions: FxHashMap<JsStr, Vec<usize>> = FxHashMap::default();
        for (i, (n, c)) in names.iter().enumerate() {
            let is_fn = fn_names.contains(n);
            let earlier: Vec<usize> = positions.get(n).cloned().unwrap_or_default();
            positions.entry(n.clone()).or_default().push(i);
            let me_fn = fn_map.get(&i).copied();
            let earlier_fns = earlier.iter().all(|j| fn_map.contains_key(j));
            let earlier_plain = earlier.iter().all(|j| fn_map.get(j) == Some(&true));
            let redeclared_fn = me_fn.is_some()
                && earlier_fns
                && !(top && self.mode == Mode::Module)
                && (var_scope || (!self.st_ref().strict && me_fn == Some(true) && earlier_plain));
            let dup_new = !earlier.is_empty() && !redeclared_fn;
            let var_clash = self.st_ref().vars.contains(n) && self.local_at_depth(n, depth);
            if var_clash && is_fn {
                continue;
            }
            let self_slot = self.st_ref().self_slot;
            let dup_old = &**n != "arguments" && {
                let st = self.st_ref();
                self.local_slots(n)
                    .iter()
                    .any(|&i| st.locals[i as usize].depth == depth && Some(i) != self_slot)
            };
            if dup_old && is_fn && var_scope && !dup_new {
                let params = self.st_ref().num_params as usize + self.st_ref().has_rest as usize;
                let param_dup = {
                    let st = self.st_ref();
                    self.local_slots(n).iter().any(|&i| {
                        st.locals[i as usize].depth == depth
                            && i as usize >= 1
                            && i as usize <= params
                    })
                };
                if param_dup {
                    continue;
                }
            }
            if dup_new || dup_old {
                return Err(self.err(format!("Identifier '{n}' has already been declared")));
            }
            if self.eval_leak_vars && var_scope && is_fn {
                continue;
            }
            fresh.push((n.clone(), *c));
        }
        let names = fresh;
        let repl = self.is_repl_top() && self.script_eval != 2;
        let import_names: Vec<JsStr> = imports.iter().filter_map(|i| i.namespace.clone()).collect();
        for (n, c) in &names {
            let lexical_kind =
                !fn_names.contains(n) && !n.starts_with("%import") && !import_names.contains(n);
            let lexical = (!repl || self.script_eval == 1) && lexical_kind;
            if top
                && lexical_kind
                && self.mode != Mode::Module
                && self.script_eval == 0
                && matches!(&**n, "undefined" | "NaN" | "Infinity")
            {
                return Err(self.err(format!("Identifier '{n}' has already been declared")));
            }
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
                if lexical_kind {
                    self.repl_lex.push(n.clone());
                } else {
                    self.repl_lex.retain(|m| m != n);
                }
                let k = self.str_const(n);
                let mut flags = self.define_flags(*c, false, lexical_kind);
                if fn_names.contains(n) {
                    flags |= crate::bytecode::DG_FUNCTION;
                }
                self.emit(Op::DefineGlobal(k, flags));
                self.emit(Op::Pop);
            } else {
                self.add_local(n.clone(), *c);
            }
        }
        if top && self.mode == Mode::Module {
            let mut pre: Vec<(JsStr, JsStr)> = self
                .exports
                .iter()
                .filter_map(|e| match e {
                    ExportEntry::Local(l, x)
                        if !self.import_aliases.iter().any(|(a, _, _)| a == l)
                            && !imports.iter().any(|i| i.namespace.as_ref() == Some(l)) =>
                    {
                        Some((x.clone(), l.clone()))
                    }
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
                if fn_names.contains(local) {
                    self.load_var(local);
                } else if var_set.contains(local) {
                    self.emit(Op::Undefined);
                } else {
                    self.emit(Op::Uninit);
                }
                let k = self.str_const(exported);
                self.emit(Op::InitProp(k));
                self.emit(Op::Pop);
            }
        }
        for (n, imp) in imports.iter().enumerate() {
            let mut requested: Vec<JsStr> = Vec::new();
            if imp.default.is_some() {
                requested.push("default".into());
            }
            for (imported, _) in &imp.named {
                requested.push(imported.clone());
            }
            self.link_imports.push((imp.source.clone(), requested));
            if let Some(ns) = &imp.namespace {
                self.ns_imports.push((ns.clone(), imp.source.clone()));
            }
            let src = self.str_const(&imp.source);
            self.emit(Op::Import(src));
            if let Some(ns) = &imp.namespace {
                self.store_var(ns, true);
            }
            let hidden: JsStr = format!("%import{n}").into();
            self.store_var(&hidden, true);
            self.emit(Op::Pop);
        }
        for (source, list) in reexports {
            let hidden = self.hidden_name("reexport");
            self.reexport_sources.push((hidden.clone(), source.clone()));
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
                    self.defining_fn = true;
                    self.store_var(n, true);
                    self.defining_fn = false;
                    if !var_scope && self.st_ref().annexb.contains(&Rc::as_ptr(f)) {
                        self.annexb_copy(n);
                    }
                }
                self.emit(Op::Pop);
            }
        }
        Ok(())
    }

    fn delete_ident_static(&mut self, n: &JsStr) {
        match self.resolve(n) {
            Var::Global if self.alias(n).is_none() => {
                let k = self.str_const(n);
                self.emit(Op::DeleteGlobal(k));
            }
            _ => {
                self.emit(Op::False);
            }
        }
    }

    fn annexb_copy(&mut self, n: &JsStr) {
        let st = self.st_ref();
        let inner = st.index.get(n).and_then(|v| v.last()).map(|&i| i as usize);
        let outer = st
            .locals
            .iter()
            .enumerate()
            .rev()
            .find(|(i, l)| &l.name == n && Some(*i) != inner && l.depth <= 1)
            .map(|(i, _)| i);
        match outer {
            Some(i) => {
                self.emit(Op::SetLocal(i as u32));
            }
            None => {
                let k = self.str_const(n);
                let ev: JsStr = "%evalvars".into();
                if matches!(self.resolve(&ev), Var::Global) {
                    self.emit(Op::SetGlobal(k));
                } else {
                    self.load_var_static(&ev);
                    self.emit(Op::Swap);
                    self.emit(Op::SetProp(k));
                }
            }
        }
    }

    fn import_binding(&self, local: &str) -> Option<(JsStr, JsStr)> {
        if let Some((_, hidden, key)) = self.import_aliases.iter().find(|(l, _, _)| &**l == local) {
            let n: usize = hidden.strip_prefix("%import")?.parse().ok()?;
            let (source, _) = self.link_imports.get(n)?;
            return Some((source.clone(), key.clone()));
        }
        self.ns_imports
            .iter()
            .find(|(l, _)| &**l == local)
            .map(|(_, source)| (source.clone(), "*".into()))
    }

    fn alias(&self, name: &str) -> Option<(JsStr, JsStr)> {
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
            .any(|st| st.index.contains_key(name))
        {
            return false;
        }
        match self.states[0].index.get(name).and_then(|v| v.last()) {
            Some(&i) => self.states[0].locals[i as usize].depth == 1,
            None => false,
        }
    }

    fn export_hook(&mut self, name: &JsStr) {
        if !self.is_export_binding(name) {
            return;
        }
        let targets: Vec<JsStr> = self
            .export_names
            .iter()
            .filter(|(l, _)| l == name)
            .map(|(_, x)| x.clone())
            .collect();
        for x in targets {
            self.emit(Op::Dup);
            self.load_var(&"%ns".into());
            self.emit(Op::Swap);
            let k = self.str_const(&x);
            self.emit(Op::InitProp(k));
            self.emit(Op::Pop);
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
            StmtKind::Decl(DeclKind::Var, _) => self.stmt(s),
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
        self.col = 0;
        let hidden_try = matches!(&s.kind, StmtKind::Try { param: Some(Pattern::Ident(p)), .. } if p.starts_with("%using"));
        if let Some(slot) = self.completion_slot().filter(|_| !hidden_try) {
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
                    | StmtKind::With(..)
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
                        if let Some(n) = &name {
                            if !self.dynamic_scopes(n).is_empty() {
                                let target = AssignTarget::Simple(Box::new(Expr {
                                    kind: ExprKind::Ident(n.clone()),
                                    line: s.line,
                                    col: 0,
                                }));
                                self.assign(None, &target, e)?;
                                self.emit(Op::Pop);
                                continue;
                            }
                        }
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
            StmtKind::With(obj, body) => {
                self.begin_scope();
                self.expr(obj)?;
                self.emit(Op::ToObject);
                let id = self.next_with;
                self.next_with += 1;
                self.dynamic_locals += 1;
                self.add_local(format!("%with{id}").into(), true);
                self.sub_stmt(body)?;
                self.end_scope();
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
                        StmtKind::Decl(kind, decls) => {
                            if *kind != DeclKind::Var {
                                let mut head = Vec::new();
                                for (p, _) in decls {
                                    p.bound_names(&mut head);
                                }
                                let mut vars = Vec::new();
                                collect_var_names(body, &mut vars);
                                if let Some(n) = head.iter().find(|n| vars.contains(n)) {
                                    return Err(self.err(format!(
                                        "Identifier '{n}' has already been declared"
                                    )));
                                }
                            }
                            let base = self.locals_len();
                            self.hoist(std::slice::from_ref(init.as_ref()))?;
                            self.stmt(init)?;
                            if self.locals_len() > base {
                                loop_base = Some(base);
                                if self.st_ref().locals[base..].iter().any(|l| l.captured) {
                                    self.emit(Op::CloseUpvalues(base as u32));
                                }
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
                let mut close_at = None;
                if let Some(b) = loop_base {
                    if self.st_ref().locals[b..].iter().any(|l| l.captured) {
                        self.emit(Op::CloseUpvalues(b as u32));
                    } else if update.as_ref().is_some_and(may_make_closure) {
                        close_at = Some(self.emit(Op::CloseUpvalues(u32::MAX)));
                    }
                }
                if let Some(u) = update {
                    self.expr_discard(u)?;
                }
                if let (Some(at), Some(b)) = (close_at, loop_base) {
                    if self.st_ref().locals[b..].iter().any(|l| l.captured) {
                        self.st().code[at] = Op::CloseUpvalues(b as u32);
                    }
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
            async_iter: None,
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
            async_iter: None,
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
        let decl = decl.filter(|k| *k != DeclKind::Var);
        let mut head_names = Vec::new();
        if let Some(kind) = decl {
            pattern.bound_names(&mut head_names);
            for (i, n) in head_names.iter().enumerate() {
                if &**n == "let" {
                    return Err(self.err("let is disallowed as a lexically bound name"));
                }
                if head_names[..i].contains(n) {
                    return Err(self.err(format!("Identifier '{n}' has already been declared")));
                }
            }
            let mut vars = Vec::new();
            collect_var_names(body, &mut vars);
            if let Some(n) = head_names.iter().find(|n| vars.contains(n)) {
                return Err(self.err(format!("Identifier '{n}' has already been declared")));
            }
            for n in &head_names {
                self.emit(Op::Uninit);
                self.add_lexical(n.clone(), kind == DeclKind::Const);
            }
        }
        let tdz_base = self.locals_len() - head_names.len();
        self.expr(source)?;
        self.pending_labels = labels;
        self.emit_at(source, if keys { Op::ForInIter } else { Op::GetIter });
        if !head_names.is_empty() {
            let first = tdz_base as u32;
            self.emit(Op::CloseUpvalues(first));
            self.emit(Op::SetLocal(first));
            self.emit(Op::PopTo(first + 1));
            self.truncate_locals(tdz_base);
        }
        let iter_name = self.hidden_name("iter");
        let iter_slot = self.add_local(iter_name, true);
        let start = self.here();
        self.begin_scope();
        if let Some(kind) = decl {
            for n in &head_names {
                self.emit(Op::Undefined);
                self.add_local(n.clone(), kind == DeclKind::Const);
            }
        }
        self.emit(Op::GetLocal(iter_slot));
        let next = self.emit(Op::IterNext(0));
        let try_depth = self.st_ref().tries.len();
        let guard = if keys {
            None
        } else {
            let g = self.emit(Op::TryBegin(0, iter_slot + 1));
            self.st().tries.push(TryCtx {
                has_handler: true,
                finally: None,
            });
            Some(g)
        };
        self.bind_pattern(
            pattern,
            if decl.is_some() {
                BindMode::Init
            } else {
                BindMode::Assign
            },
        )?;
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
            async_iter: None,
        });
        self.sub_stmt(body)?;
        let ctx = self.st().loops.pop().expect("loop");
        for j in ctx.continue_jumps {
            self.patch_to(j, start);
        }
        if guard.is_some() {
            self.emit(Op::TryEnd);
            self.st().tries.pop();
        }
        self.end_scope();
        self.emit(Op::Jump(start));
        if let Some(g) = guard {
            self.patch(g);
            self.emit(Op::IterCloseQuiet(iter_slot));
            self.emit(Op::Throw);
        }
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
        let next: JsStr = "next".into();
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
        self.emit_at(source, Op::GetAsyncIter);
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
        let done: JsStr = "done".into();
        let k = self.str_const(&done);
        self.emit(Op::GetProp(k));
        let exit = self.emit(Op::JumpIfTrue(0));
        let value: JsStr = "value".into();
        let k = self.str_const(&value);
        self.emit(Op::GetProp(k));
        let try_depth = self.st_ref().tries.len();
        let guard = self.emit(Op::TryBegin(0, iter_slot + 1));
        self.st().tries.push(TryCtx {
            has_handler: true,
            finally: None,
        });
        self.bind_pattern(
            pattern,
            if decl.is_some() {
                BindMode::Init
            } else {
                BindMode::Assign
            },
        )?;
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
            async_iter: Some(iter_slot),
        });
        self.sub_stmt(body)?;
        let ctx = self.st().loops.pop().expect("loop");
        for j in ctx.continue_jumps {
            self.patch_to(j, start);
        }
        self.emit(Op::TryEnd);
        self.st().tries.pop();
        self.end_scope();
        self.emit(Op::Jump(start));
        self.patch(guard);
        self.emit_async_close(iter_slot, true);
        self.emit(Op::Throw);
        self.patch(exit);
        self.end_scope();
        for j in ctx.break_jumps {
            self.patch(j);
        }
        Ok(())
    }

    fn emit_async_close(&mut self, slot: u32, quiet: bool) {
        if quiet {
            self.emit(Op::AsyncIterReturnQuiet(slot));
        } else {
            self.emit(Op::AsyncIterReturn(slot));
        }
        let skip = self.emit(Op::JumpIfFalse(0));
        if quiet {
            let guard = self.emit(Op::TryBeginTop(0));
            self.emit(Op::Await);
            self.emit(Op::TryEnd);
            self.emit(Op::Pop);
            let done = self.emit(Op::Jump(0));
            self.patch(guard);
            self.emit(Op::Pop);
            self.emit(Op::Pop);
            let done2 = self.emit(Op::Jump(0));
            self.patch(skip);
            self.emit(Op::Pop);
            self.patch(done);
            self.patch(done2);
        } else {
            self.emit(Op::Await);
            self.emit(Op::CheckIterResult);
            self.emit(Op::Pop);
            let done = self.emit(Op::Jump(0));
            self.patch(skip);
            self.emit(Op::Pop);
            self.patch(done);
        }
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
        let slots: Vec<(Option<u32>, Option<u32>)> = self.st_ref().loops[from..]
            .iter()
            .rev()
            .map(|l| (l.iter_slot, l.async_iter))
            .collect();
        for (sync, asynch) in slots {
            if let Some(slot) = sync {
                self.emit(Op::IterClose(slot));
            }
            if let Some(slot) = asynch {
                self.emit_async_close(slot, false);
            }
        }
    }

    fn return_top(&mut self) -> R<()> {
        self.close_iterators(0);
        if self.st_ref().tries.iter().any(|t| t.finally.is_some()) {
            let name = self.hidden_name("ret");
            let slot = self.add_local(name, true);
            self.unwind_tries(0)?;
            self.emit(Op::GetLocal(slot));
            let n = self.st_ref().locals.len() - 1;
            self.truncate_locals(n);
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
                let mut bound = Vec::new();
                p.bound_names(&mut bound);
                for s in h {
                    if let StmtKind::Function(f) = &s.kind {
                        if let Some(n) = &f.name {
                            if bound.contains(n) {
                                return Err(
                                    self.err(format!("Identifier '{n}' has already been declared"))
                                );
                            }
                        }
                    }
                }
            }
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

    fn target_ref(&mut self, p: &Pattern) -> R<TargetRef> {
        let Pattern::Target(e) = p else {
            return Ok(TargetRef::None);
        };
        match &e.kind {
            ExprKind::Member { obj, name, .. } => {
                self.expr(obj)?;
                Ok(TargetRef::Member(name.clone()))
            }
            ExprKind::Index { obj, index, .. } => {
                self.expr(obj)?;
                self.expr(index)?;
                Ok(TargetRef::Index)
            }
            ExprKind::Call { .. } => {
                self.invalid_target(e)?;
                Ok(TargetRef::None)
            }
            _ => Err(self.err("Invalid destructuring assignment target")),
        }
    }

    fn invalid_target(&mut self, e: &Expr) -> R<()> {
        self.expr(e)?;
        self.emit(Op::Pop);
        let msg: JsStr = "Invalid left-hand side in assignment".into();
        let k = self.str_const(&msg);
        self.emit(Op::ThrowRef(k));
        Ok(())
    }

    fn pattern_default(&mut self, p: &Pattern, default: Option<&Expr>) -> R<()> {
        if let Some(d) = default {
            let j = self.emit(Op::JumpIfNotUndef(0));
            let name = match p {
                Pattern::Ident(n) => Some(n.clone()),
                _ => None,
            };
            self.expr_named(d, name.as_ref())?;
            self.patch(j);
        }
        Ok(())
    }

    fn store_target(&mut self, p: &Pattern, target: &TargetRef, mode: BindMode) -> R<()> {
        match target {
            TargetRef::None => self.bind_pattern(p, mode),
            TargetRef::Member(name) => {
                let k = self.str_const(name);
                self.emit(Op::SetProp(k));
                self.emit(Op::Pop);
                Ok(())
            }
            TargetRef::Index => {
                self.emit(Op::SetIndex);
                self.emit(Op::Pop);
                Ok(())
            }
        }
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
                ExprKind::Call { .. } => self.invalid_target(e)?,
                _ => return Err(self.err("Invalid destructuring assignment target")),
            },
            Pattern::Object { props, rest } => {
                let first = match props.first().map(|p| &p.key) {
                    Some(PropKey::Named(k)) => self.str_const(k),
                    _ => NO_NAME,
                };
                self.emit(Op::RequireObject(first));
                let mut saved = 0u32;
                for prop in props {
                    let named = match &prop.key {
                        PropKey::Named(k) => Some(self.str_const(k)),
                        PropKey::Computed(e) => {
                            self.expr(e)?;
                            self.emit(Op::ToPropertyKey);
                            if rest.is_some() {
                                self.emit(Op::Dup);
                                self.emit(Op::Rot3);
                                saved += 1;
                            }
                            None
                        }
                    };
                    let target = self.target_ref(&prop.value)?;
                    let w = target.width();
                    match named {
                        Some(k) => {
                            self.emit(Op::Pick(w));
                            self.emit(Op::GetProp(k));
                        }
                        None => {
                            self.emit(Op::Pick(w + 1));
                            self.emit(Op::Pick(w + 1));
                            self.emit(Op::GetIndex);
                        }
                    }
                    self.pattern_default(&prop.value, prop.default.as_ref())?;
                    self.store_target(&prop.value, &target, mode)?;
                    if named.is_none() {
                        self.emit(Op::Pop);
                    }
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
                    self.emit(Op::ObjRest(n, saved));
                    self.bind_pattern(r, mode)?;
                }
                self.emit(Op::Pop);
            }
            Pattern::Array { elems, rest } => {
                self.emit(Op::GetIter);
                let guard = self.emit(Op::TryBeginTop(0));
                for el in elems {
                    match el {
                        None => {
                            self.emit(Op::Dup);
                            self.emit(Op::IterNextOrUndef);
                            self.emit(Op::Pop);
                        }
                        Some(PatElem { pattern, default }) => {
                            let target = self.target_ref(pattern)?;
                            self.emit(Op::Pick(target.width()));
                            self.emit(Op::IterNextOrUndef);
                            self.pattern_default(pattern, default.as_ref())?;
                            self.store_target(pattern, &target, mode)?;
                        }
                    }
                }
                let rest_target = match rest {
                    Some(r) => self.target_ref(r)?,
                    None => TargetRef::None,
                };
                self.emit(Op::TryEnd);
                if let Some(r) = rest {
                    self.emit(Op::Pick(rest_target.width()));
                    self.emit(Op::IterRest);
                    self.store_target(r, &rest_target, mode)?;
                    self.emit(Op::Pop);
                } else {
                    self.emit(Op::IterCloseTop);
                }
                let end = self.emit(Op::Jump(0));
                self.patch(guard);
                self.emit(Op::IterCloseUnder);
                self.emit(Op::Throw);
                self.patch(end);
            }
        }
        Ok(())
    }

    fn function(
        &mut self,
        f: &FuncDef,
        name_hint: Option<JsStr>,
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
        let name: JsStr = if &*name == "%default" {
            "default".into()
        } else if let Some(rest) = name.strip_prefix("\u{0}#") {
            format!("#{}", rest.split('@').next().unwrap_or(rest)).into()
        } else {
            name
        };
        let self_name = self.self_bind.take();
        let saved = (self.line, self.col);
        (self.line, self.col) = (f.line, 0);
        let mut st = FnState::new(name, kind, f.line, f.kind == FuncKind::StaticMethod);
        st.span = f.span;
        st.fields = fields;
        st.is_async = f.is_async;
        st.is_generator = f.is_generator;
        st.strict = f.strict;
        self.states.push(st);
        let saved_labels = std::mem::take(&mut self.pending_labels);
        let r = self.function_body(f, kind, self_name);
        self.pending_labels = saved_labels;
        let st = self.states.pop().expect("function state");
        (self.line, self.col) = saved;
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
        let mut param_names = Vec::new();
        for p in &f.params {
            p.pattern.bound_names(&mut param_names);
        }
        if let Some(r) = &f.rest {
            r.bound_names(&mut param_names);
        }
        let simple = f
            .params
            .iter()
            .all(|p| p.default.is_none() && matches!(p.pattern, Pattern::Ident(_)))
            && f.rest
                .as_ref()
                .is_none_or(|r| matches!(r, Pattern::Ident(_)));
        let body_fn = simple
            && match &f.body {
                FuncBody::Block(stmts) => stmts.iter().any(|s| {
                    matches!(&s.kind, StmtKind::Function(g) if g.name.as_deref() == Some("arguments"))
                }),
                FuncBody::Expr(_) => false,
            };
        let shadowed = param_names.iter().any(|n| &**n == "arguments") || body_fn;
        if f.uses_arguments && f.kind != FuncKind::Arrow && !shadowed {
            self.add_local("arguments".into(), false);
            self.st().uses_arguments = true;
        }
        if f.has_eval {
            for st in &mut self.states {
                st.reads_this = true;
            }
        }
        if f.has_eval && !self.st_ref().strict {
            self.emit(Op::NewObject);
            self.dynamic_locals += 1;
            self.add_local("%evalvars".into(), true);
        }
    }

    fn simple_params(&mut self, f: &FuncDef) -> R<()> {
        for p in &f.params {
            if let Pattern::Ident(n) = &p.pattern {
                if self.st_ref().strict
                    && self.resolve_local(self.states.len() - 1, n).is_some()
                    && &**n != "this"
                {
                    return Err(self.err(format!("Duplicate parameter name '{n}'")));
                }
                self.add_local(n.clone(), false);
            }
        }
        self.param_counts(f);
        if let Some(Pattern::Ident(n)) = &f.rest {
            if self.st_ref().strict && self.resolve_local(self.states.len() - 1, n).is_some() {
                return Err(self.err(format!("Duplicate parameter name '{n}'")));
            }
            self.add_local(n.clone(), false);
        }
        self.arguments_local(f);
        if self.st_ref().uses_arguments
            && !self.st_ref().strict
            && f.kind != FuncKind::Arrow
            && f.rest.is_none()
        {
            let mut mapped = vec![false; f.params.len()];
            let mut seen: Vec<&JsStr> = Vec::new();
            for (i, p) in f.params.iter().enumerate().rev() {
                if let Pattern::Ident(n) = &p.pattern {
                    if !seen.contains(&n) {
                        seen.push(n);
                        mapped[i] = true;
                    }
                }
            }
            self.st().mapped = mapped;
        }
        Ok(())
    }

    fn complex_params(&mut self, f: &FuncDef, self_name: Option<JsStr>) -> R<()> {
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
        if let Some(n) = self_name {
            if !names.contains(&n) && self.resolve_local(self.states.len() - 1, &n).is_none() {
                self.emit(Op::GetCallee);
                let s = self.add_local(n, true);
                self.st().self_slot = Some(s);
            }
        }
        self.st().param_exprs = true;
        self.st().param_names = names.clone();
        if self.st_ref().uses_arguments {
            self.st().param_names.push("arguments".into());
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

    fn function_body(&mut self, f: &FuncDef, kind: ProtoKind, self_name: Option<JsStr>) -> R<()> {
        self.begin_scope();
        if kind == ProtoKind::Arrow {
            self.add_local("".into(), true);
        } else if kind == ProtoKind::DerivedConstructor {
            let s = self.add_lexical("this".into(), true);
            self.st().locals[s as usize].sticky = true;
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
            self.complex_params(f, self_name.clone())?;
        }
        if f.is_generator {
            self.emit(Op::Undefined);
            self.emit_yield()?;
            self.emit(Op::Pop);
        }
        if let Some(n) = self_name {
            if self.resolve_local(self.states.len() - 1, &n).is_none() {
                self.emit(Op::GetCallee);
                let s = self.add_local(n, true);
                self.st().self_slot = Some(s);
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
        let mut instance_done = 0usize;
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
                    PropKey::Computed(_) => {
                        let hidden: JsStr = format!("\u{0}#%ik{instance_done}@0").into();
                        instance_done += 1;
                        self.emit(Op::GetCallee);
                        let k = self.str_const(&hidden);
                        self.emit_raw(Op::GetProp(k));
                        match value {
                            Some(v) => self.expr(v)?,
                            None => {
                                self.emit(Op::Undefined);
                            }
                        }
                        self.emit(Op::DefineIndex);
                    }
                }
                self.emit(Op::Pop);
            }
        }
        Ok(())
    }

    fn class(&mut self, c: &ClassDef, name_hint: Option<&JsStr>) -> R<()> {
        let saved_line = self.line;
        self.line = c.line;
        let name: JsStr = c
            .name
            .clone()
            .or_else(|| name_hint.cloned())
            .unwrap_or_else(|| "".into());
        let inner = match &c.name {
            Some(n) => {
                self.begin_scope();
                self.emit(Op::Uninit);
                Some(self.add_lexical(n.clone(), true))
            }
            None => None,
        };
        if c.private_id != 0 {
            self.emit(Op::ClassBrand(c.private_id));
        }
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
                                kind: ExprKind::Ident("%args".into()),
                                line: c.line,
                                col: 0,
                            })]),
                            line: c.line,
                            col: 0,
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
                        Some(Pattern::Ident("%args".into()))
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
                    strict: true,
                    has_eval: false,
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
        if let Some(s) = inner {
            self.emit(Op::SetLocal(s));
            self.st().locals[s as usize].tdz = false;
        }
        let mut instance_keys = 0usize;
        let mut static_keys = 0usize;
        for m in &c.members {
            if let ClassMember::Field {
                key: PropKey::Computed(e),
                is_static,
                ..
            } = m
            {
                let hidden: JsStr = if *is_static {
                    static_keys += 1;
                    format!("\u{0}#%sk{}@0", static_keys - 1).into()
                } else {
                    instance_keys += 1;
                    format!("\u{0}#%ik{}@0", instance_keys - 1).into()
                };
                self.emit(Op::Dup);
                self.expr(e)?;
                self.emit(Op::ToPropertyKey);
                let k = self.str_const(&hidden);
                self.emit_raw(Op::InitProp(k));
                self.emit(Op::Pop);
            }
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
        let mut static_done = 0usize;
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
                    PropKey::Computed(_) => {
                        let hidden: JsStr = format!("\u{0}#%sk{static_done}@0").into();
                        static_done += 1;
                        self.emit(Op::Dup);
                        self.emit(Op::Dup);
                        let k = self.str_const(&hidden);
                        self.emit_raw(Op::GetProp(k));
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
                        self.emit(Op::DefineIndex);
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
        if let Some(s) = inner {
            self.emit(Op::CloseUpvalues(s));
            self.emit(Op::Swap);
            self.emit(Op::Pop);
            self.scope_cut();
        }
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
        if c.private_id != 0 {
            self.emit(Op::EndClassBrand);
        }
        self.line = saved_line;
        Ok(())
    }

    fn static_init_value(&mut self, v: &Expr, name: Option<&JsStr>, line: u32) -> R<()> {
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
            strict: true,
            has_eval: false,
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
                    self.emit_at(e, Op::ArraySpread);
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
                let t: JsStr = t.into();
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
        if let ExprKind::OptChain(inner) = &callee.kind {
            let saved = std::mem::take(&mut self.chain);
            let r = self.call(inner, args, optional);
            let jumps = std::mem::replace(&mut self.chain, saved);
            r?;
            for j in jumps {
                self.patch(j);
            }
            return Ok(());
        }
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
                self.emit_at(callee, Op::GetIndexMethod);
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
            ExprKind::Ident(n) if self.has_dynamic(n) => {
                let scopes = self.dynamic_scopes(n);
                let k = self.str_const(n);
                let mut ends = Vec::new();
                for w in &scopes {
                    self.load_var_static(w);
                    self.emit(Op::WithHas(k));
                    let j = self.emit(Op::JumpIfFalse(0));
                    self.load_var_static(w);
                    self.emit(Op::WithGet(k));
                    self.load_var_static(w);
                    ends.push(self.emit(Op::Jump(0)));
                    self.patch(j);
                }
                self.load_var_static(n);
                self.emit(Op::Undefined);
                for e in ends {
                    self.patch(e);
                }
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

    fn enter_expr(&mut self, e: &Expr) -> (u32, u32) {
        let saved = (self.line, self.col);
        if e.col != 0 {
            self.col = e.col;
        } else if e.line != self.line {
            self.col = 0;
        }
        self.line = e.line;
        saved
    }

    fn emit_at(&mut self, e: &Expr, op: Op) -> usize {
        let saved = self.enter_expr(e);
        let at = self.emit(op);
        (self.line, self.col) = saved;
        at
    }

    fn jump_if_false(&mut self, test: &Expr) -> R<usize> {
        let saved = self.enter_expr(test);
        let r = self.jump_if_false_inner(test);
        (self.line, self.col) = saved;
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

    fn stable_target(&mut self, obj: &Expr, value: &Expr) -> bool {
        match &obj.kind {
            ExprKind::This => true,
            ExprKind::Ident(n) => {
                matches!(self.resolve(n), Var::Local(_, true)) || inert_value(value)
            }
            _ => false,
        }
    }

    fn local_slot(&mut self, e: &Expr) -> Option<u32> {
        match &e.kind {
            ExprKind::Ident(n) if self.has_dynamic(n) => None,
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

    fn store_discard(&mut self, name: &JsStr) {
        if self.is_export_binding(name) || self.has_dynamic(name) {
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
        let saved = self.enter_expr(e);
        let r = self.expr_discard_inner(e);
        (self.line, self.col) = saved;
        r
    }

    fn expr_discard_inner(&mut self, e: &Expr) -> R<()> {
        match &e.kind {
            ExprKind::Update { inc, target, .. } => {
                if let ExprKind::Ident(n) = &target.kind {
                    if let (Var::Local(s, false), false, false) = (
                        self.resolve(n),
                        self.is_export_binding(n),
                        self.has_dynamic(n),
                    ) {
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
                    col: 0,
                };
                self.expr(&as_prefix)?;
                self.emit(Op::Pop);
            }
            ExprKind::Assign {
                op,
                target: AssignTarget::Simple(t),
                value,
            } => match (&t.kind, op) {
                (ExprKind::Ident(n), None) if !self.has_dynamic(n) => {
                    self.expr_named(value, Some(n))?;
                    self.store_discard(n);
                }
                (ExprKind::Ident(n), Some(AssignOp::Bin(BinOp::Add)))
                    if matches!(self.resolve(n), Var::Local(_, false))
                        && !self.is_export_binding(n)
                        && !self.has_dynamic(n) =>
                {
                    let Var::Local(slot, _) = self.resolve(n) else {
                        unreachable!()
                    };
                    self.expr(value)?;
                    self.emit(Op::AppendLocal(slot));
                }
                (ExprKind::Ident(n), Some(AssignOp::Bin(b))) if !self.has_dynamic(n) => {
                    self.load_var(n);
                    self.expr(value)?;
                    self.emit(bin_op(*b));
                    self.store_discard(n);
                }
                (ExprKind::Member { obj, name, .. }, None) => {
                    let k = self.str_const(name);
                    match self.local_slot(obj) {
                        Some(slot) if self.stable_target(obj, value) => {
                            self.expr(value)?;
                            self.emit(Op::StoreLocalProp(slot, k));
                        }
                        _ => {
                            self.expr(obj)?;
                            self.expr(value)?;
                            self.emit(Op::StoreProp(k));
                        }
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

    fn expr_named(&mut self, e: &Expr, name: Option<&JsStr>) -> R<()> {
        let saved = self.enter_expr(e);
        let r = self.expr_inner(e, name);
        (self.line, self.col) = saved;
        r
    }

    fn expr_inner(&mut self, e: &Expr, name: Option<&JsStr>) -> R<()> {
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
                if s.starts_with("\u{0}#") {
                    self.emit(Op::PrivateKey(k));
                } else {
                    self.emit(Op::Const(k));
                }
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
                                self.emit_at(x, Op::ArraySpread);
                            }
                            ArrayElem::Hole => {
                                self.emit(Op::ArrayHole);
                            }
                        }
                    }
                }
            }
            ExprKind::Object(props) => {
                let protos = props
                    .iter()
                    .filter(|p| matches!(p, PropDef::KeyValue(PropKey::Named(n), _) if &**n == "__proto__"))
                    .count();
                if protos > 1 {
                    return Err(
                        self.err("Duplicate __proto__ fields are not allowed in object literals")
                    );
                }
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
                            self.emit(Op::ToPropertyKey);
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
                                    col: 0,
                                })),
                                line: c.line,
                            },
                        ]),
                        kind: FuncKind::Arrow,
                        is_async: false,
                        is_generator: false,
                        uses_arguments: false,
                        strict: true,
                        has_eval: false,
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
                        let scopes = self.dynamic_scopes(n);
                        let k = self.str_const(n);
                        let mut ends = Vec::new();
                        for w in &scopes {
                            self.load_var_static(w);
                            self.emit(Op::WithHas(k));
                            let j = self.emit(Op::JumpIfFalse(0));
                            self.load_var_static(w);
                            self.emit(Op::WithGet(k));
                            self.emit(Op::Typeof);
                            ends.push(self.emit(Op::Jump(0)));
                            self.patch(j);
                        }
                        if matches!(self.resolve(n), Var::Global) && self.alias(n).is_none() {
                            self.emit(Op::TypeofGlobal(k));
                        } else {
                            self.load_var_static(n);
                            self.emit(Op::Typeof);
                        }
                        for e in ends {
                            self.patch(e);
                        }
                        return Ok(());
                    }
                    self.expr(x)?;
                    self.emit(Op::Typeof);
                }
                UnaryOp::Delete => match &x.kind {
                    ExprKind::Ident(n) if self.has_dynamic(n) => {
                        let scopes = self.dynamic_scopes(n);
                        let k = self.str_const(n);
                        let mut ends = Vec::new();
                        for w in &scopes {
                            self.load_var_static(w);
                            self.emit(Op::WithHas(k));
                            let j = self.emit(Op::JumpIfFalse(0));
                            self.load_var_static(w);
                            self.emit(Op::DeleteProp(k));
                            ends.push(self.emit(Op::Jump(0)));
                            self.patch(j);
                        }
                        self.delete_ident_static(n);
                        for e in ends {
                            self.patch(e);
                        }
                    }
                    ExprKind::Ident(n) => self.delete_ident_static(n),
                    ExprKind::SuperMember(_) => {
                        self.super_info(false)?;
                        self.load_var(&"this".into());
                        self.emit(Op::Pop);
                        let msg: JsStr = "Unsupported reference to 'super'".into();
                        let k = self.str_const(&msg);
                        self.emit(Op::ThrowRef(k));
                        self.emit(Op::Undefined);
                    }
                    ExprKind::SuperIndex(key) => {
                        self.super_info(false)?;
                        self.load_var(&"this".into());
                        self.emit(Op::Pop);
                        self.expr(key)?;
                        self.emit(Op::Pop);
                        let msg: JsStr = "Unsupported reference to 'super'".into();
                        let k = self.str_const(&msg);
                        self.emit(Op::ThrowRef(k));
                        self.emit(Op::Undefined);
                    }
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
                        let scopes = self.dynamic_scopes(n);
                        let mut ends = Vec::new();
                        if !scopes.is_empty() {
                            let k = self.str_const(n);
                            for w in &scopes {
                                self.load_var_static(w);
                                self.emit(Op::WithHas(k));
                                let j = self.emit(Op::JumpIfFalse(0));
                                self.load_var_static(w);
                                self.emit(Op::Dup);
                                self.emit(Op::WithGet(k));
                                if *prefix {
                                    self.emit(step);
                                    self.emit(Op::Swap);
                                    self.emit(Op::WithSet(k));
                                } else {
                                    self.emit(Op::ToNumeric);
                                    self.emit(Op::Dup);
                                    self.emit(Op::Rot3);
                                    self.emit(step);
                                    self.emit(Op::Swap);
                                    self.emit(Op::WithSet(k));
                                    self.emit(Op::Pop);
                                }
                                ends.push(self.emit(Op::Jump(0)));
                                self.patch(j);
                            }
                        }
                        self.load_var_static(n);
                        if *prefix {
                            self.emit(step);
                            self.store_var_static(n, false);
                        } else {
                            self.emit(Op::ToNumeric);
                            self.emit(Op::Dup);
                            self.emit(step);
                            self.store_var_static(n, false);
                            self.emit(Op::Pop);
                        }
                        for e in ends {
                            self.patch(e);
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
                        self.emit(Op::ToPropertyKey);
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
                    ExprKind::SuperMember(m) => {
                        let is_static = self.super_info(false)?;
                        let k = self.str_const(m);
                        self.load_var(&"this".into());
                        self.emit(Op::Dup);
                        self.emit(Op::SuperGet(k, is_static));
                        if *prefix {
                            self.emit(step);
                            self.emit(Op::SuperSet(k, is_static));
                        } else {
                            self.emit(Op::ToNumeric);
                            self.emit(Op::Dup);
                            self.emit(Op::Rot3);
                            self.emit(step);
                            self.emit(Op::SuperSet(k, is_static));
                            self.emit(Op::Pop);
                        }
                    }
                    ExprKind::SuperIndex(key) => {
                        let is_static = self.super_info(false)?;
                        self.load_var(&"this".into());
                        self.expr(key)?;
                        self.emit(Op::ToPropertyKey);
                        self.emit(Op::Dup2);
                        self.emit(Op::SuperGetIndex(is_static));
                        if *prefix {
                            self.emit(step);
                            self.emit(Op::SuperSetIndex(is_static));
                        } else {
                            self.emit(Op::ToNumeric);
                            self.emit(Op::Dup);
                            self.emit(Op::Rot4);
                            self.emit(step);
                            self.emit(Op::SuperSetIndex(is_static));
                            self.emit(Op::Pop);
                        }
                    }
                    ExprKind::Call { .. } => {
                        self.invalid_target(target)?;
                        self.emit(Op::Undefined);
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
                if Self::has_spread(args) {
                    self.args_array(args)?;
                    self.emit(Op::DirectEval(u32::MAX, idx));
                } else {
                    let argc = self.plain_args(args)?;
                    self.emit(Op::DirectEval(argc, idx));
                }
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
                self.load_this_raw();
                let forwarded = matches!(
                    args.as_slice(),
                    [Arg::Spread(Expr { kind: ExprKind::Ident(n), .. })] if &**n == "%args"
                );
                if forwarded {
                    self.load_var(&"%args".into());
                    self.emit(Op::SuperCall(u32::MAX - 1));
                } else if Self::has_spread(args) {
                    self.args_array(args)?;
                    self.emit(Op::SuperCall(u32::MAX));
                } else {
                    let n = self.plain_args(args)?;
                    self.emit(Op::SuperCall(n));
                }
                self.load_this_raw();
                self.emit(Op::RequireUninit);
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
                ExprKind::Ident(n) => {
                    let scopes = self.dynamic_scopes(n);
                    let mut ends = Vec::new();
                    if !scopes.is_empty() {
                        let k = self.str_const(n);
                        for w in &scopes {
                            self.load_var_static(w);
                            self.emit(Op::WithHas(k));
                            let j = self.emit(Op::JumpIfFalse(0));
                            match op {
                                None => {
                                    self.expr_named(value, Some(n))?;
                                    self.load_var_static(w);
                                    self.emit(Op::WithSet(k));
                                }
                                Some(AssignOp::Bin(b)) => {
                                    self.load_var_static(w);
                                    self.emit(Op::Dup);
                                    self.emit(Op::WithGet(k));
                                    self.expr(value)?;
                                    self.emit(bin_op(b));
                                    self.emit(Op::Swap);
                                    self.emit(Op::WithSet(k));
                                }
                                Some(AssignOp::Logical(l)) => {
                                    self.load_var_static(w);
                                    self.emit(Op::Dup);
                                    self.emit(Op::WithGet(k));
                                    let j2 = self.emit(logical_jump(l));
                                    self.expr_named(value, Some(n))?;
                                    self.emit(Op::Swap);
                                    self.emit(Op::WithSet(k));
                                    let e2 = self.emit(Op::Jump(0));
                                    self.patch(j2);
                                    self.emit(Op::Swap);
                                    self.emit(Op::Pop);
                                    self.patch(e2);
                                }
                            }
                            ends.push(self.emit(Op::Jump(0)));
                            self.patch(j);
                        }
                    }
                    match op {
                        None => {
                            if scopes.is_empty()
                                && self.st_ref().strict
                                && matches!(self.resolve(n), Var::Global)
                                && self.alias(n).is_none()
                            {
                                let k = self.str_const(n);
                                self.emit(Op::GlobalRef(k));
                                self.expr_named(value, Some(n))?;
                                self.emit(Op::SetGlobalStrict(k));
                            } else {
                                self.expr_named(value, Some(n))?;
                                self.store_var_static(n, false);
                            }
                        }
                        Some(AssignOp::Bin(b)) => {
                            self.load_var_static(n);
                            self.expr(value)?;
                            self.emit(bin_op(b));
                            self.store_var_static(n, false);
                        }
                        Some(AssignOp::Logical(l)) => {
                            self.load_var_static(n);
                            let j = self.emit(logical_jump(l));
                            self.expr_named(value, Some(n))?;
                            self.store_var_static(n, false);
                            self.patch(j);
                        }
                    }
                    for e in ends {
                        self.patch(e);
                    }
                }
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
                    if op.is_some() {
                        self.emit(Op::ToPropertyKey);
                    }
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
                    if op.is_some() {
                        self.emit(Op::ToPropertyKey);
                    }
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
                ExprKind::Call { .. } => {
                    self.invalid_target(t)?;
                    self.emit(Op::Undefined);
                }
                _ => return Err(self.err("Invalid left-hand side in assignment")),
            },
        }
        Ok(())
    }
}

fn inert_value(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Null
        | ExprKind::BigInt(_)
        | ExprKind::Regex(..)
        | ExprKind::This
        | ExprKind::Ident(_)
        | ExprKind::Function(_)
        | ExprKind::NewTarget
        | ExprKind::ImportMeta => true,
        ExprKind::Template(_, parts) => parts.iter().all(inert_value),
        ExprKind::Unary(op, x) => *op != UnaryOp::Delete && inert_value(x),
        ExprKind::Binary(_, a, b) | ExprKind::Logical(_, a, b) => inert_value(a) && inert_value(b),
        ExprKind::Cond(a, b, c) => inert_value(a) && inert_value(b) && inert_value(c),
        ExprKind::Array(items) => items.iter().all(|i| match i {
            ArrayElem::Expr(x) => inert_value(x),
            ArrayElem::Hole => true,
            ArrayElem::Spread(_) => false,
        }),
        ExprKind::Object(props) => props.iter().all(|p| match p {
            PropDef::KeyValue(PropKey::Named(_), x) => inert_value(x),
            PropDef::Shorthand(_) => true,
            PropDef::Method(PropKey::Named(_), _)
            | PropDef::Getter(PropKey::Named(_), _)
            | PropDef::Setter(PropKey::Named(_), _) => true,
            _ => false,
        }),
        _ => false,
    }
}
