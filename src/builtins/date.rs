use super::{arg, make_ctor, method};
use crate::object::{Obj, ObjKind};
use crate::value::Value;
use crate::vm::Vm;
use chrono::{Local, Offset, TimeZone};
use std::cell::Cell;
use std::rc::Rc;

type R = Result<Value, Value>;

const MS_PER_DAY: f64 = 86_400_000.0;
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn install(vm: &mut Vm) {
    let dp = vm.realm.date_proto.clone();
    let methods: &[(&str, u32, crate::object::NativeFn)] = &[
        ("getTime", 0, get_time),
        ("valueOf", 0, get_time),
        ("getFullYear", 0, get_full_year),
        ("getMonth", 0, get_month),
        ("getDate", 0, get_date),
        ("getDay", 0, get_day),
        ("getHours", 0, get_hours),
        ("getMinutes", 0, get_minutes),
        ("getSeconds", 0, get_seconds),
        ("getMilliseconds", 0, get_milliseconds),
        ("getUTCFullYear", 0, get_utc_full_year),
        ("getUTCMonth", 0, get_utc_month),
        ("getUTCDate", 0, get_utc_date),
        ("getUTCDay", 0, get_utc_day),
        ("getUTCHours", 0, get_utc_hours),
        ("getUTCMinutes", 0, get_utc_minutes),
        ("getUTCSeconds", 0, get_utc_seconds),
        ("getUTCMilliseconds", 0, get_utc_milliseconds),
        ("getTimezoneOffset", 0, get_timezone_offset),
        ("setTime", 1, set_time),
        ("setMilliseconds", 1, set_milliseconds),
        ("setSeconds", 2, set_seconds),
        ("setMinutes", 3, set_minutes),
        ("setHours", 4, set_hours),
        ("setDate", 1, set_date),
        ("setMonth", 2, set_month),
        ("setFullYear", 3, set_full_year),
        ("setUTCMilliseconds", 1, set_utc_milliseconds),
        ("setUTCSeconds", 2, set_utc_seconds),
        ("setUTCMinutes", 3, set_utc_minutes),
        ("setUTCHours", 4, set_utc_hours),
        ("setUTCDate", 1, set_utc_date),
        ("setUTCMonth", 2, set_utc_month),
        ("setUTCFullYear", 3, set_utc_full_year),
        ("toISOString", 0, to_iso_string),
        ("toJSON", 1, to_json),
        ("toString", 0, to_string),
        ("toDateString", 0, to_date_string),
        ("toTimeString", 0, to_time_string),
        ("toUTCString", 0, to_utc_string),
        ("toGMTString", 0, to_utc_string),
        ("toLocaleString", 0, to_locale_string),
        ("toLocaleDateString", 0, to_locale_date_string),
        ("toLocaleTimeString", 0, to_locale_time_string),
    ];
    for (name, arity, f) in methods {
        method(vm, &dp, name, *arity, *f);
    }
    let date = make_ctor(
        vm,
        "Date",
        7,
        date_ctor,
        Some(|| ObjKind::Date(Cell::new(f64::NAN))),
        &dp,
    );
    method(vm, &date, "now", 0, date_now);
    method(vm, &date, "parse", 1, date_parse);
    method(vm, &date, "UTC", 7, date_utc);
    vm.define_global("Date", Value::Object(date));
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !year.is_finite() || !month.is_finite() || !date.is_finite() {
        return f64::NAN;
    }
    let y = year.trunc() + (month.trunc() / 12.0).floor();
    let m = month.trunc().rem_euclid(12.0);
    if y.abs() > 400_000.0 {
        return f64::NAN;
    }
    days_from_civil(y as i64, m as i64 + 1, 1) as f64 + date.trunc() - 1.0
}

fn make_time(h: f64, m: f64, s: f64, ms: f64) -> f64 {
    if !h.is_finite() || !m.is_finite() || !s.is_finite() || !ms.is_finite() {
        return f64::NAN;
    }
    h.trunc() * 3_600_000.0 + m.trunc() * 60_000.0 + s.trunc() * 1000.0 + ms.trunc()
}

fn make_date(day: f64, time: f64) -> f64 {
    if !day.is_finite() || !time.is_finite() {
        return f64::NAN;
    }
    day * MS_PER_DAY + time
}

fn time_clip(t: f64) -> f64 {
    if !t.is_finite() || t.abs() > 8.64e15 {
        f64::NAN
    } else {
        t.trunc() + 0.0
    }
}

pub fn local_offset_ms(t: f64) -> f64 {
    if !t.is_finite() {
        return 0.0;
    }
    match Local.timestamp_millis_opt(t as i64) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.offset().fix().local_minus_utc() as f64 * 1000.0
        }
        chrono::LocalResult::None => 0.0,
    }
}

fn local_time(t: f64) -> f64 {
    t + local_offset_ms(t)
}

fn utc_from_local(t: f64) -> f64 {
    if !t.is_finite() {
        return f64::NAN;
    }
    let guess = t - local_offset_ms(t);
    t - local_offset_ms(guess)
}

#[derive(Clone, Copy)]
struct Parts {
    year: f64,
    month: f64,
    date: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    ms: f64,
    weekday: f64,
}

fn split(t: f64) -> Parts {
    let day = (t / MS_PER_DAY).floor();
    let within = t - day * MS_PER_DAY;
    let (y, m, d) = civil_from_days(day as i64);
    Parts {
        year: y as f64,
        month: (m - 1) as f64,
        date: d as f64,
        hours: (within / 3_600_000.0).floor(),
        minutes: ((within / 60_000.0).floor()).rem_euclid(60.0),
        seconds: ((within / 1000.0).floor()).rem_euclid(60.0),
        ms: within.rem_euclid(1000.0),
        weekday: (day + 4.0).rem_euclid(7.0),
    }
}

fn join(p: &Parts) -> f64 {
    make_date(
        make_day(p.year, p.month, p.date),
        make_time(p.hours, p.minutes, p.seconds, p.ms),
    )
}

fn this_time(vm: &mut Vm, this: &Value) -> Result<(Rc<Obj>, f64), Value> {
    if let Value::Object(o) = this {
        if let ObjKind::Date(t) = &o.kind {
            return Ok((o.clone(), t.get()));
        }
    }
    Err(vm.type_error("this is not a Date object."))
}

fn set_this(o: &Obj, t: f64) -> f64 {
    let t = time_clip(t);
    if let ObjKind::Date(cell) = &o.kind {
        cell.set(t);
    }
    t
}

fn pad(n: f64, width: usize) -> String {
    format!("{:0width$}", n as i64, width = width)
}

pub fn iso_string(t: f64) -> Option<String> {
    if t.is_nan() {
        return None;
    }
    let p = split(t);
    let year = if (0.0..=9999.0).contains(&p.year) {
        pad(p.year, 4)
    } else if p.year < 0.0 {
        format!("-{}", pad(-p.year, 6))
    } else {
        format!("+{}", pad(p.year, 6))
    };
    Some(format!(
        "{year}-{}-{}T{}:{}:{}.{}Z",
        pad(p.month + 1.0, 2),
        pad(p.date, 2),
        pad(p.hours, 2),
        pad(p.minutes, 2),
        pad(p.seconds, 2),
        pad(p.ms, 3)
    ))
}

fn offset_text(t: f64) -> String {
    let off = local_offset_ms(t) / 60_000.0;
    let sign = if off < 0.0 { '-' } else { '+' };
    let off = off.abs();
    format!(
        "GMT{sign}{}{}",
        pad((off / 60.0).floor(), 2),
        pad(off % 60.0, 2)
    )
}

fn year_text(y: f64) -> String {
    if y < 0.0 {
        format!("-{}", pad(-y, 6))
    } else {
        pad(y, 4)
    }
}

fn date_part(p: &Parts) -> String {
    format!(
        "{} {} {} {}",
        DAYS[p.weekday as usize],
        MONTHS[p.month as usize],
        pad(p.date, 2),
        year_text(p.year)
    )
}

fn time_part(p: &Parts, t: f64) -> String {
    format!(
        "{}:{}:{} {}",
        pad(p.hours, 2),
        pad(p.minutes, 2),
        pad(p.seconds, 2),
        offset_text(t)
    )
}

pub fn date_to_string(t: f64) -> String {
    if t.is_nan() {
        return "Invalid Date".to_string();
    }
    let p = split(local_time(t));
    format!("{} {}", date_part(&p), time_part(&p, t))
}

fn locale_date(p: &Parts) -> String {
    format!("{}/{}/{}", p.month + 1.0, p.date, p.year)
}

fn locale_time(p: &Parts) -> String {
    let h = p.hours % 12.0;
    let h = if h == 0.0 { 12.0 } else { h };
    let ampm = if p.hours < 12.0 { "AM" } else { "PM" };
    format!("{}:{}:{} {ampm}", h, pad(p.minutes, 2), pad(p.seconds, 2))
}

fn number_args(vm: &mut Vm, args: &[Value]) -> Result<Vec<f64>, Value> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        out.push(vm.to_number(a)?);
    }
    Ok(out)
}

fn from_components(n: &[f64]) -> f64 {
    let year = n[0];
    let year = if year.is_finite() && (0.0..=99.0).contains(&year.trunc()) {
        1900.0 + year.trunc()
    } else {
        year
    };
    let get = |i: usize, d: f64| n.get(i).copied().unwrap_or(d);
    make_date(
        make_day(year, get(1, 0.0), get(2, 1.0)),
        make_time(get(3, 0.0), get(4, 0.0), get(5, 0.0), get(6, 0.0)),
    )
}

fn date_ctor(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let constructing = matches!(&this, Value::Object(o) if matches!(o.kind, ObjKind::Date(_)));
    if !constructing {
        return Ok(date_to_string(now_ms()).into());
    }
    let t = match args.len() {
        0 => now_ms(),
        1 => match &args[0] {
            Value::Object(o) if matches!(o.kind, ObjKind::Date(_)) => match &o.kind {
                ObjKind::Date(c) => c.get(),
                _ => f64::NAN,
            },
            v => {
                let p = vm.to_primitive(v, false)?;
                match p {
                    Value::Str(s) => parse_date(&s),
                    other => vm.to_number(&other)?,
                }
            }
        },
        _ => {
            let n = number_args(vm, args)?;
            utc_from_local(from_components(&n))
        }
    };
    if let Value::Object(o) = &this {
        set_this(o, t);
    }
    Ok(this)
}

fn date_now(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(now_ms()))
}

fn date_parse(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = vm.to_string(&arg(args, 0))?;
    Ok(Value::Number(parse_date(&s)))
}

fn date_utc(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if args.is_empty() {
        return Ok(Value::Number(f64::NAN));
    }
    let n = number_args(vm, args)?;
    Ok(Value::Number(time_clip(from_components(&n))))
}

macro_rules! getter {
    ($name:ident, $local:expr, $field:ident) => {
        fn $name(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
            let (_, t) = this_time(vm, &this)?;
            if t.is_nan() {
                return Ok(Value::Number(f64::NAN));
            }
            let p = split(if $local { local_time(t) } else { t });
            Ok(Value::Number(p.$field))
        }
    };
}

getter!(get_full_year, true, year);
getter!(get_month, true, month);
getter!(get_date, true, date);
getter!(get_day, true, weekday);
getter!(get_hours, true, hours);
getter!(get_minutes, true, minutes);
getter!(get_seconds, true, seconds);
getter!(get_milliseconds, true, ms);
getter!(get_utc_full_year, false, year);
getter!(get_utc_month, false, month);
getter!(get_utc_date, false, date);
getter!(get_utc_day, false, weekday);
getter!(get_utc_hours, false, hours);
getter!(get_utc_minutes, false, minutes);
getter!(get_utc_seconds, false, seconds);
getter!(get_utc_milliseconds, false, ms);

fn get_time(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    Ok(Value::Number(t))
}

fn get_timezone_offset(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    if t.is_nan() {
        return Ok(Value::Number(f64::NAN));
    }
    Ok(Value::Number(-local_offset_ms(t) / 60_000.0 + 0.0))
}

fn set_time(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let (o, _) = this_time(vm, &this)?;
    let t = vm.to_number(&arg(args, 0))?;
    Ok(Value::Number(set_this(&o, t)))
}

#[derive(Clone, Copy)]
enum Field {
    Ms,
    Seconds,
    Minutes,
    Hours,
    Date,
    Month,
    Year,
}

fn setter(vm: &mut Vm, this: Value, args: &[Value], field: Field, local: bool) -> R {
    let (o, t) = this_time(vm, &this)?;
    let n = number_args(vm, args)?;
    let get = |i: usize| n.get(i).copied();
    let base = if t.is_nan() {
        if matches!(field, Field::Year) {
            0.0
        } else {
            return Ok(Value::Number(f64::NAN));
        }
    } else if local {
        local_time(t)
    } else {
        t
    };
    let mut p = split(base);
    let first = get(0).unwrap_or(f64::NAN);
    match field {
        Field::Ms => p.ms = first,
        Field::Seconds => {
            p.seconds = first;
            if let Some(v) = get(1) {
                p.ms = v;
            }
        }
        Field::Minutes => {
            p.minutes = first;
            if let Some(v) = get(1) {
                p.seconds = v;
            }
            if let Some(v) = get(2) {
                p.ms = v;
            }
        }
        Field::Hours => {
            p.hours = first;
            if let Some(v) = get(1) {
                p.minutes = v;
            }
            if let Some(v) = get(2) {
                p.seconds = v;
            }
            if let Some(v) = get(3) {
                p.ms = v;
            }
        }
        Field::Date => p.date = first,
        Field::Month => {
            p.month = first;
            if let Some(v) = get(1) {
                p.date = v;
            }
        }
        Field::Year => {
            p.year = first;
            if let Some(v) = get(1) {
                p.month = v;
            }
            if let Some(v) = get(2) {
                p.date = v;
            }
        }
    }
    let joined = join(&p);
    let t = if local {
        utc_from_local(joined)
    } else {
        joined
    };
    Ok(Value::Number(set_this(&o, t)))
}

macro_rules! setter_fn {
    ($name:ident, $field:expr, $local:expr) => {
        fn $name(vm: &mut Vm, this: Value, args: &[Value]) -> R {
            setter(vm, this, args, $field, $local)
        }
    };
}

setter_fn!(set_milliseconds, Field::Ms, true);
setter_fn!(set_seconds, Field::Seconds, true);
setter_fn!(set_minutes, Field::Minutes, true);
setter_fn!(set_hours, Field::Hours, true);
setter_fn!(set_date, Field::Date, true);
setter_fn!(set_month, Field::Month, true);
setter_fn!(set_full_year, Field::Year, true);
setter_fn!(set_utc_milliseconds, Field::Ms, false);
setter_fn!(set_utc_seconds, Field::Seconds, false);
setter_fn!(set_utc_minutes, Field::Minutes, false);
setter_fn!(set_utc_hours, Field::Hours, false);
setter_fn!(set_utc_date, Field::Date, false);
setter_fn!(set_utc_month, Field::Month, false);
setter_fn!(set_utc_full_year, Field::Year, false);

fn to_iso_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    match iso_string(t) {
        Some(s) => Ok(s.into()),
        None => Err(vm.range_error("Invalid time value")),
    }
}

fn to_json(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    Ok(iso_string(t).map(Value::from).unwrap_or(Value::Null))
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    Ok(date_to_string(t).into())
}

fn to_date_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    if t.is_nan() {
        return Ok("Invalid Date".into());
    }
    Ok(date_part(&split(local_time(t))).into())
}

fn to_time_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    if t.is_nan() {
        return Ok("Invalid Date".into());
    }
    Ok(time_part(&split(local_time(t)), t).into())
}

fn to_utc_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    if t.is_nan() {
        return Ok("Invalid Date".into());
    }
    let p = split(t);
    Ok(format!(
        "{}, {} {} {} {}:{}:{} GMT",
        DAYS[p.weekday as usize],
        pad(p.date, 2),
        MONTHS[p.month as usize],
        year_text(p.year),
        pad(p.hours, 2),
        pad(p.minutes, 2),
        pad(p.seconds, 2)
    )
    .into())
}

fn to_locale_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    if t.is_nan() {
        return Ok("Invalid Date".into());
    }
    let p = split(local_time(t));
    Ok(format!("{}, {}", locale_date(&p), locale_time(&p)).into())
}

fn to_locale_date_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    if t.is_nan() {
        return Ok("Invalid Date".into());
    }
    Ok(locale_date(&split(local_time(t))).into())
}

fn to_locale_time_string(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let (_, t) = this_time(vm, &this)?;
    if t.is_nan() {
        return Ok("Invalid Date".into());
    }
    Ok(locale_time(&split(local_time(t))).into())
}

fn digits(s: &[u8], i: &mut usize, n: usize) -> Option<f64> {
    if *i + n > s.len() || !s[*i..*i + n].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let v = std::str::from_utf8(&s[*i..*i + n])
        .ok()?
        .parse::<f64>()
        .ok()?;
    *i += n;
    Some(v)
}

fn parse_iso(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let mut i = 0;
    let year = if b.first() == Some(&b'+') || b.first() == Some(&b'-') {
        let neg = b[0] == b'-';
        i = 1;
        let y = digits(b, &mut i, 6)?;
        if neg { -y } else { y }
    } else {
        digits(b, &mut i, 4)?
    };
    let mut month = 1.0;
    let mut day = 1.0;
    if b.get(i) == Some(&b'-') {
        i += 1;
        month = digits(b, &mut i, 2)?;
        if b.get(i) == Some(&b'-') {
            i += 1;
            day = digits(b, &mut i, 2)?;
        }
    }
    if !(1.0..=12.0).contains(&month) || !(1.0..=31.0).contains(&day) {
        return None;
    }
    let (mut h, mut mi, mut sec, mut ms) = (0.0, 0.0, 0.0, 0.0);
    let mut has_time = false;
    if matches!(b.get(i), Some(b'T') | Some(b't') | Some(b' ')) {
        i += 1;
        has_time = true;
        h = digits(b, &mut i, 2)?;
        if b.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        mi = digits(b, &mut i, 2)?;
        if b.get(i) == Some(&b':') {
            i += 1;
            sec = digits(b, &mut i, 2)?;
            if b.get(i) == Some(&b'.') || b.get(i) == Some(&b',') {
                i += 1;
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i == start {
                    return None;
                }
                let frac: String = s[start..i].chars().take(3).collect();
                ms = format!("{frac:0<3}").parse::<f64>().ok()?;
            }
        }
        if h > 24.0 || mi > 59.0 || sec > 59.0 || (h == 24.0 && (mi > 0.0 || sec > 0.0)) {
            return None;
        }
    }
    let mut offset: Option<f64> = None;
    match b.get(i) {
        Some(b'Z') | Some(b'z') => {
            offset = Some(0.0);
            i += 1;
        }
        Some(b'+') | Some(b'-') => {
            let neg = b[i] == b'-';
            i += 1;
            let oh = digits(b, &mut i, 2)?;
            if b.get(i) == Some(&b':') {
                i += 1;
            }
            let om = digits(b, &mut i, 2)?;
            let total = (oh * 60.0 + om) * 60_000.0;
            offset = Some(if neg { -total } else { total });
        }
        _ => {}
    }
    if i != b.len() {
        return None;
    }
    let t = make_date(make_day(year, month - 1.0, day), make_time(h, mi, sec, ms));
    Some(match offset {
        Some(off) => t - off,
        None if has_time => utc_from_local(t),
        None => t,
    })
}

fn parse_loose(s: &str) -> Option<f64> {
    let cleaned = s.replace(',', " ");
    let mut month = None;
    let mut numbers: Vec<f64> = Vec::new();
    let mut time: Option<(f64, f64, f64)> = None;
    let mut offset: Option<f64> = None;
    for tok in cleaned.split_whitespace() {
        let lower = tok.to_ascii_lowercase();
        if let Some(m) = MONTHS
            .iter()
            .position(|name| lower.starts_with(&name.to_ascii_lowercase()))
        {
            month = Some(m as f64);
            continue;
        }
        if DAYS
            .iter()
            .any(|d| lower.starts_with(&d.to_ascii_lowercase()))
        {
            continue;
        }
        if lower.starts_with('(') {
            break;
        }
        if lower == "gmt" || lower == "utc" || lower == "z" {
            offset = Some(offset.unwrap_or(0.0));
            continue;
        }
        let zone = lower
            .strip_prefix("gmt")
            .or_else(|| lower.strip_prefix("utc"))
            .unwrap_or(&lower);
        if (zone.starts_with('+') || zone.starts_with('-')) && zone.len() >= 5 {
            let neg = zone.starts_with('-');
            let digits_only: String = zone[1..].chars().filter(char::is_ascii_digit).collect();
            if digits_only.len() == 4 {
                let oh: f64 = digits_only[..2].parse().ok()?;
                let om: f64 = digits_only[2..].parse().ok()?;
                let total = (oh * 60.0 + om) * 60_000.0;
                offset = Some(if neg { -total } else { total });
                continue;
            }
        }
        if tok.contains(':') {
            let parts: Vec<f64> = tok
                .split(':')
                .map(|p| p.parse::<f64>())
                .collect::<Result<_, _>>()
                .ok()?;
            time = Some((
                parts[0],
                parts.get(1).copied().unwrap_or(0.0),
                parts.get(2).copied().unwrap_or(0.0),
            ));
            continue;
        }
        if lower == "am" || lower == "pm" {
            if let Some((h, m, s)) = time {
                let h = h % 12.0 + if lower == "pm" { 12.0 } else { 0.0 };
                time = Some((h, m, s));
            }
            continue;
        }
        numbers.push(tok.parse::<f64>().ok()?);
    }
    let month = month?;
    let (day, year) = match numbers.as_slice() {
        [a, b] if *a > 31.0 => (*b, *a),
        [a, b] => (*a, *b),
        _ => return None,
    };
    let (h, mi, sec) = time.unwrap_or((0.0, 0.0, 0.0));
    let t = make_date(make_day(year, month, day), make_time(h, mi, sec, 0.0));
    Some(match offset {
        Some(off) => t - off,
        None => utc_from_local(t),
    })
}

pub fn parse_date(s: &str) -> f64 {
    let s = s.trim();
    parse_iso(s)
        .or_else(|| parse_loose(s))
        .map(time_clip)
        .unwrap_or(f64::NAN)
}
