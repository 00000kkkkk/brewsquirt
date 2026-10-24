use std::collections::HashMap;
use std::sync::OnceLock;

static DATA: &[u8] = include_bytes!("unicode_props.bin");

struct Table {
    names: HashMap<&'static str, usize>,
    sets: Vec<Vec<(u32, u32)>>,
}

fn read_u16(at: &mut usize) -> usize {
    let v = DATA[*at] as usize | (DATA[*at + 1] as usize) << 8;
    *at += 2;
    v
}

fn read_varint(at: &mut usize) -> u32 {
    let mut n = 0u32;
    let mut shift = 0;
    loop {
        let b = DATA[*at];
        *at += 1;
        n |= ((b & 0x7f) as u32) << shift;
        if b < 0x80 {
            return n;
        }
        shift += 7;
    }
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut at = 0;
        let count = read_u16(&mut at);
        let mut names = HashMap::with_capacity(count);
        for _ in 0..count {
            let len = DATA[at] as usize;
            at += 1;
            let name = std::str::from_utf8(&DATA[at..at + len]).unwrap_or_default();
            at += len;
            names.insert(name, read_u16(&mut at));
        }
        let count = read_u16(&mut at);
        let mut sets = Vec::with_capacity(count);
        for _ in 0..count {
            let n = read_varint(&mut at) as usize;
            let mut ranges = Vec::with_capacity(n);
            let mut prev = 0;
            for _ in 0..n {
                let start = prev + read_varint(&mut at);
                let end = start + read_varint(&mut at);
                ranges.push((start, end));
                prev = end;
            }
            sets.push(ranges);
        }
        Table { names, sets }
    })
}

pub fn lookup(body: &str) -> Option<&'static [(u32, u32)]> {
    let t = table();
    let key = match body.split_once('=') {
        Some(("General_Category" | "gc", v)) => format!("gc={v}"),
        Some(("Script" | "sc", v)) => format!("sc={v}"),
        Some(("Script_Extensions" | "scx", v)) => format!("scx={v}"),
        Some(_) => return None,
        None if t.names.contains_key(body) => body.to_string(),
        None => format!("gc={body}"),
    };
    t.names.get(key.as_str()).map(|&i| t.sets[i].as_slice())
}

pub fn scalar_ranges(ranges: &[(u32, u32)], complement: bool) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(ranges.len() + 1);
    let mut push = |a: u32, b: u32| {
        if a <= b {
            if b < 0xd800 || a > 0xdfff {
                out.push((a, b));
            } else {
                if a < 0xd800 {
                    out.push((a, 0xd7ff));
                }
                if b > 0xdfff {
                    out.push((0xe000, b));
                }
            }
        }
    };
    if complement {
        let mut next = 0u32;
        for &(a, b) in ranges {
            if a > next {
                push(next, a - 1);
            }
            next = b + 1;
        }
        if next <= 0x10ffff {
            push(next, 0x10ffff);
        }
    } else {
        for &(a, b) in ranges {
            push(a, b);
        }
    }
    out
}

pub fn class_text(ranges: &[(u32, u32)], negated: bool) -> String {
    if ranges.is_empty() {
        return if negated {
            "[\\x00-\\x{10FFFF}]".into()
        } else {
            "[\\x{E000}&&\\x{E001}]".into()
        };
    }
    let mut out = String::with_capacity(ranges.len() * 20 + 3);
    out.push('[');
    if negated {
        out.push('^');
    }
    for &(a, b) in ranges {
        out.push_str(&format!("\\x{{{a:X}}}"));
        if b > a {
            out.push_str(&format!("-\\x{{{b:X}}}"));
        }
    }
    out.push(']');
    out
}
