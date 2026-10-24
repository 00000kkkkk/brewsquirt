use regex_syntax::hir::{ClassUnicode, ClassUnicodeRange};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

static STRINGS: &[u8] = include_bytes!("unicode_strings.bin");

pub const JS_SPACE: &[(u32, u32)] = &[
    (0x09, 0x0d),
    (0x20, 0x20),
    (0xa0, 0xa0),
    (0x1680, 0x1680),
    (0x2000, 0x200a),
    (0x2028, 0x2029),
    (0x202f, 0x202f),
    (0x205f, 0x205f),
    (0x3000, 0x3000),
    (0xfeff, 0xfeff),
];

const WORD: &[(u32, u32)] = &[(0x30, 0x39), (0x41, 0x5a), (0x5f, 0x5f), (0x61, 0x7a)];

type Strings = HashMap<&'static str, Vec<Vec<u32>>>;

fn string_table() -> &'static Strings {
    static TABLE: OnceLock<Strings> = OnceLock::new();
    TABLE.get_or_init(|| {
        let varint = |at: &mut usize| -> u32 {
            let mut n = 0u32;
            let mut shift = 0;
            loop {
                let b = STRINGS[*at];
                *at += 1;
                n |= ((b & 0x7f) as u32) << shift;
                if b < 0x80 {
                    return n;
                }
                shift += 7;
            }
        };
        let mut at = 1;
        let mut table = HashMap::new();
        for _ in 0..STRINGS[0] {
            let len = STRINGS[at] as usize;
            at += 1;
            let name = std::str::from_utf8(&STRINGS[at..at + len]).unwrap_or_default();
            at += len;
            let count = varint(&mut at) as usize;
            let mut list = Vec::with_capacity(count);
            for _ in 0..count {
                let n = STRINGS[at] as usize;
                at += 1;
                list.push((0..n).map(|_| varint(&mut at)).collect());
            }
            table.insert(name, list);
        }
        table
    })
}

pub fn string_property(name: &str) -> Option<Vec<&'static [u32]>> {
    let t = string_table();
    if name == "RGI_Emoji" {
        let mut all: Vec<&'static [u32]> = Vec::new();
        for list in t.values() {
            all.extend(list.iter().map(|s| s.as_slice()));
        }
        return Some(all);
    }
    t.get(name)
        .map(|list| list.iter().map(|s| s.as_slice()).collect())
}

#[derive(Clone, Default)]
struct VSet {
    ranges: Vec<(u32, u32)>,
    strings: BTreeSet<Vec<u32>>,
}

fn normalize(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (a, b) in ranges {
        match out.last_mut() {
            Some(last) if a <= last.1.saturating_add(1) => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

fn complement(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    crate::unicode_props::scalar_ranges(ranges, true)
}

fn intersect(a: &[(u32, u32)], b: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let lo = a[i].0.max(b[j].0);
        let hi = a[i].1.min(b[j].1);
        if lo <= hi {
            out.push((lo, hi));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

fn fold_ranges(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let items = ranges.iter().filter_map(|&(a, b)| {
        Some(ClassUnicodeRange::new(
            char::from_u32(a)?,
            char::from_u32(b)?,
        ))
    });
    let mut class = ClassUnicode::new(items);
    class.case_fold_simple();
    class
        .ranges()
        .iter()
        .map(|r| (r.start() as u32, r.end() as u32))
        .collect()
}

fn fold_char(c: u32) -> u32 {
    fold_ranges(&[(c, c)]).first().map_or(c, |r| r.0)
}

impl VSet {
    fn chars(ranges: Vec<(u32, u32)>) -> VSet {
        VSet {
            ranges: normalize(ranges),
            strings: BTreeSet::new(),
        }
    }

    fn add_string(&mut self, s: Vec<u32>) {
        if s.len() == 1 {
            self.ranges = normalize([self.ranges.as_slice(), &[(s[0], s[0])]].concat());
        } else {
            self.strings.insert(s);
        }
    }

    fn fold(mut self, icase: bool) -> VSet {
        if icase {
            self.ranges = fold_ranges(&self.ranges);
            self.strings = self
                .strings
                .into_iter()
                .map(|s| s.into_iter().map(fold_char).collect())
                .collect();
        }
        self
    }

    fn union(mut self, other: VSet) -> VSet {
        self.ranges = normalize([self.ranges, other.ranges].concat());
        self.strings.extend(other.strings);
        self
    }

    fn intersection(self, other: VSet) -> VSet {
        VSet {
            ranges: intersect(&self.ranges, &other.ranges),
            strings: self.strings.intersection(&other.strings).cloned().collect(),
        }
    }

    fn difference(self, other: VSet) -> VSet {
        VSet {
            ranges: intersect(&self.ranges, &complement(&other.ranges)),
            strings: self.strings.difference(&other.strings).cloned().collect(),
        }
    }
}

struct Parser<'a> {
    c: &'a [char],
    pos: usize,
    icase: bool,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.c.get(self.pos).copied()
    }

    fn at(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(k, ch)| self.c.get(self.pos + k) == Some(&ch))
    }

    fn hex(&mut self, n: usize) -> u32 {
        let mut v = 0;
        for _ in 0..n {
            v = v * 16 + self.peek().and_then(|c| c.to_digit(16)).unwrap_or(0);
            self.pos += 1;
        }
        v
    }

    fn escape_char(&mut self) -> u32 {
        let Some(c) = self.peek() else {
            return '\\' as u32;
        };
        self.pos += 1;
        match c {
            'f' => 0x0c,
            'n' => 0x0a,
            'r' => 0x0d,
            't' => 0x09,
            'v' => 0x0b,
            'b' => 0x08,
            '0' => 0,
            'c' => {
                let l = self.peek().map_or(0, |l| l as u32 % 32);
                self.pos += 1;
                l
            }
            'x' => self.hex(2),
            'u' if self.peek() == Some('{') => {
                self.pos += 1;
                let mut v = 0u32;
                while let Some(d) = self.peek().and_then(|c| c.to_digit(16)) {
                    v = v * 16 + d;
                    self.pos += 1;
                }
                self.pos += 1;
                v
            }
            'u' => {
                let hi = self.hex(4);
                if (0xd800..0xdc00).contains(&hi) && self.at("\\u") {
                    let save = self.pos;
                    self.pos += 2;
                    let lo = self.hex(4);
                    if (0xdc00..0xe000).contains(&lo) {
                        return 0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00);
                    }
                    self.pos = save;
                }
                hi
            }
            other => other as u32,
        }
    }

    fn property(&mut self, negated: bool) -> VSet {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| c != '}') {
            self.pos += 1;
        }
        let body: String = self.c[start..self.pos].iter().collect();
        self.pos += 1;
        if let Some(list) = string_property(&body) {
            let mut set = VSet::default();
            for s in list {
                set.add_string(s.to_vec());
            }
            return set.fold(self.icase);
        }
        let ranges = crate::unicode_props::lookup(&body)
            .map(|r| crate::unicode_props::scalar_ranges(r, false))
            .unwrap_or_default();
        let set = VSet::chars(ranges).fold(self.icase);
        if negated {
            VSet::chars(complement(&set.ranges))
        } else {
            set
        }
    }

    fn escape_class(&mut self, e: char) -> Option<VSet> {
        let base = match e.to_ascii_lowercase() {
            'd' => vec![(0x30, 0x39)],
            's' => JS_SPACE.to_vec(),
            'w' => WORD.to_vec(),
            _ => return None,
        };
        let set = VSet::chars(base).fold(self.icase);
        Some(if e.is_ascii_uppercase() {
            VSet::chars(complement(&set.ranges))
        } else {
            set
        })
    }

    fn string_disjunction(&mut self) -> VSet {
        self.pos += 1;
        let mut set = VSet::default();
        let mut cur: Vec<u32> = Vec::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '}' => break,
                '|' => set.add_string(std::mem::take(&mut cur)),
                '\\' => cur.push(self.escape_char()),
                c => cur.push(c as u32),
            }
        }
        set.add_string(cur);
        set.fold(self.icase)
    }

    fn operand(&mut self) -> Result<VSet, u32> {
        match self.peek() {
            Some('[') => Ok(self.class()),
            Some('\\') => {
                self.pos += 1;
                let e = self.peek().unwrap_or('\\');
                match e {
                    'q' if self.c.get(self.pos + 1) == Some(&'{') => {
                        self.pos += 1;
                        Ok(self.string_disjunction())
                    }
                    'p' | 'P' if self.c.get(self.pos + 1) == Some(&'{') => {
                        self.pos += 1;
                        Ok(self.property(e == 'P'))
                    }
                    'd' | 'D' | 's' | 'S' | 'w' | 'W' => {
                        self.pos += 1;
                        Ok(self.escape_class(e).unwrap_or_default())
                    }
                    _ => Err(self.escape_char()),
                }
            }
            Some(c) => {
                self.pos += 1;
                Err(c as u32)
            }
            None => Ok(VSet::default()),
        }
    }

    fn single(&self, c: u32) -> VSet {
        VSet::chars(vec![(c, c)]).fold(self.icase)
    }

    fn operand_set(&mut self) -> VSet {
        match self.operand() {
            Ok(set) => set,
            Err(c) => self.single(c),
        }
    }

    fn class(&mut self) -> VSet {
        self.pos += 1;
        let negated = self.peek() == Some('^');
        if negated {
            self.pos += 1;
        }
        let set = self.contents();
        self.pos += 1;
        if negated {
            VSet::chars(complement(&set.ranges))
        } else {
            set
        }
    }

    fn contents(&mut self) -> VSet {
        if self.peek() == Some(']') {
            return VSet::default();
        }
        let first = self.operand();
        if self.at("&&") {
            let mut set = first.unwrap_or_else(|c| self.single(c));
            while self.at("&&") {
                self.pos += 2;
                let rhs = self.operand_set();
                set = set.intersection(rhs);
            }
            return set;
        }
        if self.at("--") {
            let mut set = first.unwrap_or_else(|c| self.single(c));
            while self.at("--") {
                self.pos += 2;
                let rhs = self.operand_set();
                set = set.difference(rhs);
            }
            return set;
        }
        let mut set = VSet::default();
        let mut item = first;
        loop {
            match item {
                Err(lo) if self.peek() == Some('-') && !self.at("--") => {
                    self.pos += 1;
                    let hi = match self.operand() {
                        Err(hi) => hi,
                        Ok(_) => lo,
                    };
                    set = set.union(VSet::chars(vec![(lo, hi)]).fold(self.icase));
                }
                Err(c) => set = set.union(self.single(c)),
                Ok(s) => set = set.union(s),
            }
            if self.peek().is_none_or(|c| c == ']') {
                return set;
            }
            item = self.operand();
        }
    }
}

fn push_point(out: &mut Vec<char>, c: u32) {
    out.extend(format!("\\u{{{c:X}}}").chars());
}

fn emit(set: &VSet, out: &mut Vec<char>) {
    let mut strings: Vec<&Vec<u32>> = set.strings.iter().filter(|s| !s.is_empty()).collect();
    strings.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let empty = set.strings.contains(&Vec::new());
    out.extend("(?:".chars());
    let mut first = true;
    for s in strings {
        if !first {
            out.push('|');
        }
        first = false;
        for &c in s {
            push_point(out, c);
        }
    }
    if !set.ranges.is_empty() || first {
        if !first {
            out.push('|');
        }
        first = false;
        out.push('[');
        for &(a, b) in &set.ranges {
            push_point(out, a);
            if b > a {
                out.push('-');
                push_point(out, b);
            }
        }
        out.push(']');
    }
    if empty && !first {
        out.push('|');
    }
    out.push(')');
}

fn property_name_at(chars: &[char], at: usize) -> Option<(String, usize)> {
    if chars.get(at + 1) != Some(&'{') {
        return None;
    }
    let end = chars[at + 2..].iter().position(|&c| c == '}')? + at + 2;
    Some((chars[at + 2..end].iter().collect(), end))
}

fn needs_expansion(body: &[char]) -> bool {
    let mut i = 0;
    while i < body.len() {
        if body[i] == '\\' {
            match body.get(i + 1) {
                Some('q') if body.get(i + 2) == Some(&'{') => return true,
                Some('p') => {
                    if let Some((name, _)) = property_name_at(body, i + 1)
                        && string_property(&name).is_some()
                    {
                        return true;
                    }
                }
                _ => {}
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    false
}

fn class_end(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

pub fn expand(chars: Vec<char>, icase: bool) -> Vec<char> {
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    let mut changed = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            if chars.get(i + 1) == Some(&'p')
                && let Some((name, end)) = property_name_at(&chars, i + 1)
                && string_property(&name).is_some()
            {
                let mut p = Parser {
                    c: &chars,
                    pos: i + 2,
                    icase,
                };
                emit(&p.property(false), &mut out);
                changed = true;
                i = end + 1;
                continue;
            }
            out.push(c);
            if let Some(&n) = chars.get(i + 1) {
                out.push(n);
            }
            i += 2;
            continue;
        }
        if c == '['
            && let Some(end) = class_end(&chars, i)
        {
            if needs_expansion(&chars[i..=end]) {
                let mut p = Parser {
                    c: &chars[..=end],
                    pos: i,
                    icase,
                };
                emit(&p.class(), &mut out);
                changed = true;
            } else {
                out.extend_from_slice(&chars[i..=end]);
            }
            i = end + 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    if changed { out } else { chars }
}
