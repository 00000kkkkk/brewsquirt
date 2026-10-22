use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: u32,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    NewTarget,
    BigInt(Rc<str>),
    ImportMeta,
    DynImport(Box<Expr>),
    Number(f64),
    Str(Rc<str>),
    Regex(Rc<str>, Rc<str>),
    Template(Vec<Rc<str>>, Vec<Expr>),
    TemplateStrings(Vec<Option<Rc<str>>>, Vec<Rc<str>>),
    Bool(bool),
    Null,
    Ident(Rc<str>),
    This,
    Array(Vec<ArrayElem>),
    Object(Vec<PropDef>),
    Function(Rc<FuncDef>),
    Class(Rc<ClassDef>),
    Unary(UnaryOp, Box<Expr>),
    Update {
        inc: bool,
        prefix: bool,
        target: Box<Expr>,
    },
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Logical(LogicalOp, Box<Expr>, Box<Expr>),
    Assign {
        op: Option<AssignOp>,
        target: AssignTarget,
        value: Box<Expr>,
    },
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Member {
        obj: Box<Expr>,
        name: Rc<str>,
        optional: bool,
    },
    Index {
        obj: Box<Expr>,
        index: Box<Expr>,
        optional: bool,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
        optional: bool,
    },
    DirectEval(Vec<Arg>, Rc<EvalCtx>),
    New {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    SuperCall(Vec<Arg>),
    SuperMember(Rc<str>),
    SuperIndex(Box<Expr>),
    OptChain(Box<Expr>),
    Seq(Vec<Expr>),
    Await(Box<Expr>),
    Yield {
        arg: Option<Box<Expr>>,
        delegate: bool,
    },
}

#[derive(Clone, Debug)]
pub enum ArrayElem {
    Expr(Expr),
    Spread(Expr),
    Hole,
}

#[derive(Clone, Debug)]
pub enum Arg {
    Expr(Expr),
    Spread(Expr),
}

#[derive(Clone, Debug)]
pub enum PropKey {
    Named(Rc<str>),
    Computed(Expr),
}

#[derive(Clone, Debug)]
pub enum PropDef {
    KeyValue(PropKey, Expr),
    Shorthand(Rc<str>),
    ShorthandDefault(Rc<str>, Expr),
    Spread(Expr),
    Method(PropKey, Rc<FuncDef>),
    Getter(PropKey, Rc<FuncDef>),
    Setter(PropKey, Rc<FuncDef>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    Plus,
    BitNot,
    Typeof,
    Void,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    Lt,
    Gt,
    Le,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    In,
    InstanceOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
    Nullish,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Bin(BinOp),
    Logical(LogicalOp),
}

#[derive(Clone, Debug)]
pub enum AssignTarget {
    Simple(Box<Expr>),
    Pattern(Pattern),
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Ident(Rc<str>),
    Target(Box<Expr>),
    Object {
        props: Vec<ObjPatProp>,
        rest: Option<Box<Pattern>>,
    },
    Array {
        elems: Vec<Option<PatElem>>,
        rest: Option<Box<Pattern>>,
    },
}

#[derive(Clone, Debug)]
pub struct ObjPatProp {
    pub key: PropKey,
    pub value: Pattern,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct PatElem {
    pub pattern: Pattern,
    pub default: Option<Expr>,
}

impl Pattern {
    pub fn bound_names(&self, out: &mut Vec<Rc<str>>) {
        match self {
            Pattern::Ident(n) => out.push(n.clone()),
            Pattern::Target(_) => {}
            Pattern::Object { props, rest } => {
                for p in props {
                    p.value.bound_names(out);
                }
                if let Some(r) = rest {
                    r.bound_names(out);
                }
            }
            Pattern::Array { elems, rest } => {
                for e in elems.iter().flatten() {
                    e.pattern.bound_names(out);
                }
                if let Some(r) = rest {
                    r.bound_names(out);
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub pattern: Pattern,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub enum FuncBody {
    Block(Vec<Stmt>),
    Expr(Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuncKind {
    Normal,
    Arrow,
    Method,
    StaticMethod,
    ClassConstructor,
    DerivedConstructor,
}

#[derive(Debug)]
pub struct EvalCtx {
    pub privates: Vec<(u32, Rc<Vec<Rc<str>>>)>,
    pub in_function: bool,
    pub field_init: bool,
    pub strict: bool,
}

#[derive(Clone, Debug)]
pub struct FuncDef {
    pub name: Option<Rc<str>>,
    pub params: Vec<Param>,
    pub rest: Option<Pattern>,
    pub body: FuncBody,
    pub kind: FuncKind,
    pub is_async: bool,
    pub is_generator: bool,
    pub uses_arguments: bool,
    pub strict: bool,
    pub line: u32,
    pub span: (u32, u32),
}

#[derive(Clone, Debug)]
pub struct ClassDef {
    pub name: Option<Rc<str>>,
    pub superclass: Option<Expr>,
    pub constructor: Option<Rc<FuncDef>>,
    pub members: Vec<ClassMember>,
    pub line: u32,
    pub span: (u32, u32),
}

#[derive(Clone, Debug)]
pub enum ClassMember {
    Method {
        key: PropKey,
        func: Rc<FuncDef>,
        is_static: bool,
    },
    Field {
        key: PropKey,
        value: Option<Expr>,
        is_static: bool,
    },
    Accessor {
        key: PropKey,
        func: Rc<FuncDef>,
        is_static: bool,
        setter: bool,
    },
    StaticBlock(Rc<FuncDef>),
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeclKind {
    Let,
    Const,
    Var,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Expr(Expr),
    Decl(DeclKind, Vec<(Pattern, Option<Expr>)>),
    Function(Rc<FuncDef>),
    Class(Rc<ClassDef>),
    Return(Option<Expr>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    Block(Vec<Stmt>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For {
        init: Option<Box<Stmt>>,
        test: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    ForOf {
        decl: Option<DeclKind>,
        pattern: Pattern,
        iter: Expr,
        body: Box<Stmt>,
        is_await: bool,
    },
    ForIn {
        decl: Option<DeclKind>,
        pattern: Pattern,
        obj: Expr,
        body: Box<Stmt>,
    },
    Break(Option<Rc<str>>),
    Continue(Option<Rc<str>>),
    Labeled(Rc<str>, Box<Stmt>),
    Throw(Expr),
    Try {
        block: Vec<Stmt>,
        param: Option<Pattern>,
        handler: Option<Vec<Stmt>>,
        finalizer: Option<Vec<Stmt>>,
    },
    Switch(Expr, Vec<SwitchCase>),
    Empty,
    Import(ImportDecl),
    Export(ExportDecl),
}

#[derive(Clone, Debug)]
pub struct SwitchCase {
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub source: Rc<str>,
    pub default: Option<Rc<str>>,
    pub namespace: Option<Rc<str>>,
    pub named: Vec<(Rc<str>, Rc<str>)>,
}

#[derive(Clone, Debug)]
pub enum ExportDecl {
    Decl(Box<Stmt>),
    Names(Vec<(Rc<str>, Rc<str>)>),
    From {
        source: Rc<str>,
        names: Option<Vec<(Rc<str>, Rc<str>)>>,
    },
    Default(Expr),
    DefaultDecl(Box<Stmt>),
}
