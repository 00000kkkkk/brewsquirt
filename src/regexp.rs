use crate::jsstr::JsStr;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

pub struct RegexData {
    pub source: JsStr,
    pub flags: JsStr,
    pub re: fancy_regex::Regex,
    pub global: bool,
    pub sticky: bool,
    pub names: Vec<Option<JsStr>>,
}

pub struct Match {
    pub start: usize,
    pub end: usize,
    pub groups: Vec<Option<(usize, usize)>>,
}

type Cache = FxHashMap<(JsStr, JsStr), Rc<RegexData>>;

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(FxHashMap::default());
}

fn hex_digits(chars: &[char], i: usize, n: usize) -> Option<u32> {
    if i + n > chars.len() {
        return None;
    }
    let mut v = 0u32;
    for c in &chars[i..i + n] {
        v = v * 16 + c.to_digit(16)?;
    }
    Some(v)
}

fn group_defined_before(chars: &[char], open: usize, close: usize) -> bool {
    let name: String = chars[open + 2..close].iter().collect();
    let pat: Vec<char> = format!("(?<{name}>").chars().collect();
    chars[..open]
        .windows(pat.len())
        .any(|w| w == pat.as_slice())
}

fn push_code_point(out: &mut String, cp: u32) {
    out.push_str(&format!("\\x{{{cp:X}}}"));
}

fn is_quantifier(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    let start = j;
    while j < chars.len() && chars[j].is_ascii_digit() {
        j += 1;
    }
    if j == start {
        return None;
    }
    if j < chars.len() && chars[j] == ',' {
        j += 1;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
    }
    if j < chars.len() && chars[j] == '}' {
        Some(j)
    } else {
        None
    }
}

fn octal_at(chars: &[char], i: usize) -> (u32, usize) {
    let mut v = 0u32;
    let mut n = 0;
    while n < 3 {
        let Some(d) = chars.get(i + n).and_then(|c| c.to_digit(8)) else {
            break;
        };
        if v * 8 + d > 255 {
            break;
        }
        v = v * 8 + d;
        n += 1;
    }
    (v, n.max(1))
}

fn class_atom(
    out: &mut String,
    in_class: bool,
    class_last: &mut Option<u32>,
    pending_dash: &mut Option<usize>,
    code: u32,
) {
    if !in_class {
        return;
    }
    match (*pending_dash, *class_last) {
        (Some(dash), Some(start)) => {
            *pending_dash = None;
            if start > code {
                out.truncate(dash);
                *class_last = Some(code);
            } else {
                *class_last = None;
            }
        }
        _ => {
            *pending_dash = None;
            *class_last = Some(code);
        }
    }
}

const EMOJI_SEQUENCE: &str = r"(?:[\x{1F1E6}-\x{1F1FF}]{2}|[0-9#*]\x{FE0F}?\x{20E3}|\x{1F3F4}[\x{E0020}-\x{E007E}]+\x{E007F}|(?:\p{Emoji_Presentation}|\p{Emoji}\x{FE0F})[\x{1F3FB}-\x{1F3FF}]?(?:\x{200D}(?:\p{Emoji_Presentation}|\p{Emoji}\x{FE0F}?)[\x{1F3FB}-\x{1F3FF}]?)*)";

pub fn translate(source: &str, flags: &str) -> Result<String, String> {
    let chars: Vec<char> = source.chars().collect();
    let (groups, has_names) = crate::regexp_syntax::prescan(&chars, flags.contains('v'));
    let mut out = String::new();
    let mut inline = String::new();
    if flags.contains('i') {
        inline.push('i');
    }
    if flags.contains('m') {
        inline.push('m');
    }
    if flags.contains('s') {
        inline.push('s');
    }
    if !inline.is_empty() {
        out.push_str(&format!("(?{inline})"));
    }
    let dot_all = flags.contains('s');
    let unicode = flags.contains('u') || flags.contains('v');
    let mut in_class = false;
    let mut class_last: Option<u32> = None;
    let mut pending_dash: Option<usize> = None;
    let mut groups_open: Vec<Option<usize>> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                let saved_last = class_last.take();
                let saved_dash = pending_dash.take();
                i += 1;
                let Some(&e) = chars.get(i) else {
                    return Err("\\ at end of pattern".to_string());
                };
                let mut atom = |out: &mut String, code: u32| {
                    class_last = saved_last;
                    pending_dash = saved_dash;
                    class_atom(out, in_class, &mut class_last, &mut pending_dash, code);
                };
                match e {
                    'd' if !in_class => out.push_str("[0-9]"),
                    'D' if !in_class => out.push_str("[^0-9]"),
                    'w' if !in_class => out.push_str("[A-Za-z0-9_]"),
                    'W' if !in_class => out.push_str("[^A-Za-z0-9_]"),
                    'd' | 'w' | 's' | 'S' | 'D' | 'W' if in_class => {
                        match e {
                            'd' => out.push_str("0-9"),
                            'w' => out.push_str("A-Za-z0-9_"),
                            _ => {
                                out.push('\\');
                                out.push(e);
                            }
                        }
                        if !unicode
                            && chars.get(i + 1) == Some(&'-')
                            && chars.get(i + 2) != Some(&']')
                        {
                            out.push_str("\\-");
                            i += 1;
                        }
                    }
                    'b' if in_class => out.push_str("\\x08"),
                    's' | 'S' | 'b' | 'B' | 'n' | 'r' | 't' | 'f' | 'v' | 'D' | 'W' => {
                        out.push('\\');
                        out.push(e);
                    }
                    '0' if !chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) => {
                        atom(&mut out, 0);
                        out.push_str("\\x00")
                    }
                    '0'..='9' => {
                        let mut j = i;
                        let mut n: usize = 0;
                        while let Some(d) = chars.get(j).and_then(|c| c.to_digit(10)) {
                            n = n.saturating_mul(10).saturating_add(d as usize);
                            j += 1;
                        }
                        if !in_class && e != '0' && n <= groups {
                            let (before, _) =
                                crate::regexp_syntax::prescan(&chars[..i - 1], flags.contains('v'));
                            if n > before {
                                out.push_str("(?:)");
                            } else {
                                out.push('\\');
                                out.extend(chars[i..j].iter());
                            }
                            i = j - 1;
                        } else if e == '8' || e == '9' {
                            out.push(e);
                        } else {
                            let (v, used) = octal_at(&chars, i);
                            atom(&mut out, v);
                            push_code_point(&mut out, v);
                            i += used - 1;
                        }
                    }
                    'k' if chars.get(i + 1) == Some(&'<') => {
                        let close = chars[i + 2..]
                            .iter()
                            .position(|c| *c == '>')
                            .map(|p| i + 2 + p);
                        if !has_names && !flags.contains('u') && !flags.contains('v') {
                            out.push('k');
                        } else if let Some(c) =
                            close.filter(|c| !group_defined_before(&chars, i, *c))
                        {
                            out.push_str("(?:)");
                            i = c;
                        } else {
                            out.push_str("\\k");
                        }
                    }
                    'x' => match hex_digits(&chars, i + 1, 2) {
                        Some(v) => {
                            atom(&mut out, v);
                            push_code_point(&mut out, v);
                            i += 2;
                        }
                        None => out.push('x'),
                    },
                    'u' => {
                        if chars.get(i + 1) == Some(&'{') {
                            let end = chars[i + 1..]
                                .iter()
                                .position(|&ch| ch == '}')
                                .ok_or("Invalid Unicode escape")?;
                            let hex: String = chars[i + 2..i + 1 + end].iter().collect();
                            let mut v = u32::from_str_radix(&hex, 16)
                                .map_err(|_| "Invalid Unicode escape".to_string())?;
                            if (0xd800..0xe000).contains(&v) {
                                v = crate::value::LONE_BASE + (v - 0xd800);
                            }
                            atom(&mut out, v);
                            push_code_point(&mut out, v);
                            i += 1 + end;
                        } else if let Some(v) = hex_digits(&chars, i + 1, 4) {
                            i += 4;
                            let mut cp = v;
                            if (0xd800..0xdc00).contains(&v)
                                && chars.get(i + 1) == Some(&'\\')
                                && chars.get(i + 2) == Some(&'u')
                            {
                                if let Some(low) = hex_digits(&chars, i + 3, 4) {
                                    if (0xdc00..0xe000).contains(&low) {
                                        cp = 0x10000 + ((v - 0xd800) << 10) + (low - 0xdc00);
                                        i += 6;
                                    }
                                }
                            }
                            if (0xd800..0xe000).contains(&cp) {
                                cp = crate::value::LONE_BASE + (cp - 0xd800);
                            }
                            atom(&mut out, cp);
                            push_code_point(&mut out, cp);
                        } else {
                            out.push('u');
                        }
                    }
                    'c' => match chars.get(i + 1) {
                        Some(l) if l.is_ascii_alphabetic() => {
                            push_code_point(&mut out, (*l as u32) % 32);
                            i += 1;
                        }
                        _ => out.push_str("\\\\c"),
                    },
                    'p' | 'P' if chars.get(i + 1) == Some(&'{') => {
                        let end = chars[i + 1..]
                            .iter()
                            .position(|&ch| ch == '}')
                            .ok_or("Invalid property name")?;
                        let body: String = chars[i + 2..i + 1 + end].iter().collect();
                        let body = match body.split_once('=') {
                            Some(("Script_Extensions" | "scx", v)) => format!("scx={v}"),
                            Some(("Script" | "sc", v)) => format!("sc={v}"),
                            Some(("General_Category" | "gc", v)) => format!("gc={v}"),
                            _ => body,
                        };
                        let emoji_sequence = matches!(
                            body.as_str(),
                            "RGI_Emoji"
                                | "Basic_Emoji"
                                | "Emoji_Keycap_Sequence"
                                | "RGI_Emoji_Flag_Sequence"
                                | "RGI_Emoji_Modifier_Sequence"
                                | "RGI_Emoji_Tag_Sequence"
                                | "RGI_Emoji_ZWJ_Sequence"
                        );
                        if emoji_sequence && e == 'p' {
                            out.push_str(if in_class {
                                "\\p{Emoji_Presentation}"
                            } else {
                                EMOJI_SEQUENCE
                            });
                        } else if matches!(
                            body.as_str(),
                            "Cs" | "Surrogate" | "gc=Cs" | "gc=Surrogate"
                        ) {
                            if e == 'p' {
                                out.push_str("[\\x{E000}&&\\x{E001}]");
                            } else if in_class {
                                out.push_str("\\x00-\\x{10FFFF}");
                            } else {
                                out.push_str("[\\x00-\\x{10FFFF}]");
                            }
                        } else {
                            out.push('\\');
                            out.push(e);
                            out.push('{');
                            out.push_str(&body);
                            out.push('}');
                        }
                        i += 1 + end;
                    }
                    'p' | 'P' => {
                        if unicode {
                            out.push('\\');
                        }
                        out.push(e);
                    }
                    '/' => {
                        atom(&mut out, '/' as u32);
                        out.push('/')
                    }
                    '-' => {
                        atom(&mut out, '-' as u32);
                        out.push_str("\\-")
                    }
                    e if e.is_ascii_alphanumeric() => {
                        atom(&mut out, e as u32);
                        out.push(e)
                    }
                    e => {
                        atom(&mut out, e as u32);
                        out.push('\\');
                        out.push(e);
                    }
                }
            }
            '[' if !in_class => {
                if chars.get(i + 1) == Some(&']') {
                    out.push_str("(?:[^\\x00-\\x{10FFFF}])");
                    i += 2;
                    continue;
                }
                if chars.get(i + 1) == Some(&'^') && chars.get(i + 2) == Some(&']') {
                    out.push_str("(?s:.)");
                    i += 3;
                    continue;
                }
                in_class = true;
                class_last = None;
                pending_dash = None;
                out.push('[');
                if chars.get(i + 1) == Some(&'^') {
                    out.push('^');
                    i += 1;
                }
                if chars.get(i + 1) == Some(&']') {
                    out.push_str("\\]");
                    i += 1;
                }
            }
            '[' => out.push_str("\\["),
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '&' | '~' if in_class => {
                out.push('\\');
                out.push(c);
            }
            '.' if !in_class => {
                if dot_all {
                    out.push_str("(?s:.)");
                } else {
                    out.push_str("[^\\n\\r\\x{2028}\\x{2029}]");
                }
            }
            '(' if !in_class => {
                let lookahead = chars.get(i + 1) == Some(&'?')
                    && matches!(chars.get(i + 2), Some('=') | Some('!'));
                groups_open.push(if lookahead { Some(out.len()) } else { None });
                out.push('(');
            }
            ')' if !in_class => {
                out.push(')');
                if let Some(Some(start)) = groups_open.pop() {
                    if !unicode {
                        let q_end = match chars.get(i + 1) {
                            Some('*') | Some('+') | Some('?') => Some(i + 1),
                            Some('{') => is_quantifier(&chars, i + 1),
                            _ => None,
                        };
                        if let Some(mut q_end) = q_end {
                            let q: String = chars[i + 1..=q_end].iter().collect();
                            if chars.get(q_end + 1) == Some(&'?') {
                                q_end += 1;
                            }
                            let optional =
                                q.starts_with('*') || q.starts_with('?') || q.starts_with("{0");
                            if optional {
                                out.insert_str(start, "(?:");
                                out.push_str("|)");
                            }
                            i = q_end;
                        }
                    }
                }
            }
            '{' if !in_class => match is_quantifier(&chars, i) {
                Some(end) => {
                    let q: String = chars[i..=end].iter().collect();
                    out.push_str(&q);
                    i = end;
                }
                None => out.push_str("\\{"),
            },
            '}' if !in_class => out.push_str("\\}"),
            '-' if in_class
                && class_last.is_some()
                && pending_dash.is_none()
                && chars.get(i + 1) != Some(&']')
                && chars.get(i + 1) != Some(&'-') =>
            {
                pending_dash = Some(out.len());
                out.push('-');
            }
            '-' if in_class => {
                class_last = None;
                pending_dash = None;
                out.push('-');
            }
            _ => {
                class_atom(
                    &mut out,
                    in_class,
                    &mut class_last,
                    &mut pending_dash,
                    c as u32,
                );
                out.push(c);
            }
        }
        i += 1;
    }
    if in_class {
        return Err("Unterminated character class".to_string());
    }
    Ok(out)
}

pub fn compile(source: &str, flags: &str) -> Result<Rc<RegexData>, String> {
    let key: (JsStr, JsStr) = (source.into(), flags.into());
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(hit);
    }
    crate::regexp_syntax::validate(source, flags)
        .map_err(|e| format!("Invalid regular expression: /{source}/{flags}: {e}"))?;
    let translated = translate(source, flags)
        .map_err(|e| format!("Invalid regular expression: /{source}/{flags}: {e}"))?;
    let re = fancy_regex::RegexBuilder::new(&translated)
        .backtrack_limit(10_000_000)
        .build()
        .map_err(|e| format!("Invalid regular expression: /{source}/{flags}: {e}"))?;
    let names = re.capture_names().map(|n| n.map(JsStr::from)).collect();
    let data = Rc::new(RegexData {
        source: key.0.clone(),
        flags: key.1.clone(),
        re,
        global: flags.contains('g'),
        sticky: flags.contains('y'),
        names,
    });
    CACHE.with(|c| c.borrow_mut().insert(key, data.clone()));
    Ok(data)
}

impl RegexData {
    pub fn group_count(&self) -> usize {
        self.names.len()
    }

    pub fn has_names(&self) -> bool {
        self.names.iter().any(|n| n.is_some())
    }

    pub fn find_at(&self, text: &str, pos: usize) -> Result<Option<Match>, String> {
        self.find_from(text, pos, self.sticky)
    }

    pub fn find_whole(
        &self,
        text: &str,
        pos: usize,
        sticky: bool,
    ) -> Result<Option<(usize, usize)>, String> {
        if pos > text.len() {
            return Ok(None);
        }
        let found = self
            .re
            .find_from_pos(text, pos)
            .map_err(|e| format!("RegExp execution failed: {e}"))?;
        Ok(match found {
            Some(m) if !sticky || m.start() == pos => Some((m.start(), m.end())),
            _ => None,
        })
    }

    pub fn find_from(&self, text: &str, pos: usize, sticky: bool) -> Result<Option<Match>, String> {
        if pos > text.len() {
            return Ok(None);
        }
        let caps = self
            .re
            .captures_from_pos(text, pos)
            .map_err(|e| format!("RegExp execution failed: {e}"))?;
        let Some(caps) = caps else {
            return Ok(None);
        };
        let whole = caps.get(0).expect("group 0");
        if sticky && whole.start() != pos {
            return Ok(None);
        }
        let groups = (0..self.group_count())
            .map(|i| caps.get(i).map(|m| (m.start(), m.end())))
            .collect();
        Ok(Some(Match {
            start: whole.start(),
            end: whole.end(),
            groups,
        }))
    }
}

pub fn utf16_index(s: &str, byte: usize) -> usize {
    if s.is_ascii() {
        return byte;
    }
    crate::value::len16(&s[..byte.min(s.len())])
}

pub fn byte_index(s: &str, unit: usize) -> usize {
    if s.is_ascii() {
        return unit.min(s.len());
    }
    let mut count = 0;
    for (b, ch) in s.char_indices() {
        if count >= unit {
            return b;
        }
        count += crate::value::char_len16(ch);
    }
    s.len()
}

pub fn next_char_boundary(s: &str, byte: usize) -> usize {
    match s[byte..].chars().next() {
        Some(c) => byte + c.len_utf8(),
        None => byte + 1,
    }
}

pub fn escape_source(src: &str) -> String {
    if src.is_empty() {
        return "(?:)".to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut in_class = false;
    let mut escaped = false;
    for c in src.chars() {
        if escaped {
            out.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => {
                out.push(c);
                escaped = true;
            }
            '[' => {
                in_class = true;
                out.push(c);
            }
            ']' => {
                in_class = false;
                out.push(c);
            }
            '/' if !in_class => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}
