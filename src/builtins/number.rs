use super::{arg, make_ctor, method, native_fn, to_integer};
use crate::number::{
    js_pow, number_to_radix_string, number_to_string, parse_float_prefix, parse_int, to_uint32,
};
use crate::value::Value;
use crate::vm::Vm;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let np = vm.realm.number_proto.clone();
    method(vm, &np, "toFixed", 1, to_fixed);
    method(vm, &np, "toString", 1, to_string);
    method(vm, &np, "toPrecision", 1, to_precision);
    method(vm, &np, "valueOf", 0, value_of);
    method(vm, &np, "toLocaleString", 0, to_string);
    let number = make_ctor(vm, "Number", 1, number_ctor, None, &np);
    method(vm, &number, "isInteger", 1, is_integer);
    method(vm, &number, "isSafeInteger", 1, is_safe_integer);
    method(vm, &number, "isFinite", 1, number_is_finite);
    method(vm, &number, "isNaN", 1, number_is_nan);
    method(vm, &number, "parseFloat", 1, parse_float_fn);
    method(vm, &number, "parseInt", 2, parse_int_fn);
    let consts: &[(&str, f64)] = &[
        ("MAX_SAFE_INTEGER", 9007199254740991.0),
        ("MIN_SAFE_INTEGER", -9007199254740991.0),
        ("EPSILON", f64::EPSILON),
        ("MAX_VALUE", f64::MAX),
        ("MIN_VALUE", 5e-324),
        ("POSITIVE_INFINITY", f64::INFINITY),
        ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
        ("NaN", f64::NAN),
    ];
    for (k, v) in consts {
        number.set_own(*k, Value::Number(*v));
    }
    vm.define_global("Number", Value::Object(number));

    let pf = native_fn(vm, "parseFloat", 1, parse_float_fn);
    vm.define_global("parseFloat", pf);
    let pi = native_fn(vm, "parseInt", 2, parse_int_fn);
    vm.define_global("parseInt", pi);
    let isn = native_fn(vm, "isNaN", 1, global_is_nan);
    vm.define_global("isNaN", isn);
    let isf = native_fn(vm, "isFinite", 1, global_is_finite);
    vm.define_global("isFinite", isf);
    vm.define_global("NaN", Value::Number(f64::NAN));
    vm.define_global("Infinity", Value::Number(f64::INFINITY));

    let math = vm.new_object();
    let fns: &[(&str, u32, crate::object::NativeFn)] = &[
        ("abs", 1, m_abs),
        ("floor", 1, m_floor),
        ("ceil", 1, m_ceil),
        ("round", 1, m_round),
        ("trunc", 1, m_trunc),
        ("sign", 1, m_sign),
        ("sqrt", 1, m_sqrt),
        ("cbrt", 1, m_cbrt),
        ("pow", 2, m_pow),
        ("min", 2, m_min),
        ("max", 2, m_max),
        ("random", 0, m_random),
        ("sin", 1, m_sin),
        ("cos", 1, m_cos),
        ("tan", 1, m_tan),
        ("asin", 1, m_asin),
        ("acos", 1, m_acos),
        ("atan", 1, m_atan),
        ("atan2", 2, m_atan2),
        ("sinh", 1, m_sinh),
        ("cosh", 1, m_cosh),
        ("tanh", 1, m_tanh),
        ("log", 1, m_log),
        ("log2", 1, m_log2),
        ("log10", 1, m_log10),
        ("log1p", 1, m_log1p),
        ("exp", 1, m_exp),
        ("expm1", 1, m_expm1),
        ("hypot", 2, m_hypot),
        ("fround", 1, m_fround),
        ("clz32", 1, m_clz32),
    ];
    for (name, arity, f) in fns {
        method(vm, &math, name, *arity, *f);
    }
    let mconsts: &[(&str, f64)] = &[
        ("PI", std::f64::consts::PI),
        ("E", std::f64::consts::E),
        ("LN2", std::f64::consts::LN_2),
        ("LN10", std::f64::consts::LN_10),
        ("LOG2E", std::f64::consts::LOG2_E),
        ("LOG10E", std::f64::consts::LOG10_E),
        ("SQRT2", std::f64::consts::SQRT_2),
        ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
    ];
    for (k, v) in mconsts {
        math.set_own(*k, Value::Number(*v));
    }
    vm.define_global("Math", Value::Object(math));
}

fn this_num(vm: &mut Vm, this: &Value) -> Result<f64, Value> {
    match this {
        Value::Number(n) => Ok(*n),
        _ => Err(vm.type_error("Number.prototype method called on incompatible receiver")),
    }
}

fn number_ctor(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if args.is_empty() {
        return Ok(Value::Number(0.0));
    }
    Ok(Value::Number(vm.to_number(&args[0])?))
}

fn value_of(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(this_num(vm, &this)?))
}

fn to_string(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let n = this_num(vm, &this)?;
    let radix = match arg(args, 0) {
        Value::Undefined => 10,
        v => to_integer(vm, &v)? as i64,
    };
    if !(2..=36).contains(&radix) {
        return Err(vm.range_error("toString() radix must be between 2 and 36"));
    }
    Ok(number_to_radix_string(n, radix as u32).into())
}

pub fn fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return number_to_string(x);
    }
    if x.abs() >= 1e21 {
        return number_to_string(x);
    }
    let x = if x == 0.0 { 0.0 } else { x };
    let scale = 10f64.powi(digits as i32);
    let scaled = x.abs() * scale;
    if scaled < 9007199254740992.0 && scaled.fract() == 0.5 {
        let rounded = (scaled.floor() + 1.0) / scale;
        let s = format!("{:.*}", digits, rounded);
        return if x < 0.0 { format!("-{s}") } else { s };
    }
    let s = format!("{:.*}", digits, x);
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

fn to_fixed(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let n = this_num(vm, &this)?;
    let d = to_integer(vm, &arg(args, 0))?;
    if !(0.0..=100.0).contains(&d) {
        return Err(vm.range_error("toFixed() digits argument must be between 0 and 100"));
    }
    Ok(fixed(n, d as usize).into())
}

fn to_precision(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let n = this_num(vm, &this)?;
    if arg(args, 0).is_undefined() || !n.is_finite() {
        return Ok(number_to_string(n).into());
    }
    let p = to_integer(vm, &arg(args, 0))?;
    if !(1.0..=100.0).contains(&p) {
        return Err(vm.range_error("toPrecision() argument must be between 1 and 100"));
    }
    let p = p as usize;
    if n == 0.0 {
        return Ok(fixed(0.0, p - 1).into());
    }
    let sci = format!("{:.*e}", p - 1, n);
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let e: i32 = exp.parse().unwrap_or(0);
    if e < -6 || e >= p as i32 {
        let sign = if e >= 0 { "+" } else { "-" };
        return Ok(format!("{mantissa}e{sign}{}", e.abs()).into());
    }
    let decimals = (p as i32 - 1 - e).max(0) as usize;
    Ok(format!("{:.*}", decimals, n).into())
}

fn is_integer(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(
        matches!(arg(args, 0), Value::Number(n) if n.is_finite() && n.fract() == 0.0),
    ))
}

fn is_safe_integer(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(
        matches!(arg(args, 0), Value::Number(n) if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0),
    ))
}

fn number_is_finite(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(
        matches!(arg(args, 0), Value::Number(n) if n.is_finite()),
    ))
}

fn number_is_nan(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(
        matches!(arg(args, 0), Value::Number(n) if n.is_nan()),
    ))
}

fn global_is_nan(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(vm.to_number(&arg(args, 0))?.is_nan()))
}

fn global_is_finite(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    Ok(Value::Bool(vm.to_number(&arg(args, 0))?.is_finite()))
}

fn parse_float_fn(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    Ok(Value::Number(parse_float_prefix(&s)))
}

fn parse_int_fn(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    let radix = match arg(args, 1) {
        Value::Undefined => None,
        v => {
            let r = to_uint32(vm.to_number(&v)?);
            if r == 0 { None } else { Some(r) }
        }
    };
    Ok(Value::Number(parse_int(&s, radix)))
}

fn num(vm: &mut Vm, args: &[Value], i: usize) -> Result<f64, Value> {
    vm.to_number(&arg(args, i))
}

macro_rules! unary_math {
    ($($name:ident => $f:expr),* $(,)?) => {
        $(
            fn $name(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
                let x = num(vm, args, 0)?;
                let f: fn(f64) -> f64 = $f;
                Ok(Value::Number(f(x)))
            }
        )*
    };
}

unary_math! {
    m_abs => f64::abs,
    m_floor => f64::floor,
    m_ceil => f64::ceil,
    m_trunc => f64::trunc,
    m_sqrt => f64::sqrt,
    m_cbrt => f64::cbrt,
    m_sin => f64::sin,
    m_cos => f64::cos,
    m_tan => f64::tan,
    m_asin => f64::asin,
    m_acos => f64::acos,
    m_atan => f64::atan,
    m_sinh => f64::sinh,
    m_cosh => f64::cosh,
    m_tanh => f64::tanh,
    m_log => f64::ln,
    m_log2 => f64::log2,
    m_log10 => f64::log10,
    m_log1p => f64::ln_1p,
    m_exp => f64::exp,
    m_expm1 => f64::exp_m1,
    m_fround => |x| x as f32 as f64,
    m_clz32 => |x| to_uint32(x).leading_zeros() as f64,
    m_sign => |x| if x.is_nan() { f64::NAN } else if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { x },
    m_round => |x| {
        if !x.is_finite() || x.fract() == 0.0 {
            x
        } else if (-0.5..0.0).contains(&x) {
            -0.0
        } else {
            (x + 0.5).floor()
        }
    },
}

fn m_pow(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let a = num(vm, args, 0)?;
    let b = num(vm, args, 1)?;
    Ok(Value::Number(js_pow(a, b)))
}

fn m_atan2(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let a = num(vm, args, 0)?;
    let b = num(vm, args, 1)?;
    Ok(Value::Number(a.atan2(b)))
}

fn m_hypot(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut sum = 0.0;
    for a in args {
        let x = vm.to_number(a)?;
        sum += x * x;
    }
    Ok(Value::Number(sum.sqrt()))
}

fn m_min(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut r = f64::INFINITY;
    for a in args {
        let x = vm.to_number(a)?;
        if x.is_nan() {
            r = f64::NAN;
        } else if !r.is_nan() && (x < r || (x == 0.0 && r == 0.0 && x.is_sign_negative())) {
            r = x;
        }
    }
    Ok(Value::Number(r))
}

fn m_max(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let mut r = f64::NEG_INFINITY;
    for a in args {
        let x = vm.to_number(a)?;
        if x.is_nan() {
            r = f64::NAN;
        } else if !r.is_nan() && (x > r || (x == 0.0 && r == 0.0 && r.is_sign_negative())) {
            r = x;
        }
    }
    Ok(Value::Number(r))
}

fn m_random(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(vm.random()))
}
