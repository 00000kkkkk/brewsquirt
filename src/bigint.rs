use num_bigint::{BigInt, Sign};
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BigOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
}

pub enum BigError {
    Range(&'static str),
    Type(&'static str),
}

pub fn parse_literal(text: &str) -> Option<BigInt> {
    let t: String = text.chars().filter(|c| *c != '_').collect();
    let (radix, digits) = if let Some(r) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        (16, r.to_string())
    } else if let Some(r) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        (8, r.to_string())
    } else if let Some(r) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        (2, r.to_string())
    } else {
        (10, t)
    };
    if digits.is_empty() {
        return None;
    }
    BigInt::parse_bytes(digits.as_bytes(), radix)
}

pub fn from_string(s: &str) -> Option<BigInt> {
    let t = s.trim_matches(crate::value::is_js_space);
    if t.is_empty() {
        return Some(BigInt::zero());
    }
    if t.starts_with("0x")
        || t.starts_with("0X")
        || t.starts_with("0o")
        || t.starts_with("0O")
        || t.starts_with("0b")
        || t.starts_with("0B")
    {
        if t.contains('_') {
            return None;
        }
        return parse_literal(t);
    }
    let (neg, digits) = match t.as_bytes()[0] {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let v = BigInt::parse_bytes(digits.as_bytes(), 10)?;
    Some(if neg { -v } else { v })
}

pub fn from_f64(n: f64) -> Option<BigInt> {
    if !n.is_finite() || n.fract() != 0.0 {
        return None;
    }
    num_traits::FromPrimitive::from_f64(n)
}

pub fn to_f64(b: &BigInt) -> f64 {
    b.to_f64().unwrap_or(if b.is_negative() {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    })
}

pub fn to_string_radix(b: &BigInt, radix: u32) -> String {
    b.to_str_radix(radix)
}

fn floor_shr(a: &BigInt, n: u64) -> BigInt {
    if a.is_negative() {
        let divisor = BigInt::one() << n;
        let (q, r) = (a / &divisor, a % &divisor);
        if r.is_zero() { q } else { q - 1 }
    } else {
        a >> n
    }
}

fn shift(a: &BigInt, by: &BigInt, left: bool) -> Result<BigInt, BigError> {
    let left = if by.is_negative() { !left } else { left };
    let amount = by.abs();
    let Some(n) = amount.to_u64() else {
        return if left {
            Err(BigError::Range("Maximum BigInt size exceeded"))
        } else if a.is_negative() {
            Ok(-BigInt::one())
        } else {
            Ok(BigInt::zero())
        };
    };
    if left {
        if n > 1 << 30 {
            return Err(BigError::Range("Maximum BigInt size exceeded"));
        }
        Ok(a << n)
    } else {
        Ok(floor_shr(a, n))
    }
}

pub fn binop(op: BigOp, a: &BigInt, b: &BigInt) -> Result<BigInt, BigError> {
    Ok(match op {
        BigOp::Add => a + b,
        BigOp::Sub => a - b,
        BigOp::Mul => a * b,
        BigOp::Div => {
            if b.is_zero() {
                return Err(BigError::Range("Division by zero"));
            }
            a / b
        }
        BigOp::Mod => {
            if b.is_zero() {
                return Err(BigError::Range("Division by zero"));
            }
            a % b
        }
        BigOp::Pow => {
            if b.is_negative() {
                return Err(BigError::Range("Exponent must be non-negative"));
            }
            let Some(e) = b.to_u32() else {
                if a.is_zero() || a == &BigInt::one() {
                    return Ok(a.clone());
                }
                return Err(BigError::Range("Maximum BigInt size exceeded"));
            };
            if a.bits() as u128 * e as u128 > 1 << 30 {
                return Err(BigError::Range("Maximum BigInt size exceeded"));
            }
            num_traits::pow(a.clone(), e as usize)
        }
        BigOp::BitAnd => a & b,
        BigOp::BitOr => a | b,
        BigOp::BitXor => a ^ b,
        BigOp::Shl => return shift(a, b, true),
        BigOp::Shr => return shift(a, b, false),
        BigOp::UShr => {
            return Err(BigError::Type(
                "BigInts have no unsigned right shift, use >> instead",
            ));
        }
    })
}

pub fn compare_f64(a: &BigInt, n: f64) -> Option<Ordering> {
    if n.is_nan() {
        return None;
    }
    if n == f64::INFINITY {
        return Some(Ordering::Less);
    }
    if n == f64::NEG_INFINITY {
        return Some(Ordering::Greater);
    }
    let floor = n.floor();
    let fb = from_f64(floor)?;
    match a.cmp(&fb) {
        Ordering::Equal if n != floor => Some(Ordering::Less),
        Ordering::Greater if n != floor => Some(Ordering::Greater),
        o => Some(o),
    }
}

pub fn as_uint_n(bits: u64, b: &BigInt) -> BigInt {
    if bits == 0 {
        return BigInt::zero();
    }
    let m = BigInt::one() << bits;
    let r = b % &m;
    if r.is_negative() { r + m } else { r }
}

pub fn as_int_n(bits: u64, b: &BigInt) -> BigInt {
    if bits == 0 {
        return BigInt::zero();
    }
    let u = as_uint_n(bits, b);
    let half = BigInt::one() << (bits - 1);
    if u >= half {
        u - (BigInt::one() << bits)
    } else {
        u
    }
}

pub fn is_zero(b: &BigInt) -> bool {
    b.sign() == Sign::NoSign
}
