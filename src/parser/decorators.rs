use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct TsOptions {
    pub legacy_decorators: bool,
    pub emit_metadata: bool,
}

pub(super) struct DecoratedMember {
    key: Expr,
    is_static: bool,
    field: bool,
    decorators: Vec<Expr>,
}

#[derive(Default)]
pub(super) struct ClassDecorations {
    pub(super) members: Vec<DecoratedMember>,
    pub(super) hidden_keys: Vec<JsStr>,
    pub(super) ctor_params: Vec<(usize, Expr)>,
    pub(super) ctor_types: Vec<Expr>,
}

enum Ser {
    Skip,
    Void,
    Global(&'static str),
    Ref(Vec<JsStr>),
    Object,
}

fn split_top(toks: &[Tok], sep: P) -> Vec<&[Tok]> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Punct(P::LParen | P::LBracket | P::LBrace | P::Lt) => depth += 1,
            Tok::Punct(P::RParen | P::RBracket | P::RBrace | P::Gt) => depth -= 1,
            Tok::Punct(p) if *p == sep && depth == 0 => {
                out.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&toks[start..]);
    out
}

fn has_top_arrow(toks: &[Tok]) -> bool {
    let mut depth = 0i32;
    for t in toks {
        match t {
            Tok::Punct(P::LParen | P::LBracket | P::LBrace | P::Lt) => depth += 1,
            Tok::Punct(P::RParen | P::RBracket | P::RBrace | P::Gt) => depth -= 1,
            Tok::Punct(P::Arrow) if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

fn serialize_one(toks: &[Tok]) -> Ser {
    let toks: Vec<Tok> = toks.to_vec();
    if toks.is_empty() {
        return Ser::Skip;
    }
    if has_top_arrow(&toks) {
        return Ser::Global("Function");
    }
    let is_array = matches!(toks.last(), Some(Tok::Punct(P::RBracket)));
    match &toks[0] {
        Tok::Punct(P::LParen) => {
            if matches!(toks.last(), Some(Tok::Punct(P::RParen))) && !is_array {
                return serialize_union(&toks[1..toks.len() - 1]);
            }
            if is_array {
                return Ser::Global("Array");
            }
            Ser::Object
        }
        Tok::Punct(P::LBracket) => Ser::Global("Array"),
        Tok::Punct(P::LBrace) => {
            if is_array {
                Ser::Global("Array")
            } else {
                Ser::Object
            }
        }
        Tok::Punct(P::Lt) => Ser::Global("Function"),
        Tok::Str(_) | Tok::Template(_) => Ser::Global("String"),
        Tok::Num(_) | Tok::Punct(P::Minus) => Ser::Global("Number"),
        Tok::BigInt(_) => Ser::Global("BigInt"),
        Tok::Ident(n) => {
            if is_array {
                return Ser::Global("Array");
            }
            match &**n {
                "string" => Ser::Global("String"),
                "number" => Ser::Global("Number"),
                "boolean" | "true" | "false" => Ser::Global("Boolean"),
                "bigint" => Ser::Global("BigInt"),
                "symbol" => Ser::Global("Symbol"),
                "null" | "undefined" => Ser::Skip,
                "void" | "never" => Ser::Void,
                "any" | "unknown" | "object" | "typeof" | "keyof" | "unique" | "infer" | "this" => {
                    Ser::Object
                }
                "readonly" => serialize_one(&toks[1..]),
                "new" | "abstract" => Ser::Global("Function"),
                _ => {
                    let mut path = vec![n.clone()];
                    let mut i = 1;
                    while i + 1 < toks.len() {
                        match (&toks[i], &toks[i + 1]) {
                            (Tok::Punct(P::Dot), Tok::Ident(m)) => {
                                path.push(m.clone());
                                i += 2;
                            }
                            _ => break,
                        }
                    }
                    if path.len() == 1 {
                        match &*path[0] {
                            "Array" | "ReadonlyArray" => return Ser::Global("Array"),
                            "Promise" => return Ser::Global("Promise"),
                            "Function" => return Ser::Global("Function"),
                            "Object" => return Ser::Object,
                            "String" => return Ser::Global("String"),
                            "Number" => return Ser::Global("Number"),
                            "Boolean" => return Ser::Global("Boolean"),
                            "Symbol" => return Ser::Global("Symbol"),
                            "BigInt" => return Ser::Global("BigInt"),
                            "Record" | "Partial" | "Required" | "Readonly" | "Pick" | "Omit" => {
                                return Ser::Object;
                            }
                            _ => {}
                        }
                    }
                    Ser::Ref(path)
                }
            }
        }
        _ => Ser::Object,
    }
}

fn serialize_union(toks: &[Tok]) -> Ser {
    let parts: Vec<Ser> = split_top(toks, P::Pipe)
        .into_iter()
        .filter(|p| !p.is_empty())
        .map(|p| {
            let inter = split_top(p, P::Amp);
            if inter.iter().filter(|x| !x.is_empty()).count() > 1 {
                Ser::Object
            } else {
                serialize_one(p)
            }
        })
        .filter(|s| !matches!(s, Ser::Skip))
        .collect();
    match parts.len() {
        0 => Ser::Void,
        1 => parts.into_iter().next().unwrap_or(Ser::Object),
        _ => {
            let first = match &parts[0] {
                Ser::Global(g) => Some(*g),
                _ => None,
            };
            if first.is_some()
                && parts
                    .iter()
                    .all(|p| matches!(p, Ser::Global(g) if Some(*g) == first))
            {
                Ser::Global(first.unwrap_or("Object"))
            } else {
                Ser::Object
            }
        }
    }
}

impl Parser {
    pub(super) fn ts_param_type(&mut self) -> R<()> {
        let ty = if self.eat(P::Colon) {
            if self.ts_opts.emit_metadata {
                Some(self.ts_type_serialized()?)
            } else {
                self.ts_type()?;
                None
            }
        } else {
            None
        };
        if self.ts_opts.emit_metadata {
            let line = self.line();
            self.ts_param_types
                .push(ty.unwrap_or_else(|| syn_ident("Object", line)));
        }
        Ok(())
    }

    pub(super) fn ts_member_decorations(
        &mut self,
        key: Option<Expr>,
        is_static: bool,
        kind: u8,
        decorators: Vec<Expr>,
        field_type: Option<Expr>,
    ) {
        let params = std::mem::take(&mut self.ts_last_param_decs);
        let types = std::mem::take(&mut self.ts_last_param_types);
        let ret = self.ts_last_return.take();
        if decorators.is_empty() && params.is_empty() {
            return;
        }
        let Some(key) = key else {
            return;
        };
        let mut st = self.ts_class_stack.pop().unwrap_or_default();
        self.ts_record_member(
            &mut st, key, is_static, kind, decorators, params, types, ret, field_type,
        );
        self.ts_class_stack.push(st);
    }

    pub(super) fn ts_decorator(&mut self) -> R<Expr> {
        self.expect(P::At)?;
        let line = self.line();
        if self.is(P::LParen) {
            self.advance();
            let e = self.expression()?;
            self.expect(P::RParen)?;
            return Ok(e);
        }
        let name = self.ident_name()?;
        self.ts_refs.insert(name.clone());
        let mut e = Expr {
            kind: if &*name == "this" {
                ExprKind::This
            } else {
                ExprKind::Ident(name)
            },
            line,
        };
        loop {
            if self.eat(P::Dot) {
                let n = self.ident_name()?;
                e = Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(e),
                        name: n,
                        optional: false,
                    },
                    line,
                };
            } else if self.is(P::LParen) && !self.peek().nl_before {
                let args = self.arguments()?;
                e = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(e),
                        args,
                        optional: false,
                    },
                    line,
                };
            } else if self.is(P::Lt) && self.ts_try_type_args() {
                continue;
            } else {
                return Ok(e);
            }
        }
    }

    pub(super) fn ts_decorators(&mut self) -> R<Vec<Expr>> {
        let mut out = Vec::new();
        while self.is(P::At) {
            if !self.ts_opts.legacy_decorators {
                return Err(
                    self.error("Decorators need \"experimentalDecorators\": true in tsconfig.json")
                );
            }
            out.push(self.ts_decorator()?);
        }
        Ok(out)
    }

    pub(super) fn ts_type_serialized(&mut self) -> R<Expr> {
        let line = self.line();
        let start = self.pos;
        self.ts_type()?;
        let end = self.pos;
        let toks: Vec<Tok> = self.toks[start..end]
            .iter()
            .map(|t| t.tok.clone())
            .collect();
        let ser = serialize_union(&toks);
        Ok(self.ts_ser_expr(ser, line))
    }

    pub(super) fn ts_object_ref(&self, line: u32) -> Expr {
        syn_ident("Object", line)
    }

    fn ts_ser_expr(&mut self, ser: Ser, line: u32) -> Expr {
        match ser {
            Ser::Skip | Ser::Void => Expr {
                kind: ExprKind::Unary(
                    UnaryOp::Void,
                    Box::new(Expr {
                        kind: ExprKind::Number(0.0),
                        line,
                    }),
                ),
                line,
            },
            Ser::Global(g) => syn_ident(g, line),
            Ser::Object => syn_ident("Object", line),
            Ser::Ref(path) => {
                let root = path[0].clone();
                self.ts_refs.insert(root.clone());
                let mut value = syn_ident(&root, line);
                for p in &path[1..] {
                    value = Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(value),
                            name: p.clone(),
                            optional: false,
                        },
                        line,
                    };
                }
                let test = Expr {
                    kind: ExprKind::Binary(
                        BinOp::StrictEq,
                        Box::new(Expr {
                            kind: ExprKind::Unary(
                                UnaryOp::Typeof,
                                Box::new(syn_ident(&root, line)),
                            ),
                            line,
                        }),
                        Box::new(Expr {
                            kind: ExprKind::Str("undefined".into()),
                            line,
                        }),
                    ),
                    line,
                };
                Expr {
                    kind: ExprKind::Cond(
                        Box::new(test),
                        Box::new(syn_ident("Object", line)),
                        Box::new(value),
                    ),
                    line,
                }
            }
        }
    }

    fn ts_metadata(&self, key: &str, value: Expr, line: u32) -> Expr {
        syn_call(
            syn_ident("%metadata%", line),
            vec![
                Expr {
                    kind: ExprKind::Str(key.into()),
                    line,
                },
                value,
            ],
            line,
        )
    }

    fn ts_array(items: Vec<Expr>, line: u32) -> Expr {
        Expr {
            kind: ExprKind::Array(items.into_iter().map(ArrayElem::Expr).collect()),
            line,
        }
    }

    pub(super) fn ts_member_key(
        &mut self,
        key: PropKey,
        decorations: &mut ClassDecorations,
    ) -> (PropKey, Expr) {
        match key {
            PropKey::Named(n) => {
                let line = self.line();
                let k = Expr {
                    kind: ExprKind::Str(n.clone()),
                    line,
                };
                (PropKey::Named(n), k)
            }
            PropKey::Computed(e) => {
                let line = e.line;
                self.using_ids += 1;
                let hidden: JsStr = format!("%deckey{}%", self.using_ids).into();
                decorations.hidden_keys.push(hidden.clone());
                let assign = Expr {
                    kind: ExprKind::Assign {
                        op: None,
                        target: AssignTarget::Simple(Box::new(syn_ident(&hidden, line))),
                        value: Box::new(e),
                    },
                    line,
                };
                (PropKey::Computed(assign), syn_ident(&hidden, line))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn ts_record_member(
        &mut self,
        decorations: &mut ClassDecorations,
        key: Expr,
        is_static: bool,
        kind: u8,
        decorators: Vec<Expr>,
        params: Vec<(usize, Expr)>,
        types: Vec<Expr>,
        ret: Option<Expr>,
        field_type: Option<Expr>,
    ) {
        let line = key.line;
        let mut all = decorators;
        for (i, d) in params {
            all.push(syn_call(
                syn_ident("%param%", line),
                vec![
                    Expr {
                        kind: ExprKind::Number(i as f64),
                        line,
                    },
                    d,
                ],
                line,
            ));
        }
        if self.ts_opts.emit_metadata {
            match kind {
                0 => {
                    all.push(self.ts_metadata("design:type", syn_ident("Function", line), line));
                    all.push(self.ts_metadata(
                        "design:paramtypes",
                        Self::ts_array(types, line),
                        line,
                    ));
                    let r = ret.unwrap_or_else(|| self.ts_object_ref(line));
                    all.push(self.ts_metadata("design:returntype", r, line));
                }
                1 => {
                    let r = ret.unwrap_or_else(|| self.ts_object_ref(line));
                    all.push(self.ts_metadata("design:type", r, line));
                    all.push(self.ts_metadata(
                        "design:paramtypes",
                        Self::ts_array(Vec::new(), line),
                        line,
                    ));
                }
                2 => {
                    let t = types
                        .first()
                        .cloned()
                        .unwrap_or_else(|| self.ts_object_ref(line));
                    all.push(self.ts_metadata("design:type", t, line));
                    all.push(self.ts_metadata(
                        "design:paramtypes",
                        Self::ts_array(types, line),
                        line,
                    ));
                }
                _ => {
                    let t = field_type.unwrap_or_else(|| self.ts_object_ref(line));
                    all.push(self.ts_metadata("design:type", t, line));
                }
            }
        }
        if all.is_empty() {
            return;
        }
        decorations.members.push(DecoratedMember {
            key,
            is_static,
            field: kind == 3,
            decorators: all,
        });
    }

    pub(super) fn ts_decorated_class(
        &mut self,
        def: Rc<ClassDef>,
        class_decorators: Vec<Expr>,
        decorations: ClassDecorations,
    ) -> Expr {
        let line = def.line;
        let name: JsStr = def.name.clone().unwrap_or_else(|| "%decclass%".into());
        let mut body = Vec::new();
        if !decorations.hidden_keys.is_empty() {
            body.push(syn_stmt(
                StmtKind::Decl(
                    DeclKind::Let,
                    decorations
                        .hidden_keys
                        .iter()
                        .map(|k| (Pattern::Ident(k.clone()), None))
                        .collect(),
                ),
                line,
            ));
        }
        let class_expr = Expr {
            kind: ExprKind::Class(def.clone()),
            line,
        };
        let init = if def.name.is_some() {
            class_expr
        } else {
            Expr {
                kind: ExprKind::Seq(vec![
                    Expr {
                        kind: ExprKind::Number(0.0),
                        line,
                    },
                    class_expr,
                ]),
                line,
            }
        };
        body.push(syn_stmt(
            StmtKind::Decl(
                DeclKind::Let,
                vec![(Pattern::Ident(name.clone()), Some(init))],
            ),
            line,
        ));
        for m in decorations.members {
            let target = if m.is_static {
                syn_ident(&name, line)
            } else {
                Expr {
                    kind: ExprKind::Member {
                        obj: Box::new(syn_ident(&name, line)),
                        name: "prototype".into(),
                        optional: false,
                    },
                    line,
                }
            };
            let desc = if m.field {
                Expr {
                    kind: ExprKind::Unary(
                        UnaryOp::Void,
                        Box::new(Expr {
                            kind: ExprKind::Number(0.0),
                            line,
                        }),
                    ),
                    line,
                }
            } else {
                Expr {
                    kind: ExprKind::Null,
                    line,
                }
            };
            body.push(syn_stmt(
                StmtKind::Expr(syn_call(
                    syn_ident("%decorate%", line),
                    vec![Self::ts_array(m.decorators, line), target, m.key, desc],
                    line,
                )),
                line,
            ));
        }
        let mut class_level = class_decorators;
        for (i, d) in decorations.ctor_params {
            class_level.push(syn_call(
                syn_ident("%param%", line),
                vec![
                    Expr {
                        kind: ExprKind::Number(i as f64),
                        line,
                    },
                    d,
                ],
                line,
            ));
        }
        if !class_level.is_empty() {
            if self.ts_opts.emit_metadata {
                let types = Self::ts_array(decorations.ctor_types, line);
                class_level.push(self.ts_metadata("design:paramtypes", types, line));
            }
            let call = syn_call(
                syn_ident("%decorate%", line),
                vec![Self::ts_array(class_level, line), syn_ident(&name, line)],
                line,
            );
            body.push(syn_stmt(
                StmtKind::Expr(Expr {
                    kind: ExprKind::Assign {
                        op: None,
                        target: AssignTarget::Simple(Box::new(syn_ident(&name, line))),
                        value: Box::new(call),
                    },
                    line,
                }),
                line,
            ));
        }
        body.push(syn_stmt(
            StmtKind::Return(Some(syn_ident(&name, line))),
            line,
        ));
        let f = FuncDef {
            name: None,
            params: Vec::new(),
            rest: None,
            body: FuncBody::Block(body),
            kind: FuncKind::Arrow,
            is_async: false,
            is_generator: false,
            uses_arguments: false,
            strict: true,
            has_eval: false,
            line,
            span: def.span,
        };
        syn_call(
            Expr {
                kind: ExprKind::Function(Rc::new(f)),
                line,
            },
            Vec::new(),
            line,
        )
    }
}
