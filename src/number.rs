pub fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n == 0.0 {
        return "0".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n < 0.0 {
        return format!("-{}", number_to_string(-n));
    }
    if n.fract() == 0.0 && n < 1e15 {
        return format!("{}", n as i64);
    }
    let sci = format!("{n:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let e: i32 = exp.parse().unwrap_or(0);
    let point = e + 1;
    if k <= point && point <= 21 {
        let mut s = digits;
        for _ in 0..(point - k) {
            s.push('0');
        }
        s
    } else if 0 < point && point <= 21 {
        let (a, b) = digits.split_at(point as usize);
        format!("{a}.{b}")
    } else if -6 < point && point <= 0 {
        let mut s = String::from("0.");
        for _ in 0..(-point) {
            s.push('0');
        }
        s.push_str(&digits);
        s
    } else {
        let e2 = point - 1;
        let sign = if e2 >= 0 { '+' } else { '-' };
        if k == 1 {
            format!("{digits}e{sign}{}", e2.abs())
        } else {
            format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e2.abs())
        }
    }
}

pub fn number_to_radix_string(n: f64, radix: u32) -> String {
    if radix == 10 || !n.is_finite() {
        return number_to_string(n);
    }
    if n == 0.0 {
        return "0".to_string();
    }
    let negative = n < 0.0;
    let mut x = n.abs();
    let mut int_part = x.trunc();
    x -= int_part;
    let mut int_digits = Vec::new();
    if int_part == 0.0 {
        int_digits.push('0');
    }
    while int_part >= 1.0 {
        let d = (int_part % radix as f64) as u32;
        int_digits.push(std::char::from_digit(d, radix).unwrap_or('0'));
        int_part = (int_part / radix as f64).trunc();
    }
    int_digits.reverse();
    let mut s: String = int_digits.into_iter().collect();
    if x > 0.0 {
        s.push('.');
        let mut count = 0;
        while x > 0.0 && count < 52 {
            x *= radix as f64;
            let d = x.trunc() as u32;
            s.push(std::char::from_digit(d, radix).unwrap_or('0'));
            x -= d as f64;
            count += 1;
        }
    }
    if negative { format!("-{s}") } else { s }
}

pub fn string_to_number(s: &str) -> f64 {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if t.is_empty() {
        return 0.0;
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    if sign == 1.0 && t.len() > 2 && t.as_bytes()[0] == b'0' {
        let radix = match t.as_bytes()[1] {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => 0,
        };
        if radix != 0 {
            let mut v = 0f64;
            for c in t[2..].chars() {
                match c.to_digit(radix) {
                    Some(d) => v = v * radix as f64 + d as f64,
                    None => return f64::NAN,
                }
            }
            return v;
        }
    }
    let valid = body
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-');
    if !valid || body.starts_with(['e', 'E']) || body.contains("..") {
        return f64::NAN;
    }
    let normalized = if body.starts_with('.') {
        format!("0{body}")
    } else {
        body.to_string()
    };
    match normalized.parse::<f64>() {
        Ok(v) => sign * v,
        Err(_) => f64::NAN,
    }
}

pub fn parse_float_prefix(s: &str) -> f64 {
    let t = s.trim_start();
    let bytes = t.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    if t[i..].starts_with("Infinity") {
        return if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let start_digits = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == start_digits || (i == start_digits + 1 && bytes[start_digits] == b'.') {
        return f64::NAN;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    let mut text = t[..i].to_string();
    if text.ends_with('.') {
        text.push('0');
    }
    text = text.replace("-.", "-0.").replace("+.", "+0.");
    if text.starts_with('.') {
        text.insert(0, '0');
    }
    text.parse().unwrap_or(f64::NAN)
}

pub fn parse_int(s: &str, radix: Option<u32>) -> f64 {
    let t = s.trim_start();
    let (sign, mut body) = match t.as_bytes().first() {
        Some(b'-') => (-1.0, &t[1..]),
        Some(b'+') => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    let mut radix = radix.unwrap_or(0);
    if (radix == 0 || radix == 16) && (body.starts_with("0x") || body.starts_with("0X")) {
        body = &body[2..];
        radix = 16;
    }
    if radix == 0 {
        radix = 10;
    }
    if !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    let mut v = 0f64;
    let mut any = false;
    for c in body.chars() {
        match c.to_digit(radix) {
            Some(d) => {
                v = v * radix as f64 + d as f64;
                any = true;
            }
            None => break,
        }
    }
    if any { sign * v } else { f64::NAN }
}

pub fn to_int32(n: f64) -> i32 {
    if n.is_finite() && n.abs() < 2147483648.0 {
        return n as i32;
    }
    to_uint32(n) as i32
}

pub fn to_uint32(n: f64) -> u32 {
    if !n.is_finite() {
        return 0;
    }
    let t = n.trunc();
    let m = t.rem_euclid(4294967296.0);
    m as u32
}

pub fn js_pow(base: f64, exp: f64) -> f64 {
    if exp.is_nan() {
        return f64::NAN;
    }
    if exp.is_infinite() && base.abs() == 1.0 {
        return f64::NAN;
    }
    base.powf(exp)
}
