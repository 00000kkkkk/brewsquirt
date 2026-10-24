use crate::compiler::Mode;
use crate::value::Value;
use crate::vm::Vm;

pub const LAZY_GLOBALS: [&str; 1] = ["Intl"];

const LOCALE_SRC: &str = include_str!("intl_locale.js");
const LOCALE_DATA: &str = include_str!("locale_data.json");

const INTL_SRC: &str = include_str!("intl.js");
static INTL_DATA: &[u8] = include_bytes!("intl_data.zst");
static INTL_NAMES: &[u8] = include_bytes!("intl_names.zst");

type Sections = std::collections::HashMap<String, std::collections::HashMap<String, String>>;

fn unpack(blob: &[u8]) -> Sections {
    use std::io::Read;
    let mut text = String::new();
    if let Ok(mut decoder) = zstd::stream::read::Decoder::new(blob) {
        let _ = decoder.read_to_string(&mut text);
    }
    let mut sections = Sections::new();
    for line in text.lines() {
        let mut fields = line.splitn(3, '\t');
        if let (Some(section), Some(locale), Some(json)) =
            (fields.next(), fields.next(), fields.next())
        {
            sections
                .entry(section.to_string())
                .or_default()
                .insert(locale.to_string(), json.to_string());
        }
    }
    sections
}

fn intl_sections(section: &str) -> &'static Sections {
    static CORE: std::sync::OnceLock<Sections> = std::sync::OnceLock::new();
    static NAMES: std::sync::OnceLock<Sections> = std::sync::OnceLock::new();
    if section == "names" {
        NAMES.get_or_init(|| unpack(INTL_NAMES))
    } else {
        CORE.get_or_init(|| unpack(INTL_DATA))
    }
}

fn intl_data(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    let section = vm.to_string(&super::arg(args, 0))?;
    let locale = vm.to_string(&super::arg(args, 1))?;
    Ok(intl_sections(section.as_str())
        .get(section.as_str())
        .and_then(|s| s.get(locale.as_str()))
        .map_or(Value::Undefined, |json| Value::from(json.as_str())))
}

fn segment_breaks(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    use unicode_segmentation::UnicodeSegmentation;
    let text = vm.to_string(&super::arg(args, 0))?;
    let granularity = vm.to_string(&super::arg(args, 1))?;
    let s = text.as_str();
    let word = granularity.as_str() == "word";
    let pieces: Vec<&str> = match granularity.as_str() {
        "word" => s.split_word_bounds().collect(),
        "sentence" => s.split_sentence_bounds().collect(),
        _ => s.graphemes(true).collect(),
    };
    let mut starts = Vec::with_capacity(pieces.len() + 1);
    let mut words = Vec::new();
    let mut unit = 0usize;
    for piece in pieces {
        starts.push(Value::Number(unit as f64));
        if word {
            words.push(Value::Bool(piece.chars().any(char::is_alphanumeric)));
        }
        unit += piece.chars().map(char::len_utf16).sum::<usize>();
    }
    starts.push(Value::Number(unit as f64));
    let starts = vm.new_array(starts);
    let words = vm.new_array(words);
    Ok(vm.new_array(vec![starts, words]))
}

fn intl_locales(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    let section = vm.to_string(&super::arg(args, 0))?;
    let mut names: Vec<&str> = intl_sections(section.as_str())
        .get(section.as_str())
        .map(|s| s.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    names.sort_unstable();
    let values = names.into_iter().map(Value::from).collect();
    Ok(vm.new_array(values))
}

fn tz_lookup(name: &str) -> Option<chrono_tz::Tz> {
    use std::str::FromStr;
    if let Ok(tz) = chrono_tz::Tz::from_str(name) {
        return Some(tz);
    }
    let lower = name.to_ascii_lowercase();
    chrono_tz::TZ_VARIANTS
        .iter()
        .copied()
        .find(|tz| tz.name().to_ascii_lowercase() == lower)
}

fn tz_info(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    use chrono::{Offset, TimeZone};
    use chrono_tz::OffsetName;
    let name = vm.to_string(&super::arg(args, 0))?;
    let t = vm.to_number(&super::arg(args, 1))?;
    let Some(tz) = tz_lookup(&name) else {
        return Ok(Value::Undefined);
    };
    if !t.is_finite() {
        return Ok(Value::Undefined);
    }
    let Some(utc) = chrono::DateTime::from_timestamp((t / 1000.0).floor() as i64, 0) else {
        return Ok(Value::Undefined);
    };
    let offset = tz.offset_from_utc_datetime(&utc.naive_utc());
    let minutes = offset.fix().local_minus_utc() / 60;
    let abbreviation = offset.abbreviation().unwrap_or("");
    Ok(vm.new_array(vec![
        Value::Number(minutes as f64),
        Value::from(abbreviation),
    ]))
}

fn tz_canonical(vm: &mut Vm, _this: Value, args: &[Value]) -> Result<Value, Value> {
    let name = vm.to_string(&super::arg(args, 0))?;
    Ok(match tz_lookup(&name) {
        Some(tz) => Value::from(tz.name()),
        None => Value::Undefined,
    })
}

fn tz_list(vm: &mut Vm, _this: Value, _args: &[Value]) -> Result<Value, Value> {
    let mut names: Vec<&str> = chrono_tz::TZ_VARIANTS
        .iter()
        .map(|tz| tz.name())
        .filter(|name| {
            name.contains('/')
                && !name.starts_with("Etc/")
                && !name.starts_with("SystemV/")
                && !name.starts_with("US/")
                && !name.starts_with("Canada/")
                && !name.starts_with("Mexico/")
                && !name.starts_with("Brazil/")
                && !name.starts_with("Chile/")
        })
        .collect();
    names.push("UTC");
    names.sort_unstable();
    names.dedup();
    let values = names.into_iter().map(Value::from).collect();
    Ok(vm.new_array(values))
}

fn tz_system(_vm: &mut Vm, _this: Value, _args: &[Value]) -> Result<Value, Value> {
    let name = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
    let utc = [
        "UTC",
        "GMT",
        "UCT",
        "Zulu",
        "Universal",
        "Greenwich",
        "GMT0",
        "GMT+0",
        "GMT-0",
    ];
    let bare = name.strip_prefix("Etc/").unwrap_or(&name);
    if utc.contains(&bare) || tz_lookup(&name).is_none() {
        return Ok(Value::from("UTC"));
    }
    Ok(Value::from(name))
}

fn locale_data(_vm: &mut Vm, _this: Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::from(LOCALE_DATA))
}

pub fn load(vm: &mut Vm) -> Result<Value, Value> {
    if let Some(v) = vm.globals.get("Intl").cloned() {
        return Ok(v);
    }
    let natives: [(&str, u32, crate::object::NativeFn); 7] = [
        ("%tzInfo%", 2, tz_info),
        ("%tzCanonical%", 1, tz_canonical),
        ("%tzList%", 0, tz_list),
        ("%tzSystem%", 0, tz_system),
        ("%intlData%", 2, intl_data),
        ("%intlLocales%", 1, intl_locales),
        ("%segmentBreaks%", 2, segment_breaks),
    ];
    for (name, arity, f) in natives {
        let v = super::native_fn(vm, name, arity, f);
        vm.define_global(name, v);
    }
    let data = super::native_fn(vm, "%localeData%", 0, locale_data);
    vm.define_global("%localeData%", data);
    let dir = std::env::current_dir().unwrap_or_default();
    let locale_proto = match vm.compile(LOCALE_SRC, "<squirt:intl-locale>", dir.clone(), Mode::Repl)
    {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<squirt:intl-locale>")),
    };
    let locale = vm.run_proto(locale_proto)?;
    vm.define_global("%intlLocale%", locale);
    let proto = match vm.compile(INTL_SRC, "<squirt:intl>", dir, Mode::Repl) {
        Ok(p) => p,
        Err(e) => return Err(vm.syntax_error_value(&e, "<squirt:intl>")),
    };
    let intl = vm.run_proto(proto)?;
    vm.define_global("Intl", intl.clone());
    vm.globals
        .attrs
        .insert("Intl".into(), crate::object::NON_ENUM);
    Ok(intl)
}
