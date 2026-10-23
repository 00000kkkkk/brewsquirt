use super::*;

fn starts_expression(tok: &Tok) -> bool {
    match tok {
        Tok::Ident(n) => !matches!(&**n, "in" | "instanceof" | "as" | "satisfies" | "of"),
        Tok::Num(_) | Tok::Str(_) | Tok::BigInt(_) | Tok::Template(_) | Tok::Regex(..) => true,
        Tok::Punct(p) => matches!(
            p,
            P::LParen
                | P::LBracket
                | P::LBrace
                | P::Bang
                | P::Tilde
                | P::PlusPlus
                | P::MinusMinus
                | P::Hash
                | P::At
                | P::Slash
                | P::SlashEq
                | P::Lt
                | P::Plus
                | P::Minus
                | P::Ellipsis
        ),
        Tok::JsxText(_) | Tok::Eof => false,
    }
}

fn identifier_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(crate::lexer::is_id_start) && chars.all(crate::lexer::is_id_continue)
}

fn num(n: f64, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Number(n),
        line,
    }
}

fn string(s: &JsStr, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Str(s.clone()),
        line,
    }
}

fn assign(target: Expr, value: Expr, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Assign {
            op: None,
            target: AssignTarget::Simple(Box::new(target)),
            value: Box::new(value),
        },
        line,
    }
}

fn index(obj: &JsStr, key: Expr, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Index {
            obj: Box::new(syn_ident(obj, line)),
            index: Box::new(key),
            optional: false,
        },
        line,
    }
}

fn member(obj: Expr, name: &JsStr, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Member {
            obj: Box::new(obj),
            name: name.clone(),
            optional: false,
        },
        line,
    }
}

fn expr_stmt(e: Expr) -> Stmt {
    let line = e.line;
    Stmt {
        kind: StmtKind::Expr(e),
        line,
    }
}

impl Parser {
    pub(super) fn ts_checkpoint(&self) -> (usize, usize) {
        (self.pos, self.ts_splits.len())
    }

    pub(super) fn ts_restore(&mut self, cp: (usize, usize)) {
        while self.ts_splits.len() > cp.1 {
            if let Some((i, tok)) = self.ts_splits.pop() {
                self.toks[i] = tok;
            }
        }
        self.pos = cp.0;
    }

    fn ts_gt(&mut self) -> R<()> {
        let rest = match self.peek().tok {
            Tok::Punct(P::Gt) => {
                self.advance();
                return Ok(());
            }
            Tok::Punct(P::Shr) => P::Gt,
            Tok::Punct(P::UShr) => P::Shr,
            _ => return Err(self.error("'>' expected")),
        };
        let old = self.toks[self.pos].clone();
        self.ts_splits.push((self.pos, old));
        let t = &mut self.toks[self.pos];
        t.tok = Tok::Punct(rest);
        t.start += 1;
        t.col += 1;
        t.nl_before = false;
        Ok(())
    }

    pub(super) fn ts_skip_group(&mut self) -> R<()> {
        let close = self.pairs.get(self.pos).copied().unwrap_or(u32::MAX);
        if close == u32::MAX {
            return Err(self.unexpected());
        }
        self.pos = (close as usize + 1).min(self.toks.len() - 1);
        Ok(())
    }

    fn ts_fn_type_ahead(&self) -> bool {
        match &self.peek().tok {
            Tok::Punct(P::Lt) => true,
            Tok::Punct(P::LParen) => {
                let close = self.pairs.get(self.pos).copied().unwrap_or(u32::MAX);
                close != u32::MAX
                    && matches!(
                        self.toks.get(close as usize + 1).map(|t| &t.tok),
                        Some(Tok::Punct(P::Arrow))
                    )
            }
            Tok::Ident(n) if &**n == "new" => true,
            Tok::Ident(n) if &**n == "abstract" => self.is_kw_at(1, "new"),
            _ => false,
        }
    }

    pub(super) fn ts_arrow_return_type(&mut self) -> R<()> {
        let cp = self.ts_checkpoint();
        if self.ts_type().is_ok() && self.is(P::Arrow) {
            return Ok(());
        }
        self.ts_restore(cp);
        self.ts_arrow_ret = true;
        self.ts_type()
    }

    pub(super) fn ts_type(&mut self) -> R<()> {
        let arrow_ret = std::mem::take(&mut self.ts_arrow_ret);
        if !arrow_ret && self.ts_fn_type_ahead() {
            self.eat_kw("abstract");
            self.eat_kw("new");
            if self.is(P::Lt) {
                self.ts_type_params()?;
            }
            self.ts_skip_group()?;
            self.expect(P::Arrow)?;
            let saved = std::mem::replace(&mut self.ts_in_extends, false);
            let r = self.ts_type();
            self.ts_in_extends = saved;
            return r;
        }
        self.ts_union()?;
        if !self.ts_in_extends && self.is_kw("extends") && !self.peek().nl_before {
            self.advance();
            let saved = std::mem::replace(&mut self.ts_in_extends, true);
            let r = self.ts_type();
            self.ts_in_extends = saved;
            r?;
            self.expect(P::Question)?;
            self.ts_type()?;
            self.expect(P::Colon)?;
            self.ts_type()?;
        }
        Ok(())
    }

    fn ts_union(&mut self) -> R<()> {
        self.eat(P::Pipe);
        self.ts_intersection()?;
        while self.is(P::Pipe) {
            self.advance();
            self.ts_intersection()?;
        }
        Ok(())
    }

    fn ts_intersection(&mut self) -> R<()> {
        self.eat(P::Amp);
        self.ts_operator()?;
        while self.is(P::Amp) {
            self.advance();
            self.ts_operator()?;
        }
        Ok(())
    }

    fn ts_type_starts_at(&self, n: usize) -> bool {
        matches!(
            self.peek_at(n).tok,
            Tok::Ident(_)
                | Tok::Str(_)
                | Tok::Num(_)
                | Tok::BigInt(_)
                | Tok::Template(_)
                | Tok::Punct(P::LParen | P::LBracket | P::LBrace | P::Lt | P::Minus)
        )
    }

    fn ts_operator(&mut self) -> R<()> {
        if let Tok::Ident(n) = &self.peek().tok {
            let n = n.clone();
            match &*n {
                "keyof" | "unique" | "readonly" if self.ts_type_starts_at(1) => {
                    self.advance();
                    return self.ts_operator();
                }
                "infer" if matches!(self.peek_at(1).tok, Tok::Ident(_)) => {
                    self.advance();
                    self.advance();
                    if self.is_kw("extends") {
                        let cp = self.ts_checkpoint();
                        self.advance();
                        let saved = std::mem::replace(&mut self.ts_in_extends, true);
                        let ok = self.ts_type().is_ok();
                        self.ts_in_extends = saved;
                        if !ok || (!self.ts_in_extends && self.is(P::Question)) {
                            self.ts_restore(cp);
                        }
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        self.ts_primary()?;
        while self.is(P::LBracket) && !self.peek().nl_before {
            self.ts_skip_group()?;
        }
        Ok(())
    }

    fn ts_primary(&mut self) -> R<()> {
        match &self.peek().tok {
            Tok::Punct(P::LParen) => {
                self.advance();
                let saved = std::mem::replace(&mut self.ts_in_extends, false);
                let r = self.ts_type();
                self.ts_in_extends = saved;
                r?;
                self.expect(P::RParen)
            }
            Tok::Punct(P::LBracket) | Tok::Punct(P::LBrace) => self.ts_skip_group(),
            Tok::Str(_) | Tok::Num(_) | Tok::BigInt(_) | Tok::Template(_) => {
                self.advance();
                Ok(())
            }
            Tok::Punct(P::Minus) if matches!(self.peek_at(1).tok, Tok::Num(_) | Tok::BigInt(_)) => {
                self.advance();
                self.advance();
                Ok(())
            }
            Tok::Ident(n) => {
                let n = n.clone();
                self.advance();
                match &*n {
                    "typeof" => {
                        if self.is_kw("import") {
                            self.advance();
                            self.ts_skip_group()?;
                        } else {
                            self.ident_name()?;
                        }
                        while self.eat(P::Dot) {
                            self.eat(P::Hash);
                            self.ident_name()?;
                        }
                    }
                    "import" if self.is(P::LParen) => {
                        self.ts_skip_group()?;
                        while self.eat(P::Dot) {
                            self.ident_name()?;
                        }
                    }
                    "asserts"
                        if matches!(self.peek().tok, Tok::Ident(_)) && !self.peek().nl_before =>
                    {
                        self.advance();
                        if self.is_kw("is") && !self.peek().nl_before {
                            self.advance();
                            self.ts_type()?;
                        }
                        return Ok(());
                    }
                    _ => {
                        while self.eat(P::Dot) {
                            self.ident_name()?;
                        }
                    }
                }
                if self.is(P::Lt) && !self.peek().nl_before {
                    self.ts_type_args()?;
                }
                if self.is_kw("is") && !self.peek().nl_before {
                    self.advance();
                    self.ts_type()?;
                }
                Ok(())
            }
            _ => Err(self.error("Type expected")),
        }
    }

    pub(super) fn ts_type_args(&mut self) -> R<()> {
        self.expect(P::Lt)?;
        let saved = std::mem::replace(&mut self.ts_in_extends, false);
        let r = self.ts_type_list();
        self.ts_in_extends = saved;
        r?;
        self.ts_gt()
    }

    fn ts_type_list(&mut self) -> R<()> {
        loop {
            self.ts_type()?;
            if !self.eat(P::Comma) || self.is(P::Gt) {
                return Ok(());
            }
        }
    }

    pub(super) fn ts_type_params(&mut self) -> R<()> {
        self.expect(P::Lt)?;
        let saved = std::mem::replace(&mut self.ts_in_extends, false);
        let r = self.ts_type_params_inner();
        self.ts_in_extends = saved;
        r?;
        self.ts_gt()
    }

    fn ts_type_params_inner(&mut self) -> R<()> {
        while !self.is(P::Gt) {
            while (self.is_kw("const") || self.is_kw("in") || self.is_kw("out"))
                && matches!(self.peek_at(1).tok, Tok::Ident(_))
            {
                self.advance();
            }
            self.ident_name()?;
            if self.eat_kw("extends") {
                self.ts_type()?;
            }
            if self.eat(P::Eq) {
                self.ts_type()?;
            }
            if !self.eat(P::Comma) {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn ts_annotation(&mut self) -> R<()> {
        if self.eat(P::Colon) {
            self.ts_type()?;
        }
        Ok(())
    }

    pub(super) fn ts_heritage(&mut self) -> R<()> {
        loop {
            self.ident_name()?;
            while self.eat(P::Dot) {
                self.ident_name()?;
            }
            if self.is(P::Lt) {
                self.ts_type_args()?;
            }
            if !self.eat(P::Comma) {
                return Ok(());
            }
        }
    }

    pub(super) fn ts_try_type_args(&mut self) -> bool {
        let cp = self.ts_checkpoint();
        if self.ts_type_args().is_ok() {
            let t = self.peek();
            let follows = match &t.tok {
                Tok::Punct(P::LParen) | Tok::Template(_) => true,
                Tok::Punct(P::Lt | P::Gt | P::Plus | P::Minus | P::Eq) => false,
                other => t.nl_before || !starts_expression(other),
            };
            if follows {
                return true;
            }
        }
        self.ts_restore(cp);
        false
    }

    pub(super) fn ts_return_arrow_ahead(&mut self, after_paren: usize) -> bool {
        if !matches!(
            self.toks.get(after_paren).map(|t| &t.tok),
            Some(Tok::Punct(P::Colon))
        ) {
            return false;
        }
        let cp = self.ts_checkpoint();
        self.pos = after_paren + 1;
        let ok = self.ts_arrow_return_type().is_ok() && self.is(P::Arrow) && !self.peek().nl_before;
        self.ts_arrow_ret = false;
        self.ts_restore(cp);
        ok
    }

    pub(super) fn ts_generic_arrow_ahead(&mut self) -> bool {
        let cp = self.ts_checkpoint();
        let mut ok = self.ts_type_params().is_ok() && self.is(P::LParen);
        if ok {
            let close = self.pairs.get(self.pos).copied().unwrap_or(u32::MAX);
            ok = close != u32::MAX
                && {
                    let after = close as usize + 1;
                    matches!(self.toks.get(after), Some(t) if matches!(t.tok, Tok::Punct(P::Arrow)) && !t.nl_before)
                        || self.ts_return_arrow_ahead(after)
                };
        }
        self.ts_restore(cp);
        ok
    }

    pub(super) fn ts_member_modifiers(&mut self) -> (bool, bool) {
        let mut declare = false;
        let mut is_abstract = false;
        while let Tok::Ident(w) = &self.peek().tok {
            if self.peek().escaped
                || !matches!(
                    &**w,
                    "public"
                        | "private"
                        | "protected"
                        | "readonly"
                        | "override"
                        | "declare"
                        | "abstract"
                        | "accessor"
                )
            {
                break;
            }
            let continues = matches!(
                self.peek_at(1).tok,
                Tok::Ident(_)
                    | Tok::Str(_)
                    | Tok::Num(_)
                    | Tok::BigInt(_)
                    | Tok::Punct(P::LBracket | P::Hash | P::Star)
            );
            if !continues {
                break;
            }
            declare |= &**w == "declare";
            is_abstract |= &**w == "abstract";
            self.advance();
        }
        (declare, is_abstract)
    }

    pub(super) fn ts_param_modifiers(&mut self) -> bool {
        let mut any = false;
        while let Tok::Ident(w) = &self.peek().tok {
            if self.peek().escaped
                || !matches!(
                    &**w,
                    "public" | "private" | "protected" | "readonly" | "override"
                )
            {
                break;
            }
            if !matches!(
                self.peek_at(1).tok,
                Tok::Ident(_) | Tok::Punct(P::LBrace | P::LBracket)
            ) {
                break;
            }
            any = true;
            self.advance();
        }
        any
    }

    pub(super) fn ts_index_signature_ahead(&self) -> bool {
        self.is(P::LBracket)
            && matches!(self.peek_at(1).tok, Tok::Ident(_))
            && self.is_at(2, P::Colon)
    }

    pub(super) fn ts_statement(&mut self, word: &str) -> R<Option<StmtKind>> {
        if self.peek().escaped {
            return Ok(None);
        }
        let next = self.peek_at(1);
        let next_ident = matches!(next.tok, Tok::Ident(_)) && !next.nl_before;
        let next_nl = next.nl_before;
        match word {
            "interface" if next_ident => {
                self.advance();
                self.ident_name()?;
                if self.is(P::Lt) {
                    self.ts_type_params()?;
                }
                if self.eat_kw("extends") {
                    self.ts_heritage()?;
                }
                self.ts_skip_group()?;
                Ok(Some(StmtKind::Empty))
            }
            "type" if next_ident && (self.is_at(2, P::Eq) || self.is_at(2, P::Lt)) => {
                self.advance();
                self.ident_name()?;
                if self.is(P::Lt) {
                    self.ts_type_params()?;
                }
                self.expect(P::Eq)?;
                self.ts_type()?;
                self.semicolon()?;
                Ok(Some(StmtKind::Empty))
            }
            "declare" if next_ident => {
                self.advance();
                if self.is_kw("global") || self.is_kw("module") || self.is_kw("namespace") {
                    self.advance();
                    while matches!(self.peek().tok, Tok::Ident(_) | Tok::Str(_)) || self.is(P::Dot)
                    {
                        if self.is(P::LBrace) {
                            break;
                        }
                        self.advance();
                    }
                    if self.is(P::LBrace) {
                        self.ts_skip_group()?;
                    } else {
                        self.semicolon()?;
                    }
                    return Ok(Some(StmtKind::Empty));
                }
                self.ts_ambient += 1;
                let r = self.statement();
                self.ts_ambient -= 1;
                r?;
                Ok(Some(StmtKind::Empty))
            }
            "abstract" if self.is_kw_at(1, "class") && !next_nl => {
                self.advance();
                Ok(Some(StmtKind::Class(self.class(true)?)))
            }
            "enum" if next_ident => Ok(Some(self.ts_enum()?)),
            "const" if self.is_kw_at(1, "enum") && !next_nl => {
                self.advance();
                Ok(Some(self.ts_enum()?))
            }
            "namespace" | "module"
                if next_ident && (self.is_at(2, P::LBrace) || self.is_at(2, P::Dot)) =>
            {
                Ok(Some(self.ts_namespace()?))
            }
            "export" if self.is_at(1, P::Eq) => {
                self.advance();
                self.advance();
                let line = self.line();
                let value = self.assignment()?;
                self.semicolon()?;
                let target = member(syn_ident("module", line), &"exports".into(), line);
                Ok(Some(StmtKind::Expr(assign(target, value, line))))
            }
            "import" if next_ident && self.is_at(2, P::Eq) => {
                self.advance();
                let name = self.binding_ident()?;
                self.expect(P::Eq)?;
                let value = self.assignment()?;
                self.semicolon()?;
                Ok(Some(StmtKind::Decl(
                    DeclKind::Const,
                    vec![(Pattern::Ident(name), Some(value))],
                )))
            }
            _ => Ok(None),
        }
    }

    fn ts_iife(&self, param: &JsStr, body: Vec<Stmt>, seed: Expr, line: u32, start: u32) -> Expr {
        let f = FuncDef {
            name: None,
            params: vec![Param {
                pattern: Pattern::Ident(param.clone()),
                default: None,
            }],
            rest: None,
            body: FuncBody::Block(body),
            kind: FuncKind::Normal,
            is_async: false,
            is_generator: false,
            uses_arguments: false,
            strict: self.strict,
            has_eval: false,
            line,
            span: (start, self.prev_end()),
        };
        let arg = Expr {
            kind: ExprKind::Logical(
                LogicalOp::Or,
                Box::new(seed),
                Box::new(Expr {
                    kind: ExprKind::Object(Vec::new()),
                    line,
                }),
            ),
            line,
        };
        syn_call(
            Expr {
                kind: ExprKind::Function(Rc::new(f)),
                line,
            },
            vec![arg],
            line,
        )
    }

    fn ts_enum(&mut self) -> R<StmtKind> {
        let line = self.line();
        let start = self.peek().start;
        self.advance();
        let name = self.binding_ident()?;
        self.expect(P::LBrace)?;
        let cur: JsStr = "%enumv%".into();
        let mut body = vec![syn_stmt(
            StmtKind::Decl(DeclKind::Let, vec![(Pattern::Ident(cur.clone()), None)]),
            line,
        )];
        let mut first = true;
        while !self.eat(P::RBrace) {
            let mline = self.line();
            let key: JsStr = match &self.peek().tok {
                Tok::Str(s) => {
                    let s = s.clone();
                    self.advance();
                    s
                }
                Tok::Punct(P::LBracket) => {
                    self.advance();
                    let s = self.string_lit()?;
                    self.expect(P::RBracket)?;
                    s
                }
                _ => self.ident_name()?,
            };
            let init = if self.eat(P::Eq) {
                Some(self.assignment()?)
            } else {
                None
            };
            let string_valued = matches!(
                &init,
                Some(Expr {
                    kind: ExprKind::Str(_) | ExprKind::Template(..),
                    ..
                })
            );
            let value = match init {
                Some(e) => e,
                None if first => num(0.0, mline),
                None => Expr {
                    kind: ExprKind::Binary(
                        BinOp::Add,
                        Box::new(syn_ident(&cur, mline)),
                        Box::new(num(1.0, mline)),
                    ),
                    line: mline,
                },
            };
            body.push(expr_stmt(assign(syn_ident(&cur, mline), value, mline)));
            if identifier_name(&key) && !is_reserved(&key) && key != name {
                body.push(syn_stmt(
                    StmtKind::Decl(
                        DeclKind::Let,
                        vec![(Pattern::Ident(key.clone()), Some(syn_ident(&cur, mline)))],
                    ),
                    mline,
                ));
            }
            let slot = index(&name, string(&key, mline), mline);
            if string_valued {
                body.push(expr_stmt(assign(slot, syn_ident(&cur, mline), mline)));
            } else {
                let inner = assign(slot, syn_ident(&cur, mline), mline);
                body.push(expr_stmt(assign(
                    index(&name, inner, mline),
                    string(&key, mline),
                    mline,
                )));
            }
            first = false;
            if !self.eat(P::Comma) {
                self.expect(P::RBrace)?;
                break;
            }
        }
        body.push(syn_stmt(
            StmtKind::Return(Some(syn_ident(&name, line))),
            line,
        ));
        let call = self.ts_iife(&name, body, syn_ident(&name, line), line, start);
        Ok(StmtKind::Decl(
            DeclKind::Var,
            vec![(Pattern::Ident(name), Some(call))],
        ))
    }

    fn ts_namespace_body(&mut self, ns: &JsStr) -> R<Vec<Stmt>> {
        self.expect(P::LBrace)?;
        self.fn_depth += 1;
        let saved_ok = std::mem::replace(&mut self.using_ok, false);
        let mut out = Vec::new();
        while !self.eat(P::RBrace) {
            if self.is_eof() {
                return Err(self.unexpected());
            }
            if self.is_kw("export") {
                self.advance();
                let s = self.statement()?;
                let mut names = Vec::new();
                match &s.kind {
                    StmtKind::Decl(_, decls) => {
                        for (p, _) in decls {
                            p.bound_names(&mut names);
                        }
                    }
                    StmtKind::Function(f) => names.extend(f.name.clone()),
                    StmtKind::Class(c) => names.extend(c.name.clone()),
                    _ => {}
                }
                let line = s.line;
                out.push(s);
                for n in names {
                    out.push(expr_stmt(assign(
                        member(syn_ident(ns, line), &n, line),
                        syn_ident(&n, line),
                        line,
                    )));
                }
            } else {
                out.push(self.statement()?);
            }
        }
        self.using_ok = saved_ok;
        self.fn_depth -= 1;
        let line = self.line();
        out.push(syn_stmt(StmtKind::Return(Some(syn_ident(ns, line))), line));
        Ok(out)
    }

    fn ts_namespace(&mut self) -> R<StmtKind> {
        let line = self.line();
        let start = self.peek().start;
        self.advance();
        let mut names = vec![self.binding_ident()?];
        while self.eat(P::Dot) {
            names.push(self.ident_name()?);
        }
        let last = names.len() - 1;
        let mut body = self.ts_namespace_body(&names[last])?;
        for level in (1..names.len()).rev() {
            let parent = &names[level - 1];
            let child = &names[level];
            let seed = member(syn_ident(parent, line), child, line);
            let call = self.ts_iife(child, body, seed, line, start);
            body = vec![
                expr_stmt(assign(
                    member(syn_ident(parent, line), child, line),
                    call,
                    line,
                )),
                syn_stmt(StmtKind::Return(Some(syn_ident(parent, line))), line),
            ];
        }
        let call = self.ts_iife(&names[0], body, syn_ident(&names[0], line), line, start);
        Ok(StmtKind::Decl(
            DeclKind::Var,
            vec![(Pattern::Ident(names[0].clone()), Some(call))],
        ))
    }

    pub(super) fn ts_inject_props(
        &self,
        f: Rc<FuncDef>,
        props: Vec<JsStr>,
        derived: bool,
    ) -> Rc<FuncDef> {
        let mut def = (*f).clone();
        let line = def.line;
        if let FuncBody::Block(body) = &mut def.body {
            let assigns: Vec<Stmt> = props
                .iter()
                .map(|n| {
                    expr_stmt(assign(
                        member(
                            Expr {
                                kind: ExprKind::This,
                                line,
                            },
                            n,
                            line,
                        ),
                        syn_ident(n, line),
                        line,
                    ))
                })
                .collect();
            let at = if derived {
                body.iter()
                    .position(|s| {
                        matches!(&s.kind, StmtKind::Expr(e) if matches!(e.kind, ExprKind::SuperCall(_)))
                    })
                    .map_or(0, |i| i + 1)
            } else {
                body.iter()
                    .take_while(|s| matches!(&s.kind, StmtKind::Expr(e) if matches!(e.kind, ExprKind::Str(_))))
                    .count()
            };
            body.splice(at..at, assigns);
        }
        Rc::new(def)
    }

    pub(super) fn ts_elide_imports(&self, out: &mut [Stmt]) {
        for s in out.iter_mut() {
            let StmtKind::Import(decl) = &mut s.kind else {
                continue;
            };
            if decl.default.is_none() && decl.namespace.is_none() && decl.named.is_empty() {
                continue;
            }
            if decl
                .default
                .as_ref()
                .is_some_and(|n| !self.ts_refs.contains(n))
            {
                decl.default = None;
            }
            if decl
                .namespace
                .as_ref()
                .is_some_and(|n| !self.ts_refs.contains(n))
            {
                decl.namespace = None;
            }
            decl.named.retain(|(_, local)| self.ts_refs.contains(local));
            if decl.default.is_none() && decl.namespace.is_none() && decl.named.is_empty() {
                s.kind = StmtKind::Empty;
            }
        }
    }
}
