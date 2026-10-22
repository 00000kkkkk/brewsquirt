use crate::ast::*;

use crate::number::number_to_string;
use crate::token::{P, SyntaxError, TemplatePart, Tok, Token};
use std::rc::Rc;

type R<T> = Result<T, SyntaxError>;

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

type ExportList = Vec<(Rc<str>, Rc<str>)>;

pub fn parse(src: &str) -> R<Vec<Stmt>> {
    parse_goal(src, false)
}

pub fn parse_script(src: &str) -> R<Vec<Stmt>> {
    parse_goal(src, true)
}

fn parse_goal(src: &str, script: bool) -> R<Vec<Stmt>> {
    let toks = crate::lexer::tokenize_goal(src, script)?;
    let mut parser = Parser {
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
    };
    parser.strict = !script || parser.has_use_strict_from(0);
    if script && parser.strict {
        parser.check_prologue_octal(0)?;
    }
    parser.program()
}

pub fn parse_eval(src: &str, ctx: &EvalCtx) -> R<Vec<Stmt>> {
    let toks = crate::lexer::tokenize_goal(src, true)?;
    let mut parser = Parser {
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

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    no_in: bool,
    fn_depth: u32,
    in_async: bool,
    in_generator: bool,
    class_ids: Vec<(u32, Rc<Vec<Rc<str>>>)>,
    field_init: u32,
    script: bool,
    nt_ok: bool,
    strict: bool,
    static_block: bool,
    in_params: bool,
    single: bool,
    labeled_fn: bool,
    saw_eval: bool,
    src: Rc<str>,
    last_paren: (usize, usize),
    at_top: bool,
    args_used: Vec<bool>,
    span_start: Option<u32>,
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
        let mut new =
            crate::lexer::tokenize_from(&self.src, self.script, start, line, col, as_regex)?;
        if let Some(first) = new.first_mut() {
            first.nl_before = nl;
        }
        self.toks.truncate(self.pos);
        self.toks.append(&mut new);
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

    fn advance(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
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

    fn word(&self) -> Option<Rc<str>> {
        match &self.peek().tok {
            Tok::Ident(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn private_name(&mut self) -> R<Rc<str>> {
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

    fn prescan_private(&self) -> Vec<Rc<str>> {
        let mut out: Vec<Rc<str>> = Vec::new();
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
                                && matches!(&**w, "static" | "get" | "set" | "async"));
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

    fn ident_name(&mut self) -> R<Rc<str>> {
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
            return Err(self.error(format!("Unexpected reserved word '{s}'")));
        }
        Ok(())
    }

    fn binding_ident(&mut self) -> R<Rc<str>> {
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

    fn string_lit(&mut self) -> R<Rc<str>> {
        match &self.peek().tok {
            Tok::Str(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            _ => Err(self.unexpected()),
        }
    }

    fn module_name(&mut self) -> R<Rc<str>> {
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
        let mut out = Vec::new();
        while !self.is_eof() {
            self.at_top = true;
            out.push(self.statement()?);
        }
        Ok(out)
    }

    fn block(&mut self) -> R<Vec<Stmt>> {
        self.expect(P::LBrace)?;
        let mut out = Vec::new();
        while !self.eat(P::RBrace) {
            if self.is_eof() {
                return Err(self.unexpected());
            }
            out.push(self.statement()?);
        }
        Ok(out)
    }

    fn statement(&mut self) -> R<Stmt> {
        let line = self.line();
        let kind = self.statement_kind()?;
        Ok(Stmt { kind, line })
    }

    fn statement_kind(&mut self) -> R<StmtKind> {
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
        match word.as_deref() {
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
                Ok(StmtKind::Function(f))
            }
            Some("class") => Ok(StmtKind::Class(self.class(true)?)),
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
                let label: Rc<str> = w.into();
                self.advance();
                self.advance();
                let body = self.sub_statement()?;
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
            let init = if self.eat(P::Eq) {
                Some(self.assignment()?)
            } else {
                None
            };
            if init.is_none() {
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
        self.semicolon()?;
        let mut bound: Vec<&Rc<str>> = default.iter().chain(namespace.iter()).collect();
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
            if matches!(self.peek().tok, Tok::Str(_)) {
                string_local = true;
            }
            let local = self.module_name()?;
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
            if self.is_kw("class") {
                if matches!(&self.peek_at(1).tok, Tok::Ident(n) if &**n != "extends") {
                    let c = self.class(true)?;
                    return Ok(StmtKind::Export(ExportDecl::DefaultDecl(Box::new(Stmt {
                        kind: StmtKind::Class(c),
                        line,
                    }))));
                }
                let c = self.class(false)?;
                return Ok(StmtKind::Export(ExportDecl::Default(Expr {
                    kind: ExprKind::Class(c),
                    line,
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
                self.semicolon()?;
                return Ok(StmtKind::Export(ExportDecl::From {
                    source,
                    names: Some(vec![("*".into(), exported)]),
                }));
            }
            self.expect_kw("from")?;
            let source = self.string_lit()?;
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
            _ => Err(SyntaxError::new("Unexpected export", s.line, 1)),
        }
    }

    fn jump_label(&mut self) -> R<Option<Rc<str>>> {
        match self.word() {
            Some(w) if !self.peek().nl_before && !self.reserved(&w) => {
                self.advance();
                Ok(Some(w))
            }
            _ => Ok(None),
        }
    }

    fn function_rest(
        &mut self,
        name: Option<Rc<str>>,
        kind: FuncKind,
        line: u32,
    ) -> R<Rc<FuncDef>> {
        self.function_full(name, kind, line, false, false)
    }

    fn function_full(
        &mut self,
        name: Option<Rc<str>>,
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
        let parsed = self
            .params()
            .and_then(|(params, rest)| Ok((params, rest, self.function_body()?)));
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

    fn fn_expr_name(&mut self) -> R<Option<Rc<str>>> {
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
        name: &Option<Rc<str>>,
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
            if self.eat(P::Ellipsis) {
                rest = Some(self.binding_pattern()?);
                break;
            }
            let pattern = self.binding_pattern()?;
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
                let s: Rc<str> = number_to_string(*n).into();
                self.advance();
                Ok((PropKey::Named(s), false))
            }
            Tok::BigInt(s) => {
                let s = s.clone();
                self.advance();
                let text: Rc<str> = match crate::bigint::from_string(&s) {
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

    fn class(&mut self, require_name: bool) -> R<Rc<ClassDef>> {
        let saved = std::mem::replace(&mut self.strict, true);
        let r = self.class_inner(require_name);
        self.strict = saved;
        r
    }

    fn class_inner(&mut self, require_name: bool) -> R<Rc<ClassDef>> {
        let line = self.line();
        let start = self.peek().start;
        self.expect_kw("class")?;
        let name = if matches!(self.peek().tok, Tok::Ident(_)) && !self.is_kw("extends") {
            Some(self.binding_ident()?)
        } else if require_name {
            return Err(self.error("Class statements require a name"));
        } else {
            None
        };
        let superclass = if self.eat_kw("extends") {
            Some(self.call_member()?)
        } else {
            None
        };
        let id = fresh_class_id();
        let declared = self.prescan_private();
        self.class_ids.push((id, Rc::new(declared)));
        let parsed = self.class_body(superclass.is_some());
        self.class_ids.pop();
        let (constructor, members) = parsed?;
        check_class_members(&members, line)?;
        Ok(Rc::new(ClassDef {
            name,
            superclass,
            constructor,
            members,
            line,
            span: (start, self.prev_end()),
        }))
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
            let mline = self.line();
            let mut mstart = self.peek().start;
            let ends_name = |p: &Parser, n: usize| {
                p.is_at(n, P::LParen)
                    || p.is_at(n, P::Eq)
                    || p.is_at(n, P::Semi)
                    || p.is_at(n, P::RBrace)
                    || p.peek_at(n).nl_before
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
                check_accessor_params(&f, setter, mline)?;
                members.push(ClassMember::Accessor {
                    key,
                    func: f,
                    is_static,
                    setter,
                });
                continue;
            }
            if self.is(P::LParen) {
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
                let f = self.function_full(fname, kind, mline, is_async, is_generator)?;
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
            });
        }
        Ok(first)
    }

    fn is_arrow_ahead(&self) -> bool {
        match &self.peek().tok {
            Tok::Ident(n) if !self.reserved(n) => {
                self.is_at(1, P::Arrow) && !self.peek_at(1).nl_before
            }
            Tok::Punct(P::LParen) => {
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
        })
    }

    fn arrow_parts(&mut self, is_async: bool) -> R<(Vec<Param>, Option<Pattern>, FuncBody)> {
        let (params, rest) = if self.is(P::LParen) {
            self.params()?
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
            && (self.is_at(1, P::LParen) || matches!(self.peek_at(1).tok, Tok::Ident(_)))
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
        let line = self.line();
        let lhs_start = self.pos;
        let lhs = self.conditional()?;
        if matches!(self.peek().tok, Tok::Regex(..)) {
            self.relex(false)?;
        }
        if let Some(op) = self.assign_op() {
            let parenthesized = self.last_paren == (lhs_start, self.pos);
            if let ExprKind::Ident(n) = &lhs.kind {
                if self.strict && matches!(&**n, "eval" | "arguments") {
                    return Err(self.error("Unexpected eval or arguments in strict mode"));
                }
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
                            })?,
                            default: None,
                        }),
                        PropDef::ShorthandDefault(n, d) => out.push(ObjPatProp {
                            key: PropKey::Named(n.clone()),
                            value: self.expr_to_pattern(Expr {
                                kind: ExprKind::Ident(n),
                                line,
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
            let line = self.line();
            if let BinKind::Logical(l) = op {
                if built.is_some_and(|b| mixes_nullish(b, l)) {
                    return Err(self.error("Cannot mix '??' with '||' or '&&' without parentheses"));
                }
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
                    if let ExprKind::Logical(r, ..) = &right.kind {
                        if mixes_nullish(*r, l) && self.last_paren != (right_start, self.pos) {
                            return Err(
                                self.error("Cannot mix '??' with '||' or '&&' without parentheses")
                            );
                        }
                    }
                    built = Some(l);
                }
                BinKind::Bin(_) => built = None,
            }
            let kind = match op {
                BinKind::Bin(b) => ExprKind::Binary(b, Box::new(left), Box::new(right)),
                BinKind::Logical(l) => ExprKind::Logical(l, Box::new(left), Box::new(right)),
            };
            left = Expr { kind, line };
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
        let line = self.line();
        let op = match &self.peek().tok {
            Tok::Punct(P::Bang) => UnaryOp::Not,
            Tok::Punct(P::Minus) => UnaryOp::Neg,
            Tok::Punct(P::Plus) => UnaryOp::Plus,
            Tok::Punct(P::Tilde) => UnaryOp::BitNot,
            Tok::Ident(s) if &**s == "typeof" => UnaryOp::Typeof,
            Tok::Ident(s) if &**s == "void" => UnaryOp::Void,
            Tok::Ident(s) if &**s == "delete" => UnaryOp::Delete,
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
        if op == UnaryOp::Neg {
            if let ExprKind::Number(n) = operand.kind {
                return Ok(Expr {
                    kind: ExprKind::Number(-n),
                    line,
                });
            }
        }
        Ok(Expr {
            kind: ExprKind::Unary(op, Box::new(operand)),
            line,
        })
    }

    fn postfix(&mut self) -> R<Expr> {
        let e = self.call_member()?;
        if (self.is(P::PlusPlus) || self.is(P::MinusMinus)) && !self.peek().nl_before {
            self.check_update_target(&e)?;
            let inc = self.is(P::PlusPlus);
            let line = self.line();
            self.advance();
            return Ok(Expr {
                kind: ExprKind::Update {
                    inc,
                    prefix: false,
                    target: Box::new(e),
                },
                line,
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
            let line = self.line();
            if self.eat(P::Dot) {
                let name = self.ident_name()?;
                e = Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(e),
                        name,
                        optional: false,
                    },
                    line,
                };
            } else if self.eat(P::QuestionDot) {
                chain = true;
                if self.is(P::LParen) {
                    let args = self.arguments()?;
                    e = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                            optional: true,
                        },
                        line,
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
                    };
                } else {
                    let name = self.ident_name()?;
                    e = Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(e),
                            name,
                            optional: true,
                        },
                        line,
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
                };
            } else if self.is(P::LParen) {
                let args = self.arguments()?;
                let direct = matches!(&e.kind, ExprKind::Ident(n) if &**n == "eval");
                e = if direct {
                    if self.field_init == 0 {
                        if let Some(top) = self.args_used.last_mut() {
                            *top = true;
                        }
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
                    }
                } else {
                    Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(e),
                            args,
                            optional: false,
                        },
                        line,
                    }
                };
            } else if matches!(self.peek().tok, Tok::Template(_)) {
                if chain {
                    return Err(self.error("Invalid tagged template on optional chain"));
                }
                let Tok::Template(parts) = self.advance().tok else {
                    unreachable!()
                };
                let (quasis, exprs) = self.template_parts(parts)?;
                let (cooked, raw) = quasis.into_iter().map(|(c, r)| (c.ok(), r)).unzip();
                let mut args = vec![Arg::Expr(Expr {
                    kind: ExprKind::TemplateStrings(cooked, raw),
                    line,
                })];
                args.extend(exprs.into_iter().map(Arg::Expr));
                e = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(e),
                        args,
                        optional: false,
                    },
                    line,
                };
            } else {
                break;
            }
        }
        if chain {
            let line = e.line;
            e = Expr {
                kind: ExprKind::OptChain(Box::new(e)),
                line,
            };
        }
        Ok(e)
    }

    fn new_expr(&mut self) -> R<Expr> {
        let line = self.line();
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
            let mline = self.line();
            if self.eat(P::Dot) {
                let name = self.ident_name()?;
                callee = Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(callee),
                        name,
                        optional: false,
                    },
                    line: mline,
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
                };
            } else if matches!(self.peek().tok, Tok::Template(_)) {
                let Tok::Template(parts) = self.advance().tok else {
                    unreachable!()
                };
                let (quasis, exprs) = self.template_parts(parts)?;
                let (cooked, raw) = quasis.into_iter().map(|(c, r)| (c.ok(), r)).unzip();
                let mut args = vec![Arg::Expr(Expr {
                    kind: ExprKind::TemplateStrings(cooked, raw),
                    line: mline,
                })];
                args.extend(exprs.into_iter().map(Arg::Expr));
                callee = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(callee),
                        args,
                        optional: false,
                    },
                    line: mline,
                };
            } else {
                break;
            }
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
        })
    }

    fn template(&mut self, parts: Vec<TemplatePart>, line: u32) -> R<Expr> {
        let (quasis, exprs) = self.template_parts(parts)?;
        let quasis = quasis
            .into_iter()
            .map(|q| q.0)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Expr {
            kind: ExprKind::Template(quasis, exprs),
            line,
        })
    }

    #[allow(clippy::type_complexity)]
    fn template_parts(
        &mut self,
        parts: Vec<TemplatePart>,
    ) -> R<(Vec<(Result<Rc<str>, SyntaxError>, Rc<str>)>, Vec<Expr>)> {
        let mut quasis = Vec::new();
        let mut exprs = Vec::new();
        for part in parts {
            match part {
                TemplatePart::Str(s, raw) => quasis.push((s, raw)),
                TemplatePart::Expr(toks) => {
                    let mut sub = Parser {
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
                    };
                    if sub.is_eof() {
                        return Err(sub.error("Unexpected token '}'"));
                    }
                    let e = sub.expression()?;
                    if !sub.is_eof() {
                        return Err(sub.unexpected());
                    }
                    if sub.args_used.first() == Some(&true) {
                        if let Some(top) = self.args_used.last_mut() {
                            *top = true;
                        }
                    }
                    exprs.push(e);
                }
            }
        }
        Ok((quasis, exprs))
    }

    fn primary(&mut self) -> R<Expr> {
        let line = self.line();
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
                let Tok::Template(parts) = self.advance().tok else {
                    unreachable!()
                };
                return self.template(parts, line);
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
                    "class" => ExprKind::Class(self.class(false)?),
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
                        if &*name == "arguments" {
                            if let Some(top) = self.args_used.last_mut() {
                                *top = true;
                            }
                        }
                        ExprKind::Ident(name)
                    }
                }
            }
            _ => return Err(self.unexpected()),
        };
        Ok(Expr { kind, line })
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
                } else if self.is(P::LParen) {
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
                        if &*n == "arguments" {
                            if let Some(top) = self.args_used.last_mut() {
                                *top = true;
                            }
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

fn display_name(n: &Rc<str>) -> Rc<str> {
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
