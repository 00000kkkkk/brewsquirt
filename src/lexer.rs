use crate::token::{P, PUNCTUATORS, SyntaxError, TemplatePart, Tok, Token};
use std::rc::Rc;

pub fn tokenize(src: &str) -> Result<Vec<Token>, SyntaxError> {
    let chars: Vec<char> = src.chars().collect();
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut b = 0u32;
    for c in &chars {
        offsets.push(b);
        b += c.len_utf8() as u32;
    }
    offsets.push(b);
    let mut lexer = Lexer {
        chars,
        offsets,
        pos: 0,
        line: 1,
        col: 1,
    };
    if lexer.chars.first() == Some(&'#') && lexer.chars.get(1) == Some(&'!') {
        while let Some(c) = lexer.peek() {
            if c == '\n' {
                break;
            }
            lexer.bump();
        }
    }
    let mut out = Vec::new();
    lexer.lex_into(&mut out, false)?;
    Ok(out)
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
        ),
        Some(Tok::Punct(p)) => !matches!(p, P::RParen | P::RBracket | P::PlusPlus | P::MinusMinus),
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

struct Lexer {
    chars: Vec<char>,
    offsets: Vec<u32>,
    pos: usize,
    line: u32,
    col: u32,
}

impl Lexer {
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

    fn lex_into(&mut self, out: &mut Vec<Token>, in_template: bool) -> Result<(), SyntaxError> {
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
            let begin = self.pos;
            let regex_ok = regex_allowed(out.last().map(|t| &t.tok));
            let tok = if c == '/' && regex_ok {
                self.regex()?
            } else {
                self.next_token(c)?
            };
            let escaped =
                matches!(tok, Tok::Ident(_)) && self.chars[begin..self.pos].contains(&'\\');
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

    fn skip_trivia(&mut self) -> Result<bool, SyntaxError> {
        let mut nl = false;
        loop {
            match self.peek() {
                Some('\n') | Some('\u{2028}') | Some('\u{2029}') => {
                    nl = true;
                    self.bump();
                }
                Some(c) if c.is_whitespace() || c == '\u{feff}' => {
                    self.bump();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
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
                            Some('\n') => nl = true,
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
        for (text, p) in PUNCTUATORS {
            if self.matches(text) {
                if *p == P::QuestionDot && self.peek_at(2).is_some_and(|d| d.is_ascii_digit()) {
                    self.bump();
                    return Ok(Tok::Punct(P::Question));
                }
                for _ in 0..text.len() {
                    self.bump();
                }
                return Ok(Tok::Punct(*p));
            }
        }
        Err(self.err(format!("Invalid or unexpected token '{c}'")))
    }

    fn matches(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(i, ch)| self.peek_at(i) == Some(ch))
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
        self.digits(&mut s);
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
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.bump();
            } else if c == '_' && self.peek_at(1).is_some_and(|d| d.is_ascii_digit()) {
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
                None | Some('\n') => {
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
                s.push(char::from_u32(v).unwrap_or('\u{fffd}'));
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
                None | Some('\n') => {
                    return Err(SyntaxError::new(
                        "Invalid regular expression: missing /",
                        line,
                        col,
                    ));
                }
                Some('\\') => {
                    body.push('\\');
                    match self.bump() {
                        None | Some('\n') => {
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
                    if let Err(e) = self.escape(&mut cur) {
                        bad.get_or_insert(e);
                        (self.pos, self.line, self.col) = save;
                        self.bump();
                    }
                    raw.extend(&self.chars[start..self.pos]);
                }
                Some('$') if self.peek() == Some('{') => {
                    self.bump();
                    let cooked = Rc::from(std::mem::take(&mut cur));
                    parts.push(TemplatePart::Str(
                        bad.take().map_or(Ok(cooked), Err),
                        Rc::from(std::mem::take(&mut raw)),
                    ));
                    let mut toks = Vec::new();
                    self.lex_into(&mut toks, true)?;
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
