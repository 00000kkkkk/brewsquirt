use crate::ast::*;
use crate::jsstr::JsStr;

use crate::number::number_to_string;
use crate::token::{P, SyntaxError, TemplatePart, Tok, Token};
use std::rc::Rc;

type R<T> = Result<T, SyntaxError>;

mod decorators;
mod jsx;
mod std_decorators;
mod ts;

pub use decorators::TsOptions;
pub use jsx::JsxOptions;

const RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "await",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    "static",
];

pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name)
}

const STRICT_RESERVED: &[&str] = &[
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    "static",
];

type ExportList = Vec<(JsStr, JsStr)>;

pub fn parse(src: &str) -> R<Vec<Stmt>> {
    parse_goal(src, false, false, None)
}

pub fn parse_script(src: &str) -> R<Vec<Stmt>> {
    parse_goal(src, true, false, None)
}

pub fn parse_ts(src: &str, script: bool) -> R<Vec<Stmt>> {
    parse_goal(src, script, true, None)
}

pub fn parse_full(src: &str, script: bool, ts: bool, jsx: Option<JsxOptions>) -> R<Vec<Stmt>> {
    parse_goal_with(src, script, ts, TsOptions::default(), jsx)
}

pub fn parse_with_options(
    src: &str,
    script: bool,
    ts: bool,
    ts_opts: TsOptions,
    jsx: Option<JsxOptions>,
) -> R<Vec<Stmt>> {
    parse_goal_with(src, script, ts, ts_opts, jsx)
}

fn parse_goal(src: &str, script: bool, ts: bool, jsx: Option<JsxOptions>) -> R<Vec<Stmt>> {
    parse_goal_with(src, script, ts, TsOptions::default(), jsx)
}

fn parse_goal_with(
    src: &str,
    script: bool,
    ts: bool,
    ts_opts: TsOptions,
    jsx: Option<JsxOptions>,
) -> R<Vec<Stmt>> {
    let jsx = jsx.map(|mut o| {
        o.apply_pragmas(src);
        o
    });
    let toks = crate::lexer::tokenize_jsx(src, script, jsx.is_some(), ts)?;
    let mut parser = Parser {
        pairs: bracket_pairs(&toks),
        script,
        src: src.into(),
        toks,
        pos: 0,
        no_in: false,
        fn_depth: 0,
        args_used: Vec::new(),
        span_start: None,
        in_async: false,
        in_generator: false,
        class_ids: Vec::new(),
        field_init: 0,
        nt_ok: false,
        strict: !script,
        static_block: false,
        in_params: false,
        single: false,
        labeled_fn: false,
        saw_eval: false,
        last_paren: (usize::MAX, usize::MAX),
        at_top: false,
        using_ok: false,
        using_scope: None,
        using_ids: 0,
        ts: false,
        ts_splits: Vec::new(),
        ts_refs: rustc_hash::FxHashSet::default(),
        ts_props: Vec::new(),
        ts_last_props: Vec::new(),
        ts_bodyless: false,
        ts_ambient: 0,
        ts_in_extends: false,
        ts_arrow_ret: false,
        ts_opts: TsOptions::default(),
        ts_pending_decorators: Vec::new(),
        ts_param_decs: Vec::new(),
        ts_param_types: Vec::new(),
        ts_last_param_decs: Vec::new(),
        ts_last_param_types: Vec::new(),
        ts_last_return: None,
        ts_class_stack: Vec::new(),
        std_stack: Vec::new(),
        std_default_name: false,
        ts_class_iife: None,
        jsx: None,
        jsx_used: false,
    };
    parser.ts = ts;
    parser.ts_opts = ts_opts;
    parser.jsx = jsx;
    parser.strict = !script || parser.has_use_strict_from(0);
    if script && parser.strict {
        parser.check_prologue_octal(0)?;
    }
    parser.program()
}

pub fn parse_eval(src: &str, ctx: &EvalCtx) -> R<Vec<Stmt>> {
    let toks = crate::lexer::tokenize_goal(src, true)?;
    let mut parser = Parser {
        pairs: bracket_pairs(&toks),
        script: true,
        src: src.into(),
        toks,
        pos: 0,
        no_in: false,
        fn_depth: 0,
        args_used: vec![false],
        span_start: None,
        in_async: false,
        in_generator: false,
        class_ids: ctx.privates.clone(),
        field_init: u32::from(ctx.field_init),
        nt_ok: ctx.in_function,
        strict: ctx.strict,
        static_block: false,
        in_params: false,
        single: false,
        labeled_fn: false,
        saw_eval: false,
        last_paren: (usize::MAX, usize::MAX),
        at_top: false,
        using_ok: false,
        using_scope: None,
        using_ids: 0,
        ts: false,
        ts_splits: Vec::new(),
        ts_refs: rustc_hash::FxHashSet::default(),
        ts_props: Vec::new(),
        ts_last_props: Vec::new(),
        ts_bodyless: false,
        ts_ambient: 0,
        ts_in_extends: false,
        ts_arrow_ret: false,
        ts_opts: TsOptions::default(),
        ts_pending_decorators: Vec::new(),
        ts_param_decs: Vec::new(),
        ts_param_types: Vec::new(),
        ts_last_param_decs: Vec::new(),
        ts_last_param_types: Vec::new(),
        ts_last_return: None,
        ts_class_stack: Vec::new(),
        std_stack: Vec::new(),
        std_default_name: false,
        ts_class_iife: None,
        jsx: None,
        jsx_used: false,
    };
    parser.strict = ctx.strict || parser.has_use_strict_from(0);
    if parser.strict {
        parser.check_prologue_octal(0)?;
    }
    parser.program()
}

thread_local! {
    static CLASS_IDS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn fresh_class_id() -> u32 {
    CLASS_IDS.with(|c| {
        let n = c.get() + 1;
        c.set(n);
        n
    })
}

enum BinKind {
    Bin(BinOp),
    Logical(LogicalOp),
}

fn bracket_pairs(toks: &[Token]) -> Vec<u32> {
    let mut pairs = vec![u32::MAX; toks.len()];
    let mut open = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        match t.tok {
            Tok::Punct(P::LParen) | Tok::Punct(P::LBracket) | Tok::Punct(P::LBrace) => open.push(i),
            Tok::Punct(P::RParen) | Tok::Punct(P::RBracket) | Tok::Punct(P::RBrace) => {
                if let Some(o) = open.pop() {
                    pairs[o] = i as u32;
                }
            }
            _ => {}
        }
    }
    pairs
}

struct Parser {
    pairs: Vec<u32>,
    toks: Vec<Token>,
    pos: usize,
    no_in: bool,
    fn_depth: u32,
    in_async: bool,
    in_generator: bool,
    class_ids: Vec<(u32, Rc<Vec<JsStr>>)>,
    field_init: u32,
    script: bool,
    nt_ok: bool,
    strict: bool,
    static_block: bool,
    in_params: bool,
    single: bool,
    labeled_fn: bool,
    saw_eval: bool,
    src: JsStr,
    last_paren: (usize, usize),
    at_top: bool,
    args_used: Vec<bool>,
    span_start: Option<u32>,
    using_ok: bool,
    using_scope: Option<(u32, bool)>,
    using_ids: u32,
    ts: bool,
    ts_splits: Vec<(usize, Token)>,
    ts_refs: rustc_hash::FxHashSet<JsStr>,
    ts_props: Vec<JsStr>,
    ts_last_props: Vec<JsStr>,
    ts_bodyless: bool,
    ts_ambient: u32,
    ts_in_extends: bool,
    ts_arrow_ret: bool,
    ts_opts: TsOptions,
    ts_pending_decorators: Vec<Expr>,
    ts_param_decs: Vec<(usize, Expr)>,
    ts_param_types: Vec<Expr>,
    ts_last_param_decs: Vec<(usize, Expr)>,
    ts_last_param_types: Vec<Expr>,
    ts_last_return: Option<Expr>,
    ts_class_stack: Vec<decorators::ClassDecorations>,
    std_stack: Vec<std_decorators::StdClass>,
    std_default_name: bool,
    ts_class_iife: Option<Expr>,
    jsx: Option<JsxOptions>,
    jsx_used: bool,
}

impl Parser {
    fn reserved(&self, name: &str) -> bool {
        match name {
            "let" => self.strict,
            "yield" => self.strict || self.in_generator,
            _ => is_reserved(name) && (self.strict || !STRICT_RESERVED.contains(&name)),
        }
    }

    fn relex(&mut self, as_regex: bool) -> R<()> {
        let t = &self.toks[self.pos];
        let (start, line, col, nl) = (t.start, t.line, t.col, t.nl_before);
        let mut new = crate::lexer::tokenize_from(
            &self.src,
            self.script,
            start,
            line,
            col,
            as_regex,
            (self.jsx.is_some(), self.ts),
        )?;
        if let Some(first) = new.first_mut() {
            first.nl_before = nl;
        }
        self.toks.truncate(self.pos);
        self.toks.append(&mut new);
        self.pairs = bracket_pairs(&self.toks);
        Ok(())
    }

    fn sub_statement(&mut self) -> R<Stmt> {
        let saved = std::mem::replace(&mut self.single, true);
        let r = self.statement();
        self.single = saved;
        r
    }

    fn if_body(&mut self) -> R<Stmt> {
        let before = self.labeled_fn;
        self.labeled_fn = false;
        let s = self.sub_statement()?;
        let labeled = std::mem::replace(&mut self.labeled_fn, before);
        if let StmtKind::Function(f) = &s.kind {
            if self.strict || f.is_generator || f.is_async || labeled {
                return Err(SyntaxError::new(
                    "Function declarations are not allowed in this position",
                    s.line,
                    1,
                ));
            }
            let line = s.line;
            return Ok(Stmt {
                kind: StmtKind::Block(vec![s]),
                line,
            });
        }
        Ok(s)
    }

    fn prev_end(&self) -> u32 {
        if self.pos == 0 {
            0
        } else {
            self.toks[self.pos - 1].end
        }
    }

    fn peek(&self) -> &Token {
        &self.toks[self.pos]
    }

    fn peek_at(&self, n: usize) -> &Token {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)]
    }

    fn advance(&mut self) {
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
    }

    fn advance_token(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        self.advance();
        t
    }

    fn is(&self, p: P) -> bool {
        matches!(self.peek().tok, Tok::Punct(q) if q == p)
    }

    fn is_at(&self, n: usize, p: P) -> bool {
        matches!(self.peek_at(n).tok, Tok::Punct(q) if q == p)
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek().tok, Tok::Eof)
    }

    fn eat(&mut self, p: P) -> bool {
        if self.is(p) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: P) -> R<()> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    fn is_kw(&self, kw: &str) -> bool {
        let t = self.peek();
        !t.escaped && matches!(&t.tok, Tok::Ident(s) if &**s == kw)
    }

    fn is_kw_at(&self, n: usize, kw: &str) -> bool {
        let t = self.peek_at(n);
        !t.escaped && matches!(&t.tok, Tok::Ident(s) if &**s == kw)
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, kw: &str) -> R<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    fn unexpected(&self) -> SyntaxError {
        let t = self.peek();
        match t.tok {
            Tok::Eof => SyntaxError::new("Unexpected end of input", t.line, t.col),
            _ => SyntaxError::new(
                format!("Unexpected token '{}'", t.describe()),
                t.line,
                t.col,
            ),
        }
    }

    fn error(&self, msg: impl Into<String>) -> SyntaxError {
        let t = self.peek();
        SyntaxError::new(msg, t.line, t.col)
    }

    fn line(&self) -> u32 {
        self.peek().line
    }

    fn pos(&self) -> (u32, u32) {
        let t = self.peek();
        (t.line, t.col)
    }

    fn word(&self) -> Option<JsStr> {
        match &self.peek().tok {
            Tok::Ident(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn private_name(&mut self) -> R<JsStr> {
        if self.class_ids.is_empty() {
            return Err(self.error("Private field must be declared in an enclosing class"));
        }
        let hash_end = self.peek().end;
        self.advance();
        let adjacent = self.peek().start == hash_end;
        let name = match self.word() {
            Some(name) if adjacent => name,
            _ => return Err(self.unexpected()),
        };
        self.advance();
        for (id, names) in self.class_ids.iter().rev() {
            if names.contains(&name) {
                return Ok(format!("\u{0}#{name}@{id}").into());
            }
        }
        Err(self.error(format!(
            "Private field '#{name}' must be declared in an enclosing class"
        )))
    }

    fn prescan_private(&self) -> Vec<JsStr> {
        let mut out: Vec<JsStr> = Vec::new();
        let mut depth = 0i32;
        let mut i = self.pos;
        while let Some(t) = self.toks.get(i) {
            match &t.tok {
                Tok::Eof => break,
                Tok::Punct(P::LBrace) | Tok::Punct(P::LParen) | Tok::Punct(P::LBracket) => {
                    depth += 1
                }
                Tok::Punct(P::RBrace) | Tok::Punct(P::RParen) | Tok::Punct(P::RBracket) => {
                    depth -= 1;
                    if depth <= 0 {
                        break;
                    }
                }
                Tok::Punct(P::Hash) if depth == 1 && i > 0 => {
                    if let Some(Tok::Ident(n)) = self.toks.get(i + 1).map(|x| &x.tok) {
                        let prev = &self.toks[i - 1];
                        let decl = t.nl_before
                            || matches!(
                                prev.tok,
                                Tok::Punct(P::LBrace)
                                    | Tok::Punct(P::Semi)
                                    | Tok::Punct(P::RBrace)
                                    | Tok::Punct(P::Star)
                            )
                            || matches!(&prev.tok, Tok::Ident(w) if !prev.escaped
                                && (matches!(&**w, "static" | "get" | "set" | "async" | "accessor")
                                    || (self.ts && matches!(&**w, "readonly" | "declare" | "abstract" | "override"))))
                            || self.decorated_member_at(i);
                        if decl && !out.contains(n) {
                            out.push(n.clone());
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
        out
    }

    fn decorated_member_at(&self, hash: usize) -> bool {
        let mut depth = 0i32;
        let mut j = hash;
        let mut steps = 0;
        while j > 0 && steps < 256 {
            j -= 1;
            steps += 1;
            match &self.toks[j].tok {
                Tok::Punct(P::RParen) | Tok::Punct(P::RBracket) => depth += 1,
                Tok::Punct(P::LParen) | Tok::Punct(P::LBracket) => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                Tok::Punct(P::At) if depth == 0 => return true,
                Tok::Punct(P::Semi)
                | Tok::Punct(P::LBrace)
                | Tok::Punct(P::RBrace)
                | Tok::Punct(P::Eq)
                    if depth == 0 =>
                {
                    return false;
                }
                _ => {}
            }
        }
        false
    }

    fn ident_name(&mut self) -> R<JsStr> {
        match self.word() {
            Some(s) => {
                self.advance();
                Ok(s)
            }
            None if self.is(P::Hash) => self.private_name(),
            None => Err(self.unexpected()),
        }
    }

    fn check_binding_name(&self, s: &str) -> R<()> {
        if self.strict && matches!(s, "eval" | "arguments") {
            return Err(self.error("Unexpected eval or arguments in strict mode"));
        }
        if s == "await" && self.static_block {
            return Err(self.error("Unexpected reserved word 'await'"));
        }
        if self.reserved(s) && !(s == "await" && self.script && !self.in_async) {
            if self.strict
                && matches!(
                    s,
                    "let"
                        | "static"
                        | "implements"
                        | "interface"
                        | "package"
                        | "private"
                        | "protected"
                        | "public"
                        | "yield"
                )
            {
                return Err(self.error("Unexpected strict mode reserved word"));
            }
            return Err(self.error(format!("Unexpected reserved word '{s}'")));
        }
        Ok(())
    }

    fn binding_ident(&mut self) -> R<JsStr> {
        match self.word() {
            Some(s)
                if self.reserved(&s)
                    && self.peek().escaped
                    && !(&*s == "await" && self.script && !self.in_async && !self.static_block) =>
            {
                Err(self.error("Keyword must not contain escaped characters"))
            }
            Some(s) => {
                self.check_binding_name(&s)?;
                self.advance();
                Ok(s)
            }
            None => Err(self.unexpected()),
        }
    }

    fn import_attributes(&mut self) -> R<()> {
        if !(self.is_kw("with") || (self.is_kw("assert") && self.is_at(1, P::LBrace))) {
            return Ok(());
        }
        self.advance();
        self.expect(P::LBrace)?;
        while !self.is(P::RBrace) {
            if matches!(self.peek().tok, Tok::Str(_)) {
                self.string_lit()?;
            } else {
                self.ident_name()?;
            }
            self.expect(P::Colon)?;
            self.string_lit()?;
            if !self.eat(P::Comma) {
                break;
            }
        }
        self.expect(P::RBrace)?;
        Ok(())
    }

    fn string_lit(&mut self) -> R<JsStr> {
        match &self.peek().tok {
            Tok::Str(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            _ => Err(self.unexpected()),
        }
    }

    fn module_name(&mut self) -> R<JsStr> {
        match &self.peek().tok {
            Tok::Str(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            _ => self.ident_name(),
        }
    }

    fn semicolon(&mut self) -> R<()> {
        if self.eat(P::Semi) || self.is(P::RBrace) || self.is_eof() || self.peek().nl_before {
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    fn program(&mut self) -> R<Vec<Stmt>> {
        self.using_ok = !self.script;
        let mut out = Vec::new();
        while !self.is_eof() {
            self.at_top = true;
            out.push(self.statement()?);
        }
        if self.ts {
            self.ts_elide_imports(&mut out);
        }
        if let Some(s) = self.jsx_runtime_import() {
            out.insert(0, s);
        }
        if let Some((id, is_async)) = self.using_scope.take() {
            let line = self.line();
            let res = using_res(id);
            out.insert(
                0,
                syn_stmt(
                    StmtKind::Decl(
                        DeclKind::Const,
                        vec![(
                            Pattern::Ident(res.as_str().into()),
                            Some(syn_call(syn_ident("%usingStack%", line), Vec::new(), line)),
                        )],
                    ),
                    line,
                ),
            );
            if is_async {
                out.extend(using_async_finalizer(&res, id, line));
            } else {
                out.push(syn_stmt(
                    StmtKind::Expr(syn_call(
                        syn_ident("%usingDispose%", line),
                        vec![syn_ident(&res, line)],
                        line,
                    )),
                    line,
                ));
            }
        }
        Ok(out)
    }

    fn block(&mut self) -> R<Vec<Stmt>> {
        let line = self.line();
        self.expect(P::LBrace)?;
        let saved_ok = std::mem::replace(&mut self.using_ok, true);
        let saved_scope = self.using_scope.take();
        let items = self.block_items();
        let scope = std::mem::replace(&mut self.using_scope, saved_scope);
        self.using_ok = saved_ok;
        let items = items?;
        Ok(match scope {
            Some((id, is_async)) => using_wrap(items, id, is_async, line, false),
            None => items,
        })
    }

    fn block_items(&mut self) -> R<Vec<Stmt>> {
        let mut out = Vec::new();
        while !self.eat(P::RBrace) {
            if self.is_eof() {
                return Err(self.unexpected());
            }
            out.push(self.statement()?);
        }
        Ok(out)
    }

    fn await_ok(&self) -> bool {
        self.in_async || (!self.script && self.fn_depth == 0 && !self.static_block)
    }

    fn using_head(&self, for_head: bool) -> Option<bool> {
        let (off, is_async) = if self.is_kw("using") {
            (0, false)
        } else if self.is_kw("await")
            && self.await_ok()
            && self.is_kw_at(1, "using")
            && !self.peek_at(1).nl_before
        {
            (1, true)
        } else {
            return None;
        };
        let t = self.peek_at(off + 1);
        let Tok::Ident(name) = &t.tok else {
            return None;
        };
        if t.nl_before || (!t.escaped && matches!(&**name, "in" | "instanceof")) {
            return None;
        }
        if for_head && !is_async && !t.escaped && &**name == "of" && !self.is_at(off + 2, P::Eq) {
            return None;
        }
        Some(is_async)
    }

    fn using_id(&mut self, is_async: bool) -> u32 {
        if let Some((id, a)) = &mut self.using_scope {
            *a |= is_async;
            return *id;
        }
        self.using_ids += 1;
        let id = self.using_ids;
        self.using_scope = Some((id, is_async));
        id
    }

    fn using_slot(
        &mut self,
        decls: &mut Vec<(Pattern, Option<Expr>)>,
        name: &JsStr,
        id: u32,
        is_async: bool,
        line: u32,
    ) {
        self.using_ids += 1;
        let slot = format!("%usingv{}%", self.using_ids);
        let reg = syn_call(
            syn_ident("%using%", line),
            vec![
                syn_ident(&using_res(id), line),
                syn_ident(name, line),
                Expr {
                    kind: ExprKind::Bool(is_async),
                    line,
                    col: 0,
                },
            ],
            line,
        );
        decls.push((Pattern::Ident(slot.as_str().into()), Some(reg)));
    }

    fn using_name(&mut self) -> R<JsStr> {
        let name = self.binding_ident()?;
        if &*name == "let" {
            return Err(self.error("let is disallowed as a lexically bound name"));
        }
        Ok(name)
    }

    fn using_decl(&mut self, is_async: bool) -> R<StmtKind> {
        let line = self.line();
        if is_async {
            self.advance();
        }
        self.advance();
        let id = self.using_id(is_async);
        let mut decls = Vec::new();
        loop {
            let name = self.using_name()?;
            if !self.eat(P::Eq) {
                return Err(self.error("Missing initializer in using declaration"));
            }
            let init = self.assignment()?;
            decls.push((Pattern::Ident(name.clone()), Some(init)));
            self.using_slot(&mut decls, &name, id, is_async, line);
            if !self.eat(P::Comma) {
                break;
            }
        }
        Ok(StmtKind::Decl(DeclKind::Const, decls))
    }

    fn for_using(&mut self, for_await: bool, is_async: bool, line: u32) -> R<StmtKind> {
        if is_async {
            self.advance();
        }
        self.advance();
        self.using_ids += 1;
        let id = self.using_ids;
        let mut name = self.using_name()?;
        if self.eat_kw("of") {
            let iter = self.assignment()?;
            self.expect(P::RParen)?;
            let body = self.sub_statement()?;
            if matches!(
                body.kind,
                StmtKind::Decl(..) | StmtKind::Class(_) | StmtKind::Function(_)
            ) {
                return Err(SyntaxError::new(
                    "Lexical declaration cannot appear in a single-statement context",
                    body.line,
                    1,
                ));
            }
            let mut decls = Vec::new();
            self.using_slot(&mut decls, &name, id, is_async, line);
            let reg = syn_stmt(StmtKind::Decl(DeclKind::Const, decls), line);
            let inner = using_wrap(vec![reg, body], id, is_async, line, false);
            return Ok(StmtKind::ForOf {
                decl: Some(DeclKind::Const),
                pattern: Pattern::Ident(name),
                iter,
                body: Box::new(syn_stmt(StmtKind::Block(inner), line)),
                is_await: for_await,
            });
        }
        if self.is_kw("in") {
            return Err(
                self.error("The left-hand side of a for-in loop may not be a using declaration")
            );
        }
        let saved = self.no_in;
        self.no_in = true;
        let mut decls = Vec::new();
        loop {
            if !self.eat(P::Eq) {
                self.no_in = saved;
                return Err(self.error("Missing initializer in using declaration"));
            }
            let init = self.assignment()?;
            decls.push((Pattern::Ident(name.clone()), Some(init)));
            self.using_slot(&mut decls, &name, id, is_async, line);
            if !self.eat(P::Comma) {
                break;
            }
            name = self.using_name()?;
        }
        self.no_in = saved;
        self.expect(P::Semi)?;
        let test = if self.is(P::Semi) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(P::Semi)?;
        let update = if self.is(P::RParen) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(P::RParen)?;
        let body = self.sub_statement()?;
        let decl = syn_stmt(StmtKind::Decl(DeclKind::Const, decls), line);
        let lp = syn_stmt(
            StmtKind::For {
                init: None,
                test,
                update,
                body: Box::new(body),
            },
            line,
        );
        Ok(StmtKind::Block(using_wrap(
            vec![decl, lp],
            id,
            is_async,
            line,
            true,
        )))
    }

    fn statement(&mut self) -> R<Stmt> {
        let line = self.line();
        let kind = self.statement_kind()?;
        Ok(Stmt { kind, line })
    }

    fn statement_kind(&mut self) -> R<StmtKind> {
        if self.is(P::At) {
            let decs = self.ts_decorators()?;
            self.ts_pending_decorators.extend(decs);
        }
        let top = std::mem::take(&mut self.at_top);
        let single = std::mem::take(&mut self.single);
        if self.is(P::LBrace) {
            return Ok(StmtKind::Block(self.block()?));
        }
        if self.eat(P::Semi) {
            return Ok(StmtKind::Empty);
        }
        let word = if self.peek().escaped {
            match self.word() {
                Some(w)
                    if self.is_at(1, P::Colon)
                        && (!self.reserved(&w)
                            || (&*w == "await"
                                && self.script
                                && !self.in_async
                                && !self.static_block)) =>
                {
                    Some(w)
                }
                _ => None,
            }
        } else {
            self.word()
        };
        if self.ts
            && !single
            && let Some(w) = word.clone()
            && let Some(kind) = self.ts_statement(&w)?
        {
            return Ok(kind);
        }
        match word.as_deref() {
            Some("using") | Some("await") if self.using_head(false).is_some() => {
                let is_async = self.using_head(false) == Some(true);
                if single {
                    return Err(self
                        .error("Lexical declaration cannot appear in a single-statement context"));
                }
                if !self.using_ok {
                    return Err(self.error(if is_async {
                        "await using declarations are not allowed in this position"
                    } else {
                        "using declarations are not allowed in this position"
                    }));
                }
                let s = self.using_decl(is_async)?;
                self.semicolon()?;
                Ok(s)
            }
            Some("let") | Some("const") | Some("var")
                if matches!(
                    self.peek_at(1).tok,
                    Tok::Ident(_) | Tok::Punct(P::LBracket) | Tok::Punct(P::LBrace)
                ) && !(single
                    && self.is_kw("let")
                    && !self.is_at(1, P::LBracket)
                    && self.peek_at(1).nl_before) =>
            {
                let s = self.var_decl()?;
                self.semicolon()?;
                Ok(s)
            }
            Some("function") => {
                let line = self.line();
                self.span_start = Some(self.peek().start);
                self.advance();
                let is_generator = self.eat(P::Star);
                let name = self.binding_ident()?;
                let f =
                    self.function_full(Some(name), FuncKind::Normal, line, false, is_generator)?;
                if std::mem::take(&mut self.ts_bodyless) {
                    return Ok(StmtKind::Empty);
                }
                Ok(StmtKind::Function(f))
            }
            Some("async") if self.is_kw_at(1, "function") && !self.peek_at(1).nl_before => {
                let line = self.line();
                self.span_start = Some(self.peek().start);
                self.advance();
                self.advance();
                let is_generator = self.eat(P::Star);
                let name = self.binding_ident()?;
                let f =
                    self.function_full(Some(name), FuncKind::Normal, line, true, is_generator)?;
                if std::mem::take(&mut self.ts_bodyless) {
                    return Ok(StmtKind::Empty);
                }
                Ok(StmtKind::Function(f))
            }
            Some("class") => self.class_statement(),
            Some("if") => {
                self.advance();
                self.expect(P::LParen)?;
                let test = self.expression()?;
                self.expect(P::RParen)?;
                let cons = self.if_body()?;
                let alt = if self.eat_kw("else") {
                    Some(Box::new(self.if_body()?))
                } else {
                    None
                };
                Ok(StmtKind::If(test, Box::new(cons), alt))
            }
            Some("while") => {
                self.advance();
                self.expect(P::LParen)?;
                let test = self.expression()?;
                self.expect(P::RParen)?;
                let body = self.sub_statement()?;
                Ok(StmtKind::While(test, Box::new(body)))
            }
            Some("do") => {
                self.advance();
                let body = self.sub_statement()?;
                self.expect_kw("while")?;
                self.expect(P::LParen)?;
                let test = self.expression()?;
                self.expect(P::RParen)?;
                self.eat(P::Semi);
                Ok(StmtKind::DoWhile(Box::new(body), test))
            }
            Some("for") => self.for_statement(),
            Some("return") => {
                if self.fn_depth == 0 || self.static_block {
                    return Err(self.error("Illegal return statement"));
                }
                self.advance();
                let value = if self.is(P::Semi)
                    || self.is(P::RBrace)
                    || self.is_eof()
                    || self.peek().nl_before
                {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.semicolon()?;
                Ok(StmtKind::Return(value))
            }
            Some("break") => {
                self.advance();
                let label = self.jump_label()?;
                self.semicolon()?;
                Ok(StmtKind::Break(label))
            }
            Some("continue") => {
                self.advance();
                let label = self.jump_label()?;
                self.semicolon()?;
                Ok(StmtKind::Continue(label))
            }
            Some("throw") => {
                self.advance();
                if self.peek().nl_before {
                    return Err(self.error("Illegal newline after throw"));
                }
                let e = self.expression()?;
                self.semicolon()?;
                Ok(StmtKind::Throw(e))
            }
            Some("try") => {
                self.advance();
                let block = self.block()?;
                let mut param = None;
                let mut handler = None;
                if self.eat_kw("catch") {
                    if self.eat(P::LParen) {
                        let p = self.binding_pattern()?;
                        if self.ts {
                            self.ts_annotation()?;
                        }
                        let mut names = Vec::new();
                        p.bound_names(&mut names);
                        for (i, n) in names.iter().enumerate() {
                            if names[..i].contains(n) {
                                return Err(self
                                    .error(format!("Identifier '{n}' has already been declared")));
                            }
                        }
                        param = Some(p);
                        self.expect(P::RParen)?;
                    }
                    handler = Some(self.block()?);
                }
                let finalizer = if self.eat_kw("finally") {
                    Some(self.block()?)
                } else {
                    None
                };
                if handler.is_none() && finalizer.is_none() {
                    return Err(self.error("Missing catch or finally after try"));
                }
                Ok(StmtKind::Try {
                    block,
                    param,
                    handler,
                    finalizer,
                })
            }
            Some("switch") => {
                self.advance();
                self.expect(P::LParen)?;
                let disc = self.expression()?;
                self.expect(P::RParen)?;
                self.expect(P::LBrace)?;
                let saved_using = std::mem::replace(&mut self.using_ok, false);
                let mut cases = Vec::new();
                let mut seen_default = false;
                while !self.eat(P::RBrace) {
                    let test = if self.eat_kw("case") {
                        Some(self.expression()?)
                    } else if self.eat_kw("default") {
                        if seen_default {
                            return Err(
                                self.error("More than one default clause in switch statement")
                            );
                        }
                        seen_default = true;
                        None
                    } else {
                        return Err(self.unexpected());
                    };
                    self.expect(P::Colon)?;
                    let mut body = Vec::new();
                    while !self.is_kw("case") && !self.is_kw("default") && !self.is(P::RBrace) {
                        if self.is_eof() {
                            return Err(self.unexpected());
                        }
                        body.push(self.statement()?);
                    }
                    cases.push(SwitchCase { test, body });
                }
                self.using_ok = saved_using;
                Ok(StmtKind::Switch(disc, cases))
            }
            Some("import") if !self.is_at(1, P::LParen) && !self.is_at(1, P::Dot) => {
                if self.script || !top {
                    return Err(self.error("Cannot use import statement outside a module"));
                }
                self.import()
            }
            Some("export") => {
                if self.script || !top {
                    return Err(self.error("Unexpected token 'export'"));
                }
                self.export()
            }
            Some("debugger") => {
                self.advance();
                self.semicolon()?;
                Ok(StmtKind::Empty)
            }
            Some("with") => {
                if self.strict {
                    return Err(self.error("Strict mode code may not include a with statement"));
                }
                self.advance();
                self.expect(P::LParen)?;
                let saved = self.no_in;
                self.no_in = false;
                let obj = self.expression();
                self.no_in = saved;
                let obj = obj?;
                self.expect(P::RParen)?;
                let body = self.sub_statement()?;
                Ok(StmtKind::With(obj, Box::new(body)))
            }
            Some(w)
                if self.is_at(1, P::Colon)
                    && (!self.reserved(w)
                        || (w == "await"
                            && self.script
                            && !self.in_async
                            && !self.static_block)) =>
            {
                let label: JsStr = w.into();
                self.advance();
                self.advance();
                let mut body = self.sub_statement()?;
                if let Some(slot) = using_loop(&mut body) {
                    let inner = std::mem::replace(
                        slot,
                        Stmt {
                            kind: StmtKind::Empty,
                            line: 0,
                        },
                    );
                    let l = inner.line;
                    *slot = Stmt {
                        kind: StmtKind::Labeled(label, Box::new(inner)),
                        line: l,
                    };
                    return Ok(body.kind);
                }
                if let StmtKind::Function(f) = &body.kind {
                    if self.strict || f.is_generator || f.is_async {
                        return Err(
                            self.error("Labelled function declarations are not allowed here")
                        );
                    }
                    self.labeled_fn = true;
                    return Ok(body.kind);
                }
                Ok(StmtKind::Labeled(label, Box::new(body)))
            }
            _ => {
                let first = self.pos;
                let e = self.expression()?;
                self.semicolon()?;
                let directive_like = matches!(&e.kind, ExprKind::Str(t) if &**t == "use strict");
                if directive_like {
                    let t = &self.toks[first];
                    let raw = self.src.get(t.start as usize..t.end as usize);
                    if raw != Some("\"use strict\"") && raw != Some("'use strict'") {
                        let line = e.line;
                        return Ok(StmtKind::Expr(Expr {
                            kind: ExprKind::Seq(vec![e]),
                            line,
                            col: 0,
                        }));
                    }
                }
                Ok(StmtKind::Expr(e))
            }
        }
    }

    fn check_lexical_name(&self, p: &Pattern) -> R<()> {
        let mut names = Vec::new();
        p.bound_names(&mut names);
        if names.iter().any(|n| &**n == "let") {
            return Err(self.error("let is disallowed as a lexically bound name"));
        }
        Ok(())
    }

    fn var_decl(&mut self) -> R<StmtKind> {
        let kind = if self.is_kw("const") {
            DeclKind::Const
        } else if self.is_kw("var") {
            DeclKind::Var
        } else {
            DeclKind::Let
        };
        self.advance();
        let mut decls = Vec::new();
        loop {
            let pattern = self.binding_pattern()?;
            if kind != DeclKind::Var {
                self.check_lexical_name(&pattern)?;
            }
            if self.ts {
                if self.is(P::Bang) && !self.peek().nl_before {
                    self.advance();
                }
                self.ts_annotation()?;
            }
            let init = if self.eat(P::Eq) {
                Some(self.assignment()?)
            } else {
                None
            };
            if init.is_none() && self.ts_ambient == 0 {
                if kind == DeclKind::Const {
                    return Err(self.error("Missing initializer in const declaration"));
                }
                if !matches!(pattern, Pattern::Ident(_)) {
                    return Err(self.error("Missing initializer in destructuring declaration"));
                }
            }
            decls.push((pattern, init));
            if !self.eat(P::Comma) {
                break;
            }
        }
        Ok(StmtKind::Decl(kind, decls))
    }

    fn for_statement(&mut self) -> R<StmtKind> {
        self.advance();
        let is_await = self.is_kw("await");
        if is_await {
            if !self.in_async && self.fn_depth > 0 {
                return Err(self.error(
                    "for await is only valid in async functions and the top level of modules",
                ));
            }
            self.advance();
        }
        self.expect(P::LParen)?;
        let init_line = self.line();
        if let Some(is_async) = self.using_head(true) {
            return self.for_using(is_await, is_async, init_line);
        }
        let init = if (self.is_kw("let") || self.is_kw("const") || self.is_kw("var"))
            && matches!(
                self.peek_at(1).tok,
                Tok::Ident(_) | Tok::Punct(P::LBracket) | Tok::Punct(P::LBrace)
            )
            && !(self.is_kw("let") && self.is_kw_at(1, "in"))
        {
            let kind = if self.is_kw("const") {
                DeclKind::Const
            } else if self.is_kw("var") {
                DeclKind::Var
            } else {
                DeclKind::Let
            };
            self.advance();
            let pattern = self.binding_pattern()?;
            if kind != DeclKind::Var {
                self.check_lexical_name(&pattern)?;
            }
            if self.ts {
                self.ts_annotation()?;
            }
            let mut annexb_init = None;
            if kind == DeclKind::Var
                && !self.strict
                && matches!(pattern, Pattern::Ident(_))
                && self.is(P::Eq)
            {
                let saved = self.no_in;
                self.no_in = true;
                self.advance();
                let e = self.assignment()?;
                self.no_in = saved;
                if self.eat_kw("in") {
                    let obj = self.expression()?;
                    self.expect(P::RParen)?;
                    let body = self.sub_statement()?;
                    return Ok(StmtKind::Block(vec![
                        Stmt {
                            kind: StmtKind::Decl(kind, vec![(pattern.clone(), Some(e))]),
                            line: init_line,
                        },
                        Stmt {
                            kind: StmtKind::ForIn {
                                decl: Some(kind),
                                pattern,
                                obj,
                                body: Box::new(body),
                            },
                            line: init_line,
                        },
                    ]));
                }
                annexb_init = Some(e);
            }
            if annexb_init.is_none() && self.eat_kw("of") {
                let iter = self.assignment()?;
                self.expect(P::RParen)?;
                let body = self.sub_statement()?;
                return Ok(StmtKind::ForOf {
                    decl: Some(kind),
                    pattern,
                    iter,
                    body: Box::new(body),
                    is_await,
                });
            }
            if annexb_init.is_none() && self.eat_kw("in") {
                let obj = self.expression()?;
                self.expect(P::RParen)?;
                let body = self.sub_statement()?;
                return Ok(StmtKind::ForIn {
                    decl: Some(kind),
                    pattern,
                    obj,
                    body: Box::new(body),
                });
            }
            let saved = self.no_in;
            self.no_in = true;
            let mut decls = Vec::new();
            let first = match annexb_init {
                Some(e) => Some(e),
                None if self.eat(P::Eq) => Some(self.assignment()?),
                None => None,
            };
            if first.is_none() && kind == DeclKind::Const {
                return Err(self.error("Missing initializer in const declaration"));
            }
            decls.push((pattern, first));
            while self.eat(P::Comma) {
                let p = self.binding_pattern()?;
                if self.ts {
                    self.ts_annotation()?;
                }
                let i = if self.eat(P::Eq) {
                    Some(self.assignment()?)
                } else {
                    None
                };
                decls.push((p, i));
            }
            self.no_in = saved;
            self.expect(P::Semi)?;
            Some(Box::new(Stmt {
                kind: StmtKind::Decl(kind, decls),
                line: init_line,
            }))
        } else if self.eat(P::Semi) {
            None
        } else {
            let saved = self.no_in;
            self.no_in = true;
            let lhs_start = self.pos;
            let e = self.expression()?;
            self.no_in = saved;
            if self.is_kw("of") || self.is_kw("in") {
                let is_of = self.is_kw("of");
                if matches!(e.kind, ExprKind::Array(_) | ExprKind::Object(_)) {
                    self.check_rest_position(lhs_start, self.pos)?;
                }
                let plain_async = self.pos == lhs_start + 1
                    && !self.toks[lhs_start].escaped
                    && matches!(&e.kind, ExprKind::Ident(n) if &**n == "async");
                if is_of && !is_await && plain_async {
                    return Err(
                        self.error("The left-hand side of a for-of loop may not be 'async'")
                    );
                }
                self.advance();
                let pattern = if !self.strict
                    && matches!(&e.kind, ExprKind::Call { args, .. } if !is_tagged(args))
                {
                    Pattern::Target(Box::new(e))
                } else {
                    self.expr_to_pattern(e)?
                };
                let rhs = if is_of {
                    self.assignment()?
                } else {
                    self.expression()?
                };
                self.expect(P::RParen)?;
                let body = Box::new(self.statement()?);
                return Ok(if is_of {
                    StmtKind::ForOf {
                        decl: None,
                        pattern,
                        iter: rhs,
                        body,
                        is_await,
                    }
                } else {
                    StmtKind::ForIn {
                        decl: None,
                        pattern,
                        obj: rhs,
                        body,
                    }
                });
            }
            self.expect(P::Semi)?;
            Some(Box::new(Stmt {
                kind: StmtKind::Expr(e),
                line: init_line,
            }))
        };
        let test = if self.is(P::Semi) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(P::Semi)?;
        let update = if self.is(P::RParen) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(P::RParen)?;
        let body = self.sub_statement()?;
        Ok(StmtKind::For {
            init,
            test,
            update,
            body: Box::new(body),
        })
    }

    fn import(&mut self) -> R<StmtKind> {
        self.advance();
        if self.ts
            && self.is_kw("type")
            && (matches!(self.peek_at(1).tok, Tok::Ident(_)) && !self.is_kw_at(1, "from")
                || self.is_at(1, P::LBrace)
                || self.is_at(1, P::Star))
        {
            while !self.is_eof() && !self.is(P::Semi) && !matches!(self.peek().tok, Tok::Str(_)) {
                if self.is(P::LBrace) || self.is(P::LParen) {
                    self.ts_skip_group()?;
                } else {
                    self.advance();
                }
            }
            if matches!(self.peek().tok, Tok::Str(_)) {
                self.advance();
                self.import_attributes()?;
            }
            self.semicolon()?;
            return Ok(StmtKind::Empty);
        }
        if let Tok::Str(s) = &self.peek().tok {
            let source = s.clone();
            self.advance();
            self.semicolon()?;
            return Ok(StmtKind::Import(ImportDecl {
                source,
                default: None,
                namespace: None,
                named: Vec::new(),
            }));
        }
        let mut default = None;
        let mut namespace = None;
        let mut named = Vec::new();
        let mut need_more = true;
        if matches!(self.peek().tok, Tok::Ident(_)) {
            default = Some(self.binding_ident()?);
            need_more = self.eat(P::Comma);
        }
        if need_more {
            if self.eat(P::Star) {
                self.expect_kw("as")?;
                namespace = Some(self.binding_ident()?);
            } else if self.eat(P::LBrace) {
                while !self.eat(P::RBrace) {
                    if self.ts
                        && self.is_kw("type")
                        && matches!(self.peek_at(1).tok, Tok::Ident(_) | Tok::Str(_))
                        && !(self.is_kw_at(1, "as")
                            && !matches!(self.peek_at(2).tok, Tok::Ident(_)))
                    {
                        self.advance();
                        self.module_name()?;
                        if self.eat_kw("as") {
                            self.binding_ident()?;
                        }
                        if !self.eat(P::Comma) {
                            self.expect(P::RBrace)?;
                            break;
                        }
                        continue;
                    }
                    let was_string = matches!(self.peek().tok, Tok::Str(_));
                    let imported = self.module_name()?;
                    self.check_export_name(&imported)?;
                    if was_string && !self.is_kw("as") {
                        return Err(self.error("Unexpected string in import list"));
                    }
                    let local = if self.eat_kw("as") {
                        self.binding_ident()?
                    } else {
                        if is_reserved(&imported) {
                            return Err(
                                self.error(format!("Unexpected reserved word '{imported}'"))
                            );
                        }
                        if matches!(&*imported, "eval" | "arguments") {
                            return Err(self.error("Unexpected eval or arguments in strict mode"));
                        }
                        imported.clone()
                    };
                    named.push((imported, local));
                    if !self.eat(P::Comma) {
                        self.expect(P::RBrace)?;
                        break;
                    }
                }
            } else {
                return Err(self.unexpected());
            }
        }
        self.expect_kw("from")?;
        let source = self.string_lit()?;
        self.import_attributes()?;
        self.semicolon()?;
        let mut bound: Vec<&JsStr> = default.iter().chain(namespace.iter()).collect();
        bound.extend(named.iter().map(|(_, local)| local));
        for (i, n) in bound.iter().enumerate() {
            if bound[..i].contains(n) {
                return Err(self.error(format!("Identifier '{n}' has already been declared")));
            }
        }
        Ok(StmtKind::Import(ImportDecl {
            source,
            default,
            namespace,
            named,
        }))
    }

    fn export_names(&mut self) -> R<(ExportList, bool)> {
        let mut names = Vec::new();
        let mut string_local = false;
        while !self.eat(P::RBrace) {
            if self.ts
                && self.is_kw("type")
                && matches!(self.peek_at(1).tok, Tok::Ident(_) | Tok::Str(_))
                && !(self.is_kw_at(1, "as") && !matches!(self.peek_at(2).tok, Tok::Ident(_)))
            {
                self.advance();
                self.module_name()?;
                if self.eat_kw("as") {
                    self.module_name()?;
                }
                if !self.eat(P::Comma) {
                    self.expect(P::RBrace)?;
                    break;
                }
                continue;
            }
            if matches!(self.peek().tok, Tok::Str(_)) {
                string_local = true;
            }
            let local = self.module_name()?;
            if self.ts {
                self.ts_refs.insert(local.clone());
            }
            self.check_export_name(&local)?;
            let exported = if self.eat_kw("as") {
                self.module_name()?
            } else {
                local.clone()
            };
            self.check_export_name(&exported)?;
            names.push((local, exported));
            if !self.eat(P::Comma) {
                self.expect(P::RBrace)?;
                break;
            }
        }
        Ok((names, string_local))
    }

    fn check_export_name(&self, name: &str) -> R<()> {
        if name.chars().any(crate::value::is_lone) {
            return Err(self.error("Invalid module export name: contains unpaired surrogate"));
        }
        Ok(())
    }

    fn export(&mut self) -> R<StmtKind> {
        self.advance();
        if self.ts {
            if self.is_kw("type") && (self.is_at(1, P::LBrace) || self.is_at(1, P::Star)) {
                self.advance();
                if self.eat(P::Star) {
                    if self.eat_kw("as") {
                        self.module_name()?;
                    }
                } else {
                    self.ts_skip_group()?;
                }
                if self.eat_kw("from") {
                    self.string_lit()?;
                    self.import_attributes()?;
                }
                self.semicolon()?;
                return Ok(StmtKind::Empty);
            }
            if self.eat(P::Eq) {
                let line = self.line();
                let value = self.assignment()?;
                self.semicolon()?;
                let target = Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(syn_ident("module", line)),
                        name: "exports".into(),
                        optional: false,
                    },
                    line,
                    col: 0,
                };
                return Ok(StmtKind::Expr(Expr {
                    kind: ExprKind::Assign {
                        op: None,
                        target: AssignTarget::Simple(Box::new(target)),
                        value: Box::new(value),
                    },
                    line,
                    col: 0,
                }));
            }
            if self.is_kw("as") && self.is_kw_at(1, "namespace") {
                self.advance();
                self.advance();
                self.ident_name()?;
                self.semicolon()?;
                return Ok(StmtKind::Empty);
            }
            if self.is_kw("default") && self.is_kw_at(1, "interface") {
                self.advance();
                let s = self.statement()?;
                if matches!(s.kind, StmtKind::Empty) {
                    return Ok(StmtKind::Empty);
                }
                return Err(SyntaxError::new("Unexpected export", s.line, 1));
            }
            if self.is_kw("default") && self.is_kw_at(1, "abstract") && self.is_kw_at(2, "class") {
                self.advance();
                let line = self.line();
                self.advance();
                let named = matches!(&self.peek_at(1).tok, Tok::Ident(n) if &**n != "extends" && &**n != "implements");
                let c = self.class(named)?;
                if named {
                    return Ok(StmtKind::Export(ExportDecl::DefaultDecl(Box::new(Stmt {
                        kind: StmtKind::Class(c),
                        line,
                    }))));
                }
                return Ok(StmtKind::Export(ExportDecl::Default(Expr {
                    kind: ExprKind::Class(c),
                    line,
                    col: 0,
                })));
            }
        }
        if self.eat_kw("default") {
            let line = self.line();
            let is_async =
                self.is_kw("async") && self.is_kw_at(1, "function") && !self.peek_at(1).nl_before;
            let fn_at = usize::from(is_async);
            if self.is_kw_at(fn_at, "function") {
                let star = self.is_at(fn_at + 1, P::Star);
                let name_at = fn_at + 1 + usize::from(star);
                if matches!(self.peek_at(name_at).tok, Tok::Ident(_)) {
                    let s = self.statement()?;
                    if matches!(s.kind, StmtKind::Empty) {
                        return Ok(StmtKind::Empty);
                    }
                    return Ok(StmtKind::Export(ExportDecl::DefaultDecl(Box::new(s))));
                }
                self.span_start = Some(self.peek().start);
                if is_async {
                    self.advance();
                }
                self.advance();
                let is_generator = self.eat(P::Star);
                let f = self.function_full(
                    Some("%default".into()),
                    FuncKind::Normal,
                    line,
                    is_async,
                    is_generator,
                )?;
                return Ok(StmtKind::Export(ExportDecl::DefaultDecl(Box::new(Stmt {
                    kind: StmtKind::Function(f),
                    line,
                }))));
            }
            if self.is(P::At) {
                let decs = self.ts_decorators()?;
                self.ts_pending_decorators.extend(decs);
            }
            if self.is_kw("class") {
                if matches!(&self.peek_at(1).tok, Tok::Ident(n) if &**n != "extends") {
                    let c = self.class(true)?;
                    if let Some(e) = self.ts_class_iife.take() {
                        self.semicolon().ok();
                        return Ok(StmtKind::Export(ExportDecl::Default(e)));
                    }
                    return Ok(StmtKind::Export(ExportDecl::DefaultDecl(Box::new(Stmt {
                        kind: StmtKind::Class(c),
                        line,
                    }))));
                }
                self.std_default_name = true;
                let c = self.class(false);
                self.std_default_name = false;
                let c = c?;
                if let Some(e) = self.ts_class_iife.take() {
                    return Ok(StmtKind::Export(ExportDecl::Default(e)));
                }
                return Ok(StmtKind::Export(ExportDecl::Default(Expr {
                    kind: ExprKind::Class(c),
                    line,
                    col: 0,
                })));
            }
            let e = self.assignment()?;
            self.semicolon()?;
            return Ok(StmtKind::Export(ExportDecl::Default(e)));
        }
        if self.eat(P::Star) {
            if self.eat_kw("as") {
                let exported = self.module_name()?;
                self.check_export_name(&exported)?;
                self.expect_kw("from")?;
                let source = self.string_lit()?;
                self.import_attributes()?;
                self.semicolon()?;
                return Ok(StmtKind::Export(ExportDecl::From {
                    source,
                    names: Some(vec![("*".into(), exported)]),
                }));
            }
            self.expect_kw("from")?;
            let source = self.string_lit()?;
            self.import_attributes()?;
            self.semicolon()?;
            return Ok(StmtKind::Export(ExportDecl::From {
                source,
                names: None,
            }));
        }
        if self.eat(P::LBrace) {
            let (names, string_local) = self.export_names()?;
            if self.eat_kw("from") {
                let source = self.string_lit()?;
                self.import_attributes()?;
                self.semicolon()?;
                return Ok(StmtKind::Export(ExportDecl::From {
                    source,
                    names: Some(names),
                }));
            }
            if string_local {
                return Err(self.error("Unexpected string in export list"));
            }
            self.semicolon()?;
            return Ok(StmtKind::Export(ExportDecl::Names(names)));
        }
        let s = self.statement()?;
        match s.kind {
            StmtKind::Decl(..) | StmtKind::Function(_) | StmtKind::Class(_) => {
                Ok(StmtKind::Export(ExportDecl::Decl(Box::new(s))))
            }
            StmtKind::Empty if self.ts => Ok(StmtKind::Empty),
            _ => Err(SyntaxError::new("Unexpected export", s.line, 1)),
        }
    }

    fn jump_label(&mut self) -> R<Option<JsStr>> {
        match self.word() {
            Some(w) if !self.peek().nl_before && !self.reserved(&w) => {
                self.advance();
                Ok(Some(w))
            }
            _ => Ok(None),
        }
    }

    fn function_rest(&mut self, name: Option<JsStr>, kind: FuncKind, line: u32) -> R<Rc<FuncDef>> {
        self.function_full(name, kind, line, false, false)
    }

    fn function_full(
        &mut self,
        name: Option<JsStr>,
        kind: FuncKind,
        line: u32,
        is_async: bool,
        is_generator: bool,
    ) -> R<Rc<FuncDef>> {
        let start = self.span_start.take().unwrap_or_else(|| self.peek().start);
        let saved = (self.in_async, self.in_generator);
        let saved_eval = std::mem::take(&mut self.saw_eval);
        self.in_async = is_async;
        self.in_generator = is_generator;
        self.args_used.push(false);
        let saved_field = std::mem::take(&mut self.field_init);
        let saved_sb = std::mem::take(&mut self.static_block);
        let nt = std::mem::replace(&mut self.nt_ok, true);
        let saved_props = std::mem::take(&mut self.ts_props);
        let saved_decs = std::mem::take(&mut self.ts_param_decs);
        let saved_types = std::mem::take(&mut self.ts_param_types);
        let mut own_props = Vec::new();
        let mut own_decs = Vec::new();
        let mut own_types = Vec::new();
        let mut own_ret = None;
        let mut bodyless = false;
        let parsed = (|| {
            if self.ts && self.is(P::Lt) {
                self.ts_type_params()?;
            }
            let (params, rest) = self.params()?;
            own_props = std::mem::take(&mut self.ts_props);
            own_decs = std::mem::take(&mut self.ts_param_decs);
            own_types = std::mem::take(&mut self.ts_param_types);
            if self.ts {
                if self.eat(P::Colon) {
                    if self.ts_opts.emit_metadata {
                        own_ret = Some(self.ts_type_serialized()?);
                    } else {
                        self.ts_type()?;
                    }
                }
                if !self.is(P::LBrace) {
                    self.semicolon()?;
                    bodyless = true;
                    return Ok((params, rest, Vec::new()));
                }
            }
            Ok((params, rest, self.function_body()?))
        })();
        self.ts_props = saved_props;
        self.ts_param_decs = saved_decs;
        self.ts_param_types = saved_types;
        self.ts_last_props = own_props;
        self.ts_last_param_decs = own_decs;
        self.ts_last_param_types = own_types;
        self.ts_last_return = own_ret;
        self.ts_bodyless = bodyless;
        self.nt_ok = nt;
        self.static_block = saved_sb;
        self.field_init = saved_field;
        let uses_arguments = self.args_used.pop().unwrap_or(false);
        let has_eval = std::mem::replace(&mut self.saw_eval, saved_eval);
        let end = self.prev_end();
        (self.in_async, self.in_generator) = saved;
        let (params, rest, body) = parsed?;
        let unique = !matches!(kind, FuncKind::Normal);
        check_strict_params(&params, &rest, &body, line, self.strict, unique)?;
        let strict = self.strict || body_use_strict(&body);
        if strict
            && kind == FuncKind::Normal
            && matches!(name.as_deref(), Some("eval" | "arguments"))
        {
            return Err(SyntaxError::new(
                "Unexpected eval or arguments in strict mode",
                line,
                1,
            ));
        }
        Ok(Rc::new(FuncDef {
            name,
            params,
            rest,
            body: FuncBody::Block(body),
            kind,
            is_async,
            is_generator,
            uses_arguments,
            strict,
            has_eval,
            span: (start, end),
            line,
        }))
    }

    fn function_body(&mut self) -> R<Vec<Stmt>> {
        let saved = self.no_in;
        self.no_in = false;
        self.fn_depth += 1;
        let saved_strict = self.strict;
        if !self.strict && self.has_use_strict() {
            self.strict = true;
            self.check_prologue_octal(self.pos + 1)?;
        }
        let saved_ip = std::mem::take(&mut self.in_params);
        let body = self.block();
        self.in_params = saved_ip;
        self.strict = saved_strict;
        self.fn_depth -= 1;
        self.no_in = saved;
        body
    }

    fn has_use_strict(&self) -> bool {
        self.has_use_strict_from(self.pos + 1)
    }

    fn check_prologue_octal(&self, start: usize) -> R<()> {
        let mut i = start;
        while let Some(t) = self.toks.get(i) {
            match &t.tok {
                Tok::Str(_) if t.escaped => {
                    return Err(SyntaxError::new(
                        "Octal escape sequences are not allowed in strict mode.",
                        t.line,
                        t.col,
                    ));
                }
                Tok::Str(_) | Tok::Punct(P::Semi) => i += 1,
                _ => break,
            }
        }
        Ok(())
    }

    fn check_rest_position(&self, start: usize, end: usize) -> R<()> {
        let mut depth = 0i32;
        let mut rest_depth: Option<i32> = None;
        let mut init_depth: Option<i32> = None;
        for t in &self.toks[start..end.min(self.toks.len())] {
            match &t.tok {
                Tok::Punct(P::LParen) | Tok::Punct(P::LBracket) | Tok::Punct(P::LBrace) => {
                    depth += 1;
                }
                Tok::Punct(P::RParen) | Tok::Punct(P::RBracket) | Tok::Punct(P::RBrace) => {
                    depth -= 1;
                    if rest_depth.is_some_and(|d| depth < d) {
                        rest_depth = None;
                    }
                    if init_depth.is_some_and(|d| depth < d) {
                        init_depth = None;
                    }
                }
                Tok::Punct(P::Eq) if init_depth.is_none() => init_depth = Some(depth),
                Tok::Punct(P::Comma) if init_depth == Some(depth) => init_depth = None,
                _ if init_depth.is_some() => {}
                Tok::Punct(P::Ellipsis) => {
                    if rest_depth.is_none() {
                        rest_depth = Some(depth);
                    }
                }
                Tok::Punct(P::Comma) if rest_depth == Some(depth) => {
                    return Err(SyntaxError::new(
                        "Rest element must be last element",
                        t.line,
                        t.col,
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_ident_ref(&self, name: &str) -> R<()> {
        if name == "arguments" && self.field_init > 0 {
            return Err(self.error(
                "'arguments' is not allowed in class field initializer or static initialization block",
            ));
        }
        if name == "await" && self.static_block {
            return Err(self.error("Unexpected reserved word 'await'"));
        }
        Ok(())
    }

    fn fn_expr_name(&mut self) -> R<Option<JsStr>> {
        if !matches!(self.peek().tok, Tok::Ident(_)) {
            return Ok(None);
        }
        let saved_sb = std::mem::take(&mut self.static_block);
        let saved = (self.in_async, self.in_generator);
        self.in_async = false;
        self.in_generator = false;
        let name = self.binding_ident();
        (self.in_async, self.in_generator) = saved;
        self.static_block = saved_sb;
        Ok(Some(name?))
    }

    fn check_fn_expr_name(
        &self,
        name: &Option<JsStr>,
        is_async: bool,
        is_generator: bool,
    ) -> R<()> {
        match name.as_deref() {
            Some("yield") if is_generator => Err(self.error("Unexpected reserved word 'yield'")),
            Some("await") if is_async => Err(self.error("Unexpected reserved word 'await'")),
            _ => Ok(()),
        }
    }

    fn has_use_strict_from(&self, start: usize) -> bool {
        let last = self.toks.len() - 1;
        let mut i = start;
        loop {
            let Tok::Str(s) = &self.toks[i.min(last)].tok else {
                return false;
            };
            if &**s == "use strict" {
                return true;
            }
            i += 1;
            let t = &self.toks[i.min(last)];
            match &t.tok {
                Tok::Punct(P::Semi) => i += 1,
                Tok::Str(_) if t.nl_before => {}
                _ => return false,
            }
        }
    }

    fn params(&mut self) -> R<(Vec<Param>, Option<Pattern>)> {
        self.expect(P::LParen)?;
        let saved = self.no_in;
        let saved_ip = std::mem::replace(&mut self.in_params, true);
        self.no_in = false;
        let mut params = Vec::new();
        let mut rest = None;
        while !self.is(P::RParen) {
            let mut is_prop = false;
            let mut param_decs = Vec::new();
            if self.ts {
                param_decs = self.ts_decorators()?;
                if self.is_kw("this")
                    && (self.is_at(1, P::Colon)
                        || self.is_at(1, P::Comma)
                        || self.is_at(1, P::RParen))
                {
                    self.advance();
                    self.ts_annotation()?;
                    if !self.eat(P::Comma) {
                        break;
                    }
                    continue;
                }
                is_prop = self.ts_param_modifiers();
            }
            if self.eat(P::Ellipsis) {
                rest = Some(self.binding_pattern()?);
                if self.ts {
                    self.ts_param_type()?;
                }
                break;
            }
            let pattern = self.binding_pattern()?;
            if self.ts {
                if self.is(P::Question)
                    && matches!(
                        self.peek_at(1).tok,
                        Tok::Punct(P::Colon | P::Comma | P::RParen | P::Eq)
                    )
                {
                    self.advance();
                }
                self.ts_param_type()?;
                if is_prop && let Pattern::Ident(n) = &pattern {
                    self.ts_props.push(n.clone());
                }
                for d in param_decs {
                    self.ts_param_decs.push((params.len(), d));
                }
            }
            let default = if self.eat(P::Eq) {
                Some(self.assignment()?)
            } else {
                None
            };
            params.push(Param { pattern, default });
            if !self.eat(P::Comma) {
                break;
            }
        }
        self.no_in = saved;
        self.in_params = saved_ip;
        self.expect(P::RParen)?;
        Ok((params, rest))
    }

    fn prop_key(&mut self) -> R<(PropKey, bool)> {
        match &self.peek().tok {
            Tok::Ident(s) => {
                let s = s.clone();
                self.advance();
                Ok((PropKey::Named(s), true))
            }
            Tok::Str(s) => {
                if self.strict && self.peek().escaped {
                    return Err(
                        self.error("Octal escape sequences are not allowed in strict mode.")
                    );
                }
                let s = s.clone();
                self.advance();
                Ok((PropKey::Named(s), false))
            }
            Tok::Num(n) => {
                if self.strict && self.peek().escaped {
                    return Err(self.error("Octal literals are not allowed in strict mode."));
                }
                let s: JsStr = number_to_string(*n).into();
                self.advance();
                Ok((PropKey::Named(s), false))
            }
            Tok::BigInt(s) => {
                let s = s.clone();
                self.advance();
                let text: JsStr = match crate::bigint::from_string(&s) {
                    Some(b) => b.to_string().into(),
                    None => s,
                };
                Ok((PropKey::Named(text), false))
            }
            Tok::Punct(P::LBracket) => {
                self.advance();
                let saved = self.no_in;
                self.no_in = false;
                let e = self.assignment()?;
                self.no_in = saved;
                self.expect(P::RBracket)?;
                Ok((PropKey::Computed(e), false))
            }
            Tok::Punct(P::Hash) => Ok((PropKey::Named(self.private_name()?), false)),
            _ => Err(self.unexpected()),
        }
    }

    fn binding_pattern(&mut self) -> R<Pattern> {
        if self.eat(P::LBracket) {
            let mut elems = Vec::new();
            let mut rest = None;
            loop {
                if self.eat(P::RBracket) {
                    break;
                }
                if self.eat(P::Comma) {
                    elems.push(None);
                    continue;
                }
                if self.eat(P::Ellipsis) {
                    rest = Some(Box::new(self.binding_pattern()?));
                    self.expect(P::RBracket)?;
                    break;
                }
                let pattern = self.binding_pattern()?;
                let default = if self.eat(P::Eq) {
                    Some(self.assignment()?)
                } else {
                    None
                };
                elems.push(Some(PatElem { pattern, default }));
                if !self.eat(P::Comma) {
                    self.expect(P::RBracket)?;
                    break;
                }
            }
            Ok(Pattern::Array { elems, rest })
        } else if self.eat(P::LBrace) {
            let mut props = Vec::new();
            let mut rest = None;
            loop {
                if self.eat(P::RBrace) {
                    break;
                }
                if self.eat(P::Ellipsis) {
                    rest = Some(Box::new(Pattern::Ident(self.binding_ident()?)));
                    self.eat(P::Comma);
                    self.expect(P::RBrace)?;
                    break;
                }
                if self.is(P::Hash) {
                    return Err(self.error("Unexpected private field"));
                }
                let (key, is_ident) = self.prop_key()?;
                let (value, default) = if self.eat(P::Colon) {
                    let v = self.binding_pattern()?;
                    let d = if self.eat(P::Eq) {
                        Some(self.assignment()?)
                    } else {
                        None
                    };
                    (v, d)
                } else {
                    match &key {
                        PropKey::Named(n) if is_ident => {
                            self.check_binding_name(n)?;
                            let d = if self.eat(P::Eq) {
                                Some(self.assignment()?)
                            } else {
                                None
                            };
                            (Pattern::Ident(n.clone()), d)
                        }
                        _ => return Err(self.unexpected()),
                    }
                };
                props.push(ObjPatProp {
                    key,
                    value,
                    default,
                });
                if !self.eat(P::Comma) {
                    self.expect(P::RBrace)?;
                    break;
                }
            }
            Ok(Pattern::Object { props, rest })
        } else {
            Ok(Pattern::Ident(self.binding_ident()?))
        }
    }

    fn class_statement(&mut self) -> R<StmtKind> {
        let c = self.class(true)?;
        if let Some(e) = self.ts_class_iife.take() {
            let name = c.name.clone().unwrap_or_else(|| "%decclass%".into());
            return Ok(StmtKind::Decl(
                DeclKind::Let,
                vec![(Pattern::Ident(name), Some(e))],
            ));
        }
        Ok(StmtKind::Class(c))
    }

    fn class(&mut self, require_name: bool) -> R<Rc<ClassDef>> {
        let saved = std::mem::replace(&mut self.strict, true);
        let r = self.class_inner(require_name);
        self.strict = saved;
        r
    }

    fn class_inner(&mut self, require_name: bool) -> R<Rc<ClassDef>> {
        let class_decorators = std::mem::take(&mut self.ts_pending_decorators);
        let line = self.line();
        let start = self.peek().start;
        self.expect_kw("class")?;
        let name = if matches!(self.peek().tok, Tok::Ident(_))
            && !self.is_kw("extends")
            && !(self.ts && self.is_kw("implements"))
        {
            Some(self.binding_ident()?)
        } else if require_name {
            return Err(self.error("Class statements require a name"));
        } else {
            None
        };
        if self.ts && self.is(P::Lt) {
            self.ts_type_params()?;
        }
        let superclass = if self.eat_kw("extends") {
            let e = self.call_member()?;
            if self.ts && self.is(P::Lt) {
                self.ts_type_args()?;
            }
            Some(e)
        } else {
            None
        };
        if self.ts && self.eat_kw("implements") {
            self.ts_heritage()?;
        }
        let id = fresh_class_id();
        let declared = self.prescan_private();
        let private_id = if declared.is_empty() { 0 } else { id };
        self.class_ids.push((id, Rc::new(declared)));
        if self.ts {
            self.ts_class_stack
                .push(decorators::ClassDecorations::default());
        }
        let standard = self.std_decorators_active();
        if standard {
            self.std_class_start();
        }
        let parsed = self.class_body(superclass.is_some());
        let decorations = if self.ts {
            self.ts_class_stack.pop()
        } else {
            None
        };
        let std_class = if standard { self.std_stack.pop() } else { None };
        let default_name = std::mem::take(&mut self.std_default_name);
        self.class_ids.pop();
        let (constructor, members) = parsed?;
        check_class_members(&members, line)?;
        let def = Rc::new(ClassDef {
            name,
            superclass,
            constructor,
            members,
            line,
            span: (start, self.prev_end()),
            private_id,
        });
        if let Some(st) = std_class {
            if !class_decorators.is_empty() || st.wrap {
                self.ts_class_iife =
                    Some(self.std_decorated_class(def.clone(), class_decorators, st, default_name));
            }
        } else if let Some(d) = decorations
            && (!class_decorators.is_empty() || !d.members.is_empty() || !d.ctor_params.is_empty())
        {
            self.ts_class_iife = Some(self.ts_decorated_class(def.clone(), class_decorators, d));
        }
        Ok(def)
    }

    fn class_body(&mut self, has_super: bool) -> R<(Option<Rc<FuncDef>>, Vec<ClassMember>)> {
        self.expect(P::LBrace)?;
        let mut constructor = None;
        let mut members = Vec::new();
        while !self.eat(P::RBrace) {
            if self.is_eof() {
                return Err(self.unexpected());
            }
            if self.eat(P::Semi) {
                continue;
            }
            let member_decs = if self.is(P::At) {
                self.ts_decorators()?
            } else {
                Vec::new()
            };
            let legacy = self.ts && self.ts_opts.legacy_decorators;
            let mline = self.line();
            let mut mstart = self.peek().start;
            let ends_name = |p: &Parser, n: usize| {
                p.is_at(n, P::LParen)
                    || p.is_at(n, P::Eq)
                    || p.is_at(n, P::Semi)
                    || p.is_at(n, P::RBrace)
                    || p.peek_at(n).nl_before
                    || (p.ts
                        && (p.is_at(n, P::Colon)
                            || p.is_at(n, P::Question)
                            || p.is_at(n, P::Bang)
                            || p.is_at(n, P::Lt)))
            };
            let (mut ts_declare, mut ts_abstract) = if self.ts {
                self.ts_member_modifiers()
            } else {
                (false, false)
            };
            let mut is_static = false;
            if self.is_kw("static") && !ends_name(self, 1) {
                self.advance();
                is_static = true;
                mstart = self.peek().start;
                if self.is(P::LBrace) {
                    let saved = (self.in_async, self.in_generator);
                    self.in_async = false;
                    self.in_generator = false;
                    self.args_used.push(false);
                    self.field_init += 1;
                    let nt = std::mem::replace(&mut self.nt_ok, true);
                    let saved_no_in = std::mem::replace(&mut self.no_in, false);
                    let saved_sb = std::mem::replace(&mut self.static_block, true);
                    let body = self.block();
                    self.static_block = saved_sb;
                    self.no_in = saved_no_in;
                    self.nt_ok = nt;
                    self.field_init -= 1;
                    self.args_used.pop();
                    (self.in_async, self.in_generator) = saved;
                    let body = body?;
                    members.push(ClassMember::StaticBlock(Rc::new(FuncDef {
                        name: Some("static".into()),
                        params: Vec::new(),
                        rest: None,
                        body: FuncBody::Block(body),
                        kind: FuncKind::StaticMethod,
                        is_async: false,
                        is_generator: false,
                        uses_arguments: false,
                        strict: true,
                        has_eval: false,
                        line: mline,
                        span: (0, 0),
                    })));
                    continue;
                }
            }
            if self.ts {
                let (d, a) = self.ts_member_modifiers();
                ts_declare |= d;
                ts_abstract |= a;
                if self.ts_index_signature_ahead() {
                    self.ts_skip_group()?;
                    self.ts_annotation()?;
                    self.semicolon()?;
                    continue;
                }
            }
            if self.is_kw("accessor")
                && !ends_name(self, 1)
                && !self.is_at(1, P::Star)
                && !matches!(&self.peek_at(1).tok, Tok::Ident(w) if matches!(&**w, "get" | "set" | "async") && !ends_name(self, 2))
            {
                self.advance();
                let (key, _) = self.prop_key()?;
                if self.ts && (self.is(P::Question) || self.is(P::Bang)) && !self.peek().nl_before {
                    self.advance();
                }
                if self.ts && self.eat(P::Colon) {
                    self.ts_type()?;
                }
                let value = if self.eat(P::Eq) {
                    self.fn_depth += 1;
                    self.field_init += 1;
                    let nt = std::mem::replace(&mut self.nt_ok, true);
                    let v = self.assignment();
                    self.nt_ok = nt;
                    self.field_init -= 1;
                    self.fn_depth -= 1;
                    Some(v?)
                } else {
                    None
                };
                self.semicolon()?;
                if ts_declare || ts_abstract {
                    continue;
                }
                if legacy && !member_decs.is_empty() {
                    return Err(SyntaxError::new(
                        "Legacy decorators cannot decorate auto-accessors",
                        mline,
                        1,
                    ));
                }
                let expanded = self.std_auto_accessor(key, value, is_static, member_decs, mline)?;
                members.extend(expanded);
                continue;
            }
            let mut is_async = false;
            let mut is_generator = false;
            let mut accessor = None;
            if self.is_kw("async") && !ends_name(self, 1) {
                self.advance();
                is_async = true;
            }
            if self.eat(P::Star) {
                is_generator = true;
            }
            if !is_async
                && !is_generator
                && (self.is_kw("get") || self.is_kw("set"))
                && !ends_name(self, 1)
            {
                accessor = Some(self.is_kw("set"));
                self.advance();
            }
            let (key, _) = self.prop_key()?;
            let (key, key_expr) = if legacy && !member_decs.is_empty() {
                let mut st = self.ts_class_stack.pop().unwrap_or_default();
                let (k, e) = self.ts_member_key(key, &mut st);
                self.ts_class_stack.push(st);
                (k, Some(e))
            } else if !legacy && !member_decs.is_empty() {
                let (k, e) = self.std_member_key(key, mline);
                (k, Some(e))
            } else {
                let e = match &key {
                    PropKey::Named(n) => Some(Expr {
                        kind: ExprKind::Str(n.clone()),
                        line: mline,
                        col: 0,
                    }),
                    PropKey::Computed(_) => None,
                };
                (key, e)
            };
            if self.ts && (self.is(P::Question) || self.is(P::Bang)) && !self.peek().nl_before {
                self.advance();
            }
            if let Some(setter) = accessor {
                let fname = match &key {
                    PropKey::Named(n) => Some(display_name(n)),
                    PropKey::Computed(_) => None,
                };
                let kind = if is_static {
                    FuncKind::StaticMethod
                } else {
                    FuncKind::Method
                };
                self.span_start = Some(mstart);
                let f = self.function_rest(fname, kind, mline)?;
                if std::mem::take(&mut self.ts_bodyless) {
                    continue;
                }
                if legacy {
                    self.ts_member_decorations(
                        key_expr,
                        is_static,
                        if setter { 2 } else { 1 },
                        member_decs,
                        None,
                    );
                } else if !member_decs.is_empty() {
                    let k = key_expr.unwrap_or_else(|| syn_ident("undefined", mline));
                    self.std_record(
                        if setter { "setter" } else { "getter" },
                        &key,
                        k,
                        is_static,
                        member_decs,
                        mline,
                    )?;
                } else if self.ts {
                    self.ts_member_decorations(
                        key_expr,
                        is_static,
                        if setter { 2 } else { 1 },
                        Vec::new(),
                        None,
                    );
                }
                check_accessor_params(&f, setter, mline)?;
                members.push(ClassMember::Accessor {
                    key,
                    func: f,
                    is_static,
                    setter,
                });
                continue;
            }
            if self.is(P::LParen) || (self.ts && self.is(P::Lt)) {
                let is_ctor =
                    !is_static && matches!(&key, PropKey::Named(n) if &**n == "constructor");
                let kind = if is_ctor {
                    if has_super {
                        FuncKind::DerivedConstructor
                    } else {
                        FuncKind::ClassConstructor
                    }
                } else if is_static {
                    FuncKind::StaticMethod
                } else {
                    FuncKind::Method
                };
                let fname = match &key {
                    PropKey::Named(n) => Some(display_name(n)),
                    PropKey::Computed(_) => None,
                };
                if is_ctor && (is_async || is_generator) {
                    return Err(SyntaxError::new(
                        "Class constructor may not be async or a generator",
                        mline,
                        1,
                    ));
                }
                self.span_start = Some(mstart);
                let mut f = self.function_full(fname, kind, mline, is_async, is_generator)?;
                if std::mem::take(&mut self.ts_bodyless) {
                    continue;
                }
                if is_ctor && self.ts {
                    let props = std::mem::take(&mut self.ts_last_props);
                    if !props.is_empty() {
                        f = self.ts_inject_props(f, props, has_super);
                    }
                    let pdecs = std::mem::take(&mut self.ts_last_param_decs);
                    let types = std::mem::take(&mut self.ts_last_param_types);
                    if let Some(st) = self.ts_class_stack.last_mut() {
                        st.ctor_params = pdecs;
                        st.ctor_types = types;
                    }
                } else if legacy {
                    self.ts_member_decorations(key_expr, is_static, 0, member_decs, None);
                } else if !member_decs.is_empty() {
                    if is_ctor {
                        return Err(SyntaxError::new(
                            "Decorators are not valid on class constructors",
                            mline,
                            1,
                        ));
                    }
                    let k = key_expr.unwrap_or_else(|| syn_ident("undefined", mline));
                    self.std_record("method", &key, k, is_static, member_decs, mline)?;
                } else if self.ts {
                    self.ts_member_decorations(key_expr, is_static, 0, Vec::new(), None);
                }
                if is_ctor {
                    if constructor.is_some() {
                        return Err(SyntaxError::new(
                            "A class may only have one constructor",
                            mline,
                            1,
                        ));
                    }
                    constructor = Some(f);
                } else {
                    members.push(ClassMember::Method {
                        key,
                        func: f,
                        is_static,
                    });
                }
            } else {
                if is_async || is_generator {
                    return Err(self.unexpected());
                }
                let mut field_type = None;
                if self.ts && self.eat(P::Colon) {
                    if self.ts_opts.emit_metadata && !member_decs.is_empty() {
                        field_type = Some(self.ts_type_serialized()?);
                    } else {
                        self.ts_type()?;
                    }
                }
                let value = if self.eat(P::Eq) {
                    self.fn_depth += 1;
                    self.field_init += 1;
                    let nt = std::mem::replace(&mut self.nt_ok, true);
                    let v = self.assignment();
                    self.nt_ok = nt;
                    self.field_init -= 1;
                    self.fn_depth -= 1;
                    Some(v?)
                } else {
                    None
                };
                self.semicolon()?;
                if legacy && !member_decs.is_empty() {
                    self.ts_member_decorations(key_expr, is_static, 3, member_decs, field_type);
                    if !(ts_declare || ts_abstract) {
                        members.push(ClassMember::Field {
                            key,
                            value,
                            is_static,
                        });
                    }
                    continue;
                }
                if ts_declare || ts_abstract {
                    continue;
                }
                if !member_decs.is_empty() {
                    let k = key_expr.unwrap_or_else(|| syn_ident("undefined", mline));
                    let inits = self.std_record("field", &key, k, is_static, member_decs, mline)?;
                    let value = match inits {
                        Some(i) => Some(self.std_wrap_init(&i, value, is_static, mline)),
                        None => value,
                    };
                    members.push(ClassMember::Field {
                        key,
                        value,
                        is_static,
                    });
                    if let Some(extra) = self.std_field_extras(is_static, mline) {
                        members.push(extra);
                    }
                    continue;
                }
                members.push(ClassMember::Field {
                    key,
                    value,
                    is_static,
                });
            }
        }
        Ok((constructor, members))
    }

    fn expression(&mut self) -> R<Expr> {
        let line = self.line();
        let first = self.assignment()?;
        if self.is(P::Comma) {
            let mut items = vec![first];
            while self.eat(P::Comma) {
                items.push(self.assignment()?);
            }
            return Ok(Expr {
                kind: ExprKind::Seq(items),
                line,
                col: 0,
            });
        }
        Ok(first)
    }

    fn is_arrow_ahead(&mut self) -> bool {
        match &self.peek().tok {
            Tok::Ident(n) if !self.reserved(n) => {
                self.is_at(1, P::Arrow) && !self.peek_at(1).nl_before
            }
            Tok::Punct(P::Lt) if self.ts => self.ts_generic_arrow_ahead(),
            Tok::Punct(P::LParen) => {
                let paired = self.pairs.get(self.pos).copied().unwrap_or(u32::MAX);
                if paired != u32::MAX {
                    let t = self.toks.get(paired as usize + 1);
                    if matches!(t, Some(t) if matches!(t.tok, Tok::Punct(P::Arrow)) && !t.nl_before)
                    {
                        return true;
                    }
                    return self.ts && self.ts_return_arrow_ahead(paired as usize + 1);
                }
                let mut depth = 0i32;
                let mut i = self.pos;
                loop {
                    match &self.toks[i].tok {
                        Tok::Punct(P::LParen) | Tok::Punct(P::LBracket) | Tok::Punct(P::LBrace) => {
                            depth += 1
                        }
                        Tok::Punct(P::RParen) | Tok::Punct(P::RBracket) | Tok::Punct(P::RBrace) => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Tok::Eof => return false,
                        _ => {}
                    }
                    i += 1;
                }
                matches!(self.toks.get(i + 1), Some(t) if matches!(t.tok, Tok::Punct(P::Arrow)) && !t.nl_before)
            }
            _ => false,
        }
    }

    fn arrow(&mut self, is_async: bool) -> R<Expr> {
        let line = self.line();
        let start = if is_async && self.pos > 0 {
            self.toks[self.pos - 1].start
        } else {
            self.peek().start
        };
        let saved = (self.in_async, self.in_generator);
        let saved_eval = std::mem::take(&mut self.saw_eval);
        self.in_async |= is_async;
        let parsed = self.arrow_parts(is_async);
        (self.in_async, self.in_generator) = saved;
        let has_eval = std::mem::replace(&mut self.saw_eval, saved_eval);
        let (params, rest, body) = parsed?;
        let stmts: &[Stmt] = match &body {
            FuncBody::Block(stmts) => stmts,
            FuncBody::Expr(_) => &[],
        };
        check_strict_params(&params, &rest, stmts, line, self.strict, true)?;
        let strict = self.strict || matches!(&body, FuncBody::Block(b) if body_use_strict(b));
        Ok(Expr {
            kind: ExprKind::Function(Rc::new(FuncDef {
                name: None,
                params,
                rest,
                body,
                kind: FuncKind::Arrow,
                is_async,
                is_generator: false,
                uses_arguments: false,
                strict,
                has_eval,
                line,
                span: (start, self.prev_end()),
            })),
            line,
            col: 0,
        })
    }

    fn arrow_parts(&mut self, is_async: bool) -> R<(Vec<Param>, Option<Pattern>, FuncBody)> {
        if self.ts && self.is(P::Lt) {
            self.ts_type_params()?;
        }
        let (params, rest) = if self.is(P::LParen) {
            let saved_props = std::mem::take(&mut self.ts_props);
            let saved_decs = std::mem::take(&mut self.ts_param_decs);
            let saved_types = std::mem::take(&mut self.ts_param_types);
            let r = self.params();
            self.ts_props = saved_props;
            self.ts_param_decs = saved_decs;
            self.ts_param_types = saved_types;
            let r = r?;
            if self.ts && self.eat(P::Colon) {
                self.ts_arrow_return_type()?;
            }
            r
        } else {
            let n = self.binding_ident()?;
            (
                vec![Param {
                    pattern: Pattern::Ident(n),
                    default: None,
                }],
                None,
            )
        };
        self.expect(P::Arrow)?;
        self.in_async = is_async;
        let saved_sb = std::mem::take(&mut self.static_block);
        let saved_gen = std::mem::take(&mut self.in_generator);
        let body = if self.is(P::LBrace) {
            FuncBody::Block(self.function_body()?)
        } else {
            self.fn_depth += 1;
            let saved_ip = std::mem::take(&mut self.in_params);
            let e = self.assignment();
            self.in_params = saved_ip;
            self.fn_depth -= 1;
            FuncBody::Expr(Box::new(e?))
        };
        self.static_block = saved_sb;
        self.in_generator = saved_gen;
        Ok((params, rest, body))
    }

    fn yield_expr(&mut self) -> R<Expr> {
        let line = self.line();
        if self.in_params {
            return Err(self.error("Yield expression not allowed in formal parameter"));
        }
        self.advance();
        let delegate = !self.peek().nl_before && self.eat(P::Star);
        let ends = self.peek().nl_before
            || self.is_eof()
            || self.is(P::RParen)
            || self.is(P::RBracket)
            || self.is(P::RBrace)
            || self.is(P::Comma)
            || self.is(P::Semi)
            || self.is(P::Colon);
        let arg = if !delegate && ends {
            None
        } else {
            Some(Box::new(self.assignment()?))
        };
        Ok(Expr {
            kind: ExprKind::Yield { arg, delegate },
            line,
            col: 0,
        })
    }

    fn assign_op(&self) -> Option<Option<AssignOp>> {
        let Tok::Punct(p) = self.peek().tok else {
            return None;
        };
        let op = match p {
            P::Eq => return Some(None),
            P::PlusEq => AssignOp::Bin(BinOp::Add),
            P::MinusEq => AssignOp::Bin(BinOp::Sub),
            P::StarEq => AssignOp::Bin(BinOp::Mul),
            P::SlashEq => AssignOp::Bin(BinOp::Div),
            P::PercentEq => AssignOp::Bin(BinOp::Mod),
            P::StarStarEq => AssignOp::Bin(BinOp::Pow),
            P::AmpEq => AssignOp::Bin(BinOp::BitAnd),
            P::PipeEq => AssignOp::Bin(BinOp::BitOr),
            P::CaretEq => AssignOp::Bin(BinOp::BitXor),
            P::ShlEq => AssignOp::Bin(BinOp::Shl),
            P::ShrEq => AssignOp::Bin(BinOp::Shr),
            P::UShrEq => AssignOp::Bin(BinOp::UShr),
            P::AmpAmpEq => AssignOp::Logical(LogicalOp::And),
            P::PipePipeEq => AssignOp::Logical(LogicalOp::Or),
            P::NullishEq => AssignOp::Logical(LogicalOp::Nullish),
            _ => return None,
        };
        Some(Some(op))
    }

    fn assignment(&mut self) -> R<Expr> {
        if self.is_arrow_ahead() {
            return self.arrow(false);
        }
        if self.is_kw("async")
            && (self.is_at(1, P::LParen)
                || matches!(self.peek_at(1).tok, Tok::Ident(_))
                || (self.ts && self.is_at(1, P::Lt)))
            && !self.peek_at(1).nl_before
        {
            let save = self.pos;
            self.advance();
            if self.is_arrow_ahead() {
                return self.arrow(true);
            }
            self.pos = save;
        }
        if self.is_kw("yield") {
            if self.in_generator {
                return self.yield_expr();
            }
            if self.strict {
                return Err(self.error("yield is only valid in generator functions"));
            }
        }
        let start = self.pos();
        let lhs_start = self.pos;
        let lhs = self.conditional()?;
        if matches!(self.peek().tok, Tok::Regex(..)) {
            self.relex(false)?;
        }
        if let Some(op) = self.assign_op() {
            let (line, col) = if op.is_some() { start } else { self.pos() };
            let parenthesized = self.last_paren == (lhs_start, self.pos);
            if let ExprKind::Ident(n) = &lhs.kind
                && self.strict
                && matches!(&**n, "eval" | "arguments")
            {
                return Err(self.error("Unexpected eval or arguments in strict mode"));
            }
            if parenthesized && matches!(lhs.kind, ExprKind::Array(_) | ExprKind::Object(_)) {
                return Err(self.error("Invalid left-hand side in assignment"));
            }
            let target = match (&lhs.kind, op) {
                (ExprKind::SuperMember(_), _) | (ExprKind::SuperIndex(_), _) => {
                    AssignTarget::Simple(Box::new(lhs))
                }
                (ExprKind::Call { args, .. }, None | Some(AssignOp::Bin(_)))
                    if !self.strict && !parenthesized && !is_tagged(args) =>
                {
                    AssignTarget::Simple(Box::new(lhs))
                }
                (ExprKind::Ident(_), _)
                | (
                    ExprKind::Member {
                        optional: false, ..
                    },
                    _,
                )
                | (
                    ExprKind::Index {
                        optional: false, ..
                    },
                    _,
                ) => AssignTarget::Simple(Box::new(lhs)),
                (ExprKind::Array(_), None) | (ExprKind::Object(_), None) => {
                    self.check_rest_position(lhs_start, self.pos)?;
                    AssignTarget::Pattern(self.expr_to_pattern(lhs)?)
                }
                _ => return Err(self.error("Invalid left-hand side in assignment")),
            };
            self.advance();
            let value = self.assignment()?;
            let value = if parenthesized && op.is_none() {
                Expr {
                    line: value.line,
                    kind: ExprKind::Seq(vec![value]),
                    col: 0,
                }
            } else {
                value
            };
            return Ok(Expr {
                kind: ExprKind::Assign {
                    op,
                    target,
                    value: Box::new(value),
                },
                line,
                col,
            });
        }
        Ok(lhs)
    }

    fn expr_to_pattern(&self, e: Expr) -> R<Pattern> {
        let line = e.line;
        match e.kind {
            ExprKind::Ident(n) => {
                if self.strict && matches!(&*n, "eval" | "arguments") {
                    return Err(SyntaxError::new(
                        "Unexpected eval or arguments in strict mode",
                        line,
                        1,
                    ));
                }
                if self.reserved(&n) {
                    return Err(SyntaxError::new(
                        "Invalid destructuring assignment target",
                        line,
                        1,
                    ));
                }
                Ok(Pattern::Ident(n))
            }
            ExprKind::Member {
                optional: false, ..
            }
            | ExprKind::Index {
                optional: false, ..
            } => Ok(Pattern::Target(Box::new(e))),
            ExprKind::Array(elems) => {
                let n = elems.len();
                let mut out = Vec::new();
                let mut rest = None;
                for (i, el) in elems.into_iter().enumerate() {
                    match el {
                        ArrayElem::Hole => out.push(None),
                        ArrayElem::Spread(x) => {
                            if i != n - 1 {
                                return Err(SyntaxError::new(
                                    "Rest element must be last element",
                                    line,
                                    1,
                                ));
                            }
                            rest = Some(Box::new(self.expr_to_pattern(x)?));
                        }
                        ArrayElem::Expr(x) => out.push(Some(self.elem_to_pattern(x)?)),
                    }
                }
                Ok(Pattern::Array { elems: out, rest })
            }
            ExprKind::Object(props) => {
                let mut out = Vec::new();
                let mut rest = None;
                for p in props {
                    match p {
                        PropDef::KeyValue(key, v) => {
                            let el = self.elem_to_pattern(v)?;
                            out.push(ObjPatProp {
                                key,
                                value: el.pattern,
                                default: el.default,
                            });
                        }
                        PropDef::Shorthand(n) => out.push(ObjPatProp {
                            key: PropKey::Named(n.clone()),
                            value: self.expr_to_pattern(Expr {
                                kind: ExprKind::Ident(n),
                                line,
                                col: 0,
                            })?,
                            default: None,
                        }),
                        PropDef::ShorthandDefault(n, d) => out.push(ObjPatProp {
                            key: PropKey::Named(n.clone()),
                            value: self.expr_to_pattern(Expr {
                                kind: ExprKind::Ident(n),
                                line,
                                col: 0,
                            })?,
                            default: Some(d),
                        }),
                        PropDef::Spread(x) => rest = Some(Box::new(self.expr_to_pattern(x)?)),
                        PropDef::Method(..) | PropDef::Getter(..) | PropDef::Setter(..) => {
                            return Err(SyntaxError::new(
                                "Invalid destructuring assignment target",
                                line,
                                1,
                            ));
                        }
                    }
                }
                Ok(Pattern::Object { props: out, rest })
            }
            _ => Err(SyntaxError::new(
                "Invalid destructuring assignment target",
                line,
                1,
            )),
        }
    }

    fn elem_to_pattern(&self, e: Expr) -> R<PatElem> {
        if let ExprKind::Assign {
            op: None,
            target,
            value,
        } = e.kind
        {
            let pattern = match target {
                AssignTarget::Simple(t) => self.expr_to_pattern(*t)?,
                AssignTarget::Pattern(p) => p,
            };
            Ok(PatElem {
                pattern,
                default: Some(*value),
            })
        } else {
            Ok(PatElem {
                pattern: self.expr_to_pattern(e)?,
                default: None,
            })
        }
    }

    fn conditional(&mut self) -> R<Expr> {
        let line = self.line();
        let test = self.binary(0)?;
        if self.eat(P::Question) {
            let saved = self.no_in;
            self.no_in = false;
            let cons = self.assignment()?;
            self.no_in = saved;
            self.expect(P::Colon)?;
            let alt = self.assignment()?;
            return Ok(Expr {
                kind: ExprKind::Cond(Box::new(test), Box::new(cons), Box::new(alt)),
                line,
                col: 0,
            });
        }
        Ok(test)
    }

    fn binop_info(&self) -> Option<(u8, BinKind)> {
        match &self.peek().tok {
            Tok::Punct(p) => Some(match p {
                P::Nullish => (1, BinKind::Logical(LogicalOp::Nullish)),
                P::PipePipe => (2, BinKind::Logical(LogicalOp::Or)),
                P::AmpAmp => (3, BinKind::Logical(LogicalOp::And)),
                P::Pipe => (4, BinKind::Bin(BinOp::BitOr)),
                P::Caret => (5, BinKind::Bin(BinOp::BitXor)),
                P::Amp => (6, BinKind::Bin(BinOp::BitAnd)),
                P::EqEq => (7, BinKind::Bin(BinOp::Eq)),
                P::NotEq => (7, BinKind::Bin(BinOp::Ne)),
                P::EqEqEq => (7, BinKind::Bin(BinOp::StrictEq)),
                P::NotEqEq => (7, BinKind::Bin(BinOp::StrictNe)),
                P::Lt => (8, BinKind::Bin(BinOp::Lt)),
                P::Gt => (8, BinKind::Bin(BinOp::Gt)),
                P::Le => (8, BinKind::Bin(BinOp::Le)),
                P::Ge => (8, BinKind::Bin(BinOp::Ge)),
                P::Shl => (9, BinKind::Bin(BinOp::Shl)),
                P::Shr => (9, BinKind::Bin(BinOp::Shr)),
                P::UShr => (9, BinKind::Bin(BinOp::UShr)),
                P::Plus => (10, BinKind::Bin(BinOp::Add)),
                P::Minus => (10, BinKind::Bin(BinOp::Sub)),
                P::Star => (11, BinKind::Bin(BinOp::Mul)),
                P::Slash => (11, BinKind::Bin(BinOp::Div)),
                P::Percent => (11, BinKind::Bin(BinOp::Mod)),
                P::StarStar => (12, BinKind::Bin(BinOp::Pow)),
                _ => return None,
            }),
            Tok::Ident(s) if &**s == "instanceof" => Some((8, BinKind::Bin(BinOp::InstanceOf))),
            Tok::Ident(s) if &**s == "in" && !self.no_in => Some((8, BinKind::Bin(BinOp::In))),
            _ => None,
        }
    }

    fn binary(&mut self, min: u8) -> R<Expr> {
        let t = self.peek();
        let mut unary_left = matches!(
            t.tok,
            Tok::Punct(P::Bang) | Tok::Punct(P::Minus) | Tok::Punct(P::Plus) | Tok::Punct(P::Tilde)
        ) || (!t.escaped
            && matches!(&t.tok, Tok::Ident(s) if matches!(&**s, "typeof" | "void" | "delete")));
        if self.is(P::Hash) && (min > 8 || !self.is_kw_at(2, "in")) {
            return Err(self.error("Unexpected private name"));
        }
        let mut left = self.unary()?;
        let mut built: Option<LogicalOp> = None;
        loop {
            if matches!(self.peek().tok, Tok::Regex(..)) {
                self.relex(false)?;
            }
            if self.ts
                && min <= 8
                && (self.is_kw("as") || self.is_kw("satisfies"))
                && !self.peek().nl_before
            {
                self.advance();
                if self.is_kw("const") {
                    self.advance();
                } else {
                    self.ts_type()?;
                }
                unary_left = false;
                continue;
            }
            let Some((prec, op)) = self.binop_info() else {
                break;
            };
            if prec < min {
                break;
            }
            if unary_left && matches!(op, BinKind::Bin(BinOp::Pow)) {
                return Err(self.error(
                    "Unary operator used immediately before exponentiation expression. Parenthesis must be used to disambiguate operator precedence",
                ));
            }
            unary_left = false;
            let (line, col) = self.pos();
            if let BinKind::Logical(l) = op
                && built.is_some_and(|b| mixes_nullish(b, l))
            {
                return Err(self.error("Cannot mix '??' with '||' or '&&' without parentheses"));
            }
            self.advance();
            let right_start = self.pos;
            let right = if matches!(op, BinKind::Bin(BinOp::Pow)) {
                self.binary(prec)?
            } else {
                self.binary(prec + 1)?
            };
            match op {
                BinKind::Logical(l) => {
                    if let ExprKind::Logical(r, ..) = &right.kind
                        && mixes_nullish(*r, l)
                        && self.last_paren != (right_start, self.pos)
                    {
                        return Err(
                            self.error("Cannot mix '??' with '||' or '&&' without parentheses")
                        );
                    }
                    built = Some(l);
                }
                BinKind::Bin(_) => built = None,
            }
            let kind = match op {
                BinKind::Bin(b) => ExprKind::Binary(b, Box::new(left), Box::new(right)),
                BinKind::Logical(l) => ExprKind::Logical(l, Box::new(left), Box::new(right)),
            };
            left = Expr { kind, line, col };
        }
        Ok(left)
    }

    fn check_update_target(&self, e: &Expr) -> R<()> {
        match &e.kind {
            ExprKind::Ident(n) if self.strict && matches!(&**n, "eval" | "arguments") => Err(
                SyntaxError::new("Unexpected eval or arguments in strict mode", e.line, 1),
            ),
            ExprKind::Ident(_)
            | ExprKind::Member {
                optional: false, ..
            }
            | ExprKind::Index {
                optional: false, ..
            }
            | ExprKind::SuperMember(_)
            | ExprKind::SuperIndex(_) => Ok(()),
            ExprKind::Call { args, .. } if !self.strict && !is_tagged(args) => Ok(()),
            _ => Err(SyntaxError::new(
                "Invalid left-hand side expression in update operation",
                e.line,
                1,
            )),
        }
    }

    fn unary(&mut self) -> R<Expr> {
        if self.ts && self.is(P::Lt) {
            self.ts_type_args()?;
            return self.unary();
        }
        let (line, col) = self.pos();
        let op = match &self.peek().tok {
            Tok::Punct(P::Bang) => UnaryOp::Not,
            Tok::Punct(P::Minus) => UnaryOp::Neg,
            Tok::Punct(P::Plus) => UnaryOp::Plus,
            Tok::Punct(P::Tilde) => UnaryOp::BitNot,
            Tok::Ident(s) if &**s == "typeof" => UnaryOp::Typeof,
            Tok::Ident(s) if &**s == "void" => UnaryOp::Void,
            Tok::Ident(s) if &**s == "delete" => UnaryOp::Delete,
            Tok::Ident(s)
                if &**s == "await"
                    && !self.peek().escaped
                    && self.script
                    && !self.in_async
                    && !self.static_block
                    && self.fn_depth > 0
                    && !self.peek_at(1).nl_before
                    && match &self.peek_at(1).tok {
                        Tok::Ident(n) => !matches!(&**n, "in" | "instanceof" | "of"),
                        Tok::Num(_) | Tok::Str(_) | Tok::BigInt(_) => true,
                        _ => false,
                    } =>
            {
                return Err(self
                    .error("await is only valid in async functions and the top level of modules"));
            }
            Tok::Ident(s)
                if &**s == "await" && !self.peek().escaped && (self.in_async || !self.script) =>
            {
                if !self.in_async && self.fn_depth > 0 {
                    return Err(self.error(
                        "await is only valid in async functions and the top level of modules",
                    ));
                }
                if self.in_params && self.in_async {
                    return Err(self
                        .error("Illegal await-expression in formal parameters of async function"));
                }
                self.advance();
                let operand = self.unary()?;
                return Ok(Expr {
                    kind: ExprKind::Await(Box::new(operand)),
                    line,
                    col,
                });
            }
            Tok::Punct(P::PlusPlus) | Tok::Punct(P::MinusMinus) => {
                let inc = self.is(P::PlusPlus);
                self.advance();
                let target = self.unary()?;
                self.check_update_target(&target)?;
                return Ok(Expr {
                    kind: ExprKind::Update {
                        inc,
                        prefix: true,
                        target: Box::new(target),
                    },
                    line,
                    col,
                });
            }
            _ => return self.postfix(),
        };
        self.advance();
        let operand = self.unary()?;
        if op == UnaryOp::Delete {
            let inner = &operand;
            if matches!(&inner.kind, ExprKind::Member { name, .. } if name.starts_with("\u{0}#")) {
                return Err(self.error("Private fields can not be deleted"));
            }
            if self.strict && matches!(&inner.kind, ExprKind::Ident(_)) {
                return Err(self.error("Delete of an unqualified identifier in strict mode."));
            }
        }
        if op == UnaryOp::Neg
            && let ExprKind::Number(n) = operand.kind
        {
            return Ok(Expr {
                kind: ExprKind::Number(-n),
                line,
                col,
            });
        }
        Ok(Expr {
            kind: ExprKind::Unary(op, Box::new(operand)),
            line,
            col,
        })
    }

    fn postfix(&mut self) -> R<Expr> {
        let (line, col) = self.pos();
        let e = self.call_member()?;
        if (self.is(P::PlusPlus) || self.is(P::MinusMinus)) && !self.peek().nl_before {
            self.check_update_target(&e)?;
            let inc = self.is(P::PlusPlus);
            self.advance();
            return Ok(Expr {
                kind: ExprKind::Update {
                    inc,
                    prefix: false,
                    target: Box::new(e),
                },
                line,
                col,
            });
        }
        Ok(e)
    }

    fn arguments(&mut self) -> R<Vec<Arg>> {
        self.expect(P::LParen)?;
        let saved = self.no_in;
        self.no_in = false;
        let mut args = Vec::new();
        while !self.is(P::RParen) {
            if self.eat(P::Ellipsis) {
                args.push(Arg::Spread(self.assignment()?));
            } else {
                args.push(Arg::Expr(self.assignment()?));
            }
            if !self.eat(P::Comma) {
                break;
            }
        }
        self.no_in = saved;
        self.expect(P::RParen)?;
        Ok(args)
    }

    fn index_expr(&mut self) -> R<Expr> {
        let saved = self.no_in;
        self.no_in = false;
        let e = self.expression();
        self.no_in = saved;
        let e = e?;
        self.expect(P::RBracket)?;
        Ok(e)
    }

    fn call_member(&mut self) -> R<Expr> {
        let mut e = if self.is_kw("new") {
            self.new_expr()?
        } else {
            self.primary()?
        };
        let mut chain = false;
        loop {
            let (line, col) = self.pos();
            if self.ts && self.is(P::Bang) && !self.peek().nl_before {
                self.advance();
                continue;
            }
            if self.ts && self.is(P::Lt) && self.ts_try_type_args() {
                continue;
            }
            if self.eat(P::Dot) {
                let (line, col) = self.pos();
                let name = self.ident_name()?;
                e = Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(e),
                        name,
                        optional: false,
                    },
                    line,
                    col,
                };
            } else if self.eat(P::QuestionDot) {
                chain = true;
                if self.is(P::LParen) {
                    let (line, col) = call_pos(&e, self.pos());
                    let args = self.arguments()?;
                    e = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                            optional: true,
                        },
                        line,
                        col,
                    };
                } else if self.eat(P::LBracket) {
                    let index = self.index_expr()?;
                    e = Expr {
                        kind: ExprKind::Index {
                            obj: Box::new(e),
                            index: Box::new(index),
                            optional: true,
                        },
                        line,
                        col,
                    };
                } else {
                    let (line, col) = self.pos();
                    let name = self.ident_name()?;
                    e = Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(e),
                            name,
                            optional: true,
                        },
                        line,
                        col,
                    };
                }
            } else if self.eat(P::LBracket) {
                let index = self.index_expr()?;
                e = Expr {
                    kind: ExprKind::Index {
                        obj: Box::new(e),
                        index: Box::new(index),
                        optional: false,
                    },
                    line,
                    col,
                };
            } else if self.is(P::LParen) {
                let (line, col) = call_pos(&e, (line, col));
                let args = self.arguments()?;
                let direct = matches!(&e.kind, ExprKind::Ident(n) if &**n == "eval");
                e = if direct {
                    if self.field_init == 0
                        && let Some(top) = self.args_used.last_mut()
                    {
                        *top = true;
                    }
                    self.saw_eval = true;
                    let ctx = Rc::new(EvalCtx {
                        privates: self.class_ids.clone(),
                        in_function: self.nt_ok,
                        field_init: self.field_init > 0,
                        strict: self.strict,
                        in_params: self.in_params,
                    });
                    Expr {
                        kind: ExprKind::DirectEval(args, ctx),
                        line,
                        col,
                    }
                } else {
                    Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                            optional: false,
                        },
                        line,
                        col,
                    }
                };
            } else if matches!(self.peek().tok, Tok::Template(_)) {
                if chain {
                    return Err(self.error("Invalid tagged template on optional chain"));
                }
                let Tok::Template(parts) = self.advance_token().tok else {
                    unreachable!()
                };
                let (quasis, exprs) = self.template_parts(parts)?;
                let (cooked, raw) = quasis.into_iter().map(|(c, r)| (c.ok(), r)).unzip();
                let mut args = vec![Arg::Expr(Expr {
                    kind: ExprKind::TemplateStrings(cooked, raw),
                    line,
                    col,
                })];
                args.extend(exprs.into_iter().map(Arg::Expr));
                e = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(e),
                        args,
                        optional: false,
                    },
                    line,
                    col,
                };
            } else {
                break;
            }
        }
        if chain {
            let (line, col) = (e.line, e.col);
            e = Expr {
                kind: ExprKind::OptChain(Box::new(e)),
                line,
                col,
            };
        }
        Ok(e)
    }

    fn new_expr(&mut self) -> R<Expr> {
        let (line, col) = self.pos();
        self.advance();
        if self.is_kw("import") && !self.is_at(1, P::Dot) {
            return Err(self.error("Cannot use new with import"));
        }
        if self.is(P::Dot) {
            self.advance();
            match self.ident_name()? {
                n if &*n == "target" => {
                    if !self.nt_ok {
                        return Err(self.error("new.target expression is not allowed here"));
                    }
                    return Ok(Expr {
                        kind: ExprKind::NewTarget,
                        line,
                        col,
                    });
                }
                _ => return Err(self.error("Unexpected identifier after 'new.'")),
            }
        }
        let mut callee = if self.is_kw("new") {
            self.new_expr()?
        } else {
            self.primary()?
        };
        loop {
            let (mline, mcol) = self.pos();
            if self.eat(P::Dot) {
                let (line, col) = self.pos();
                let name = self.ident_name()?;
                callee = Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(callee),
                        name,
                        optional: false,
                    },
                    line,
                    col,
                };
            } else if self.eat(P::LBracket) {
                let index = self.index_expr()?;
                callee = Expr {
                    kind: ExprKind::Index {
                        obj: Box::new(callee),
                        index: Box::new(index),
                        optional: false,
                    },
                    line: mline,
                    col: mcol,
                };
            } else if matches!(self.peek().tok, Tok::Template(_)) {
                let Tok::Template(parts) = self.advance_token().tok else {
                    unreachable!()
                };
                let (quasis, exprs) = self.template_parts(parts)?;
                let (cooked, raw) = quasis.into_iter().map(|(c, r)| (c.ok(), r)).unzip();
                let mut args = vec![Arg::Expr(Expr {
                    kind: ExprKind::TemplateStrings(cooked, raw),
                    line: mline,
                    col: mcol,
                })];
                args.extend(exprs.into_iter().map(Arg::Expr));
                callee = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(callee),
                        args,
                        optional: false,
                    },
                    line: mline,
                    col: mcol,
                };
            } else {
                break;
            }
        }
        if self.ts && self.is(P::Lt) {
            self.ts_try_type_args();
        }
        let args = if self.is(P::LParen) {
            self.arguments()?
        } else {
            Vec::new()
        };
        Ok(Expr {
            kind: ExprKind::New {
                callee: Box::new(callee),
                args,
            },
            line,
            col,
        })
    }

    fn template(&mut self, parts: Vec<TemplatePart>, line: u32, col: u32) -> R<Expr> {
        let (quasis, exprs) = self.template_parts(parts)?;
        let quasis = quasis
            .into_iter()
            .map(|q| q.0)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Expr {
            kind: ExprKind::Template(quasis, exprs),
            line,
            col,
        })
    }

    #[allow(clippy::type_complexity)]
    fn template_parts(
        &mut self,
        parts: Vec<TemplatePart>,
    ) -> R<(Vec<(Result<JsStr, SyntaxError>, JsStr)>, Vec<Expr>)> {
        let mut quasis = Vec::new();
        let mut exprs = Vec::new();
        for part in parts {
            match part {
                TemplatePart::Str(s, raw) => quasis.push((s, raw)),
                TemplatePart::Expr(toks) => {
                    let mut sub = Parser {
                        pairs: bracket_pairs(&toks),
                        src: self.src.clone(),
                        toks,
                        pos: 0,
                        no_in: false,
                        fn_depth: self.fn_depth,
                        in_async: self.in_async,
                        in_generator: self.in_generator,
                        class_ids: self.class_ids.clone(),
                        field_init: self.field_init,
                        script: self.script,
                        nt_ok: self.nt_ok,
                        strict: self.strict,
                        static_block: self.static_block,
                        in_params: self.in_params,
                        single: false,
                        labeled_fn: false,
                        saw_eval: false,
                        last_paren: (usize::MAX, usize::MAX),
                        at_top: false,
                        args_used: vec![false],
                        span_start: None,
                        using_ok: false,
                        using_scope: None,
                        using_ids: self.using_ids,
                        ts: self.ts,
                        ts_splits: Vec::new(),
                        ts_refs: rustc_hash::FxHashSet::default(),
                        ts_props: Vec::new(),
                        ts_last_props: Vec::new(),
                        ts_bodyless: false,
                        ts_ambient: self.ts_ambient,
                        ts_in_extends: false,
                        ts_arrow_ret: false,
                        ts_opts: self.ts_opts,
                        ts_pending_decorators: Vec::new(),
                        ts_param_decs: Vec::new(),
                        ts_param_types: Vec::new(),
                        ts_last_param_decs: Vec::new(),
                        ts_last_param_types: Vec::new(),
                        ts_last_return: None,
                        ts_class_stack: Vec::new(),
                        std_stack: Vec::new(),
                        std_default_name: false,
                        ts_class_iife: None,
                        jsx: self.jsx.clone(),
                        jsx_used: false,
                    };
                    if sub.is_eof() {
                        return Err(sub.error("Unexpected token '}'"));
                    }
                    let e = sub.expression()?;
                    self.using_ids = sub.using_ids;
                    self.ts_refs.extend(std::mem::take(&mut sub.ts_refs));
                    self.jsx_used |= sub.jsx_used;
                    if !sub.is_eof() {
                        return Err(sub.unexpected());
                    }
                    if sub.args_used.first() == Some(&true)
                        && let Some(top) = self.args_used.last_mut()
                    {
                        *top = true;
                    }
                    exprs.push(e);
                }
            }
        }
        Ok((quasis, exprs))
    }

    fn primary(&mut self) -> R<Expr> {
        if self.is(P::JsxOpen) {
            return self.jsx_element();
        }
        if self.is(P::At) {
            let decs = self.ts_decorators()?;
            self.ts_pending_decorators.extend(decs);
            if !self.is_kw("class") {
                return Err(self.error("Decorators are only valid on classes and class members"));
            }
            return self.primary();
        }
        let (line, col) = self.pos();
        if matches!(
            self.peek().tok,
            Tok::Punct(P::Slash) | Tok::Punct(P::SlashEq)
        ) {
            self.relex(true)?;
        }
        let kind = match &self.peek().tok {
            Tok::BigInt(b) => {
                let b = b.clone();
                self.advance();
                ExprKind::BigInt(b)
            }
            Tok::Num(n) => {
                if self.strict && self.peek().escaped {
                    return Err(self.error("Octal literals are not allowed in strict mode."));
                }
                let n = *n;
                self.advance();
                ExprKind::Number(n)
            }
            Tok::Str(s) => {
                if self.strict && self.peek().escaped {
                    return Err(
                        self.error("Octal escape sequences are not allowed in strict mode.")
                    );
                }
                let s = s.clone();
                self.advance();
                ExprKind::Str(s)
            }
            Tok::Regex(p, f) => {
                let (p, f) = (p.clone(), f.clone());
                self.advance();
                ExprKind::Regex(p, f)
            }
            Tok::Template(_) => {
                let Tok::Template(parts) = self.advance_token().tok else {
                    unreachable!()
                };
                return self.template(parts, line, col);
            }
            Tok::Punct(P::LParen) => {
                let start = self.pos;
                self.advance();
                let saved = self.no_in;
                self.no_in = false;
                let e = self.expression();
                self.no_in = saved;
                let e = e?;
                self.expect(P::RParen)?;
                self.last_paren = (start, self.pos);
                return Ok(e);
            }
            Tok::Punct(P::LBracket) => {
                self.advance();
                let saved = self.no_in;
                self.no_in = false;
                let mut elems = Vec::new();
                loop {
                    if self.eat(P::RBracket) {
                        break;
                    }
                    if self.eat(P::Comma) {
                        elems.push(ArrayElem::Hole);
                        continue;
                    }
                    if self.eat(P::Ellipsis) {
                        elems.push(ArrayElem::Spread(self.assignment()?));
                    } else {
                        elems.push(ArrayElem::Expr(self.assignment()?));
                    }
                    if !self.eat(P::Comma) {
                        self.expect(P::RBracket)?;
                        break;
                    }
                }
                self.no_in = saved;
                ExprKind::Array(elems)
            }
            Tok::Punct(P::LBrace) => {
                self.advance();
                let saved = self.no_in;
                self.no_in = false;
                let props = self.object_props()?;
                self.no_in = saved;
                ExprKind::Object(props)
            }
            Tok::Punct(P::Hash) => ExprKind::Str(self.private_name()?),
            Tok::Ident(name)
                if self.peek().escaped
                    && self.reserved(name)
                    && !(&**name == "await"
                        && self.script
                        && !self.in_async
                        && !self.static_block) =>
            {
                return Err(self.error("Keyword must not contain escaped characters"));
            }
            Tok::Ident(name) if self.peek().escaped => {
                let name = name.clone();
                self.check_ident_ref(&name)?;
                self.advance();
                if self.ts {
                    self.ts_refs.insert(name.clone());
                }
                ExprKind::Ident(name)
            }
            Tok::Ident(name) => {
                let name = name.clone();
                match &*name {
                    "true" => {
                        self.advance();
                        ExprKind::Bool(true)
                    }
                    "false" => {
                        self.advance();
                        ExprKind::Bool(false)
                    }
                    "null" => {
                        self.advance();
                        ExprKind::Null
                    }
                    "this" => {
                        self.advance();
                        ExprKind::This
                    }
                    "function" => {
                        self.span_start = Some(self.peek().start);
                        self.advance();
                        let is_generator = self.eat(P::Star);
                        let fname = self.fn_expr_name()?;
                        self.check_fn_expr_name(&fname, false, is_generator)?;
                        ExprKind::Function(self.function_full(
                            fname,
                            FuncKind::Normal,
                            line,
                            false,
                            is_generator,
                        )?)
                    }
                    "class" => {
                        let c = self.class(false)?;
                        if let Some(e) = self.ts_class_iife.take() {
                            return Ok(e);
                        }
                        ExprKind::Class(c)
                    }
                    "super" => {
                        self.advance();
                        if self.is(P::LParen) {
                            ExprKind::SuperCall(self.arguments()?)
                        } else if self.eat(P::Dot) {
                            if self.is(P::Hash) {
                                return Err(self.error("Unexpected private field"));
                            }
                            ExprKind::SuperMember(self.ident_name()?)
                        } else if self.eat(P::LBracket) {
                            let saved = self.no_in;
                            self.no_in = false;
                            let key = self.expression();
                            self.no_in = saved;
                            let key = key?;
                            self.expect(P::RBracket)?;
                            ExprKind::SuperIndex(Box::new(key))
                        } else {
                            return Err(self.error("'super' keyword unexpected here"));
                        }
                    }
                    "async" if self.is_kw_at(1, "function") && !self.peek_at(1).nl_before => {
                        self.span_start = Some(self.peek().start);
                        self.advance();
                        self.advance();
                        let is_generator = self.eat(P::Star);
                        let fname = self.fn_expr_name()?;
                        self.check_fn_expr_name(&fname, true, is_generator)?;
                        ExprKind::Function(self.function_full(
                            fname,
                            FuncKind::Normal,
                            line,
                            true,
                            is_generator,
                        )?)
                    }
                    "import" => {
                        self.advance();
                        if self.eat(P::Dot) {
                            let n = self.ident_name()?;
                            if &*n != "meta" {
                                return Err(self.error(
                                    "The only valid meta property for import is 'import.meta'",
                                ));
                            }
                            if self.script {
                                return Err(self.error("Cannot use 'import.meta' outside a module"));
                            }
                            ExprKind::ImportMeta
                        } else {
                            self.expect(P::LParen)?;
                            let saved = self.no_in;
                            self.no_in = false;
                            let spec = self.assignment();
                            self.no_in = saved;
                            let spec = spec?;
                            if self.eat(P::Comma) && !self.is(P::RParen) {
                                let _ = self.assignment()?;
                                self.eat(P::Comma);
                            }
                            self.expect(P::RParen)?;
                            ExprKind::DynImport(Box::new(spec))
                        }
                    }
                    n if self.reserved(n) && !(n == "await" && self.script && !self.in_async) => {
                        return Err(self.unexpected());
                    }
                    _ => {
                        self.check_ident_ref(&name)?;
                        self.advance();
                        if &*name == "arguments"
                            && let Some(top) = self.args_used.last_mut()
                        {
                            *top = true;
                        }
                        if self.ts {
                            self.ts_refs.insert(name.clone());
                        }
                        ExprKind::Ident(name)
                    }
                }
            }
            _ => return Err(self.unexpected()),
        };
        Ok(Expr { kind, line, col })
    }

    fn object_props(&mut self) -> R<Vec<PropDef>> {
        let mut props = Vec::new();
        while !self.eat(P::RBrace) {
            if self.is_eof() {
                return Err(self.unexpected());
            }
            if self.eat(P::Ellipsis) {
                props.push(PropDef::Spread(self.assignment()?));
            } else {
                let line = self.line();
                let kstart = self.peek().start;
                let plain_next = |p: &Parser| {
                    p.is_at(1, P::LParen)
                        || p.is_at(1, P::Colon)
                        || p.is_at(1, P::Comma)
                        || p.is_at(1, P::RBrace)
                        || p.is_at(1, P::Eq)
                        || (p.ts && p.is_at(1, P::Lt))
                };
                let mut is_async = false;
                let mut is_generator = false;
                let mut accessor = None;
                if self.is_kw("async") && !plain_next(self) {
                    if self.peek_at(1).nl_before {
                        return Err(self.error("Unexpected line terminator after 'async'"));
                    }
                    self.advance();
                    is_async = true;
                }
                if self.eat(P::Star) {
                    is_generator = true;
                }
                if !is_async
                    && !is_generator
                    && (self.is_kw("get") || self.is_kw("set"))
                    && !plain_next(self)
                {
                    accessor = Some(self.is_kw("set"));
                    self.advance();
                }
                if self.is(P::Hash) {
                    return Err(self.error("Unexpected private field"));
                }
                let (key, is_ident) = self.prop_key()?;
                let name = match &key {
                    PropKey::Named(n) => Some(n.clone()),
                    PropKey::Computed(_) => None,
                };
                if let Some(setter) = accessor {
                    self.span_start = Some(kstart);
                    let f = self.function_rest(name, FuncKind::Method, line)?;
                    check_accessor_params(&f, setter, line)?;
                    props.push(if setter {
                        PropDef::Setter(key, f)
                    } else {
                        PropDef::Getter(key, f)
                    });
                } else if self.is(P::LParen) || (self.ts && self.is(P::Lt)) {
                    let f = {
                        self.span_start = Some(kstart);
                        self.function_full(name, FuncKind::Method, line, is_async, is_generator)?
                    };
                    props.push(PropDef::Method(key, f));
                } else if is_async || is_generator {
                    return Err(self.unexpected());
                } else if self.eat(P::Colon) {
                    props.push(PropDef::KeyValue(key, self.assignment()?));
                } else if is_ident {
                    let PropKey::Named(n) = key else {
                        unreachable!()
                    };
                    if self.eat(P::Eq) {
                        props.push(PropDef::ShorthandDefault(n, self.assignment()?));
                    } else {
                        if self.reserved(&n) {
                            return Err(self.error(format!("Unexpected reserved word '{n}'")));
                        }
                        if &*n == "arguments"
                            && let Some(top) = self.args_used.last_mut()
                        {
                            *top = true;
                        }
                        if self.ts {
                            self.ts_refs.insert(n.clone());
                        }
                        props.push(PropDef::Shorthand(n));
                    }
                } else {
                    return Err(self.unexpected());
                }
            }
            if !self.eat(P::Comma) {
                self.expect(P::RBrace)?;
                break;
            }
        }
        Ok(props)
    }
}

fn private_text(key: &PropKey) -> Option<&str> {
    match key {
        PropKey::Named(n) if n.starts_with("\u{0}#") => {
            let body = &n[2..];
            Some(body.rsplit_once('@').map_or(body, |(a, _)| a))
        }
        _ => None,
    }
}

fn check_class_members(members: &[ClassMember], line: u32) -> R<()> {
    let mut seen: Vec<(String, u8, bool)> = Vec::new();
    for m in members {
        let (key, kind, is_static, is_field) = match m {
            ClassMember::Method { key, is_static, .. } => (key, 0u8, *is_static, false),
            ClassMember::Field { key, is_static, .. } => (key, 0u8, *is_static, true),
            ClassMember::Accessor {
                key,
                is_static,
                setter,
                ..
            } => (key, if *setter { 2u8 } else { 1u8 }, *is_static, false),
            ClassMember::StaticBlock(_) => continue,
        };
        if let Some(name) = private_text(key) {
            if name == "constructor" {
                return Err(SyntaxError::new(
                    "Classes may not have a private field named '#constructor'",
                    line,
                    1,
                ));
            }
            let clash = seen
                .iter()
                .filter(|(n, _, _)| n == name)
                .any(|(_, k, s)| !(kind != 0 && *k != 0 && *k != kind && *s == is_static))
                || seen.iter().filter(|(n, _, _)| n == name).count() > 1;
            if clash {
                return Err(SyntaxError::new(
                    format!("Identifier '#{name}' has already been declared"),
                    line,
                    1,
                ));
            }
            seen.push((name.to_string(), kind, is_static));
            continue;
        }
        let PropKey::Named(n) = key else {
            continue;
        };
        if is_field && (&**n == "constructor" || (is_static && &**n == "prototype")) {
            return Err(SyntaxError::new(
                format!("Classes may not have a field named '{n}'"),
                line,
                1,
            ));
        }
        if !is_field && is_static && &**n == "prototype" {
            return Err(SyntaxError::new(
                "Classes may not have a static property named 'prototype'",
                line,
                1,
            ));
        }
        if kind != 0 && !is_static && &**n == "constructor" {
            return Err(SyntaxError::new(
                "Class constructor may not be an accessor",
                line,
                1,
            ));
        }
    }
    Ok(())
}

fn check_accessor_params(f: &FuncDef, setter: bool, line: u32) -> R<()> {
    let ok = if setter {
        f.params.len() == 1 && f.rest.is_none()
    } else {
        f.params.is_empty() && f.rest.is_none()
    };
    if ok {
        return Ok(());
    }
    Err(SyntaxError::new(
        if setter {
            "Setter must have exactly one formal parameter"
        } else {
            "Getter must not have any formal parameters"
        },
        line,
        1,
    ))
}

fn call_pos(callee: &Expr, paren: (u32, u32)) -> (u32, u32) {
    match callee.kind {
        ExprKind::Ident(_) | ExprKind::Member { .. } if callee.col != 0 => {
            (callee.line, callee.col)
        }
        _ => paren,
    }
}

fn syn_stmt(kind: StmtKind, line: u32) -> Stmt {
    Stmt { kind, line }
}

fn syn_ident(name: &str, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Ident(name.into()),
        line,
        col: 0,
    }
}

fn syn_call(callee: Expr, args: Vec<Expr>, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Call {
            callee: Box::new(callee),
            args: args.into_iter().map(Arg::Expr).collect(),
            optional: false,
        },
        line,
        col: 0,
    }
}

fn syn_member(obj: Expr, name: &str, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Member {
            obj: Box::new(obj),
            name: name.into(),
            optional: false,
        },
        line,
        col: 0,
    }
}

fn syn_assign(name: &str, value: Expr, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Assign {
            op: None,
            target: AssignTarget::Simple(Box::new(syn_ident(name, line))),
            value: Box::new(value),
        },
        line,
        col: 0,
    }
}

fn using_res(id: u32) -> String {
    format!("%usingres{id}%")
}

fn using_async_finalizer(res: &str, id: u32, line: u32) -> Vec<Stmt> {
    let g = format!("%usingg{id}%");
    let st = format!("%usings{id}%");
    let x = format!("%usingx{id}%");
    let next =
        |args: Vec<Expr>| syn_call(syn_member(syn_ident(&g, line), "next", line), args, line);
    let awaited = Expr {
        kind: ExprKind::Await(Box::new(syn_member(syn_ident(&st, line), "value", line))),
        line,
        col: 0,
    };
    let step = syn_stmt(
        StmtKind::Try {
            block: vec![syn_stmt(
                StmtKind::Expr(syn_assign(&st, next(vec![awaited]), line)),
                line,
            )],
            param: Some(Pattern::Ident(x.as_str().into())),
            handler: Some(vec![syn_stmt(
                StmtKind::Expr(syn_assign(
                    &st,
                    syn_call(
                        syn_member(syn_ident(&g, line), "throw", line),
                        vec![syn_ident(&x, line)],
                        line,
                    ),
                    line,
                )),
                line,
            )]),
            finalizer: None,
        },
        line,
    );
    let test = Expr {
        kind: ExprKind::Unary(
            UnaryOp::Not,
            Box::new(syn_member(syn_ident(&st, line), "done", line)),
        ),
        line,
        col: 0,
    };
    vec![
        syn_stmt(
            StmtKind::Decl(
                DeclKind::Let,
                vec![(
                    Pattern::Ident(g.as_str().into()),
                    Some(syn_call(
                        syn_ident("%usingDisposeAsync%", line),
                        vec![syn_ident(res, line)],
                        line,
                    )),
                )],
            ),
            line,
        ),
        syn_stmt(
            StmtKind::Decl(
                DeclKind::Let,
                vec![(Pattern::Ident(st.as_str().into()), Some(next(Vec::new())))],
            ),
            line,
        ),
        syn_stmt(
            StmtKind::While(test, Box::new(syn_stmt(StmtKind::Block(vec![step]), line))),
            line,
        ),
    ]
}

fn using_wrap(
    mut items: Vec<Stmt>,
    id: u32,
    is_async: bool,
    line: u32,
    for_loop: bool,
) -> Vec<Stmt> {
    let n = items
        .iter()
        .take_while(|s| matches!(&s.kind, StmtKind::Expr(e) if matches!(e.kind, ExprKind::Str(_))))
        .count();
    let body = items.split_off(n);
    let res = using_res(id);
    items.push(syn_stmt(
        StmtKind::Decl(
            DeclKind::Const,
            vec![(
                Pattern::Ident(res.as_str().into()),
                Some(syn_call(syn_ident("%usingStack%", line), Vec::new(), line)),
            )],
        ),
        line,
    ));
    let err = if for_loop {
        format!("%usingfor{id}%")
    } else {
        format!("%usingerr{id}%")
    };
    let handler = vec![syn_stmt(
        StmtKind::Expr(syn_call(
            syn_ident("%usingError%", line),
            vec![syn_ident(&res, line), syn_ident(&err, line)],
            line,
        )),
        line,
    )];
    let finalizer = if is_async {
        using_async_finalizer(&res, id, line)
    } else {
        vec![syn_stmt(
            StmtKind::Expr(syn_call(
                syn_ident("%usingDispose%", line),
                vec![syn_ident(&res, line)],
                line,
            )),
            line,
        )]
    };
    items.push(syn_stmt(
        StmtKind::Try {
            block: body,
            param: Some(Pattern::Ident(err.as_str().into())),
            handler: Some(handler),
            finalizer: Some(finalizer),
        },
        line,
    ));
    items
}

fn using_loop(s: &mut Stmt) -> Option<&mut Stmt> {
    let StmtKind::Block(items) = &mut s.kind else {
        return None;
    };
    let Some(Stmt {
        kind:
            StmtKind::Try {
                block,
                param: Some(Pattern::Ident(p)),
                ..
            },
        ..
    }) = items.last_mut()
    else {
        return None;
    };
    if !p.starts_with("%usingfor") {
        return None;
    }
    block.last_mut()
}

pub fn body_use_strict(body: &[Stmt]) -> bool {
    for s in body {
        match &s.kind {
            StmtKind::Expr(e) => match &e.kind {
                ExprKind::Str(t) => {
                    if &**t == "use strict" {
                        return true;
                    }
                }
                _ => return false,
            },
            _ => return false,
        }
    }
    false
}

fn mixes_nullish(a: LogicalOp, b: LogicalOp) -> bool {
    (a == LogicalOp::Nullish) != (b == LogicalOp::Nullish)
}

fn check_strict_params(
    params: &[Param],
    rest: &Option<Pattern>,
    body: &[Stmt],
    line: u32,
    strict: bool,
    unique: bool,
) -> R<()> {
    let simple = rest.is_none()
        && params
            .iter()
            .all(|p| p.default.is_none() && matches!(p.pattern, Pattern::Ident(_)));
    let directive = body_use_strict(body);
    if !simple && directive {
        return Err(SyntaxError::new(
            "Illegal 'use strict' directive in function with non-simple parameter list",
            line,
            1,
        ));
    }
    let dup_error = !simple || strict || directive || unique;
    if !(strict || directive || dup_error) {
        return Ok(());
    }
    let mut names = Vec::new();
    for p in params {
        p.pattern.bound_names(&mut names);
    }
    if let Some(r) = rest {
        r.bound_names(&mut names);
    }
    for (i, n) in names.iter().enumerate() {
        if (strict || directive) && matches!(&**n, "eval" | "arguments") {
            return Err(SyntaxError::new(
                "Unexpected eval or arguments in strict mode",
                line,
                1,
            ));
        }
        if dup_error && names[..i].contains(n) {
            return Err(SyntaxError::new(
                format!("Duplicate parameter name '{n}'"),
                line,
                1,
            ));
        }
    }
    Ok(())
}

fn display_name(n: &JsStr) -> JsStr {
    match n.strip_prefix('\u{0}') {
        Some(body) if body.starts_with('#') => {
            body.rsplit_once('@').map_or(body, |(a, _)| a).into()
        }
        _ => n.clone(),
    }
}

fn is_tagged(args: &[Arg]) -> bool {
    matches!(args.first(), Some(Arg::Expr(e)) if matches!(e.kind, ExprKind::TemplateStrings(..)))
}
