use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P {
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semi,
    Comma,
    Dot,
    Ellipsis,
    Question,
    QuestionDot,
    Nullish,
    NullishEq,
    Colon,
    Arrow,
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    Percent,
    PlusPlus,
    MinusMinus,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    NotEq,
    EqEqEq,
    NotEqEq,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Bang,
    AmpAmp,
    PipePipe,
    Shl,
    Shr,
    UShr,
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    StarStarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    UShrEq,
    AmpAmpEq,
    PipePipeEq,
    Hash,
    At,
}

pub const PUNCTUATORS: &[(&str, P)] = &[
    (">>>=", P::UShrEq),
    ("===", P::EqEqEq),
    ("!==", P::NotEqEq),
    ("**=", P::StarStarEq),
    ("...", P::Ellipsis),
    ("<<=", P::ShlEq),
    (">>=", P::ShrEq),
    (">>>", P::UShr),
    ("&&=", P::AmpAmpEq),
    ("||=", P::PipePipeEq),
    ("??=", P::NullishEq),
    ("=>", P::Arrow),
    ("==", P::EqEq),
    ("!=", P::NotEq),
    ("<=", P::Le),
    (">=", P::Ge),
    ("&&", P::AmpAmp),
    ("||", P::PipePipe),
    ("??", P::Nullish),
    ("?.", P::QuestionDot),
    ("++", P::PlusPlus),
    ("--", P::MinusMinus),
    ("+=", P::PlusEq),
    ("-=", P::MinusEq),
    ("*=", P::StarEq),
    ("/=", P::SlashEq),
    ("%=", P::PercentEq),
    ("&=", P::AmpEq),
    ("|=", P::PipeEq),
    ("^=", P::CaretEq),
    ("**", P::StarStar),
    ("<<", P::Shl),
    (">>", P::Shr),
    ("{", P::LBrace),
    ("}", P::RBrace),
    ("(", P::LParen),
    (")", P::RParen),
    ("[", P::LBracket),
    ("]", P::RBracket),
    (";", P::Semi),
    (",", P::Comma),
    ("<", P::Lt),
    (">", P::Gt),
    ("+", P::Plus),
    ("-", P::Minus),
    ("*", P::Star),
    ("/", P::Slash),
    ("%", P::Percent),
    ("&", P::Amp),
    ("|", P::Pipe),
    ("^", P::Caret),
    ("!", P::Bang),
    ("~", P::Tilde),
    ("?", P::Question),
    (":", P::Colon),
    ("=", P::Eq),
    (".", P::Dot),
    ("#", P::Hash),
    ("@", P::At),
];

impl P {
    pub fn as_str(self) -> &'static str {
        PUNCTUATORS
            .iter()
            .find(|(_, p)| *p == self)
            .map(|(s, _)| *s)
            .unwrap_or("?")
    }
}

#[derive(Clone, Debug)]
pub enum TemplatePart {
    Str(Result<Rc<str>, SyntaxError>, Rc<str>),
    Expr(Vec<Token>),
}

#[derive(Clone, Debug)]
pub enum Tok {
    Num(f64),
    Str(Rc<str>),
    Template(Vec<TemplatePart>),
    BigInt(Rc<str>),
    Regex(Rc<str>, Rc<str>),
    Ident(Rc<str>),
    Punct(P),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
    pub nl_before: bool,
    pub start: u32,
    pub end: u32,
}

impl Token {
    pub fn describe(&self) -> String {
        match &self.tok {
            Tok::Num(n) => crate::number::number_to_string(*n),
            Tok::BigInt(s) => format!("{s}n"),
            Tok::Str(s) => format!("string \"{s}\""),
            Tok::Template(_) => "template literal".to_string(),
            Tok::Regex(p, f) => format!("/{p}/{f}"),
            Tok::Ident(s) => s.to_string(),
            Tok::Punct(p) => p.as_str().to_string(),
            Tok::Eof => "end of input".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SyntaxError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl SyntaxError {
    pub fn new(message: impl Into<String>, line: u32, col: u32) -> Self {
        SyntaxError {
            message: message.into(),
            line,
            col,
        }
    }

    pub fn is_eof(&self) -> bool {
        self.message.contains("end of input") || self.message.starts_with("Unterminated")
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}:{})", self.message, self.line, self.col)
    }
}

impl std::error::Error for SyntaxError {}
