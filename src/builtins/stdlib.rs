use super::{arg, method};
use crate::object::Obj;
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::rc::Rc;

type R = Result<Value, Value>;

const NATIVE_SEP: char = std::path::MAIN_SEPARATOR;

fn is_sep(c: char, sep: char) -> bool {
    c == '/' || (sep == '\\' && c == '\\')
}

fn drive_root(p: &str, sep: char) -> (String, &str) {
    if sep == '\\' {
        let b = p.as_bytes();
        if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
            let rest = &p[2..];
            if rest.starts_with(['/', '\\']) {
                return (format!("{}{}", &p[..2], sep), &rest[1..]);
            }
            return (p[..2].to_string(), rest);
        }
    }
    if p.starts_with(|c| is_sep(c, sep)) {
        return (sep.to_string(), &p[1..]);
    }
    (String::new(), p)
}

fn normalize_with(p: &str, sep: char) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let trailing = p.ends_with(|c| is_sep(c, sep)) && p.len() > 1;
    let (root, rest) = drive_root(p, sep);
    let absolute = root.ends_with(sep);
    let mut parts: Vec<&str> = Vec::new();
    for seg in rest.split(|c| is_sep(c, sep)) {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    let mut out = root.clone();
    out.push_str(&parts.join(&sep.to_string()));
    if out.is_empty() {
        out.push('.');
    }
    if trailing && !out.ends_with(sep) {
        out.push(sep);
    }
    out
}

fn is_absolute_with(p: &str, sep: char) -> bool {
    let (root, _) = drive_root(p, sep);
    root.ends_with(sep)
}

fn cwd_string() -> String {
    std::env::current_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

fn resolve_with(parts: &[String], sep: char) -> String {
    let mut acc = String::new();
    for p in parts.iter().rev() {
        if p.is_empty() {
            continue;
        }
        acc = if acc.is_empty() {
            p.clone()
        } else {
            format!("{p}{sep}{acc}")
        };
        if is_absolute_with(&acc, sep) {
            break;
        }
    }
    if !is_absolute_with(&acc, sep) {
        let cwd = if sep == NATIVE_SEP {
            cwd_string()
        } else {
            cwd_string().replace('\\', "/")
        };
        acc = if acc.is_empty() {
            cwd
        } else {
            format!("{cwd}{sep}{acc}")
        };
    }
    let n = normalize_with(&acc, sep);
    if n.len() > 1 && n.ends_with(sep) && !n.ends_with(&format!(":{sep}")) {
        n[..n.len() - 1].to_string()
    } else {
        n
    }
}

fn split_last(p: &str, sep: char) -> (&str, &str) {
    let trimmed = p.trim_end_matches(|c| is_sep(c, sep));
    if trimmed.is_empty() {
        return (if p.is_empty() { "" } else { &p[..1] }, "");
    }
    match trimmed.rfind(|c| is_sep(c, sep)) {
        Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
        None => ("", trimmed),
    }
}

fn dirname_with(p: &str, sep: char) -> String {
    let (root, rest) = drive_root(p, sep);
    if rest.trim_matches(|c| is_sep(c, sep)).is_empty() {
        return if root.is_empty() {
            ".".to_string()
        } else {
            root
        };
    }
    let (dir, _) = split_last(p, sep);
    if dir.is_empty() {
        if root.is_empty() {
            ".".to_string()
        } else {
            root
        }
    } else if dir.len() < root.len() {
        root
    } else {
        dir.to_string()
    }
}

fn basename_with(p: &str, sep: char) -> String {
    let (_, rest) = drive_root(p, sep);
    let (_, base) = split_last(rest, sep);
    base.to_string()
}

fn extname_of(base: &str) -> String {
    match base.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => base[i..].to_string(),
    }
}

fn relative_with(from: &str, to: &str, sep: char) -> String {
    let from = resolve_with(&[from.to_string()], sep);
    let to = resolve_with(&[to.to_string()], sep);
    let fold = |s: &str| {
        if sep == '\\' {
            s.to_lowercase()
        } else {
            s.to_string()
        }
    };
    let fa: Vec<&str> = from.split(sep).filter(|s| !s.is_empty()).collect();
    let ta: Vec<&str> = to.split(sep).filter(|s| !s.is_empty()).collect();
    let mut i = 0;
    while i < fa.len() && i < ta.len() && fold(fa[i]) == fold(ta[i]) {
        i += 1;
    }
    let mut parts: Vec<&str> = vec![".."; fa.len() - i];
    parts.extend(&ta[i..]);
    parts.join(&sep.to_string())
}

fn strings(vm: &mut Vm, args: &[Value]) -> Result<Vec<String>, Value> {
    let mut out = Vec::new();
    for a in args {
        match a {
            Value::Str(s) => out.push(s.to_string()),
            other => {
                return Err(vm.type_error(format!(
                    "The \"path\" argument must be of type string. Received {}",
                    crate::inspect::inspect_plain(other)
                )));
            }
        }
    }
    Ok(out)
}

fn sep_of(this: &Value) -> char {
    match this {
        Value::Object(o) => match o.get_own("sep") {
            Some(Value::Str(s)) if &*s == "/" => '/',
            Some(Value::Str(s)) if &*s == "\\" => '\\',
            _ => NATIVE_SEP,
        },
        _ => NATIVE_SEP,
    }
}

fn p_join(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let parts = strings(vm, args)?;
    let joined = parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(&sep.to_string());
    Ok(normalize_with(&joined, sep).into())
}

fn p_resolve(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let parts = strings(vm, args)?;
    Ok(resolve_with(&parts, sep).into())
}

fn p_normalize(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let p = strings(vm, &args[..args.len().min(1)])?;
    Ok(normalize_with(p.first().map(String::as_str).unwrap_or(""), sep).into())
}

fn p_dirname(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let p = strings(vm, &[arg(args, 0)])?;
    Ok(dirname_with(&p[0], sep).into())
}

fn p_basename(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let p = strings(vm, &[arg(args, 0)])?;
    let mut base = basename_with(&p[0], sep);
    if let Value::Str(ext) = arg(args, 1) {
        if !ext.is_empty() && base.ends_with(&*ext) && base.len() > ext.len() {
            base.truncate(base.len() - ext.len());
        }
    }
    Ok(base.into())
}

fn p_extname(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let p = strings(vm, &[arg(args, 0)])?;
    Ok(extname_of(&basename_with(&p[0], sep)).into())
}

fn p_is_absolute(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let p = strings(vm, &[arg(args, 0)])?;
    Ok(Value::Bool(is_absolute_with(&p[0], sep)))
}

fn p_to_namespaced(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let input = arg(args, 0);
    if !matches!(input, Value::Str(_)) {
        return Ok(input);
    }
    let p = strings(vm, &[input])?;
    let path = p[0].as_str();
    if sep != '\\' || path.is_empty() || !is_absolute_with(path, sep) || path.starts_with("\\\\?\\")
    {
        return Ok(path.into());
    }
    if let Some(rest) = path.strip_prefix("\\\\") {
        return Ok(format!("\\\\?\\UNC\\{rest}").into());
    }
    Ok(format!("\\\\?\\{path}").into())
}

fn p_relative(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let p = strings(vm, &[arg(args, 0), arg(args, 1)])?;
    Ok(relative_with(&p[0], &p[1], sep).into())
}

fn p_parse(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let p = strings(vm, &[arg(args, 0)])?;
    let path = &p[0];
    let (root, _) = drive_root(path, sep);
    let base = basename_with(path, sep);
    let ext = extname_of(&base);
    let name = base[..base.len() - ext.len()].to_string();
    let dir = if base.is_empty() {
        root.clone()
    } else {
        let d = dirname_with(path, sep);
        if d == "." && !path.starts_with('.') {
            String::new()
        } else {
            d
        }
    };
    let o = vm.new_object();
    o.set_own("root", root.into());
    o.set_own("dir", dir.into());
    o.set_own("base", base.into());
    o.set_own("ext", ext.into());
    o.set_own("name", name.into());
    Ok(Value::Object(o))
}

fn p_format(vm: &mut Vm, this: Value, args: &[Value]) -> R {
    let sep = sep_of(&this);
    let o = arg(args, 0);
    let get = |vm: &mut Vm, k: &str| -> Result<String, Value> {
        match vm.get_prop(&o, k)? {
            Value::Undefined | Value::Null => Ok(String::new()),
            v => Ok(vm.to_string(&v)?.to_string()),
        }
    };
    let dir = get(vm, "dir")?;
    let root = get(vm, "root")?;
    let mut base = get(vm, "base")?;
    if base.is_empty() {
        base = format!("{}{}", get(vm, "name")?, get(vm, "ext")?);
    }
    let head = if dir.is_empty() { root.clone() } else { dir };
    Ok(if head.is_empty() {
        base
    } else if head == root || head.ends_with(sep) {
        format!("{head}{base}")
    } else {
        format!("{head}{sep}{base}")
    }
    .into())
}

fn path_object(vm: &mut Vm, sep: char) -> Rc<Obj> {
    let o = vm.new_object();
    let fns: &[(&str, u32, crate::object::NativeFn)] = &[
        ("join", 0, p_join),
        ("resolve", 0, p_resolve),
        ("normalize", 1, p_normalize),
        ("dirname", 1, p_dirname),
        ("basename", 2, p_basename),
        ("extname", 1, p_extname),
        ("isAbsolute", 1, p_is_absolute),
        ("relative", 2, p_relative),
        ("parse", 1, p_parse),
        ("format", 1, p_format),
        ("toNamespacedPath", 1, p_to_namespaced),
    ];
    for (name, arity, f) in fns {
        method(vm, &o, name, *arity, *f);
    }
    o.set_own("sep", sep.to_string().into());
    o.set_own("delimiter", if sep == '\\' { ";" } else { ":" }.into());
    o
}

pub fn path_module(vm: &mut Vm) -> Value {
    let native = path_object(vm, NATIVE_SEP);
    let posix = path_object(vm, '/');
    let win32 = path_object(vm, '\\');
    native.set_own("posix", Value::Object(posix.clone()));
    native.set_own("win32", Value::Object(win32.clone()));
    posix.set_own("posix", Value::Object(posix.clone()));
    posix.set_own("win32", Value::Object(win32.clone()));
    win32.set_own("posix", Value::Object(posix));
    win32.set_own("win32", Value::Object(win32.clone()));
    native.set_hidden("default", Value::Object(native.clone()));
    Value::Object(native)
}

fn os_platform(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
    .into())
}

fn os_arch(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        other => other,
    }
    .into())
}

fn os_type(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(match std::env::consts::OS {
        "windows" => "Windows_NT",
        "macos" => "Darwin",
        "linux" => "Linux",
        other => other,
    }
    .into())
}

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default()
}

fn os_homedir(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(home_dir().into())
}

fn os_tmpdir(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let t = std::env::temp_dir().display().to_string();
    let t = t.trim_end_matches(['/', '\\']).to_string();
    Ok(t.into())
}

fn os_hostname(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
        })
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .unwrap_or_else(|| "localhost".to_string());
    Ok(name.into())
}

fn parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn os_parallelism(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(parallelism() as f64))
}

fn os_cpus(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let list = (0..parallelism())
        .map(|_| {
            let c = vm.new_object();
            c.set_own("model", "".into());
            c.set_own("speed", Value::Number(0.0));
            Value::Object(c)
        })
        .collect();
    Ok(vm.new_array(list))
}

#[cfg(windows)]
mod osinfo {
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        csd: [u16; 128],
    }

    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page: u64,
        avail_page: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended: u64,
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buf: *mut MemoryStatusEx) -> i32;
        fn GetTickCount64() -> u64;
    }

    fn version_parts() -> (u32, u32, u32) {
        let mut info = OsVersionInfoW {
            size: std::mem::size_of::<OsVersionInfoW>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform_id: 0,
            csd: [0; 128],
        };
        let status = unsafe { RtlGetVersion(&mut info) };
        if status == 0 {
            (info.major, info.minor, info.build)
        } else {
            (10, 0, 0)
        }
    }

    pub fn release() -> String {
        let (major, minor, build) = version_parts();
        format!("{major}.{minor}.{build}")
    }

    pub fn version() -> String {
        let (_, _, build) = version_parts();
        format!("Windows {}", if build >= 22000 { "11" } else { "10" })
    }

    pub fn memory() -> (u64, u64) {
        let mut status = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page: 0,
            avail_page: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended: 0,
        };
        if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
            (status.total_phys, status.avail_phys)
        } else {
            (0, 0)
        }
    }

    pub fn uptime() -> f64 {
        unsafe { GetTickCount64() as f64 / 1000.0 }
    }

    pub fn loadavg() -> [f64; 3] {
        [0.0; 3]
    }

    pub fn ids() -> (f64, f64) {
        (-1.0, -1.0)
    }
}

#[cfg(target_os = "linux")]
mod osinfo {
    fn read(path: &str) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub fn release() -> String {
        let s = read("/proc/sys/kernel/osrelease");
        let s = s.trim();
        if s.is_empty() {
            "0.0.0".to_string()
        } else {
            s.to_string()
        }
    }

    pub fn version() -> String {
        let s = read("/proc/version");
        match s.find("#") {
            Some(i) => s[i..].trim().to_string(),
            None => s.trim().to_string(),
        }
    }

    pub fn memory() -> (u64, u64) {
        let mut total = 0;
        let mut avail = 0;
        for line in read("/proc/meminfo").lines() {
            let kb = |l: &str| {
                l.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0)
                    * 1024
            };
            if line.starts_with("MemTotal:") {
                total = kb(line);
            } else if line.starts_with("MemAvailable:") {
                avail = kb(line);
            }
        }
        (total, avail)
    }

    pub fn uptime() -> f64 {
        read("/proc/uptime")
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    pub fn loadavg() -> [f64; 3] {
        let s = read("/proc/loadavg");
        let mut parts = s
            .split_whitespace()
            .map(|v| v.parse::<f64>().unwrap_or(0.0));
        [
            parts.next().unwrap_or(0.0),
            parts.next().unwrap_or(0.0),
            parts.next().unwrap_or(0.0),
        ]
    }

    pub fn ids() -> (f64, f64) {
        unsafe extern "C" {
            fn getuid() -> u32;
            fn getgid() -> u32;
        }
        unsafe { (getuid() as f64, getgid() as f64) }
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
mod osinfo {
    fn cmd(program: &str, args: &[&str]) -> String {
        std::process::Command::new(program)
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    pub fn release() -> String {
        let s = cmd("uname", &["-r"]);
        if s.is_empty() { "0.0.0".to_string() } else { s }
    }

    pub fn version() -> String {
        cmd("uname", &["-v"])
    }

    pub fn memory() -> (u64, u64) {
        let total = cmd("sysctl", &["-n", "hw.memsize"])
            .parse::<u64>()
            .unwrap_or(0);
        let page = cmd("sysctl", &["-n", "hw.pagesize"])
            .parse::<u64>()
            .unwrap_or(4096);
        let mut free = 0;
        for line in cmd("vm_stat", &[]).lines() {
            if line.starts_with("Pages free:") || line.starts_with("Pages inactive:") {
                let n = line
                    .split(':')
                    .nth(1)
                    .map(|v| v.trim().trim_end_matches('.'))
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
                free += n * page;
            }
        }
        (total, free)
    }

    pub fn uptime() -> f64 {
        let s = cmd("sysctl", &["-n", "kern.boottime"]);
        let boot = s
            .split("sec = ")
            .nth(1)
            .and_then(|v| v.split([',', ' ']).next())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        if boot > 0.0 { now - boot } else { 0.0 }
    }

    pub fn loadavg() -> [f64; 3] {
        let s = cmd("sysctl", &["-n", "vm.loadavg"]);
        let mut parts = s
            .trim_matches(|c| c == '{' || c == '}' || c == ' ')
            .split_whitespace()
            .map(|v| v.parse::<f64>().unwrap_or(0.0));
        [
            parts.next().unwrap_or(0.0),
            parts.next().unwrap_or(0.0),
            parts.next().unwrap_or(0.0),
        ]
    }

    pub fn ids() -> (f64, f64) {
        unsafe extern "C" {
            fn getuid() -> u32;
            fn getgid() -> u32;
        }
        unsafe { (getuid() as f64, getgid() as f64) }
    }
}

#[cfg(not(any(windows, unix)))]
mod osinfo {
    pub fn release() -> String {
        "0.0.0".to_string()
    }
    pub fn version() -> String {
        String::new()
    }
    pub fn memory() -> (u64, u64) {
        (0, 0)
    }
    pub fn uptime() -> f64 {
        0.0
    }
    pub fn loadavg() -> [f64; 3] {
        [0.0; 3]
    }
    pub fn ids() -> (f64, f64) {
        (-1.0, -1.0)
    }
}

pub fn os_ids() -> (f64, f64) {
    osinfo::ids()
}

fn os_release(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(osinfo::release().into())
}

fn os_version(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(osinfo::version().into())
}

fn os_machine(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        "x86" => "i686",
        other => other,
    }
    .into())
}

fn os_user_info(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let (uid, gid) = osinfo::ids();
    let username = ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .unwrap_or_default();
    let o = vm.new_object();
    o.set_own("uid", Value::Number(uid));
    o.set_own("gid", Value::Number(gid));
    o.set_own("username", username.into());
    o.set_own("homedir", home_dir().into());
    o.set_own(
        "shell",
        match std::env::var("SHELL") {
            Ok(s) if !cfg!(windows) => s.into(),
            _ => Value::Null,
        },
    );
    Ok(Value::Object(o))
}

fn os_loadavg(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let list = osinfo::loadavg()
        .iter()
        .map(|v| Value::Number(*v))
        .collect();
    Ok(vm.new_array(list))
}

fn os_uptime(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(osinfo::uptime().floor()))
}

fn os_totalmem(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(osinfo::memory().0 as f64))
}

fn os_freemem(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(osinfo::memory().1 as f64))
}

fn os_endianness(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(if cfg!(target_endian = "little") {
        "LE"
    } else {
        "BE"
    }
    .into())
}

fn os_get_priority(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(0.0))
}

fn os_set_priority(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Undefined)
}

fn os_network_interfaces(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let entry = vm.new_object();
    entry.set_own("address", "127.0.0.1".into());
    entry.set_own("netmask", "255.0.0.0".into());
    entry.set_own("family", "IPv4".into());
    entry.set_own("mac", "00:00:00:00:00:00".into());
    entry.set_own("internal", Value::Bool(true));
    entry.set_own("cidr", "127.0.0.1/8".into());
    let list = vm.new_array(vec![Value::Object(entry)]);
    let o = vm.new_object();
    let name = if cfg!(windows) {
        "Loopback Pseudo-Interface 1"
    } else {
        "lo"
    };
    o.set_own(name, list);
    Ok(Value::Object(o))
}

fn numbers(vm: &mut Vm, pairs: &[(&str, f64)]) -> Value {
    let o = vm.new_object();
    for (k, v) in pairs {
        o.set_own(*k, Value::Number(*v));
    }
    Value::Object(o)
}

fn os_constants(vm: &mut Vm) -> Value {
    let signals = numbers(
        vm,
        &[
            ("SIGHUP", 1.0),
            ("SIGINT", 2.0),
            ("SIGQUIT", 3.0),
            ("SIGILL", 4.0),
            ("SIGTRAP", 5.0),
            ("SIGABRT", 6.0),
            ("SIGIOT", 6.0),
            ("SIGBUS", 7.0),
            ("SIGFPE", 8.0),
            ("SIGKILL", 9.0),
            ("SIGUSR1", 10.0),
            ("SIGSEGV", 11.0),
            ("SIGUSR2", 12.0),
            ("SIGPIPE", 13.0),
            ("SIGALRM", 14.0),
            ("SIGTERM", 15.0),
            ("SIGCHLD", 17.0),
            ("SIGCONT", 18.0),
            ("SIGSTOP", 19.0),
            ("SIGTSTP", 20.0),
            ("SIGTTIN", 21.0),
            ("SIGTTOU", 22.0),
            ("SIGURG", 23.0),
            ("SIGXCPU", 24.0),
            ("SIGXFSZ", 25.0),
            ("SIGVTALRM", 26.0),
            ("SIGPROF", 27.0),
            ("SIGWINCH", 28.0),
            ("SIGIO", 29.0),
            ("SIGSYS", 31.0),
        ],
    );
    let errno = numbers(
        vm,
        &[
            ("E2BIG", 7.0),
            ("EACCES", 13.0),
            ("EADDRINUSE", 98.0),
            ("EADDRNOTAVAIL", 99.0),
            ("EAGAIN", 11.0),
            ("EBADF", 9.0),
            ("EBUSY", 16.0),
            ("ECONNABORTED", 103.0),
            ("ECONNREFUSED", 111.0),
            ("ECONNRESET", 104.0),
            ("EEXIST", 17.0),
            ("EINTR", 4.0),
            ("EINVAL", 22.0),
            ("EIO", 5.0),
            ("EISDIR", 21.0),
            ("EMFILE", 24.0),
            ("ENOENT", 2.0),
            ("ENOMEM", 12.0),
            ("ENOSPC", 28.0),
            ("ENOTDIR", 20.0),
            ("ENOTEMPTY", 39.0),
            ("ENOTSUP", 95.0),
            ("EPERM", 1.0),
            ("EPIPE", 32.0),
            ("ETIMEDOUT", 110.0),
        ],
    );
    let priority = numbers(
        vm,
        &[
            ("PRIORITY_LOW", 19.0),
            ("PRIORITY_BELOW_NORMAL", 10.0),
            ("PRIORITY_NORMAL", 0.0),
            ("PRIORITY_ABOVE_NORMAL", -7.0),
            ("PRIORITY_HIGH", -14.0),
            ("PRIORITY_HIGHEST", -20.0),
        ],
    );
    let dlopen = numbers(
        vm,
        &[
            ("RTLD_LAZY", 1.0),
            ("RTLD_NOW", 2.0),
            ("RTLD_GLOBAL", 256.0),
            ("RTLD_LOCAL", 0.0),
        ],
    );
    let o = vm.new_object();
    o.set_own("UV_UDP_REUSEADDR", Value::Number(4.0));
    o.set_own("signals", signals);
    o.set_own("errno", errno);
    o.set_own("priority", priority);
    o.set_own("dlopen", dlopen);
    Value::Object(o)
}

pub fn os_module(vm: &mut Vm) -> Value {
    let o = vm.new_object();
    let fns: &[(&str, crate::object::NativeFn)] = &[
        ("platform", os_platform),
        ("arch", os_arch),
        ("type", os_type),
        ("homedir", os_homedir),
        ("tmpdir", os_tmpdir),
        ("hostname", os_hostname),
        ("availableParallelism", os_parallelism),
        ("cpus", os_cpus),
        ("release", os_release),
        ("version", os_version),
        ("machine", os_machine),
        ("userInfo", os_user_info),
        ("loadavg", os_loadavg),
        ("uptime", os_uptime),
        ("totalmem", os_totalmem),
        ("freemem", os_freemem),
        ("endianness", os_endianness),
        ("networkInterfaces", os_network_interfaces),
        ("getPriority", os_get_priority),
        ("setPriority", os_set_priority),
    ];
    for (name, f) in fns {
        method(vm, &o, name, 0, *f);
    }
    o.set_own("EOL", if cfg!(windows) { "\r\n" } else { "\n" }.into());
    o.set_own(
        "devNull",
        if cfg!(windows) {
            "\\\\.\\nul"
        } else {
            "/dev/null"
        }
        .into(),
    );
    let constants = os_constants(vm);
    o.set_own("constants", constants);
    o.set_hidden("default", Value::Object(o.clone()));
    Value::Object(o)
}

struct RunResult {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_command(
    vm: &mut Vm,
    program: &str,
    args: &[String],
    options: &Value,
    shell: bool,
) -> Result<RunResult, Value> {
    let mut cmd = if shell {
        let mut line = program.to_string();
        for a in args {
            line.push(' ');
            line.push_str(a);
        }
        shell_command(&line)
    } else {
        let mut c = std::process::Command::new(program);
        c.args(args);
        c
    };
    let mut input = None;
    if options.is_object() {
        if let Value::Str(dir) = vm.get_prop(options, "cwd")? {
            cmd.current_dir(&*dir);
        }
        match vm.get_prop(options, "input")? {
            Value::Undefined | Value::Null => {}
            v => input = Some(vm.to_string(&v)?.to_string()),
        }
        let env = vm.get_prop(options, "env")?;
        if env.is_object() {
            for (k, v) in vm.own_entries(&env) {
                let v = vm.to_string(&v)?;
                cmd.env(&*k, &*v);
            }
        }
    }
    cmd.stdin(if input.is_some() {
        std::process::Stdio::piped()
    } else {
        std::process::Stdio::null()
    });
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| vm.make_error(ErrorKind::Error, format!("spawn {program} failed: {e}")))?;
    if let Some(text) = input {
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| vm.make_error(ErrorKind::Error, format!("{program} failed: {e}")))?;
    Ok(RunResult {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

#[cfg(windows)]
fn shell_command(line: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new("cmd");
    c.args(["/d", "/s", "/c"]);
    c.raw_arg(format!("\"{line}\""));
    c
}

#[cfg(not(windows))]
fn shell_command(line: &str) -> std::process::Command {
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(line);
    c
}

fn exec_sync(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let command = vm.to_string(&arg(args, 0))?;
    let r = run_command(vm, &command, &[], &arg(args, 1), true)?;
    if r.status != Some(0) {
        let e = vm.make_error(
            ErrorKind::Error,
            format!("Command failed: {command}\n{}", r.stderr.trim_end()),
        );
        if let Value::Object(o) = &e {
            o.set_own(
                "status",
                r.status
                    .map(|s| Value::Number(s as f64))
                    .unwrap_or(Value::Null),
            );
            o.set_own("stdout", r.stdout.into());
            o.set_own("stderr", r.stderr.into());
        }
        return Err(e);
    }
    Ok(r.stdout.into())
}

fn spawn_sync(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let program = vm.to_string(&arg(args, 0))?;
    let list = match arg(args, 1) {
        Value::Undefined | Value::Null => Vec::new(),
        v => vm.iterate(&v)?,
    };
    let mut argv = Vec::new();
    for a in list {
        argv.push(vm.to_string(&a)?.to_string());
    }
    let options = arg(args, 2);
    let shell = options.is_object() && vm.get_prop(&options, "shell")?.truthy();
    let o = vm.new_object();
    let r = match run_command(vm, &program, &argv, &options, shell) {
        Ok(r) => r,
        Err(e) => {
            let text = match &e {
                Value::Object(err) => match err.get_own("message") {
                    Some(Value::Str(s)) => s.to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            };
            if !(text.contains("not found") || text.contains("os error 2)")) {
                return Err(e);
            }
            let err = vm.make_error(ErrorKind::Error, format!("spawnSync {program} ENOENT"));
            if let Value::Object(eo) = &err {
                eo.set_own("code", "ENOENT".into());
                eo.set_own(
                    "errno",
                    Value::Number(if cfg!(windows) { -4058.0 } else { -2.0 }),
                );
                eo.set_own("syscall", format!("spawnSync {program}").into());
                eo.set_own("path", Value::from(program.to_string()));
                let spawnargs = argv.iter().map(|a| Value::from(a.clone())).collect();
                let spawnargs = vm.new_array(spawnargs);
                eo.set_own("spawnargs", spawnargs);
            }
            o.set_own("error", err);
            o.set_own("status", Value::Null);
            o.set_own("signal", Value::Null);
            o.set_own("output", Value::Null);
            o.set_own("pid", Value::Number(0.0));
            return Ok(Value::Object(o));
        }
    };
    o.set_own(
        "status",
        r.status
            .map(|s| Value::Number(s as f64))
            .unwrap_or(Value::Null),
    );
    o.set_own("stdout", r.stdout.into());
    o.set_own("stderr", r.stderr.into());
    Ok(Value::Object(o))
}

pub fn child_process_module(vm: &mut Vm) -> Value {
    let o = vm.new_object();
    method(vm, &o, "execSync", 2, exec_sync);
    method(vm, &o, "spawnSync", 3, spawn_sync);
    o.set_hidden("default", Value::Object(o.clone()));
    Value::Object(o)
}
