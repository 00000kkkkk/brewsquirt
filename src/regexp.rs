use crate::jsstr::JsStr;
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct RegexData {
    pub source: JsStr,
    pub flags: JsStr,
    pub re: fancy_regex::Regex,
    pub global: bool,
    pub sticky: bool,
    pub names: Vec<Option<JsStr>>,
    fast: Option<fancy_regex::Regex>,
    fast_lines: bool,
    fast_seen: Cell<Option<(usize, usize, bool)>>,
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
    let Some(start) = chars[..open]
        .windows(pat.len())
        .position(|w| w == pat.as_slice())
    else {
        return false;
    };
    let mut depth = 0usize;
    let mut in_class = false;
    let mut i = start;
    while i < open {
        match chars[i] {
            '\\' => i += 1,
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '(' if !in_class => depth += 1,
            ')' if !in_class => {
                depth -= 1;
                if depth == 0 {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
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

fn js_space(negated: bool) -> String {
    crate::unicode_props::class_text(crate::regexp_sets::JS_SPACE, negated)
}

fn surrogate_class(
    chars: &[char],
    open: usize,
    lo: u32,
    hi: u32,
) -> Option<(Vec<(u32, u32)>, usize)> {
    if chars.get(open) != Some(&'[') || chars.get(open + 1) == Some(&'^') {
        return None;
    }
    let mut ranges = Vec::new();
    let mut i = open + 1;
    let unit = |at: usize| -> Option<u32> {
        if chars.get(at) == Some(&'\\') && chars.get(at + 1) == Some(&'u') {
            hex_digits(chars, at + 2, 4).filter(|v| (lo..=hi).contains(v))
        } else {
            None
        }
    };
    loop {
        if chars.get(i) == Some(&']') {
            break;
        }
        let start = unit(i)?;
        i += 6;
        if chars.get(i) == Some(&'-') && chars.get(i + 1) != Some(&']') {
            let end = unit(i + 1)?;
            if end < start {
                return None;
            }
            ranges.push((start, end));
            i += 7;
        } else {
            ranges.push((start, start));
        }
    }
    if ranges.is_empty() {
        return None;
    }
    if lo == 0xdc00 && matches!(chars.get(i + 1), Some('*' | '+' | '?' | '{')) {
        return None;
    }
    Some((ranges, i))
}

fn astral_class(highs: &[(u32, u32)], lows: &[(u32, u32)]) -> String {
    let mut spans: Vec<(u32, u32)> = Vec::new();
    for &(h1, h2) in highs {
        for h in h1..=h2 {
            for &(l1, l2) in lows {
                let a = 0x10000 + ((h - 0xd800) << 10) + (l1 - 0xdc00);
                let b = 0x10000 + ((h - 0xd800) << 10) + (l2 - 0xdc00);
                match spans.last_mut() {
                    Some(last) if last.1 + 1 == a => last.1 = b,
                    _ => spans.push((a, b)),
                }
            }
        }
    }
    let mut out = String::from("[");
    for (a, b) in spans {
        if a == b {
            out.push_str(&format!("\\x{{{a:X}}}"));
        } else {
            out.push_str(&format!("\\x{{{a:X}}}-\\x{{{b:X}}}"));
        }
    }
    out.push(']');
    out
}

const LINE_START: &str = r"(?:\A|(?<=[\n\r\x{2028}\x{2029}]))";
const LINE_END: &str = r"(?:\z|(?=[\n\r\x{2028}\x{2029}]))";

fn word_class(extended: bool, negated: bool) -> &'static str {
    match (extended, negated) {
        (false, false) => "(?-i:[A-Za-z0-9_])",
        (false, true) => "(?-i:[^A-Za-z0-9_])",
        (true, false) => r"(?-i:[A-Za-z0-9_\x{17F}\x{212A}])",
        (true, true) => r"(?-i:[^A-Za-z0-9_\x{17F}\x{212A}])",
    }
}

fn word_boundary(extended: bool, negated: bool) -> String {
    let w = word_class(extended, false);
    if negated {
        format!("(?:(?<={w})(?={w})|(?<!{w})(?!{w}))")
    } else {
        format!("(?:(?<={w})(?!{w})|(?<!{w})(?={w}))")
    }
}

fn backref(numbers: &[usize], plain: bool) -> String {
    if plain {
        let alternatives: Vec<String> = numbers.iter().map(|n| format!("\\{n}")).collect();
        return format!("(?:{})", alternatives.join("|"));
    }
    let mut out = String::new();
    for n in numbers {
        out.push_str(&format!("(?({n})\\{n}|"));
    }
    out.push_str(&")".repeat(numbers.len()));
    out
}

fn named_groups(chars: &[char], v: bool) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut depth = 0usize;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '[' => depth += 1,
            ']' if depth > 0 => {
                if v {
                    depth -= 1;
                } else {
                    depth = 0;
                }
            }
            '(' if depth == 0
                && chars.get(i + 1) == Some(&'?')
                && chars.get(i + 2) == Some(&'<')
                && !matches!(chars.get(i + 3), Some('=') | Some('!')) =>
            {
                if let Some((name, _)) = group_name_at(chars, i + 2) {
                    let (before, _) = crate::regexp_syntax::prescan(&chars[..i], v);
                    out.push((name, before + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn group_name_at(chars: &[char], open: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut i = open + 1;
    loop {
        let c = *chars.get(i)?;
        if c == '>' {
            return Some((name, i));
        }
        if c != '\\' {
            name.push(c);
            i += 1;
            continue;
        }
        if chars.get(i + 1) != Some(&'u') {
            return None;
        }
        let unit = |at: usize| hex_digits(chars, at, 4);
        let v = if chars.get(i + 2) == Some(&'{') {
            let end = chars[i + 3..].iter().position(|&ch| ch == '}')? + i + 3;
            let hex: String = chars[i + 3..end].iter().collect();
            i = end + 1;
            u32::from_str_radix(&hex, 16).ok()?
        } else {
            let hi = unit(i + 2)?;
            i += 6;
            if (0xd800..0xdc00).contains(&hi)
                && chars.get(i) == Some(&'\\')
                && chars.get(i + 1) == Some(&'u')
            {
                match unit(i + 2) {
                    Some(lo) if (0xdc00..0xe000).contains(&lo) => {
                        i += 6;
                        0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
                    }
                    _ => hi,
                }
            } else {
                hi
            }
        };
        name.push(char::from_u32(v)?);
    }
}

pub fn translate(source: &str, flags: &str) -> Result<String, String> {
    translate_with(source, flags, false)
}

fn translate_with(source: &str, flags: &str, fast: bool) -> Result<String, String> {
    let vmode = flags.contains('v');
    let mut chars: Vec<char> = source.chars().collect();
    if vmode {
        chars = crate::regexp_sets::expand(chars, flags.contains('i'));
    }
    let (groups, has_names) = crate::regexp_syntax::prescan(&chars, flags.contains('v'));
    let mut out = String::new();
    let mut inline = String::new();
    if flags.contains('i') {
        inline.push('i');
    }
    if flags.contains('s') {
        inline.push('s');
    }
    if !inline.is_empty() {
        out.push_str(&format!("(?{inline})"));
    }
    let unicode = flags.contains('u') || flags.contains('v');
    let mut icase = flags.contains('i');
    let mut multiline = flags.contains('m');
    let mut dot_all = flags.contains('s');
    let mut modifiers: Vec<(bool, bool, bool, bool)> = Vec::new();
    let mut lookbehind = 0usize;
    let mut in_class = false;
    let mut class_depth = 0usize;
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
                    'w' if !in_class => out.push_str(word_class(icase && unicode, false)),
                    'W' if !in_class => out.push_str(word_class(icase && unicode, true)),
                    'b' | 'B' if !in_class && fast => {
                        out.push('\\');
                        out.push(e);
                    }
                    'b' if !in_class => out.push_str(&word_boundary(icase && unicode, false)),
                    'B' if !in_class => out.push_str(&word_boundary(icase && unicode, true)),
                    'd' | 'w' | 's' | 'S' | 'D' | 'W' if in_class => {
                        match e {
                            'd' => out.push_str("0-9"),
                            'w' => out.push_str("A-Za-z0-9_"),
                            's' => out.push_str(&js_space(false)[1..]),
                            'S' => out.push_str(&js_space(true)),
                            'D' => out.push_str("[^0-9]"),
                            'W' => out.push_str("[^A-Za-z0-9_]"),
                            _ => {
                                out.push('\\');
                                out.push(e);
                            }
                        }
                        if e == 's' {
                            out.pop();
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
                    's' | 'S' => out.push_str(&js_space(e == 'S')),
                    'b' | 'B' | 'n' | 'r' | 't' | 'f' | 'v' | 'D' | 'W' => {
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
                                out.push_str(&backref(&[n], lookbehind > 0));
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
                        } else if let Some((name, end)) = group_name_at(&chars, i + 1) {
                            let numbers: Vec<usize> = named_groups(&chars, flags.contains('v'))
                                .into_iter()
                                .filter(|(n, _)| *n == name)
                                .map(|(_, number)| number)
                                .collect();
                            if numbers.is_empty() {
                                out.push_str("\\k<");
                                out.push_str(&name);
                                out.push('>');
                            } else {
                                out.push_str(&backref(&numbers, lookbehind > 0));
                            }
                            i = end;
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
                            if !in_class && !unicode && (0xd800..0xdc00).contains(&cp) {
                                if let Some((lows, close)) =
                                    surrogate_class(&chars, i + 1, 0xdc00, 0xdfff)
                                {
                                    out.push_str(&astral_class(&[(cp, cp)], &lows));
                                    i = close;
                                    i += 1;
                                    continue;
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
                    'p' | 'P' if unicode && chars.get(i + 1) == Some(&'{') => {
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
                        if matches!(body.as_str(), "Cs" | "Surrogate" | "gc=Cs" | "gc=Surrogate") {
                            if e == 'p' {
                                out.push_str("[\\x{E000}&&\\x{E001}]");
                            } else if in_class {
                                out.push_str("\\x00-\\x{10FFFF}");
                            } else {
                                out.push_str("[\\x00-\\x{10FFFF}]");
                            }
                        } else if let Some(ranges) = crate::unicode_props::lookup(&body) {
                            let complement = e == 'P' && !vmode;
                            let set = crate::unicode_props::scalar_ranges(ranges, complement);
                            out.push_str(&crate::unicode_props::class_text(
                                &set,
                                e == 'P' && vmode,
                            ));
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
                if !unicode {
                    if let Some((highs, close)) = surrogate_class(&chars, i, 0xd800, 0xdbff) {
                        if let Some((lows, close2)) =
                            surrogate_class(&chars, close + 1, 0xdc00, 0xdfff)
                        {
                            out.push_str(&astral_class(&highs, &lows));
                            i = close2 + 1;
                            continue;
                        }
                    }
                }
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
            '[' if vmode => {
                class_depth += 1;
                class_last = None;
                pending_dash = None;
                out.push('[');
                if chars.get(i + 1) == Some(&'^') {
                    out.push('^');
                    i += 1;
                }
            }
            '[' => out.push_str("\\["),
            ']' if in_class && class_depth > 0 => {
                class_depth -= 1;
                out.push(']');
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '&' if in_class && vmode && chars.get(i + 1) == Some(&'&') => {
                out.push_str("&&");
                i += 1;
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
            '^' if !in_class => out.push_str(match (multiline, lookbehind > 0 || fast) {
                (false, _) => "\\A",
                (true, false) => LINE_START,
                (true, true) => "(?m:^)",
            }),
            '$' if !in_class => out.push_str(match (multiline, lookbehind > 0 || fast) {
                (false, _) => "\\z",
                (true, false) => LINE_END,
                (true, true) => "(?m:$)",
            }),
            '(' if !in_class => {
                let behind = chars.get(i + 1) == Some(&'?')
                    && chars.get(i + 2) == Some(&'<')
                    && matches!(chars.get(i + 3), Some('=') | Some('!'));
                if behind {
                    lookbehind += 1;
                }
                modifiers.push((icase, multiline, dot_all, behind));
                if chars.get(i + 1) == Some(&'?') {
                    let mut j = i + 2;
                    let mut on = true;
                    let mut changes = Vec::new();
                    while let Some(&m) = chars.get(j) {
                        match m {
                            'i' | 'm' | 's' => changes.push((m, on)),
                            '-' => on = false,
                            _ => break,
                        }
                        j += 1;
                    }
                    if chars.get(j) == Some(&':') {
                        for (m, value) in changes {
                            match m {
                                'i' => icase = value,
                                'm' => multiline = value,
                                _ => dot_all = value,
                            }
                        }
                    }
                }
                let lookahead = chars.get(i + 1) == Some(&'?')
                    && matches!(chars.get(i + 2), Some('=') | Some('!'));
                groups_open.push(if lookahead { Some(out.len()) } else { None });
                out.push('(');
                let named = chars.get(i + 1) == Some(&'?')
                    && chars.get(i + 2) == Some(&'<')
                    && !matches!(chars.get(i + 3), Some('=') | Some('!'));
                if let Some((name, end)) = named.then(|| group_name_at(&chars, i + 2)).flatten() {
                    out.push_str("?<");
                    out.push_str(&name);
                    out.push('>');
                    i = end;
                }
            }
            ')' if !in_class => {
                if let Some((a, b, c, behind)) = modifiers.pop() {
                    icase = a;
                    multiline = b;
                    dot_all = c;
                    if behind {
                        lookbehind -= 1;
                    }
                }
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
    let quick = translate_with(source, flags, true)
        .ok()
        .filter(|t| *t != translated);
    let fast_lines = quick.as_ref().is_some_and(|t| t.contains("(?m:"));
    let fast = quick.and_then(|t| {
        fancy_regex::RegexBuilder::new(&t)
            .backtrack_limit(10_000_000)
            .build()
            .ok()
    });
    let mut names: Vec<Option<JsStr>> = re.capture_names().map(|n| n.map(JsStr::from)).collect();
    let chars: Vec<char> = source.chars().collect();
    for (name, number) in named_groups(&chars, flags.contains('v')) {
        if let Some(slot) = names.get_mut(number) {
            *slot = Some(name.as_str().into());
        }
    }
    let data = Rc::new(RegexData {
        source: key.0.clone(),
        flags: key.1.clone(),
        re,
        global: flags.contains('g'),
        sticky: flags.contains('y'),
        names,
        fast,
        fast_lines,
        fast_seen: Cell::new(None),
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

    fn engine(&self, text: &str) -> &fancy_regex::Regex {
        let Some(fast) = &self.fast else {
            return &self.re;
        };
        let key = (text.as_ptr() as usize, text.len());
        let usable = match self.fast_seen.get() {
            Some((p, l, ok)) if (p, l) == key => ok,
            _ => {
                let ok = text.is_ascii() && !(self.fast_lines && text.as_bytes().contains(&b'\r'));
                self.fast_seen.set(Some((key.0, key.1, ok)));
                ok
            }
        };
        if usable { fast } else { &self.re }
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
            .engine(text)
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
            .engine(text)
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
