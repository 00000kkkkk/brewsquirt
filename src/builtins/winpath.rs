fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

fn is_device_root(c: char) -> bool {
    c.is_ascii_alphabetic()
}

fn slice(chars: &[char], a: usize, b: usize) -> String {
    chars[a.min(chars.len())..b.min(chars.len())]
        .iter()
        .collect()
}

fn normalize_string(path: &[char], allow_above_root: bool) -> String {
    normalize_string_with(path, allow_above_root, true)
}

fn normalize_string_with(path: &[char], allow_above_root: bool, win: bool) -> String {
    let sep = if win { '\\' } else { '/' };
    let is_sep = |c: char| c == '/' || (win && c == '\\');
    let mut res = String::new();
    let mut last_segment_length = 0usize;
    let mut last_slash: isize = -1;
    let mut dots: i32 = 0;
    let mut code = '\0';
    let len = path.len();
    for i in 0..=len {
        if i < len {
            code = path[i];
        } else if is_sep(code) {
            break;
        } else {
            code = '/';
        }
        if is_sep(code) {
            if last_slash == i as isize - 1 || dots == 1 {
            } else if dots == 2 {
                let bytes = res.as_bytes();
                let ends_with_dots = res.len() >= 2
                    && last_segment_length == 2
                    && bytes[bytes.len() - 1] == b'.'
                    && bytes[bytes.len() - 2] == b'.';
                if !ends_with_dots {
                    if res.len() > 2 {
                        match res.rfind(sep) {
                            None => {
                                res.clear();
                                last_segment_length = 0;
                            }
                            Some(idx) => {
                                res.truncate(idx);
                                last_segment_length = match res.rfind(sep) {
                                    Some(p) => res.len() - 1 - p,
                                    None => res.len(),
                                };
                            }
                        }
                        last_slash = i as isize;
                        dots = 0;
                        continue;
                    } else if !res.is_empty() {
                        res.clear();
                        last_segment_length = 0;
                        last_slash = i as isize;
                        dots = 0;
                        continue;
                    }
                }
                if allow_above_root {
                    if !res.is_empty() {
                        res.push(sep);
                    }
                    res.push_str("..");
                    last_segment_length = 2;
                }
            } else {
                let segment = slice(path, (last_slash + 1) as usize, i);
                if !res.is_empty() {
                    res.push(sep);
                }
                res.push_str(&segment);
                last_segment_length = (i as isize - last_slash - 1) as usize;
            }
            last_slash = i as isize;
            dots = 0;
        } else if code == '.' && dots != -1 {
            dots += 1;
        } else {
            dots = -1;
        }
    }
    res
}

fn unc_root(path: &[char]) -> Option<(usize, usize, usize, bool)> {
    let len = path.len();
    if len < 2 || !is_sep(path[0]) || !is_sep(path[1]) {
        return None;
    }
    let mut j = 2;
    let last = j;
    while j < len && !is_sep(path[j]) {
        j += 1;
    }
    if !(j < len && j != last) {
        return None;
    }
    let first_end = j;
    let mut last = j;
    while j < len && is_sep(path[j]) {
        j += 1;
    }
    if !(j < len && j != last) {
        return None;
    }
    last = j;
    while j < len && !is_sep(path[j]) {
        j += 1;
    }
    if j == len {
        return Some((first_end, last, j, true));
    }
    if j != last {
        return Some((first_end, last, j, false));
    }
    None
}

pub fn resolve(args: &[String], cwd: &str, drive_cwd: impl Fn(&str) -> Option<String>) -> String {
    let mut resolved_device = String::new();
    let mut resolved_tail = String::new();
    let mut resolved_absolute = false;
    let mut i = args.len() as isize - 1;
    while i >= -1 {
        let path: String = if i >= 0 {
            let p = args[i as usize].clone();
            if p.is_empty() {
                i -= 1;
                continue;
            }
            p
        } else if resolved_device.is_empty() {
            cwd.to_string()
        } else {
            let candidate = drive_cwd(&resolved_device).unwrap_or_else(|| cwd.to_string());
            let cand: Vec<char> = candidate.chars().collect();
            let same_drive = candidate.len() >= 2
                && candidate[..2].to_lowercase() == resolved_device.to_lowercase();
            if !same_drive && cand.get(2) == Some(&'\\') {
                format!("{resolved_device}\\")
            } else {
                candidate
            }
        };
        let chars: Vec<char> = path.chars().collect();
        let len = chars.len();
        let mut root_end = 0;
        let mut device = String::new();
        let mut is_absolute = false;
        let code = chars[0];
        if len == 1 {
            if is_sep(code) {
                root_end = 1;
                is_absolute = true;
            }
        } else if is_sep(code) {
            is_absolute = true;
            if is_sep(chars[1]) {
                if let Some((first_end, last, j, _)) = unc_root(&chars) {
                    device = format!(
                        "\\\\{}\\{}",
                        slice(&chars, 2, first_end),
                        slice(&chars, last, j)
                    );
                    root_end = j;
                }
            } else {
                root_end = 1;
            }
        } else if is_device_root(code) && chars[1] == ':' {
            device = slice(&chars, 0, 2);
            root_end = 2;
            if len > 2 && is_sep(chars[2]) {
                is_absolute = true;
                root_end = 3;
            }
        }
        if !device.is_empty() {
            if !resolved_device.is_empty() {
                if device.to_lowercase() != resolved_device.to_lowercase() {
                    i -= 1;
                    continue;
                }
            } else {
                resolved_device = device;
            }
        }
        if resolved_absolute {
            if !resolved_device.is_empty() {
                break;
            }
        } else {
            resolved_tail = format!("{}\\{}", slice(&chars, root_end, len), resolved_tail);
            resolved_absolute = is_absolute;
            if is_absolute && !resolved_device.is_empty() {
                break;
            }
        }
        i -= 1;
    }
    let tail: Vec<char> = resolved_tail.chars().collect();
    let resolved_tail = normalize_string(&tail, !resolved_absolute);
    if resolved_absolute {
        format!("{resolved_device}\\{resolved_tail}")
    } else {
        let out = format!("{resolved_device}{resolved_tail}");
        if out.is_empty() { ".".to_string() } else { out }
    }
}

pub fn normalize(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    let len = chars.len();
    if len == 0 {
        return ".".to_string();
    }
    let mut root_end = 0;
    let mut device: Option<String> = None;
    let mut is_absolute = false;
    let code = chars[0];
    if len == 1 {
        return if code == '/' {
            "\\".to_string()
        } else {
            path.to_string()
        };
    }
    if is_sep(code) {
        is_absolute = true;
        if is_sep(chars[1]) {
            if let Some((first_end, last, j, only)) = unc_root(&chars) {
                if only {
                    return format!(
                        "\\\\{}\\{}\\",
                        slice(&chars, 2, first_end),
                        slice(&chars, last, len)
                    );
                }
                device = Some(format!(
                    "\\\\{}\\{}",
                    slice(&chars, 2, first_end),
                    slice(&chars, last, j)
                ));
                root_end = j;
            }
        } else {
            root_end = 1;
        }
    } else if is_device_root(code) && chars[1] == ':' {
        device = Some(slice(&chars, 0, 2));
        root_end = 2;
        if len > 2 && is_sep(chars[2]) {
            is_absolute = true;
            root_end = 3;
        }
    }
    let mut tail = if root_end < len {
        normalize_string(&chars[root_end..], !is_absolute)
    } else {
        String::new()
    };
    if tail.is_empty() && !is_absolute {
        tail = ".".to_string();
    }
    if !tail.is_empty() && is_sep(chars[len - 1]) {
        tail.push('\\');
    }
    match device {
        None => {
            if is_absolute {
                format!("\\{tail}")
            } else {
                tail
            }
        }
        Some(d) => {
            if is_absolute {
                format!("{d}\\{tail}")
            } else {
                format!("{d}{tail}")
            }
        }
    }
}

pub fn is_absolute(path: &str) -> bool {
    let chars: Vec<char> = path.chars().collect();
    match chars.first() {
        None => false,
        Some(&c) => {
            is_sep(c)
                || (chars.len() > 2 && is_device_root(c) && chars[1] == ':' && is_sep(chars[2]))
        }
    }
}

pub fn join(args: &[String]) -> String {
    let parts: Vec<&String> = args.iter().filter(|a| !a.is_empty()).collect();
    if parts.is_empty() {
        return ".".to_string();
    }
    let first: Vec<char> = parts[0].chars().collect();
    let mut joined = parts
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\\");
    let mut needs_replace = true;
    let mut slash_count = 0;
    if is_sep(first[0]) {
        slash_count += 1;
        if first.len() > 1 && is_sep(first[1]) {
            slash_count += 1;
            if first.len() > 2 {
                if is_sep(first[2]) {
                    slash_count += 1;
                } else {
                    needs_replace = false;
                }
            }
        }
    }
    if needs_replace {
        let jc: Vec<char> = joined.chars().collect();
        while slash_count < jc.len() && is_sep(jc[slash_count]) {
            slash_count += 1;
        }
        if slash_count >= 2 {
            joined = format!("\\{}", slice(&jc, slash_count, jc.len()));
        }
    }
    normalize(&joined)
}

pub fn relative(from: &str, to: &str, resolve_one: impl Fn(&str) -> String) -> String {
    if from == to {
        return String::new();
    }
    let from_orig = resolve_one(from);
    let to_orig = resolve_one(to);
    if from_orig == to_orig {
        return String::new();
    }
    let from: Vec<char> = from_orig.to_lowercase().chars().collect();
    let to: Vec<char> = to_orig.to_lowercase().chars().collect();
    let to_orig_chars: Vec<char> = to_orig.chars().collect();
    if from == to {
        return String::new();
    }
    let mut from_start = 0;
    while from_start < from.len() && from[from_start] == '\\' {
        from_start += 1;
    }
    let mut from_end = from.len();
    while from_end > from_start + 1 && from[from_end - 1] == '\\' {
        from_end -= 1;
    }
    let from_len = from_end - from_start;
    let mut to_start = 0;
    while to_start < to.len() && to[to_start] == '\\' {
        to_start += 1;
    }
    let mut to_end = to.len();
    while to_end > to_start + 1 && to[to_end - 1] == '\\' {
        to_end -= 1;
    }
    let to_len = to_end - to_start;
    let length = from_len.min(to_len);
    let mut last_common_sep: isize = -1;
    let mut i = 0;
    while i < length {
        let fc = from[from_start + i];
        if fc != to[to_start + i] {
            break;
        } else if fc == '\\' {
            last_common_sep = i as isize;
        }
        i += 1;
    }
    if i != length {
        if last_common_sep == -1 {
            return to_orig;
        }
    } else {
        if to_len > length {
            if to[to_start + i] == '\\' {
                return slice(&to_orig_chars, to_start + i + 1, to_orig_chars.len());
            }
            if i == 2 {
                return slice(&to_orig_chars, to_start + i, to_orig_chars.len());
            }
        }
        if from_len > length {
            if from[from_start + i] == '\\' {
                last_common_sep = i as isize;
            } else if i == 2 {
                last_common_sep = 3;
            }
        }
        if last_common_sep == -1 {
            last_common_sep = 0;
        }
    }
    let mut out = String::new();
    let mut k = (from_start as isize + last_common_sep + 1) as usize;
    while k <= from_end {
        if k == from_end || from[k] == '\\' {
            out.push_str(if out.is_empty() { ".." } else { "\\.." });
        }
        k += 1;
    }
    let mut to_start = (to_start as isize + last_common_sep) as usize;
    if !out.is_empty() {
        return format!("{out}{}", slice(&to_orig_chars, to_start, to_end));
    }
    if to_orig_chars.get(to_start) == Some(&'\\') {
        to_start += 1;
    }
    slice(&to_orig_chars, to_start, to_end)
}

pub fn dirname(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    let len = chars.len();
    if len == 0 {
        return ".".to_string();
    }
    let mut root_end: isize = -1;
    let mut offset = 0;
    let code = chars[0];
    if len == 1 {
        return if is_sep(code) {
            path.to_string()
        } else {
            ".".to_string()
        };
    }
    if is_sep(code) {
        root_end = 1;
        offset = 1;
        if is_sep(chars[1])
            && let Some((_, _, j, only)) = unc_root(&chars)
        {
            if only {
                return path.to_string();
            }
            root_end = j as isize + 1;
            offset = j + 1;
        }
    } else if is_device_root(code) && chars[1] == ':' {
        root_end = if len > 2 && is_sep(chars[2]) { 3 } else { 2 };
        offset = root_end as usize;
    }
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut i = len as isize - 1;
    while i >= offset as isize {
        if is_sep(chars[i as usize]) {
            if !matched_slash {
                end = i;
                break;
            }
        } else {
            matched_slash = false;
        }
        i -= 1;
    }
    if end == -1 {
        if root_end == -1 {
            return ".".to_string();
        }
        end = root_end;
    }
    slice(&chars, 0, end as usize)
}

pub fn basename(path: &str, suffix: Option<&str>) -> String {
    let chars: Vec<char> = path.chars().collect();
    let mut start = 0usize;
    let mut end: isize = -1;
    let mut matched_slash = true;
    if chars.len() >= 2 && is_device_root(chars[0]) && chars[1] == ':' {
        start = 2;
    }
    if let Some(suffix) = suffix.filter(|s| !s.is_empty() && s.chars().count() <= chars.len()) {
        if suffix == path {
            return String::new();
        }
        let sfx: Vec<char> = suffix.chars().collect();
        let mut ext_idx = sfx.len() as isize - 1;
        let mut first_non_slash_end: isize = -1;
        let mut i = chars.len() as isize - 1;
        while i >= start as isize {
            let code = chars[i as usize];
            if is_sep(code) {
                if !matched_slash {
                    start = i as usize + 1;
                    break;
                }
            } else {
                if first_non_slash_end == -1 {
                    matched_slash = false;
                    first_non_slash_end = i + 1;
                }
                if ext_idx >= 0 {
                    if code == sfx[ext_idx as usize] {
                        ext_idx -= 1;
                        if ext_idx == -1 {
                            end = i;
                        }
                    } else {
                        ext_idx = -1;
                        end = first_non_slash_end;
                    }
                }
            }
            i -= 1;
        }
        if start as isize == end {
            end = first_non_slash_end;
        } else if end == -1 {
            end = chars.len() as isize;
        }
        return slice(&chars, start, end.max(start as isize) as usize);
    }
    let mut i = chars.len() as isize - 1;
    while i >= start as isize {
        if is_sep(chars[i as usize]) {
            if !matched_slash {
                start = i as usize + 1;
                break;
            }
        } else if end == -1 {
            matched_slash = false;
            end = i + 1;
        }
        i -= 1;
    }
    if end == -1 {
        return String::new();
    }
    slice(&chars, start, end as usize)
}

fn ext_scan(chars: &[char], start: usize, part0: usize) -> (isize, usize, isize, i32) {
    let mut start_dot: isize = -1;
    let mut start_part = part0;
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut pre_dot_state = 0;
    let mut i = chars.len() as isize - 1;
    while i >= start as isize {
        let code = chars[i as usize];
        if is_sep(code) {
            if !matched_slash {
                start_part = i as usize + 1;
                break;
            }
            i -= 1;
            continue;
        }
        if end == -1 {
            matched_slash = false;
            end = i + 1;
        }
        if code == '.' {
            if start_dot == -1 {
                start_dot = i;
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot != -1 {
            pre_dot_state = -1;
        }
        i -= 1;
    }
    (start_dot, start_part, end, pre_dot_state)
}

fn no_ext(start_dot: isize, start_part: usize, end: isize, pre_dot_state: i32) -> bool {
    start_dot == -1
        || end == -1
        || pre_dot_state == 0
        || (pre_dot_state == 1 && start_dot == end - 1 && start_dot == start_part as isize + 1)
}

pub fn extname(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    let start = if chars.len() >= 2 && chars[1] == ':' && is_device_root(chars[0]) {
        2
    } else {
        0
    };
    let (start_dot, start_part, end, pre) = ext_scan(&chars, start, start);
    if no_ext(start_dot, start_part, end, pre) {
        return String::new();
    }
    slice(&chars, start_dot as usize, end as usize)
}

pub struct Parsed {
    pub root: String,
    pub dir: String,
    pub base: String,
    pub ext: String,
    pub name: String,
}

pub fn parse(path: &str) -> Parsed {
    let mut ret = Parsed {
        root: String::new(),
        dir: String::new(),
        base: String::new(),
        ext: String::new(),
        name: String::new(),
    };
    let chars: Vec<char> = path.chars().collect();
    let len = chars.len();
    if len == 0 {
        return ret;
    }
    let mut root_end = 0;
    let code = chars[0];
    if len == 1 {
        if is_sep(code) {
            ret.root = path.to_string();
            ret.dir = path.to_string();
        } else {
            ret.base = path.to_string();
            ret.name = path.to_string();
        }
        return ret;
    }
    if is_sep(code) {
        root_end = 1;
        if is_sep(chars[1])
            && let Some((_, _, j, only)) = unc_root(&chars)
        {
            root_end = if only { j } else { j + 1 };
        }
    } else if is_device_root(code) && chars[1] == ':' {
        if len <= 2 {
            ret.root = path.to_string();
            ret.dir = path.to_string();
            return ret;
        }
        root_end = 2;
        if is_sep(chars[2]) {
            if len == 3 {
                ret.root = path.to_string();
                ret.dir = path.to_string();
                return ret;
            }
            root_end = 3;
        }
    }
    if root_end > 0 {
        ret.root = slice(&chars, 0, root_end);
    }
    let (start_dot, start_part, end, pre) = ext_scan(&chars, root_end, root_end);
    if end != -1 {
        if no_ext(start_dot, start_part, end, pre) {
            ret.base = slice(&chars, start_part, end as usize);
            ret.name = ret.base.clone();
        } else {
            ret.name = slice(&chars, start_part, start_dot as usize);
            ret.base = slice(&chars, start_part, end as usize);
            ret.ext = slice(&chars, start_dot as usize, end as usize);
        }
    }
    if start_part > 0 && start_part != root_end {
        ret.dir = slice(&chars, 0, start_part - 1);
    } else {
        ret.dir = ret.root.clone();
    }
    ret
}

pub fn posix_resolve(args: &[String], cwd: &str) -> String {
    let mut resolved = String::new();
    let mut absolute = false;
    let mut i = args.len() as isize - 1;
    while i >= -1 && !absolute {
        let path = if i >= 0 {
            args[i as usize].clone()
        } else {
            cwd.to_string()
        };
        i -= 1;
        if path.is_empty() {
            continue;
        }
        resolved = format!("{path}/{resolved}");
        absolute = path.starts_with('/');
    }
    let chars: Vec<char> = resolved.chars().collect();
    let resolved = normalize_string_with(&chars, !absolute, false);
    if absolute {
        format!("/{resolved}")
    } else if resolved.is_empty() {
        ".".to_string()
    } else {
        resolved
    }
}

pub fn posix_normalize(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let absolute = path.starts_with('/');
    let trailing = path.ends_with('/');
    let chars: Vec<char> = path.chars().collect();
    let mut out = normalize_string_with(&chars, !absolute, false);
    if out.is_empty() {
        if absolute {
            return "/".to_string();
        }
        return if trailing { "./" } else { "." }.to_string();
    }
    if trailing {
        out.push('/');
    }
    if absolute { format!("/{out}") } else { out }
}

pub fn posix_join(args: &[String]) -> String {
    let parts: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| !a.is_empty())
        .collect();
    if parts.is_empty() {
        return ".".to_string();
    }
    posix_normalize(&parts.join("/"))
}

pub fn posix_relative(from: &str, to: &str, cwd: &str) -> String {
    if from == to {
        return String::new();
    }
    let from = posix_resolve(&[from.to_string()], cwd);
    let to = posix_resolve(&[to.to_string()], cwd);
    if from == to {
        return String::new();
    }
    let f: Vec<char> = from.chars().collect();
    let t: Vec<char> = to.chars().collect();
    let from_start = 1;
    let from_end = f.len();
    let from_len = from_end - from_start;
    let to_start = 1;
    let to_len = t.len() - to_start;
    let length = from_len.min(to_len);
    let mut last_common_sep: isize = -1;
    let mut i = 0;
    while i < length {
        let c = f[from_start + i];
        if c != t[to_start + i] {
            break;
        } else if c == '/' {
            last_common_sep = i as isize;
        }
        i += 1;
    }
    if i == length {
        if to_len > length {
            if t[to_start + i] == '/' {
                return slice(&t, to_start + i + 1, t.len());
            }
            if i == 0 {
                return slice(&t, to_start + i, t.len());
            }
        } else if from_len > length {
            if f[from_start + i] == '/' {
                last_common_sep = i as isize;
            } else if i == 0 {
                last_common_sep = 0;
            }
        }
    }
    let mut out = String::new();
    let mut k = (from_start as isize + last_common_sep + 1) as usize;
    while k <= from_end {
        if k == from_end || f[k] == '/' {
            out.push_str(if out.is_empty() { ".." } else { "/.." });
        }
        k += 1;
    }
    format!(
        "{out}{}",
        slice(&t, (to_start as isize + last_common_sep) as usize, t.len())
    )
}

pub fn posix_dirname(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.is_empty() {
        return ".".to_string();
    }
    let has_root = chars[0] == '/';
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut i = chars.len() as isize - 1;
    while i >= 1 {
        if chars[i as usize] == '/' {
            if !matched_slash {
                end = i;
                break;
            }
        } else {
            matched_slash = false;
        }
        i -= 1;
    }
    if end == -1 {
        return if has_root { "/" } else { "." }.to_string();
    }
    if has_root && end == 1 {
        return "//".to_string();
    }
    slice(&chars, 0, end as usize)
}

pub fn posix_basename(path: &str, suffix: Option<&str>) -> String {
    let swapped: String = path
        .chars()
        .map(|c| if c == '\\' { '\u{e000}' } else { c })
        .collect();
    let suffix_swapped = suffix.map(|s| {
        s.chars()
            .map(|c| if c == '\\' { '\u{e000}' } else { c })
            .collect::<String>()
    });
    let out = if swapped.len() >= 2 && swapped.as_bytes()[1] == b':' {
        let guarded = format!("\u{e001}{swapped}");
        let r = basename(&guarded, suffix_swapped.as_deref());
        r.trim_start_matches('\u{e001}').to_string()
    } else {
        basename(&swapped, suffix_swapped.as_deref())
    };
    out.chars()
        .map(|c| if c == '\u{e000}' { '\\' } else { c })
        .collect()
}

pub fn posix_extname(path: &str) -> String {
    let chars: Vec<char> = path
        .chars()
        .map(|c| if c == '\\' { '\u{e000}' } else { c })
        .collect();
    let (start_dot, start_part, end, pre) = ext_scan(&chars, 0, 0);
    if no_ext(start_dot, start_part, end, pre) {
        return String::new();
    }
    slice(&chars, start_dot as usize, end as usize).replace('\u{e000}', "\\")
}

pub fn posix_parse(path: &str) -> Parsed {
    let mut ret = Parsed {
        root: String::new(),
        dir: String::new(),
        base: String::new(),
        ext: String::new(),
        name: String::new(),
    };
    if path.is_empty() {
        return ret;
    }
    let chars: Vec<char> = path
        .chars()
        .map(|c| if c == '\\' { '\u{e000}' } else { c })
        .collect();
    let back = |s: String| s.replace('\u{e000}', "\\");
    let absolute = chars[0] == '/';
    let start = if absolute {
        ret.root = "/".to_string();
        1
    } else {
        0
    };
    let (start_dot, start_part, end, pre) = ext_scan(&chars, start, 0);
    if end != -1 {
        let start_part_eff = if start_part == 0 && absolute {
            1
        } else {
            start_part
        };
        if no_ext(start_dot, start_part, end, pre) {
            ret.base = back(slice(&chars, start_part_eff, end as usize));
            ret.name = ret.base.clone();
        } else {
            ret.name = back(slice(&chars, start_part_eff, start_dot as usize));
            ret.base = back(slice(&chars, start_part_eff, end as usize));
            ret.ext = back(slice(&chars, start_dot as usize, end as usize));
        }
    }
    if start_part > 0 {
        ret.dir = back(slice(&chars, 0, start_part - 1));
    } else if absolute {
        ret.dir = "/".to_string();
    }
    ret
}
