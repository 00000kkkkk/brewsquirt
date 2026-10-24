use crate::jsstr::JsStr;
use crate::token::{P, SyntaxError, TemplatePart, Tok, Token};

pub fn tokenize(src: &str) -> Result<Vec<Token>, SyntaxError> {
    tokenize_goal(src, false)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Top,
    Template,
    JsxExpr,
}

pub fn tokenize_from(
    src: &str,
    html: bool,
    byte_start: u32,
    line: u32,
    col: u32,
    regex_first: bool,
    jsx: (bool, bool),
) -> Result<Vec<Token>, SyntaxError> {
    let chars: Vec<char> = src.chars().collect();
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut b = 0u32;
    for c in &chars {
        offsets.push(b);
        b += c.len_utf8() as u32;
    }
    offsets.push(b);
    let pos = offsets
        .iter()
        .position(|&o| o == byte_start)
        .unwrap_or(chars.len());
    let mut lexer = Lexer {
        src,
        names: rustc_hash::FxHashMap::default(),
        html,
        chars,
        offsets,
        pos,
        line,
        col,
        legacy_escape: false,
        in_template: false,
        sep_seen: false,
        force_regex: Some(regex_first),
        parens: Vec::new(),
        close_head: false,
        jsx: jsx.0,
        jsx_ts: jsx.1,
    };
    let mut out = Vec::new();
    lexer.lex_into(&mut out, Mode::Top)?;
    Ok(out)
}

pub fn tokenize_goal(src: &str, html: bool) -> Result<Vec<Token>, SyntaxError> {
    tokenize_jsx(src, html, false, false)
}

pub fn tokenize_jsx(src: &str, html: bool, jsx: bool, ts: bool) -> Result<Vec<Token>, SyntaxError> {
    let chars: Vec<char> = src.chars().collect();
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut b = 0u32;
    for c in &chars {
        offsets.push(b);
        b += c.len_utf8() as u32;
    }
    offsets.push(b);
    let mut lexer = Lexer {
        src,
        names: rustc_hash::FxHashMap::default(),
        html,
        chars,
        offsets,
        pos: 0,
        line: 1,
        col: 1,
        legacy_escape: false,
        in_template: false,
        sep_seen: false,
        force_regex: None,
        parens: Vec::new(),
        close_head: false,
        jsx,
        jsx_ts: ts,
    };
    if lexer.chars.first() == Some(&'#') && lexer.chars.get(1) == Some(&'!') {
        lexer.skip_line();
    }
    let mut out = Vec::new();
    lexer.lex_into(&mut out, Mode::Top)?;
    Ok(out)
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let end = tail.find(';').filter(|&e| e > 0 && e <= 10);
        let decoded = end.and_then(|e| {
            let name = &tail[..e];
            let code =
                if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    crate::entities::lookup(name)
                };
            code.and_then(char::from_u32).map(|c| (c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &tail[e + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn opens_statement_head(before: &[Token]) -> bool {
    let n = before.len();
    let ident = |i: usize| match &before[i].tok {
        Tok::Ident(name) if !before[i].escaped => Some(&**name),
        _ => None,
    };
    if n == 0 {
        return false;
    }
    let after_dot = n >= 2
        && matches!(
            before[n - 2].tok,
            Tok::Punct(P::Dot) | Tok::Punct(P::QuestionDot)
        );
    match ident(n - 1) {
        Some("if" | "for" | "while" | "with") => !after_dot,
        Some("await") => n >= 2 && ident(n - 2) == Some("for"),
        _ => false,
    }
}

fn regex_allowed_after(out: &[Token], close_head: bool) -> bool {
    if matches!(out.last().map(|t| &t.tok), Some(Tok::Punct(P::RParen))) {
        return close_head;
    }
    let n = out.len();
    if n >= 2
        && matches!(out[n - 1].tok, Tok::Ident(_))
        && matches!(
            out[n - 2].tok,
            Tok::Punct(P::Dot) | Tok::Punct(P::QuestionDot)
        )
    {
        return false;
    }
    regex_allowed(out.last().map(|t| &t.tok))
}

fn regex_allowed(prev: Option<&Tok>) -> bool {
    match prev {
        None => true,
        Some(Tok::Num(_))
        | Some(Tok::Str(_))
        | Some(Tok::Template(_))
        | Some(Tok::Regex(..))
        | Some(Tok::BigInt(_)) => false,
        Some(Tok::Ident(name)) => matches!(
            &**name,
            "return"
                | "typeof"
                | "instanceof"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "case"
                | "do"
                | "else"
                | "yield"
                | "await"
                | "extends"
                | "default"
        ),
        Some(Tok::Punct(p)) => !matches!(p, P::RParen | P::RBracket | P::PlusPlus | P::MinusMinus),
        Some(Tok::JsxText(_)) => false,
        Some(Tok::Eof) => true,
    }
}

fn nfkc_gap(c: char) -> bool {
    matches!(
        c,
        '\u{37a}' | '\u{e33}' | '\u{eb3}' | '\u{309b}' | '\u{309c}' | '\u{fc5e}'
            ..='\u{fc63}'
                | '\u{fdfa}'
                | '\u{fdfb}'
                | '\u{fe70}'
                | '\u{fe72}'
                | '\u{fe74}'
                | '\u{fe76}'
                | '\u{fe78}'
                | '\u{fe7a}'
                | '\u{fe7c}'
                | '\u{fe7e}'
                | '\u{ff9e}'
                | '\u{ff9f}'
    )
}

pub fn is_id_start(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic() || c == '_' || c == '$';
    }
    unicode_ident::is_xid_start(c) || nfkc_gap(c)
}

pub fn is_id_continue(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_' || c == '$';
    }
    unicode_ident::is_xid_continue(c) || nfkc_gap(c) || c == '\u{200c}' || c == '\u{200d}'
}

struct Lexer<'a> {
    src: &'a str,
    names: rustc_hash::FxHashMap<&'a str, JsStr>,
    html: bool,
    chars: Vec<char>,
    offsets: Vec<u32>,
    pos: usize,
    line: u32,
    col: u32,
    legacy_escape: bool,
    in_template: bool,
    sep_seen: bool,
    force_regex: Option<bool>,
    parens: Vec<bool>,
    close_head: bool,
    jsx: bool,
    jsx_ts: bool,
}

fn ascii_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

impl<'a> Lexer<'a> {
    fn punct(&self, c: char) -> Option<(P, usize)> {
        let at = |i: usize| self.chars.get(self.pos + i).copied();
        let next = at(1);
        Some(match c {
            '{' => (P::LBrace, 1),
            '}' => (P::RBrace, 1),
            '(' => (P::LParen, 1),
            ')' => (P::RParen, 1),
            '[' => (P::LBracket, 1),
            ']' => (P::RBracket, 1),
            ';' => (P::Semi, 1),
            ',' => (P::Comma, 1),
            '~' => (P::Tilde, 1),
            ':' => (P::Colon, 1),
            '#' => (P::Hash, 1),
            '@' => (P::At, 1),
            '.' => {
                if next == Some('.') && at(2) == Some('.') {
                    (P::Ellipsis, 3)
                } else {
                    (P::Dot, 1)
                }
            }
            '=' => match next {
                Some('=') if at(2) == Some('=') => (P::EqEqEq, 3),
                Some('=') => (P::EqEq, 2),
                Some('>') => (P::Arrow, 2),
                _ => (P::Eq, 1),
            },
            '!' => match next {
                Some('=') if at(2) == Some('=') => (P::NotEqEq, 3),
                Some('=') => (P::NotEq, 2),
                _ => (P::Bang, 1),
            },
            '<' => match next {
                Some('<') if at(2) == Some('=') => (P::ShlEq, 3),
                Some('<') => (P::Shl, 2),
                Some('=') => (P::Le, 2),
                _ => (P::Lt, 1),
            },
            '>' => match next {
                Some('>') => match at(2) {
                    Some('>') if at(3) == Some('=') => (P::UShrEq, 4),
                    Some('>') => (P::UShr, 3),
                    Some('=') => (P::ShrEq, 3),
                    _ => (P::Shr, 2),
                },
                Some('=') => (P::Ge, 2),
                _ => (P::Gt, 1),
            },
            '&' => match next {
                Some('&') if at(2) == Some('=') => (P::AmpAmpEq, 3),
                Some('&') => (P::AmpAmp, 2),
                Some('=') => (P::AmpEq, 2),
                _ => (P::Amp, 1),
            },
            '|' => match next {
                Some('|') if at(2) == Some('=') => (P::PipePipeEq, 3),
                Some('|') => (P::PipePipe, 2),
                Some('=') => (P::PipeEq, 2),
                _ => (P::Pipe, 1),
            },
            '?' => match next {
                Some('?') if at(2) == Some('=') => (P::NullishEq, 3),
                Some('?') => (P::Nullish, 2),
                Some('.') if !at(2).is_some_and(|d| d.is_ascii_digit()) => (P::QuestionDot, 2),
                _ => (P::Question, 1),
            },
            '+' => match next {
                Some('+') => (P::PlusPlus, 2),
                Some('=') => (P::PlusEq, 2),
                _ => (P::Plus, 1),
            },
            '-' => match next {
                Some('-') => (P::MinusMinus, 2),
                Some('=') => (P::MinusEq, 2),
                _ => (P::Minus, 1),
            },
            '*' => match next {
                Some('*') if at(2) == Some('=') => (P::StarStarEq, 3),
                Some('*') => (P::StarStar, 2),
                Some('=') => (P::StarEq, 2),
                _ => (P::Star, 1),
            },
            '/' => match next {
                Some('=') => (P::SlashEq, 2),
                _ => (P::Slash, 1),
            },
            '%' => match next {
                Some('=') => (P::PercentEq, 2),
                _ => (P::Percent, 1),
            },
            '^' => match next {
                Some('=') => (P::CaretEq, 2),
                _ => (P::Caret, 1),
            },
            _ => return None,
        })
    }

    fn intern(&mut self, begin: usize, end: usize) -> JsStr {
        let text = &self.src[self.offsets[begin] as usize..self.offsets[end] as usize];
        if let Some(s) = self.names.get(text) {
            return s.clone();
        }
        let s = JsStr::from(text);
        self.names.insert(text, s.clone());
        s
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn err(&self, msg: impl Into<String>) -> SyntaxError {
        SyntaxError::new(msg, self.line, self.col)
    }

    fn lex_into(&mut self, out: &mut Vec<Token>, mode: Mode) -> Result<(), SyntaxError> {
        let in_template = mode == Mode::Template;
        let mut depth = 0usize;
        let mut nl = true;
        loop {
            nl |= self.skip_trivia()?;
            let (line, col) = (self.line, self.col);
            let start = self.offsets[self.pos];
            let Some(c) = self.peek() else {
                if in_template {
                    return Err(self.err("Unterminated template literal"));
                }
                if mode == Mode::JsxExpr {
                    return Err(self.err("Unterminated JSX contents"));
                }
                out.push(Token {
                    tok: Tok::Eof,
                    line,
                    col,
                    nl_before: true,
                    escaped: false,
                    start,
                    end: start,
                });
                return Ok(());
            };
            if in_template {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    if depth == 0 {
                        self.bump();
                        out.push(Token {
                            tok: Tok::Eof,
                            line,
                            col,
                            nl_before: true,
                            escaped: false,
                            start,
                            end: start,
                        });
                        return Ok(());
                    }
                    depth -= 1;
                }
            }
            if mode == Mode::JsxExpr {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    if depth == 0 {
                        let begin = self.pos;
                        self.bump();
                        self.jsx_push(out, Tok::Punct(P::RBrace), begin, line, col, nl);
                        return Ok(());
                    }
                    depth -= 1;
                }
            }
            if c == '<' && self.jsx && self.jsx_start(out) {
                self.lex_jsx_element(out, nl)?;
                nl = false;
                continue;
            }
            let begin = self.pos;
            let forced = self.force_regex.take();
            let regex_ok = c == '/'
                && match forced {
                    Some(f) => f,
                    None => regex_allowed_after(out, self.close_head),
                };
            let tok = if regex_ok {
                let save = (self.pos, self.line, self.col);
                match self.regex() {
                    Ok(t) => t,
                    Err(_) => {
                        (self.pos, self.line, self.col) = save;
                        self.next_token(c)?
                    }
                }
            } else {
                self.next_token(c)?
            };
            let escaped = match &tok {
                Tok::Ident(_) => self.chars[begin..self.pos].contains(&'\\'),
                Tok::Str(_) | Tok::Num(_) => std::mem::take(&mut self.legacy_escape),
                _ => false,
            };
            let paren = match &tok {
                Tok::Punct(P::LParen) => Some(true),
                Tok::Punct(P::RParen) => Some(false),
                _ => None,
            };
            match paren {
                Some(true) => {
                    let head = opens_statement_head(out);
                    self.parens.push(head);
                }
                Some(false) => self.close_head = self.parens.pop().unwrap_or(false),
                None => {}
            }
            out.push(Token {
                tok,
                line,
                col,
                nl_before: nl,
                escaped,
                start,
                end: self.offsets[self.pos],
            });
            nl = false;
        }
    }

    fn jsx_push(
        &self,
        out: &mut Vec<Token>,
        tok: Tok,
        begin: usize,
        line: u32,
        col: u32,
        nl: bool,
    ) {
        out.push(Token {
            tok,
            line,
            col,
            nl_before: nl,
            escaped: false,
            start: self.offsets[begin],
            end: self.offsets[self.pos],
        });
    }

    fn jsx_punct(&mut self, out: &mut Vec<Token>, p: P) {
        let (line, col, begin) = (self.line, self.col, self.pos);
        self.bump();
        self.jsx_push(out, Tok::Punct(p), begin, line, col, false);
    }

    fn jsx_start(&self, out: &[Token]) -> bool {
        if !regex_allowed_after(out, self.close_head) {
            return false;
        }
        if let [.., before, last] = out
            && matches!(last.tok, Tok::Punct(P::Bang))
            && last.start == before.end
            && matches!(
                before.tok,
                Tok::Ident(_) | Tok::Punct(P::RParen | P::RBracket)
            )
        {
            return false;
        }
        let mut i = self.pos + 1;
        while self.chars.get(i).is_some_and(|c| *c == ' ' || *c == '\t') {
            i += 1;
        }
        match self.chars.get(i).copied() {
            Some('>') => true,
            Some(c) if is_id_start(c) => {
                if !self.jsx_ts {
                    return true;
                }
                let mut j = i;
                while self.chars.get(j).is_some_and(|c| is_id_continue(*c)) {
                    j += 1;
                }
                let mut k = j;
                while self.chars.get(k).is_some_and(|c| c.is_whitespace()) {
                    k += 1;
                }
                if self.chars.get(k) == Some(&',') {
                    return false;
                }
                let word: String = self.chars[k..(k + 7).min(self.chars.len())]
                    .iter()
                    .collect();
                if word == "extends" && k > j {
                    let mut m = k + 7;
                    while self.chars.get(m).is_some_and(|c| c.is_whitespace()) {
                        m += 1;
                    }
                    if self
                        .chars
                        .get(m)
                        .is_some_and(|c| !matches!(c, '=' | '>' | '/'))
                    {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn jsx_name(&mut self) -> Option<String> {
        let c = self.peek()?;
        if !is_id_start(c) {
            return None;
        }
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if is_id_continue(c) || c == '-' {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        Some(s)
    }

    fn jsx_ident(&mut self, out: &mut Vec<Token>) -> Result<(), SyntaxError> {
        let (line, col, begin) = (self.line, self.col, self.pos);
        let Some(name) = self.jsx_name() else {
            return Err(self.err("Expected a JSX identifier"));
        };
        self.jsx_push(out, Tok::Ident(name.into()), begin, line, col, false);
        Ok(())
    }

    fn lex_jsx_tag_name(&mut self, out: &mut Vec<Token>) -> Result<(), SyntaxError> {
        self.jsx_ident(out)?;
        if self.peek() == Some(':') {
            self.jsx_punct(out, P::Colon);
            return self.jsx_ident(out);
        }
        while self.peek() == Some('.') {
            self.jsx_punct(out, P::Dot);
            self.jsx_ident(out)?;
        }
        Ok(())
    }

    fn lex_jsx_element(&mut self, out: &mut Vec<Token>, nl: bool) -> Result<(), SyntaxError> {
        let (line, col, begin) = (self.line, self.col, self.pos);
        self.bump();
        self.jsx_push(out, Tok::Punct(P::JsxOpen), begin, line, col, nl);
        self.skip_trivia()?;
        if self.peek() == Some('>') {
            self.jsx_punct(out, P::Gt);
            return self.lex_jsx_children(out);
        }
        self.lex_jsx_tag_name(out)?;
        if self.jsx_ts {
            self.skip_trivia()?;
            if self.peek() == Some('<') {
                self.lex_jsx_type_args(out)?;
            }
        }
        loop {
            self.skip_trivia()?;
            let (line, col, begin) = (self.line, self.col, self.pos);
            match self.peek() {
                None => return Err(SyntaxError::new("Unterminated JSX contents", line, col)),
                Some('/') => {
                    self.jsx_punct(out, P::Slash);
                    self.skip_trivia()?;
                    if self.peek() != Some('>') {
                        return Err(self.err("Expected '>'"));
                    }
                    self.jsx_punct(out, P::Gt);
                    return Ok(());
                }
                Some('>') => {
                    self.jsx_punct(out, P::Gt);
                    return self.lex_jsx_children(out);
                }
                Some('{') => {
                    self.jsx_punct(out, P::LBrace);
                    self.lex_into(out, Mode::JsxExpr)?;
                }
                Some(_) => {
                    let Some(mut name) = self.jsx_name() else {
                        return Err(self.err("Invalid JSX attribute"));
                    };
                    if self.peek() == Some(':') {
                        self.bump();
                        if let Some(n2) = self.jsx_name() {
                            name = format!("{name}:{n2}");
                        }
                    }
                    self.jsx_push(out, Tok::Ident(name.into()), begin, line, col, false);
                    self.skip_trivia()?;
                    if self.peek() == Some('=') {
                        self.jsx_punct(out, P::Eq);
                        self.skip_trivia()?;
                        let (line, col, begin) = (self.line, self.col, self.pos);
                        match self.peek() {
                            Some(q @ ('"' | '\'')) => {
                                self.bump();
                                let mut s = String::new();
                                loop {
                                    match self.bump() {
                                        None => {
                                            return Err(SyntaxError::new(
                                                "Unterminated string constant",
                                                line,
                                                col,
                                            ));
                                        }
                                        Some(c) if c == q => break,
                                        Some(c) => s.push(c),
                                    }
                                }
                                self.jsx_push(
                                    out,
                                    Tok::Str(decode_entities(&s).into()),
                                    begin,
                                    line,
                                    col,
                                    false,
                                );
                            }
                            Some('{') => {
                                self.jsx_punct(out, P::LBrace);
                                self.lex_into(out, Mode::JsxExpr)?;
                            }
                            Some('<') => self.lex_jsx_element(out, false)?,
                            _ => {
                                return Err(self.err(
                                    "JSX value should be either an expression or a quoted JSX text",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    fn lex_jsx_type_args(&mut self, out: &mut Vec<Token>) -> Result<(), SyntaxError> {
        let mut depth = 0usize;
        loop {
            self.skip_trivia()?;
            let (line, col, begin) = (self.line, self.col, self.pos);
            match self.peek() {
                None => return Err(SyntaxError::new("Unterminated JSX contents", line, col)),
                Some('<') => {
                    self.jsx_punct(out, P::Lt);
                    depth += 1;
                }
                Some('>') => {
                    self.jsx_punct(out, P::Gt);
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some(c) => {
                    let tok = self.next_token(c)?;
                    self.jsx_push(out, tok, begin, line, col, false);
                }
            }
        }
    }

    fn lex_jsx_children(&mut self, out: &mut Vec<Token>) -> Result<(), SyntaxError> {
        loop {
            let (line, col, begin) = (self.line, self.col, self.pos);
            let mut text = String::new();
            while let Some(c) = self.peek() {
                if c == '<' || c == '{' {
                    break;
                }
                text.push(c);
                self.bump();
            }
            if !text.is_empty() {
                self.jsx_push(
                    out,
                    Tok::JsxText(decode_entities(&text).into()),
                    begin,
                    line,
                    col,
                    false,
                );
            }
            let (line, col, begin) = (self.line, self.col, self.pos);
            match self.peek() {
                None => return Err(SyntaxError::new("Unterminated JSX contents", line, col)),
                Some('{') => {
                    self.jsx_punct(out, P::LBrace);
                    self.lex_into(out, Mode::JsxExpr)?;
                }
                _ => {
                    let mut i = self.pos + 1;
                    while self.chars.get(i).is_some_and(|c| c.is_whitespace()) {
                        i += 1;
                    }
                    if self.chars.get(i) == Some(&'/') {
                        self.bump();
                        self.jsx_push(out, Tok::Punct(P::JsxOpen), begin, line, col, false);
                        self.skip_trivia()?;
                        self.jsx_punct(out, P::Slash);
                        self.skip_trivia()?;
                        if self.peek() != Some('>') {
                            self.lex_jsx_tag_name(out)?;
                            self.skip_trivia()?;
                        }
                        if self.peek() != Some('>') {
                            return Err(self.err("Expected '>'"));
                        }
                        self.jsx_punct(out, P::Gt);
                        return Ok(());
                    }
                    self.lex_jsx_element(out, false)?;
                }
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                break;
            }
            self.bump();
        }
    }

    fn skip_trivia(&mut self) -> Result<bool, SyntaxError> {
        let mut nl = false;
        let at_start = self.pos == 0;
        loop {
            match self.peek() {
                Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}') => {
                    nl = true;
                    self.bump();
                }
                Some(c) if c.is_whitespace() || c == '\u{feff}' => {
                    self.bump();
                }
                Some('/') if self.peek_at(1) == Some('/') => self.skip_line(),
                Some('<')
                    if self.html
                        && self.peek_at(1) == Some('!')
                        && self.peek_at(2) == Some('-')
                        && self.peek_at(3) == Some('-') =>
                {
                    self.skip_line()
                }
                Some('-')
                    if self.html
                        && (nl || at_start)
                        && self.peek_at(1) == Some('-')
                        && self.peek_at(2) == Some('>') =>
                {
                    self.skip_line()
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    let (line, col) = (self.line, self.col);
                    self.bump();
                    self.bump();
                    loop {
                        match self.bump() {
                            None => {
                                return Err(SyntaxError::new("Unterminated comment", line, col));
                            }
                            Some('*') if self.peek() == Some('/') => {
                                self.bump();
                                break;
                            }
                            Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}') => {
                                nl = true
                            }
                            _ => {}
                        }
                    }
                }
                _ => return Ok(nl),
            }
        }
    }

    fn next_token(&mut self, c: char) -> Result<Tok, SyntaxError> {
        if c.is_ascii_digit() || (c == '.' && self.peek_at(1).is_some_and(|d| d.is_ascii_digit())) {
            return self.number();
        }
        if c == '"' || c == '\'' {
            return self.string(c);
        }
        if c == '`' {
            return self.template();
        }
        if c.is_ascii() && ascii_ident(c) && !c.is_ascii_digit() {
            let begin = self.pos;
            let mut end = begin + 1;
            while self.chars.get(end).is_some_and(|&ch| ascii_ident(ch)) {
                end += 1;
            }
            if self
                .chars
                .get(end)
                .is_none_or(|&ch| ch.is_ascii() && ch != '\\')
            {
                let name = self.intern(begin, end);
                self.col += (end - begin) as u32;
                self.pos = end;
                return Ok(Tok::Ident(name));
            }
        }
        if is_id_start(c) || c == '\\' {
            let mut s = String::new();
            while let Some(c) = self.peek() {
                let first = s.is_empty();
                if c == '\\' {
                    self.bump();
                    if self.bump() != Some('u') {
                        return Err(self.err("Invalid Unicode escape sequence"));
                    }
                    let ch = char::from_u32(self.unicode_escape()?)
                        .filter(|&ch| {
                            if first {
                                is_id_start(ch)
                            } else {
                                is_id_continue(ch)
                            }
                        })
                        .ok_or_else(|| self.err("Invalid Unicode escape sequence"))?;
                    s.push(ch);
                } else if is_id_continue(c) {
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            return Ok(Tok::Ident(s.into()));
        }
        if let Some((p, len)) = self.punct(c) {
            self.pos += len;
            self.col += len as u32;
            return Ok(Tok::Punct(p));
        }
        Err(self.err(format!("Invalid or unexpected token '{c}'")))
    }

    fn number(&mut self) -> Result<Tok, SyntaxError> {
        if self.peek() == Some('0') {
            let radix = match self.peek_at(1) {
                Some('x') | Some('X') => 16,
                Some('o') | Some('O') => 8,
                Some('b') | Some('B') => 2,
                _ => 0,
            };
            if radix != 0 {
                let mut text = String::new();
                text.push(self.bump().unwrap_or('0'));
                text.push(self.bump().unwrap_or('x'));
                let mut value = 0f64;
                let mut any = false;
                while let Some(c) = self.peek() {
                    if c == '_' {
                        if !any || !self.peek_at(1).is_some_and(|d| d.is_digit(radix)) {
                            return Err(self.err("Numeric separators are not allowed here"));
                        }
                        self.bump();
                        continue;
                    }
                    match c.to_digit(radix) {
                        Some(d) => {
                            value = value * radix as f64 + d as f64;
                            text.push(c);
                            any = true;
                            self.bump();
                        }
                        None => break,
                    }
                }
                if !any {
                    return Err(self.err("Invalid or unexpected token"));
                }
                if self.peek() == Some('n') {
                    self.bump();
                    self.check_number_end()?;
                    return Ok(Tok::BigInt(text.into()));
                }
                self.check_number_end()?;
                return Ok(Tok::Num(value));
            }
        }
        let mut s = String::new();
        let leading_zero = self.peek() == Some('0');
        self.sep_seen = false;
        self.digits(&mut s);
        if leading_zero && s.len() > 1 {
            if self.sep_seen {
                return Err(self.err("Numeric separators are not allowed in legacy octal literals"));
            }
            self.legacy_escape = true;
            if s.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
                if self.peek() == Some('n') {
                    return Err(self.err("Invalid BigInt syntax"));
                }
                self.check_number_end()?;
                let v = s
                    .bytes()
                    .fold(0f64, |acc, b| acc * 8.0 + f64::from(b - b'0'));
                return Ok(Tok::Num(v));
            }
            if self.peek() == Some('n') {
                return Err(self.err("Invalid BigInt syntax"));
            }
        }
        if self.peek() == Some('n') && !s.is_empty() {
            self.bump();
            self.check_number_end()?;
            return Ok(Tok::BigInt(s.into()));
        }
        if self.peek() == Some('.') {
            s.push('.');
            self.bump();
            self.digits(&mut s);
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            let sign = matches!(self.peek_at(1), Some('+') | Some('-'));
            let digit_at = if sign { 2 } else { 1 };
            if self.peek_at(digit_at).is_some_and(|d| d.is_ascii_digit()) {
                s.push('e');
                self.bump();
                if sign {
                    s.push(self.bump().unwrap_or('+'));
                }
                self.digits(&mut s);
            }
        }
        self.check_number_end()?;
        if s.starts_with('.') {
            s.insert(0, '0');
        }
        if s.ends_with('.') {
            s.push('0');
        }
        s.parse::<f64>()
            .map(Tok::Num)
            .map_err(|_| self.err("Invalid number literal"))
    }

    fn digits(&mut self, s: &mut String) {
        let mut any = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.bump();
                any = true;
            } else if c == '_' && any && self.peek_at(1).is_some_and(|d| d.is_ascii_digit()) {
                self.sep_seen = true;
                self.bump();
            } else {
                break;
            }
        }
    }

    fn check_number_end(&self) -> Result<(), SyntaxError> {
        match self.peek() {
            Some(c) if is_id_start(c) || c.is_ascii_digit() => {
                Err(self.err("Invalid or unexpected token"))
            }
            _ => Ok(()),
        }
    }

    fn string(&mut self, quote: char) -> Result<Tok, SyntaxError> {
        let (line, col) = (self.line, self.col);
        self.bump();
        let mut s = String::new();
        loop {
            match self.bump() {
                None | Some('\n') | Some('\r') => {
                    return Err(SyntaxError::new("Unterminated string literal", line, col));
                }
                Some(c) if c == quote => break,
                Some('\\') => self.escape(&mut s)?,
                Some(c) => s.push(c),
            }
        }
        Ok(Tok::Str(s.into()))
    }

    fn hex(&mut self, count: usize) -> Result<u32, SyntaxError> {
        let mut v = 0u32;
        for _ in 0..count {
            let d = self
                .bump()
                .and_then(|c| c.to_digit(16))
                .ok_or_else(|| self.err("Invalid hexadecimal escape sequence"))?;
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn unicode_escape(&mut self) -> Result<u32, SyntaxError> {
        if self.peek() == Some('{') {
            self.bump();
            let mut v = 0u32;
            let mut any = false;
            loop {
                match self.bump() {
                    Some('}') if any => break,
                    Some(c) if c.is_ascii_hexdigit() => {
                        v = v
                            .checked_mul(16)
                            .and_then(|v| v.checked_add(c.to_digit(16).unwrap_or(0)))
                            .filter(|v| *v <= 0x10ffff)
                            .ok_or_else(|| self.err("Undefined Unicode code-point"))?;
                        any = true;
                    }
                    _ => return Err(self.err("Invalid Unicode escape sequence")),
                }
            }
            Ok(v)
        } else {
            self.hex(4)
        }
    }

    fn escape(&mut self, s: &mut String) -> Result<(), SyntaxError> {
        let c = self
            .bump()
            .ok_or_else(|| self.err("Unterminated string literal"))?;
        match c {
            'n' => s.push('\n'),
            't' => s.push('\t'),
            'r' => s.push('\r'),
            'b' => s.push('\u{8}'),
            'f' => s.push('\u{c}'),
            'v' => s.push('\u{b}'),
            '0' if !self.peek().is_some_and(|d| d.is_ascii_digit()) => s.push('\0'),
            '0'..='7' => {
                if self.in_template {
                    return Err(
                        self.err("Octal escape sequences are not allowed in template strings")
                    );
                }
                self.legacy_escape = true;
                let mut v = c.to_digit(8).unwrap_or(0);
                let more = if c <= '3' { 2 } else { 1 };
                for _ in 0..more {
                    match self.peek().and_then(|d| d.to_digit(8)) {
                        Some(d) => {
                            v = v * 8 + d;
                            self.bump();
                        }
                        None => break,
                    }
                }
                s.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            '8' | '9' => {
                if self.in_template {
                    return Err(self.err("\\8 and \\9 are not allowed in template strings"));
                }
                self.legacy_escape = true;
                s.push(c);
            }
            'x' => {
                let v = self.hex(2)?;
                s.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            'u' => {
                let v = self.unicode_escape()?;
                if (0xd800..0xdc00).contains(&v)
                    && self.peek() == Some('\\')
                    && self.peek_at(1) == Some('u')
                {
                    let save = (self.pos, self.line, self.col);
                    self.bump();
                    self.bump();
                    let low = self.unicode_escape()?;
                    if (0xdc00..0xe000).contains(&low) {
                        let cp = 0x10000 + ((v - 0xd800) << 10) + (low - 0xdc00);
                        s.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        return Ok(());
                    }
                    (self.pos, self.line, self.col) = save;
                }
                let c = char::from_u32(v).unwrap_or_else(|| crate::value::lone_char(v as u16));
                let mut buf = [0u8; 4];
                crate::value::push16(s, c.encode_utf8(&mut buf));
            }
            '\r' => {
                if self.peek() == Some('\n') {
                    self.bump();
                }
            }
            '\n' | '\u{2028}' | '\u{2029}' => {}
            other => s.push(other),
        }
        Ok(())
    }

    fn regex(&mut self) -> Result<Tok, SyntaxError> {
        let (line, col) = (self.line, self.col);
        self.bump();
        let mut body = String::new();
        let mut in_class = false;
        loop {
            match self.bump() {
                None | Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}') => {
                    return Err(SyntaxError::new(
                        "Invalid regular expression: missing /",
                        line,
                        col,
                    ));
                }
                Some('\\') => {
                    body.push('\\');
                    match self.bump() {
                        None | Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}') => {
                            return Err(SyntaxError::new(
                                "Invalid regular expression: missing /",
                                line,
                                col,
                            ));
                        }
                        Some(c) => body.push(c),
                    }
                }
                Some('[') => {
                    in_class = true;
                    body.push('[');
                }
                Some(']') => {
                    in_class = false;
                    body.push(']');
                }
                Some('/') if !in_class => break,
                Some(c) => body.push(c),
            }
        }
        let mut flags = String::new();
        while let Some(c) = self.peek() {
            if is_id_continue(c) {
                flags.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let valid = flags.chars().all(|c| "dgimsuyv".contains(c))
            && flags
                .chars()
                .enumerate()
                .all(|(i, c)| !flags[i + 1..].contains(c));
        if !valid {
            return Err(SyntaxError::new(
                format!("Invalid regular expression flags '{flags}'"),
                line,
                col,
            ));
        }
        Ok(Tok::Regex(body.into(), flags.into()))
    }

    fn template(&mut self) -> Result<Tok, SyntaxError> {
        let (line, col) = (self.line, self.col);
        self.bump();
        let mut parts = Vec::new();
        let mut cur = String::new();
        let mut raw = String::new();
        let mut bad = None;
        loop {
            let start = self.pos;
            match self.bump() {
                None => return Err(SyntaxError::new("Unterminated template literal", line, col)),
                Some('`') => break,
                Some('\\') => {
                    let save = (self.pos, self.line, self.col);
                    self.in_template = true;
                    let escaped = self.escape(&mut cur);
                    self.in_template = false;
                    if let Err(e) = escaped {
                        bad.get_or_insert(e);
                        (self.pos, self.line, self.col) = save;
                        self.bump();
                    }
                    let mut i = start;
                    while i < self.pos {
                        let c = self.chars[i];
                        if c == '\r' {
                            raw.push('\n');
                            if self.chars.get(i + 1) == Some(&'\n') {
                                i += 1;
                            }
                        } else {
                            raw.push(c);
                        }
                        i += 1;
                    }
                }
                Some('$') if self.peek() == Some('{') => {
                    self.bump();
                    let cooked = JsStr::from(std::mem::take(&mut cur));
                    parts.push(TemplatePart::Str(
                        bad.take().map_or(Ok(cooked), Err),
                        JsStr::from(std::mem::take(&mut raw)),
                    ));
                    let mut toks = Vec::new();
                    self.lex_into(&mut toks, Mode::Template)?;
                    parts.push(TemplatePart::Expr(toks));
                }
                Some('\r') => {
                    if self.peek() == Some('\n') {
                        self.bump();
                    }
                    cur.push('\n');
                    raw.push('\n');
                }
                Some(c) => {
                    cur.push(c);
                    raw.push(c);
                }
            }
        }
        parts.push(TemplatePart::Str(
            bad.map_or(Ok(cur.into()), Err),
            raw.into(),
        ));
        Ok(Tok::Template(parts))
    }
}
