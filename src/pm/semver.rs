use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s
            .strip_prefix('v')
            .or_else(|| s.strip_prefix('='))
            .unwrap_or(s);
        let s = s.split('+').next().unwrap_or("");
        let (core, pre) = match s.split_once('-') {
            Some((c, p)) => (c, p.split('.').map(str::to_string).collect()),
            None => (s, Vec::new()),
        };
        let mut nums = core.split('.');
        let major = nums.next()?.parse().ok()?;
        let minor = nums.next()?.parse().ok()?;
        let patch = nums.next()?.parse().ok()?;
        if nums.next().is_some() {
            return None;
        }
        Some(Version {
            major,
            minor,
            patch,
            pre,
        })
    }

    fn core(&self) -> (u64, u64, u64) {
        (self.major, self.minor, self.patch)
    }
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(m), Ok(n)) => m.cmp(&n),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            _ => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    a.len().cmp(&b.len())
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core()
            .cmp(&other.core())
            .then_with(|| cmp_pre(&self.pre, &other.pre))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

#[derive(Debug, Clone)]
struct Comparator {
    op: Op,
    version: Version,
}

impl Comparator {
    fn matches(&self, v: &Version) -> bool {
        match self.op {
            Op::Lt => v < &self.version,
            Op::Le => v <= &self.version,
            Op::Gt => v > &self.version,
            Op::Ge => v >= &self.version,
            Op::Eq => v == &self.version,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Range {
    sets: Vec<Vec<Comparator>>,
    text: String,
}

struct Partial {
    major: Option<u64>,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: Vec<String>,
}

fn parse_partial(s: &str) -> Option<Partial> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, p.split('.').map(str::to_string).collect()),
        None => (s, Vec::new()),
    };
    let mut out = Partial {
        major: None,
        minor: None,
        patch: None,
        pre,
    };
    let wild = |t: &str| matches!(t, "x" | "X" | "*" | "");
    for (i, part) in core.split('.').enumerate() {
        if i > 2 {
            return None;
        }
        let value = if wild(part) {
            None
        } else {
            Some(part.parse::<u64>().ok()?)
        };
        match i {
            0 => out.major = value,
            1 => out.minor = value,
            _ => out.patch = value,
        }
        if value.is_none() {
            break;
        }
    }
    Some(out)
}

fn v(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        major,
        minor,
        patch,
        pre: Vec::new(),
    }
}

fn comparators_for(op_text: &str, p: Partial) -> Option<Vec<Comparator>> {
    let full = |p: &Partial| Version {
        major: p.major.unwrap_or(0),
        minor: p.minor.unwrap_or(0),
        patch: p.patch.unwrap_or(0),
        pre: p.pre.clone(),
    };
    let ge = |ver: Version| Comparator {
        op: Op::Ge,
        version: ver,
    };
    let lt = |ver: Version| Comparator {
        op: Op::Lt,
        version: ver,
    };
    Some(match op_text {
        "" | "=" => match (p.major, p.minor, p.patch) {
            (None, _, _) => vec![ge(v(0, 0, 0))],
            (Some(a), None, _) => vec![ge(v(a, 0, 0)), lt(v(a + 1, 0, 0))],
            (Some(a), Some(b), None) => vec![ge(v(a, b, 0)), lt(v(a, b + 1, 0))],
            _ => vec![Comparator {
                op: Op::Eq,
                version: full(&p),
            }],
        },
        "^" => match (p.major, p.minor, p.patch) {
            (None, _, _) => vec![ge(v(0, 0, 0))],
            (Some(a), None, _) => vec![ge(v(a, 0, 0)), lt(v(a + 1, 0, 0))],
            (Some(0), Some(b), None) => vec![ge(v(0, b, 0)), lt(v(0, b + 1, 0))],
            (Some(a), Some(_), None) => vec![ge(full(&p)), lt(v(a + 1, 0, 0))],
            (Some(0), Some(0), Some(c)) => vec![ge(full(&p)), lt(v(0, 0, c + 1))],
            (Some(0), Some(b), Some(_)) => vec![ge(full(&p)), lt(v(0, b + 1, 0))],
            (Some(a), _, _) => vec![ge(full(&p)), lt(v(a + 1, 0, 0))],
        },
        "~" | "~>" => match (p.major, p.minor, p.patch) {
            (None, _, _) => vec![ge(v(0, 0, 0))],
            (Some(a), None, _) => vec![ge(v(a, 0, 0)), lt(v(a + 1, 0, 0))],
            (Some(a), Some(b), _) => vec![ge(full(&p)), lt(v(a, b + 1, 0))],
        },
        ">=" => vec![ge(full(&p))],
        ">" => match (p.major, p.minor, p.patch) {
            (None, _, _) => vec![lt(v(0, 0, 0))],
            (Some(a), None, _) => vec![ge(v(a + 1, 0, 0))],
            (Some(a), Some(b), None) => vec![ge(v(a, b + 1, 0))],
            _ => vec![Comparator {
                op: Op::Gt,
                version: full(&p),
            }],
        },
        "<" => vec![lt(full(&p))],
        "<=" => match (p.major, p.minor, p.patch) {
            (None, _, _) => vec![ge(v(0, 0, 0))],
            (Some(a), None, _) => vec![lt(v(a + 1, 0, 0))],
            (Some(a), Some(b), None) => vec![lt(v(a, b + 1, 0))],
            _ => vec![Comparator {
                op: Op::Le,
                version: full(&p),
            }],
        },
        _ => return None,
    })
}

fn parse_set(text: &str) -> Option<Vec<Comparator>> {
    let text = text.trim();
    if text.is_empty() || text == "*" || text == "latest" {
        return Some(vec![Comparator {
            op: Op::Ge,
            version: v(0, 0, 0),
        }]);
    }
    let raw: Vec<&str> = text.split_whitespace().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let t = raw[i];
        let is_op = matches!(t, ">=" | "<=" | ">" | "<" | "=" | "^" | "~" | "~>");
        if is_op && i + 1 < raw.len() {
            tokens.push(format!("{t}{}", raw[i + 1]));
            i += 2;
        } else {
            tokens.push(t.to_string());
            i += 1;
        }
    }
    let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
    if tokens.len() == 3 && tokens[1] == "-" {
        let lo = parse_partial(tokens[0])?;
        let hi = parse_partial(tokens[2])?;
        let mut out = comparators_for(">=", lo)?;
        out.extend(comparators_for("<=", hi)?);
        return Some(out);
    }
    let mut out = Vec::new();
    for tok in tokens {
        let split = tok
            .find(|c: char| c.is_ascii_digit() || c == 'x' || c == 'X' || c == '*' || c == 'v')
            .unwrap_or(tok.len());
        let (op, rest) = tok.split_at(split);
        let op = op.trim();
        let rest = rest.strip_prefix('v').unwrap_or(rest);
        out.extend(comparators_for(op, parse_partial(rest)?)?);
    }
    Some(out)
}

impl Range {
    pub fn parse(text: &str) -> Option<Range> {
        let mut sets = Vec::new();
        for part in text.split("||") {
            sets.push(parse_set(part)?);
        }
        Some(Range {
            sets,
            text: text.trim().to_string(),
        })
    }

    pub fn any() -> Range {
        Range::parse("*").expect("wildcard range")
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.sets.iter().any(|set| {
            if !set.iter().all(|c| c.matches(version)) {
                return false;
            }
            if version.pre.is_empty() {
                return true;
            }
            set.iter()
                .any(|c| !c.version.pre.is_empty() && c.version.core() == version.core())
        })
    }

    pub fn max_satisfying<'a, I>(&self, versions: I) -> Option<&'a Version>
    where
        I: IntoIterator<Item = &'a Version>,
    {
        versions.into_iter().filter(|ver| self.matches(ver)).max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ver(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn caret_and_tilde() {
        let r = Range::parse("^1.2.3").unwrap();
        assert!(r.matches(&ver("1.9.0")));
        assert!(!r.matches(&ver("2.0.0")));
        assert!(!r.matches(&ver("1.2.2")));
        let r = Range::parse("^0.2.3").unwrap();
        assert!(r.matches(&ver("0.2.9")));
        assert!(!r.matches(&ver("0.3.0")));
        let r = Range::parse("~1.2.3").unwrap();
        assert!(r.matches(&ver("1.2.9")));
        assert!(!r.matches(&ver("1.3.0")));
    }

    #[test]
    fn wildcards_hyphens_and_or() {
        assert!(Range::parse("1.x").unwrap().matches(&ver("1.5.0")));
        assert!(!Range::parse("1.x").unwrap().matches(&ver("2.0.0")));
        assert!(Range::parse("*").unwrap().matches(&ver("7.0.0")));
        let r = Range::parse("1.2.3 - 2.3").unwrap();
        assert!(r.matches(&ver("2.3.9")));
        assert!(!r.matches(&ver("2.4.0")));
        let spaced = Range::parse(">= 2.1.2 < 3.0.0").unwrap();
        assert!(spaced.matches(&ver("2.1.2")));
        assert!(spaced.matches(&ver("2.9.0")));
        assert!(!spaced.matches(&ver("3.0.0")));
        let r = Range::parse(">=1.0.0 <1.5.0 || 3.x").unwrap();
        assert!(r.matches(&ver("1.4.9")));
        assert!(r.matches(&ver("3.1.0")));
        assert!(!r.matches(&ver("2.0.0")));
    }

    #[test]
    fn prereleases_and_ordering() {
        assert!(ver("1.0.0-alpha") < ver("1.0.0"));
        assert!(ver("1.0.0-alpha.1") < ver("1.0.0-alpha.2"));
        assert!(ver("1.0.0-alpha.9") < ver("1.0.0-alpha.10"));
        assert!(!Range::parse("^1.0.0").unwrap().matches(&ver("1.1.0-beta")));
        assert!(
            Range::parse(">=1.1.0-alpha")
                .unwrap()
                .matches(&ver("1.1.0-beta"))
        );
        let versions = [ver("1.0.0"), ver("1.4.0"), ver("2.0.0"), ver("1.5.0-rc.1")];
        let best = Range::parse("^1.0.0")
            .unwrap()
            .max_satisfying(versions.iter());
        assert_eq!(best.unwrap().to_string(), "1.4.0");
    }
}
