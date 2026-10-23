use super::*;

pub(super) struct StdMember {
    kind: &'static str,
    name: Expr,
    private_key: Option<JsStr>,
    key: Expr,
    is_static: bool,
    decorators: JsStr,
    initializers: Option<JsStr>,
    extras: JsStr,
}

#[derive(Default)]
pub(super) struct StdClass {
    members: Vec<StdMember>,
    arrays: Vec<(JsStr, Vec<Expr>)>,
    empties: Vec<JsStr>,
    hidden_keys: Vec<JsStr>,
    instance_extras: Option<JsStr>,
    static_extras: Option<JsStr>,
    result: JsStr,
    pub(super) wrap: bool,
}

fn num(n: f64, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Number(n),
        line,
        col: 0,
    }
}

fn void0(line: u32) -> Expr {
    Expr {
        kind: ExprKind::Unary(UnaryOp::Void, Box::new(num(0.0, line))),
        line,
        col: 0,
    }
}

fn string(s: &str, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Str(s.into()),
        line,
        col: 0,
    }
}

fn arrow(params: &[&str], body: FuncBody, line: u32) -> Expr {
    Expr {
        kind: ExprKind::Function(Rc::new(FuncDef {
            name: None,
            params: params
                .iter()
                .map(|p| Param {
                    pattern: Pattern::Ident((*p).into()),
                    default: None,
                })
                .collect(),
            rest: None,
            body,
            kind: FuncKind::Arrow,
            is_async: false,
            is_generator: false,
            uses_arguments: false,
            strict: true,
            has_eval: false,
            line,
            span: (0, 0),
        })),
        line,
        col: 0,
    }
}

fn static_block(body: Vec<Stmt>, line: u32) -> ClassMember {
    ClassMember::StaticBlock(Rc::new(FuncDef {
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
        line,
        span: (0, 0),
    }))
}

fn accessor_fn(storage: &JsStr, setter: bool, is_static: bool, line: u32) -> Rc<FuncDef> {
    let slot = Expr {
        kind: ExprKind::Member {
            obj: Box::new(Expr {
                kind: ExprKind::This,
                line,
                col: 0,
            }),
            name: storage.clone(),
            optional: false,
        },
        line,
        col: 0,
    };
    let (params, body) = if setter {
        (
            vec![Param {
                pattern: Pattern::Ident("value".into()),
                default: None,
            }],
            vec![syn_stmt(
                StmtKind::Expr(Expr {
                    kind: ExprKind::Assign {
                        op: None,
                        target: AssignTarget::Simple(Box::new(slot)),
                        value: Box::new(syn_ident("value", line)),
                    },
                    line,
                    col: 0,
                }),
                line,
            )],
        )
    } else {
        (
            Vec::new(),
            vec![syn_stmt(StmtKind::Return(Some(slot)), line)],
        )
    };
    Rc::new(FuncDef {
        name: None,
        params,
        rest: None,
        body: FuncBody::Block(body),
        kind: if is_static {
            FuncKind::StaticMethod
        } else {
            FuncKind::Method
        },
        is_async: false,
        is_generator: false,
        uses_arguments: false,
        strict: true,
        has_eval: false,
        line,
        span: (0, 0),
    })
}

impl Parser {
    pub(super) fn std_decorators_active(&self) -> bool {
        !(self.ts && self.ts_opts.legacy_decorators)
    }

    pub(super) fn std_class_start(&mut self) {
        let result = self.std_name("res");
        self.std_stack.push(StdClass {
            result,
            ..StdClass::default()
        });
    }

    fn std_receiver(&self, is_static: bool, line: u32) -> Expr {
        match self.std_stack.last() {
            Some(st) if is_static => syn_ident(&st.result, line),
            _ => Expr {
                kind: ExprKind::This,
                line,
                col: 0,
            },
        }
    }

    fn std_name(&mut self, tag: &str) -> JsStr {
        self.using_ids += 1;
        format!("%sd{}{tag}%", self.using_ids).into()
    }

    fn std_private(&mut self, tag: &str) -> JsStr {
        self.using_ids += 1;
        let id = self.class_ids.last().map(|(id, _)| *id).unwrap_or_default();
        format!("\u{0}#%sd{}{tag}%@{id}", self.using_ids).into()
    }

    pub(super) fn std_member_key(&mut self, key: PropKey, line: u32) -> (PropKey, Expr) {
        match key {
            PropKey::Named(n) => {
                let e = string(&n, line);
                (PropKey::Named(n), e)
            }
            PropKey::Computed(e) => {
                let hidden = self.std_name("key");
                if let Some(st) = self.std_stack.last_mut() {
                    st.hidden_keys.push(hidden.clone());
                    st.wrap = true;
                }
                let assign = syn_assign(&hidden, e, line);
                (PropKey::Computed(assign), syn_ident(&hidden, line))
            }
        }
    }

    pub(super) fn std_record(
        &mut self,
        kind: &'static str,
        key: &PropKey,
        key_expr: Expr,
        is_static: bool,
        decorators: Vec<Expr>,
        line: u32,
    ) -> R<Option<JsStr>> {
        let private_key = match key {
            PropKey::Named(n) if n.starts_with("\u{0}#") => Some(n.clone()),
            _ => None,
        };
        if private_key.is_some() && kind != "field" {
            return Err(SyntaxError::new(
                "Decorators on private methods and accessors are not supported",
                line,
                1,
            ));
        }
        let name = match &private_key {
            Some(p) => string(&display_name(p), line),
            None => key_expr.clone(),
        };
        let decorators_var = self.std_name("decs");
        let field_like = kind == "field" || kind == "accessor";
        let initializers = field_like.then(|| self.std_name("inits"));
        let extras = if kind == "field" {
            self.std_name("extras")
        } else if is_static {
            self.std_extras(true)
        } else {
            self.std_extras(false)
        };
        let Some(st) = self.std_stack.last_mut() else {
            return Ok(None);
        };
        st.wrap = true;
        st.arrays.push((decorators_var.clone(), decorators));
        if let Some(i) = &initializers {
            st.empties.push(i.clone());
        }
        if kind == "field" {
            st.empties.push(extras.clone());
        }
        st.members.push(StdMember {
            kind,
            name,
            private_key,
            key: key_expr,
            is_static,
            decorators: decorators_var,
            initializers: initializers.clone(),
            extras,
        });
        Ok(initializers)
    }

    fn std_extras(&mut self, is_static: bool) -> JsStr {
        let existing = self.std_stack.last().and_then(|st| {
            if is_static {
                st.static_extras.clone()
            } else {
                st.instance_extras.clone()
            }
        });
        if let Some(e) = existing {
            return e;
        }
        let name = self.std_name(if is_static { "sx" } else { "ix" });
        if let Some(st) = self.std_stack.last_mut() {
            st.empties.push(name.clone());
            if is_static {
                st.static_extras = Some(name.clone());
            } else {
                st.instance_extras = Some(name.clone());
            }
        }
        name
    }

    pub(super) fn std_wrap_init(
        &self,
        inits: &JsStr,
        value: Option<Expr>,
        is_static: bool,
        line: u32,
    ) -> Expr {
        syn_call(
            syn_ident("%runInitializers%", line),
            vec![
                self.std_receiver(is_static, line),
                syn_ident(inits, line),
                value.unwrap_or_else(|| void0(line)),
            ],
            line,
        )
    }

    pub(super) fn std_field_extras(&mut self, is_static: bool, line: u32) -> Option<ClassMember> {
        let extras = self.std_stack.last()?.members.last()?.extras.clone();
        let key = self.std_private("fx");
        let receiver = self.std_receiver(is_static, line);
        Some(ClassMember::Field {
            key: PropKey::Named(key),
            value: Some(syn_call(
                syn_ident("%runInitializers%", line),
                vec![receiver, syn_ident(&extras, line)],
                line,
            )),
            is_static,
        })
    }

    pub(super) fn std_auto_accessor(
        &mut self,
        key: PropKey,
        value: Option<Expr>,
        is_static: bool,
        decorators: Vec<Expr>,
        line: u32,
    ) -> R<Vec<ClassMember>> {
        let storage = self.std_private("acc");
        let (getter_key, setter_key, key_expr) = match key {
            PropKey::Named(n) => (
                PropKey::Named(n.clone()),
                PropKey::Named(n.clone()),
                string(&n, line),
            ),
            PropKey::Computed(e) => {
                let hidden = self.std_name("key");
                if let Some(st) = self.std_stack.last_mut() {
                    st.hidden_keys.push(hidden.clone());
                    st.wrap = true;
                }
                (
                    PropKey::Computed(syn_assign(&hidden, e, line)),
                    PropKey::Computed(syn_ident(&hidden, line)),
                    syn_ident(&hidden, line),
                )
            }
        };
        let value = if decorators.is_empty() {
            value
        } else {
            let inits = self.std_record(
                "accessor",
                &getter_key,
                key_expr,
                is_static,
                decorators,
                line,
            )?;
            match inits {
                Some(i) => Some(self.std_wrap_init(&i, value, is_static, line)),
                None => value,
            }
        };
        Ok(vec![
            ClassMember::Field {
                key: PropKey::Named(storage.clone()),
                value,
                is_static,
            },
            ClassMember::Accessor {
                key: getter_key,
                func: accessor_fn(&storage, false, is_static, line),
                is_static,
                setter: false,
            },
            ClassMember::Accessor {
                key: setter_key,
                func: accessor_fn(&storage, true, is_static, line),
                is_static,
                setter: true,
            },
        ])
    }

    fn std_access(m: &StdMember, line: u32) -> Expr {
        let target = |obj: Expr| match &m.private_key {
            Some(p) => Expr {
                kind: ExprKind::Member {
                    obj: Box::new(obj),
                    name: p.clone(),
                    optional: false,
                },
                line,
                col: 0,
            },
            None => Expr {
                kind: ExprKind::Index {
                    obj: Box::new(obj),
                    index: Box::new(m.key.clone()),
                    optional: false,
                },
                line,
                col: 0,
            },
        };
        let has_key = match &m.private_key {
            Some(p) => string(p, line),
            None => m.key.clone(),
        };
        let mut props = vec![PropDef::KeyValue(
            PropKey::Named("has".into()),
            arrow(
                &["o"],
                FuncBody::Expr(Box::new(Expr {
                    kind: ExprKind::Binary(
                        BinOp::In,
                        Box::new(has_key),
                        Box::new(syn_ident("o", line)),
                    ),
                    line,
                    col: 0,
                })),
                line,
            ),
        )];
        if m.kind != "setter" {
            props.push(PropDef::KeyValue(
                PropKey::Named("get".into()),
                arrow(
                    &["o"],
                    FuncBody::Expr(Box::new(target(syn_ident("o", line)))),
                    line,
                ),
            ));
        }
        if matches!(m.kind, "setter" | "field" | "accessor") {
            let assign = Expr {
                kind: ExprKind::Assign {
                    op: None,
                    target: AssignTarget::Simple(Box::new(target(syn_ident("o", line)))),
                    value: Box::new(syn_ident("v", line)),
                },
                line,
                col: 0,
            };
            props.push(PropDef::KeyValue(
                PropKey::Named("set".into()),
                arrow(
                    &["o", "v"],
                    FuncBody::Block(vec![syn_stmt(StmtKind::Expr(assign), line)]),
                    line,
                ),
            ));
        }
        Expr {
            kind: ExprKind::Object(props),
            line,
            col: 0,
        }
    }

    fn std_spec(m: &StdMember, line: u32) -> Expr {
        let kv = |k: &str, v: Expr| PropDef::KeyValue(PropKey::Named(k.into()), v);
        Expr {
            kind: ExprKind::Object(vec![
                kv("kind", string(m.kind, line)),
                kv("name", m.name.clone()),
                kv(
                    "static",
                    Expr {
                        kind: ExprKind::Bool(m.is_static),
                        line,
                        col: 0,
                    },
                ),
                kv(
                    "private",
                    Expr {
                        kind: ExprKind::Bool(m.private_key.is_some()),
                        line,
                        col: 0,
                    },
                ),
                kv("access", Self::std_access(m, line)),
                kv("decorators", syn_ident(&m.decorators, line)),
                kv(
                    "initializers",
                    match &m.initializers {
                        Some(i) => syn_ident(i, line),
                        None => Expr {
                            kind: ExprKind::Null,
                            line,
                            col: 0,
                        },
                    },
                ),
                kv("extras", syn_ident(&m.extras, line)),
            ]),
            line,
            col: 0,
        }
    }

    pub(super) fn std_decorated_class(
        &mut self,
        def: Rc<ClassDef>,
        class_decorators: Vec<Expr>,
        st: StdClass,
        default_name: bool,
    ) -> Expr {
        let line = def.line;
        let binding: JsStr = match &def.name {
            Some(n) => n.clone(),
            None if default_name => "default".into(),
            None => self.std_name("class"),
        };
        let result = st.result.clone();
        let class_decs = self.std_name("cd");
        let class_extras = self.std_name("cx");
        let mut body = Vec::new();
        let decl = |name: JsStr, value: Expr, body: &mut Vec<Stmt>| {
            body.push(syn_stmt(
                StmtKind::Decl(DeclKind::Const, vec![(Pattern::Ident(name), Some(value))]),
                line,
            ));
        };
        decl(
            class_decs.clone(),
            Expr {
                kind: ExprKind::Array(class_decorators.into_iter().map(ArrayElem::Expr).collect()),
                line,
                col: 0,
            },
            &mut body,
        );
        for (name, exprs) in &st.arrays {
            decl(
                name.clone(),
                Expr {
                    kind: ExprKind::Array(exprs.iter().cloned().map(ArrayElem::Expr).collect()),
                    line,
                    col: 0,
                },
                &mut body,
            );
        }
        for name in st.empties.iter().chain(std::iter::once(&class_extras)) {
            decl(
                name.clone(),
                Expr {
                    kind: ExprKind::Array(Vec::new()),
                    line,
                    col: 0,
                },
                &mut body,
            );
        }
        body.push(syn_stmt(
            StmtKind::Decl(
                DeclKind::Let,
                st.hidden_keys
                    .iter()
                    .chain(std::iter::once(&result))
                    .map(|k| (Pattern::Ident(k.clone()), None))
                    .collect(),
            ),
            line,
        ));
        let rank = |m: &StdMember| match (m.kind == "field", m.is_static) {
            (false, true) => 0,
            (false, false) => 1,
            (true, true) => 2,
            (true, false) => 3,
        };
        let mut ordered: Vec<&StdMember> = st.members.iter().collect();
        ordered.sort_by_key(|m| rank(m));
        let specs = Expr {
            kind: ExprKind::Array(
                ordered
                    .iter()
                    .map(|m| ArrayElem::Expr(Self::std_spec(m, line)))
                    .collect(),
            ),
            line,
            col: 0,
        };
        let mut first = vec![syn_stmt(
            StmtKind::Expr(syn_assign(
                &result,
                syn_call(
                    syn_ident("%applyDecorators%", line),
                    vec![
                        Expr {
                            kind: ExprKind::This,
                            line,
                            col: 0,
                        },
                        specs,
                        syn_ident(&class_decs, line),
                        syn_ident(&class_extras, line),
                    ],
                    line,
                ),
                line,
            )),
            line,
        )];
        first.push(syn_stmt(
            StmtKind::Expr(syn_assign(&binding, syn_ident(&result, line), line)),
            line,
        ));
        if let Some(sx) = &st.static_extras {
            first.push(syn_stmt(
                StmtKind::Expr(syn_call(
                    syn_ident("%runInitializers%", line),
                    vec![syn_ident(&result, line), syn_ident(sx, line)],
                    line,
                )),
                line,
            ));
        }
        let mut members = vec![static_block(first, line)];
        if let Some(ix) = &st.instance_extras {
            let key = self.std_private("ixf");
            members.push(ClassMember::Field {
                key: PropKey::Named(key),
                value: Some(syn_call(
                    syn_ident("%runInitializers%", line),
                    vec![
                        Expr {
                            kind: ExprKind::This,
                            line,
                            col: 0,
                        },
                        syn_ident(ix, line),
                    ],
                    line,
                )),
                is_static: false,
            });
        }
        members.extend(def.members.iter().cloned());
        members.push(static_block(
            vec![syn_stmt(
                StmtKind::Expr(syn_call(
                    syn_ident("%runInitializers%", line),
                    vec![syn_ident(&result, line), syn_ident(&class_extras, line)],
                    line,
                )),
                line,
            )],
            line,
        ));
        let class = Rc::new(ClassDef {
            name: None,
            superclass: def.superclass.clone(),
            constructor: def.constructor.clone(),
            members,
            line,
            span: def.span,
        });
        let class_expr = Expr {
            kind: ExprKind::Class(class),
            line,
            col: 0,
        };
        let init = if def.name.is_some() || default_name {
            class_expr
        } else {
            Expr {
                kind: ExprKind::Seq(vec![num(0.0, line), class_expr]),
                line,
                col: 0,
            }
        };
        body.push(syn_stmt(
            StmtKind::Decl(
                DeclKind::Var,
                vec![(Pattern::Ident(binding.clone()), Some(init))],
            ),
            line,
        ));
        body.push(syn_stmt(
            StmtKind::Expr(syn_assign(&binding, syn_ident(&result, line), line)),
            line,
        ));
        body.push(syn_stmt(
            StmtKind::Return(Some(syn_ident(&binding, line))),
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
                col: 0,
            },
            Vec::new(),
            line,
        )
    }
}
