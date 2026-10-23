use super::*;

#[derive(Clone, Debug)]
pub struct JsxOptions {
    pub automatic: bool,
    pub import_source: String,
    pub factory: String,
    pub fragment: String,
}

impl Default for JsxOptions {
    fn default() -> Self {
        JsxOptions {
            automatic: true,
            import_source: "react".into(),
            factory: "React.createElement".into(),
            fragment: "React.Fragment".into(),
        }
    }
}

impl JsxOptions {
    pub fn apply_pragmas(&mut self, src: &str) {
        let word_after = |key: &str| -> Option<String> {
            let mut rest = src;
            while let Some(i) = rest.find(key) {
                let after = &rest[i + key.len()..];
                if after.starts_with([' ', '\t']) {
                    let w: String = after
                        .trim_start()
                        .chars()
                        .take_while(|c| !c.is_whitespace() && *c != '*')
                        .collect();
                    if !w.is_empty() {
                        return Some(w);
                    }
                }
                rest = after;
            }
            None
        };
        if let Some(source) = word_after("@jsxImportSource") {
            self.import_source = source;
            self.automatic = true;
        }
        if let Some(runtime) = word_after("@jsxRuntime") {
            self.automatic = runtime != "classic";
        }
        if let Some(factory) = word_after("@jsx") {
            self.factory = factory;
            self.automatic = false;
        }
        if let Some(fragment) = word_after("@jsxFrag") {
            self.fragment = fragment;
        }
    }
}

enum JsxAttr {
    Named(JsStr, Option<Expr>),
    Spread(Expr),
}

const RUNTIME: &str = "%jsxrt%";

fn clean_text(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let last_non_empty = lines
        .iter()
        .rposition(|l| l.chars().any(|c| c != ' ' && c != '\t' && c != '\r'))?;
    let mut out = String::new();
    let count = lines.len();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\r').replace('\t', " ");
        let mut trimmed: &str = &line;
        if i != 0 {
            trimmed = trimmed.trim_start_matches(' ');
        }
        if i != count - 1 {
            trimmed = trimmed.trim_end_matches(' ');
        }
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            if i != last_non_empty {
                out.push(' ');
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

fn intrinsic(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase()) || name.contains('-')
}

impl Parser {
    fn jsx_ident(&mut self) -> R<JsStr> {
        match &self.peek().tok {
            Tok::Ident(n) => {
                let n = n.clone();
                self.advance();
                Ok(n)
            }
            _ => Err(self.unexpected()),
        }
    }

    fn jsx_tag(&mut self) -> R<(Expr, String)> {
        let line = self.line();
        let first = self.jsx_ident()?;
        if self.eat(P::Colon) {
            let second = self.jsx_ident()?;
            let text = format!("{first}:{second}");
            return Ok((
                Expr {
                    kind: ExprKind::Str(text.as_str().into()),
                    line,
                },
                text,
            ));
        }
        let mut text = first.to_string();
        let mut e = if intrinsic(&first) && !self.is(P::Dot) {
            Expr {
                kind: ExprKind::Str(first.clone()),
                line,
            }
        } else {
            self.ts_refs.insert(first.clone());
            Expr {
                kind: if &*first == "this" {
                    ExprKind::This
                } else {
                    ExprKind::Ident(first.clone())
                },
                line,
            }
        };
        while self.eat(P::Dot) {
            let n = self.jsx_ident()?;
            text.push('.');
            text.push_str(&n);
            e = Expr {
                kind: ExprKind::Member {
                    obj: Box::new(e),
                    name: n,
                    optional: false,
                },
                line,
            };
        }
        Ok((e, text))
    }

    pub(super) fn jsx_element(&mut self) -> R<Expr> {
        let line = self.line();
        self.expect(P::JsxOpen)?;
        if self.eat(P::Gt) {
            let children = self.jsx_children(None)?;
            return Ok(self.jsx_build(None, Vec::new(), children, line));
        }
        let (tag, text) = self.jsx_tag()?;
        if self.ts && self.is(P::Lt) {
            self.ts_type_args()?;
        }
        let mut attrs = Vec::new();
        loop {
            if self.eat(P::Slash) {
                self.expect(P::Gt)?;
                return Ok(self.jsx_build(Some(tag), attrs, Vec::new(), line));
            }
            if self.eat(P::Gt) {
                break;
            }
            if self.eat(P::LBrace) {
                self.expect(P::Ellipsis)?;
                let e = self.assignment()?;
                self.expect(P::RBrace)?;
                attrs.push(JsxAttr::Spread(e));
                continue;
            }
            let name = self.jsx_ident()?;
            let value = if self.eat(P::Eq) {
                match &self.peek().tok {
                    Tok::Str(s) => {
                        let s = s.clone();
                        let l = self.line();
                        self.advance();
                        Some(Expr {
                            kind: ExprKind::Str(s),
                            line: l,
                        })
                    }
                    Tok::Punct(P::LBrace) => {
                        self.advance();
                        let e = self.assignment()?;
                        self.expect(P::RBrace)?;
                        Some(e)
                    }
                    Tok::Punct(P::JsxOpen) => Some(self.jsx_element()?),
                    _ => return Err(self.unexpected()),
                }
            } else {
                None
            };
            attrs.push(JsxAttr::Named(name, value));
        }
        let children = self.jsx_children(Some(&text))?;
        Ok(self.jsx_build(Some(tag), attrs, children, line))
    }

    fn jsx_children(&mut self, open: Option<&str>) -> R<Vec<Arg>> {
        let mut out = Vec::new();
        loop {
            let line = self.line();
            match &self.peek().tok {
                Tok::JsxText(t) => {
                    let t = t.clone();
                    self.advance();
                    if let Some(s) = clean_text(&t) {
                        out.push(Arg::Expr(Expr {
                            kind: ExprKind::Str(s.as_str().into()),
                            line,
                        }));
                    }
                }
                Tok::Punct(P::LBrace) => {
                    self.advance();
                    if self.eat(P::RBrace) {
                        continue;
                    }
                    if self.eat(P::Ellipsis) {
                        let e = self.assignment()?;
                        self.expect(P::RBrace)?;
                        out.push(Arg::Spread(e));
                        continue;
                    }
                    let e = self.expression()?;
                    self.expect(P::RBrace)?;
                    out.push(Arg::Expr(e));
                }
                Tok::Punct(P::JsxOpen) if self.is_at(1, P::Slash) => {
                    self.advance();
                    self.advance();
                    let closing = if self.is(P::Gt) {
                        String::new()
                    } else {
                        self.jsx_tag()?.1
                    };
                    self.expect(P::Gt)?;
                    let expected = open.unwrap_or("");
                    if closing != expected {
                        return Err(self.error(if expected.is_empty() {
                            "Expected corresponding closing tag for JSX fragment".to_string()
                        } else {
                            format!("Expected corresponding JSX closing tag for <{expected}>")
                        }));
                    }
                    return Ok(out);
                }
                Tok::Punct(P::JsxOpen) => out.push(Arg::Expr(self.jsx_element()?)),
                _ => return Err(self.unexpected()),
            }
        }
    }

    fn jsx_path(&mut self, path: &str, line: u32) -> Expr {
        let mut parts = path.split('.');
        let root: JsStr = parts.next().unwrap_or("React").into();
        self.ts_refs.insert(root.clone());
        let mut e = syn_ident(&root, line);
        for p in parts {
            e = Expr {
                kind: ExprKind::Member {
                    obj: Box::new(e),
                    name: p.into(),
                    optional: false,
                },
                line,
            };
        }
        e
    }

    fn jsx_build(
        &mut self,
        tag: Option<Expr>,
        attrs: Vec<JsxAttr>,
        children: Vec<Arg>,
        line: u32,
    ) -> Expr {
        let opts = self.jsx.clone().unwrap_or_default();
        let truthy = || Expr {
            kind: ExprKind::Bool(true),
            line,
        };
        let runtime = |name: &str| Expr {
            kind: ExprKind::Member {
                obj: Box::new(syn_ident(RUNTIME, line)),
                name: name.into(),
                optional: false,
            },
            line,
        };
        if opts.automatic {
            self.jsx_used = true;
            let mut props = Vec::new();
            let mut key = None;
            for a in attrs {
                match a {
                    JsxAttr::Named(n, v) if &*n == "key" => key = Some(v.unwrap_or_else(truthy)),
                    JsxAttr::Named(n, v) => props.push(PropDef::KeyValue(
                        PropKey::Named(n),
                        v.unwrap_or_else(truthy),
                    )),
                    JsxAttr::Spread(e) => props.push(PropDef::Spread(e)),
                }
            }
            let multi = children.len() > 1 || children.iter().any(|c| matches!(c, Arg::Spread(_)));
            if !children.is_empty() {
                let value = if multi {
                    Expr {
                        kind: ExprKind::Array(
                            children
                                .into_iter()
                                .map(|c| match c {
                                    Arg::Expr(e) => ArrayElem::Expr(e),
                                    Arg::Spread(e) => ArrayElem::Spread(e),
                                })
                                .collect(),
                        ),
                        line,
                    }
                } else {
                    match children.into_iter().next() {
                        Some(Arg::Expr(e)) | Some(Arg::Spread(e)) => e,
                        None => truthy(),
                    }
                };
                props.push(PropDef::KeyValue(PropKey::Named("children".into()), value));
            }
            let ty = tag.unwrap_or_else(|| runtime("Fragment"));
            let mut args = vec![
                ty,
                Expr {
                    kind: ExprKind::Object(props),
                    line,
                },
            ];
            if let Some(k) = key {
                args.push(k);
            }
            return syn_call(runtime(if multi { "jsxs" } else { "jsx" }), args, line);
        }
        let factory = self.jsx_path(&opts.factory, line);
        let ty = match tag {
            Some(t) => t,
            None => self.jsx_path(&opts.fragment, line),
        };
        let props = if attrs.is_empty() {
            Expr {
                kind: ExprKind::Null,
                line,
            }
        } else {
            Expr {
                kind: ExprKind::Object(
                    attrs
                        .into_iter()
                        .map(|a| match a {
                            JsxAttr::Named(n, v) => {
                                PropDef::KeyValue(PropKey::Named(n), v.unwrap_or_else(truthy))
                            }
                            JsxAttr::Spread(e) => PropDef::Spread(e),
                        })
                        .collect(),
                ),
                line,
            }
        };
        let mut args = vec![Arg::Expr(ty), Arg::Expr(props)];
        args.extend(children);
        Expr {
            kind: ExprKind::Call {
                callee: Box::new(factory),
                args,
                optional: false,
            },
            line,
        }
    }

    pub(super) fn jsx_runtime_import(&self) -> Option<Stmt> {
        if !self.jsx_used {
            return None;
        }
        let opts = self.jsx.clone().unwrap_or_default();
        let source: JsStr = format!("{}/jsx-runtime", opts.import_source).into();
        Some(if self.script {
            Stmt {
                kind: StmtKind::Decl(
                    DeclKind::Const,
                    vec![(
                        Pattern::Ident(RUNTIME.into()),
                        Some(syn_call(
                            syn_ident("require", 1),
                            vec![Expr {
                                kind: ExprKind::Str(source),
                                line: 1,
                            }],
                            1,
                        )),
                    )],
                ),
                line: 1,
            }
        } else {
            Stmt {
                kind: StmtKind::Import(ImportDecl {
                    source,
                    default: None,
                    namespace: Some(RUNTIME.into()),
                    named: Vec::new(),
                }),
                line: 1,
            }
        })
    }
}
