use crate::inspect::inspect_plain;
use crate::jsstr::JsStr;
use crate::value::Value;
use std::fmt::Write;
use std::path::PathBuf;
use std::rc::Rc;

pub const NO_NAME: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Const(u32),
    Undefined,
    Null,
    True,
    False,
    Pop,
    Dup,
    Dup2,
    Swap,
    Rot3,
    Rot4,
    GetLocal(u32),
    SetLocal(u32),
    GetUpval(u32),
    SetUpval(u32),
    Uninit,
    GetLocalChecked(u32, u32),
    SetLocalChecked(u32, u32),
    GetUpvalChecked(u32, u32),
    SetUpvalChecked(u32, u32),
    GetGlobal(u32),
    SetGlobal(u32),
    DefineGlobal(u32, u8),
    CheckGlobalLex(u32),
    CheckGlobalVar(u32),
    GlobalRef(u32),
    SetGlobalStrict(u32),
    DeleteGlobal(u32),
    EvalVar(u32),
    CanDeclareGlobalFn(u32),
    WithGet(u32),
    WithSet(u32),
    ThrowRef(u32),
    AsyncIterReturn(u32),
    AsyncIterReturnQuiet(u32),
    CheckIterResult,
    TypeofGlobal(u32),
    GetProp(u32),
    SetProp(u32),
    GetIndex,
    SetIndex,
    DeleteProp(u32),
    DeleteIndex,
    GetMethod(u32),
    GetIndexMethod,
    SuperGet(u32, bool),
    SuperGetMethod(u32, bool),
    SuperGetIndex(bool),
    SuperGetIndexMethod(bool),
    SuperSet(u32, bool),
    SuperSetIndex(bool),
    ToPropertyKey,
    DelegateInit(bool),
    DelegateCall,
    DelegateDone(u32, u32),
    YieldDelegate,
    DelegateEnd,
    GetPrivate(u32),
    SetPrivate(u32),
    InitPrivate(u32),
    CopyPrivates,
    DefineField(u32),
    DefineIndex,
    SuperCall(u32),
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Neg,
    ToNumber,
    Not,
    BitNot,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    Lt,
    Gt,
    Le,
    Ge,
    In,
    InstanceOf,
    Typeof,
    Inc,
    Dec,
    Jump(u32),
    JumpIfFalse(u32),
    JumpIfTrue(u32),
    JumpIfFalseKeep(u32),
    JumpIfTrueKeep(u32),
    JumpIfNotNullishKeep(u32),
    JumpIfNullish(u32),
    JumpIfNullishCall(u32),
    JumpIfNotUndef(u32),
    Call(u32, u32),
    CallSpread(u32),
    New(u32, u32),
    NewSpread(u32),
    Return,
    Closure(u32),
    PopTo(u32),
    CloseUpvalues(u32),
    Collapse(u32),
    NewArray(u32),
    ArrayPush,
    ArraySpread,
    ArrayHole,
    NewObject,
    InitProp(u32),
    InitIndex,
    ObjSpread,
    ObjRest(u32, u32),
    Pick(u32),
    RequireUninit,
    WithHas(u32),
    ToObject,
    MakeClass(u32, bool),
    DefineMethod(u32, bool),
    DefineMethodComputed(bool),
    GetIter,
    ForInIter,
    IterNext(u32),
    IterNextOrUndef,
    IterRest,
    Template(u32),
    Throw,
    TryBegin(u32, u32),
    TryEnd,
    ThrowConstAssign(u32),
    Import(u32),
    ExportAll,
    GetCallee,
    Await,
    Yield,
    InitAccessor(u32, bool),
    InitAccessorComputed(bool),
    DefineAccessor(u32, bool, bool),
    DefineAccessorComputed(bool, bool),
    NewRegex(u32, u32),
    CmpJump(Cmp, u32),
    IncLocal(u32),
    DecLocal(u32),
    StoreLocal(u32),
    StoreUpval(u32),
    StoreProp(u32),
    GetAsyncIter,
    AppendLocal(u32),
    GetLocalProp(u32, u32),
    StoreLocalProp(u32, u32),
    GetLocalMethod(u32, u32),
    InitHidden(u32),
    InitMethod(u32),
    InitMethodComputed,
    InitProto,
    NewTarget,
    DirectEval(u32, u32),
    SetHome,
    YieldReturn(u32),
    IterClose(u32),
    IterCloseTop,
    IterCloseQuiet(u32),
    IterCloseUnder,
    TryBeginTop(u32),
    ModuleNamespace,
    ImportMeta,
    DynImport,
    TemplateCache(u32, u32),
    TemplateStore(u32),
    ToNumeric,
    RequireObject(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    StrictEq,
    StrictNe,
    Eq,
    Ne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtoKind {
    Normal,
    Arrow,
    Method,
    ClassConstructor,
    DerivedConstructor,
    Module,
}

#[derive(Clone, Copy, Debug)]
pub struct UpvalDesc {
    pub is_local: bool,
    pub index: u32,
}

pub const DG_KEEP: u8 = 1;
pub const DG_CONFIGURABLE: u8 = 2;
pub const DG_CONST: u8 = 4;
pub const DG_FUNCTION: u8 = 8;
pub const DG_LEXICAL: u8 = 16;

#[derive(Debug)]
pub struct ModuleInfo {
    pub name: JsStr,
    pub dir: PathBuf,
    pub source: JsStr,
    pub sloppy: bool,
}

#[derive(Debug)]
pub struct FunctionProto {
    pub name: JsStr,
    pub kind: ProtoKind,
    pub num_params: u32,
    pub arity: u32,
    pub has_rest: bool,
    pub uses_arguments: bool,
    pub mapped: Box<[bool]>,
    pub is_async: bool,
    pub is_generator: bool,
    pub sloppy: bool,
    pub code: Vec<Op>,
    pub lines: Vec<u32>,
    pub consts: Vec<Value>,
    pub functions: Vec<Rc<FunctionProto>>,
    pub upvalues: Vec<UpvalDesc>,
    pub module: Rc<ModuleInfo>,
    pub line: u32,
    pub global_cache: Box<[std::cell::Cell<u32>]>,
    pub span: (u32, u32),
    pub templates: Box<[std::cell::RefCell<Option<Value>>]>,
    pub evals: Vec<Rc<EvalScope>>,
    pub link: Option<Rc<ModuleLink>>,
}

#[derive(Debug, Default)]
pub struct ModuleLink {
    pub esm: bool,
    pub imports: Vec<(JsStr, Vec<JsStr>)>,
    pub local_exports: Vec<JsStr>,
    pub indirect: Vec<(JsStr, JsStr, JsStr)>,
    pub stars: Vec<JsStr>,
}

pub type EvalUpvals = Rc<Vec<(JsStr, u32, bool)>>;

#[derive(Debug)]
pub struct EvalScope {
    pub locals: Vec<(JsStr, bool, bool)>,
    pub upvals: EvalUpvals,
    pub kind: ProtoKind,
    pub is_static: bool,
    pub in_arrow: bool,
    pub self_names: Vec<JsStr>,
    pub ctx: Rc<crate::ast::EvalCtx>,
}

impl FunctionProto {
    pub fn is_constructor(&self) -> bool {
        !self.is_coroutine()
            && matches!(
                self.kind,
                ProtoKind::Normal | ProtoKind::ClassConstructor | ProtoKind::DerivedConstructor
            )
    }

    pub fn is_coroutine(&self) -> bool {
        self.is_async || self.is_generator
    }

    pub fn is_class(&self) -> bool {
        matches!(
            self.kind,
            ProtoKind::ClassConstructor | ProtoKind::DerivedConstructor
        )
    }

    pub fn line_at(&self, ip: usize) -> u32 {
        self.lines
            .get(ip.saturating_sub(1))
            .copied()
            .unwrap_or(self.line)
    }

    pub fn disassemble(&self) -> String {
        let mut out = String::new();
        self.disassemble_into(&mut out);
        out
    }

    fn const_text(&self, i: u32) -> String {
        self.consts
            .get(i as usize)
            .map(inspect_plain)
            .unwrap_or_default()
    }

    fn disassemble_into(&self, out: &mut String) {
        let name = if self.name.is_empty() {
            "<anonymous>"
        } else {
            &self.name
        };
        let _ = writeln!(
            out,
            "== {name} ({:?}, params {}, rest {}, upvalues {}) ==",
            self.kind,
            self.num_params,
            self.has_rest,
            self.upvalues.len()
        );
        let mut last_line = 0;
        for (i, op) in self.code.iter().enumerate() {
            let line = self.lines[i];
            let line_text = if line == last_line {
                "   |".to_string()
            } else {
                format!("{line:4}")
            };
            last_line = line;
            let detail = match op {
                Op::Const(c)
                | Op::GetGlobal(c)
                | Op::SetGlobal(c)
                | Op::DefineGlobal(c, _)
                | Op::DeleteGlobal(c)
                | Op::EvalVar(c)
                | Op::CanDeclareGlobalFn(c)
                | Op::CheckGlobalLex(c)
                | Op::CheckGlobalVar(c)
                | Op::GlobalRef(c)
                | Op::SetGlobalStrict(c)
                | Op::WithGet(c)
                | Op::WithSet(c)
                | Op::ThrowRef(c)
                | Op::TypeofGlobal(c)
                | Op::GetProp(c)
                | Op::SetProp(c)
                | Op::DeleteProp(c)
                | Op::GetMethod(c)
                | Op::InitProp(c)
                | Op::InitHidden(c)
                | Op::InitMethod(c)
                | Op::StoreProp(c)
                | Op::Import(c)
                | Op::ThrowConstAssign(c)
                | Op::SuperGet(c, _)
                | Op::SuperGetMethod(c, _)
                | Op::DefineMethod(c, _)
                | Op::InitAccessor(c, _)
                | Op::DefineAccessor(c, _, _)
                | Op::MakeClass(c, _) => format!("  {}", self.const_text(*c)),
                Op::GetLocalProp(_, c) | Op::StoreLocalProp(_, c) | Op::GetLocalMethod(_, c) => {
                    format!("  {}", self.const_text(*c))
                }
                Op::NewRegex(src, flags) => {
                    format!("  /{}/{}", self.const_text(*src), self.const_text(*flags))
                }
                Op::Closure(f) => format!(
                    "  <fn {}>",
                    self.functions
                        .get(*f as usize)
                        .map(|p| p.name.to_string())
                        .unwrap_or_default()
                ),
                _ => String::new(),
            };
            let _ = writeln!(out, "{i:05} {line_text}  {op:?}{detail}");
        }
        for f in &self.functions {
            out.push('\n');
            f.disassemble_into(out);
        }
    }
}
