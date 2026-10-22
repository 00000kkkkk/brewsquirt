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
    ];
    for (name, f) in fns {
        method(vm, &o, name, 0, *f);
    }
    o.set_own("EOL", if cfg!(windows) { "\r\n" } else { "\n" }.into());
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
    let r = run_command(vm, &program, &argv, &options, shell)?;
    let o = vm.new_object();
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
