use super::{arg, expect_fn, method, native_fn, to_integer};
use crate::inspect::{inspect_plain, inspect_plain_vm, inspect_top_vm};
use crate::value::Value;
use crate::vm::{ErrorKind, Vm};
use std::path::Path;

type R = Result<Value, Value>;

pub fn install(vm: &mut Vm) {
    let console = vm.new_object();
    method(vm, &console, "log", 0, log);
    method(vm, &console, "info", 0, log);
    method(vm, &console, "debug", 0, log);
    method(vm, &console, "warn", 0, error);
    method(vm, &console, "error", 0, error);
    method(vm, &console, "assert", 0, assert);
    method(vm, &console, "group", 0, group);
    method(vm, &console, "groupCollapsed", 0, group);
    method(vm, &console, "groupEnd", 0, group_end);
    vm.define_global("console", Value::Object(console));

    let process = vm.new_object();
    let argv: Vec<Value> = vm.argv.iter().map(|s| Value::from(s.as_str())).collect();
    let argv = vm.new_array(argv);
    process.set_own("argv", argv);
    let env = vm.new_object();
    for (k, v) in std::env::vars() {
        env.set_own(k.as_str(), Value::from(v));
    }
    process.set_own("env", Value::Object(env));
    let platform = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    };
    process.set_own("platform", platform.into());
    process.set_own("version", "v22.12.0".into());
    method(vm, &process, "exit", 1, exit);
    method(vm, &process, "cwd", 0, cwd);
    method(vm, &process, "chdir", 1, chdir);
    method(vm, &process, "nextTick", 1, next_tick);
    method(vm, &process, "hrtime", 1, hrtime);
    method(vm, &process, "uptime", 0, uptime);
    method(vm, &process, "memoryUsage", 0, memory_usage);
    method(
        vm,
        &process,
        "availableParallelism",
        0,
        available_parallelism,
    );
    method(vm, &process, "on", 2, process_on);
    method(vm, &process, "once", 2, process_on);
    method(vm, &process, "off", 2, process_on);
    if cfg!(unix) {
        method(vm, &process, "getuid", 0, process_uid);
        method(vm, &process, "geteuid", 0, process_uid);
        method(vm, &process, "getgid", 0, process_gid);
        method(vm, &process, "getegid", 0, process_gid);
    }
    process.set_own("pid", Value::Number(std::process::id() as f64));
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        other => other,
    };
    process.set_own("arch", arch.into());
    process.set_own("exitCode", Value::Undefined);
    process.set_own(
        "execPath",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
            .into(),
    );
    let versions = vm.new_object();
    versions.set_own("node", "22.12.0".into());
    versions.set_own("v8", "12.4.254.21-node.21".into());
    versions.set_own("uv", "1.49.1".into());
    versions.set_own("modules", "127".into());
    versions.set_own("jsq", crate::VERSION.into());
    versions.set_own("javasquirt", crate::VERSION.into());
    process.set_own("versions", Value::Object(versions));
    process.set_own(
        "release",
        Value::Object({
            let r = vm.new_object();
            r.set_own("name", "node".into());
            r.set_own("lts", "Jod".into());
            r.set_own("javasquirt", crate::VERSION.into());
            r
        }),
    );
    for (name, fd, f) in [
        ("stdout", 1.0, stdout_write as crate::object::NativeFn),
        ("stderr", 2.0, stderr_write as crate::object::NativeFn),
    ] {
        let stream = vm.new_object();
        method(vm, &stream, "write", 1, f);
        use std::io::IsTerminal;
        let tty = if fd == 1.0 {
            std::io::stdout().is_terminal()
        } else {
            std::io::stderr().is_terminal()
        };
        if tty {
            stream.set_own("isTTY", Value::Bool(true));
        }
        stream.set_own("writable", Value::Bool(true));
        stream.set_own("fd", Value::Number(fd));
        method(vm, &stream, "on", 2, process_on);
        method(vm, &stream, "once", 2, process_on);
        process.set_own(name, Value::Object(stream));
    }
    let getter = native_fn(vm, "stdin", 0, process_stdin);
    let accessor = crate::object::Obj::new(
        crate::object::ObjKind::Accessor(Box::new(crate::object::Accessor {
            get: std::cell::RefCell::new(getter),
            set: std::cell::RefCell::new(Value::Undefined),
        })),
        None,
    );
    process.set_own("stdin", Value::Object(accessor));
    vm.define_global("process", Value::Object(process));

    let require = native_fn(vm, "require", 1, require);
    if let Value::Object(r) = &require {
        method(vm, r, "resolve", 1, require_resolve);
        let cache = vm.require_cache_object_public();
        r.set_own("cache", Value::Object(cache));
        let main = vm.new_object();
        r.set_own("main", Value::Object(main));
    }
    vm.define_global("require", require);

    let performance = vm.new_object();
    performance.set_with_attrs(
        crate::object::TO_STRING_TAG_KEY,
        "Performance".into(),
        crate::object::NON_ENUM | crate::object::READ_ONLY,
    );
    method(vm, &performance, "now", 0, perf_now);
    vm.define_global("performance", Value::Object(performance));

    let gt = native_fn(vm, "squirt", 0, squirt);
    vm.define_global("squirt", gt);

    let timers: &[(&str, u32, crate::object::NativeFn)] = &[
        ("setTimeout", 2, set_timeout),
        ("setInterval", 2, set_interval),
        ("clearTimeout", 1, clear_timer),
        ("clearInterval", 1, clear_timer),
        ("queueMicrotask", 1, queue_microtask),
        ("sleep", 1, sleep),
        ("%unrefTimer%", 1, unref_timer),
        ("%refTimer%", 1, ref_timer),
    ];
    for (name, arity, f) in timers {
        let v = native_fn(vm, name, *arity, *f);
        vm.define_global(name, v);
    }
}

fn timer(vm: &mut Vm, args: &[Value], repeat: bool) -> R {
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    let ms = match arg(args, 1) {
        Value::Undefined => 0.0,
        v => vm.to_number(&v)?,
    };
    let rest = args.iter().skip(2).cloned().collect();
    Ok(Value::Number(vm.add_timer(f, ms, rest, repeat) as f64))
}

fn set_timeout(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    timer(vm, args, false)
}

fn set_interval(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    timer(vm, args, true)
}

fn clear_timer(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if let Value::Number(n) = arg(args, 0) {
        if n >= 0.0 {
            vm.clear_timer(n as u64);
        }
    }
    Ok(Value::Undefined)
}

fn unref_timer(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if let Value::Number(n) = arg(args, 0) {
        if n >= 0.0 {
            vm.unref_timer(n as u64);
        }
    }
    Ok(Value::Undefined)
}

fn ref_timer(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if let Value::Number(n) = arg(args, 0) {
        if n >= 0.0 {
            vm.ref_timer(n as u64);
        }
    }
    Ok(Value::Undefined)
}

fn queue_microtask(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    vm.queue_microtask(f, Vec::new());
    Ok(Value::Undefined)
}

fn sleep(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let ms = match arg(args, 0) {
        Value::Undefined => 0.0,
        v => vm.to_number(&v)?,
    };
    let p = vm.new_promise();
    let (resolve, _) = vm.resolving_functions(&p);
    vm.add_timer(resolve, ms, Vec::new(), false);
    Ok(Value::Object(p))
}

pub fn format_args(vm: &mut Vm, args: &[Value]) -> Result<String, Value> {
    let mut parts = Vec::new();
    let mut rest = args;
    if let Some(Value::Str(fmt)) = args.first() {
        if fmt.contains('%') {
            let mut out = String::new();
            let mut i = 1;
            let mut chars = fmt.chars().peekable();
            while let Some(c) = chars.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match chars.peek().copied() {
                    Some('%') => {
                        chars.next();
                        out.push('%');
                    }
                    Some('c') if i < args.len() => {
                        chars.next();
                        i += 1;
                    }
                    Some(spec @ ('s' | 'd' | 'i' | 'f' | 'o' | 'O' | 'j')) if i < args.len() => {
                        chars.next();
                        let a = &args[i];
                        i += 1;
                        match spec {
                            's' => match a {
                                Value::Str(s) => out.push_str(s),
                                Value::Object(o) if !o.is_callable() => {
                                    let custom = matches!(
                                        vm.get_prop(a, "toString"),
                                        Ok(Value::Object(f)) if matches!(
                                            f.kind,
                                            crate::object::ObjKind::Function(_)
                                                | crate::object::ObjKind::Bound(_)
                                        )
                                    );
                                    if custom {
                                        let text = vm.to_string(a)?;
                                        out.push_str(&text);
                                    } else {
                                        let options = crate::inspect::InspectOptions {
                                            depth: 0,
                                            ..Default::default()
                                        };
                                        out.push_str(&crate::inspect::inspect_with(vm, a, options));
                                    }
                                }
                                _ => out.push_str(&inspect_plain_vm(vm, a)),
                            },
                            'o' => {
                                let options = crate::inspect::InspectOptions {
                                    depth: 4,
                                    ..Default::default()
                                };
                                out.push_str(&crate::inspect::inspect_with(vm, a, options));
                            }
                            'd' | 'i' if matches!(a, Value::BigInt(_)) => {
                                out.push_str(&inspect_plain(a));
                            }
                            'd' | 'i' => {
                                let n = match a {
                                    Value::Object(_) => f64::NAN,
                                    _ => vm.to_number(a)?,
                                };
                                let n = if spec == 'i' { n.trunc() } else { n };
                                out.push_str(&crate::number::number_to_string(n));
                            }
                            'f' => {
                                let n = match a {
                                    Value::BigInt(_) => inspect_plain(a)
                                        .trim_end_matches('n')
                                        .parse::<f64>()
                                        .unwrap_or(f64::NAN),
                                    Value::Object(_) => f64::NAN,
                                    _ => vm.to_number(a)?,
                                };
                                out.push_str(&crate::number::number_to_string(n));
                            }
                            'j' => match super::json_stringify(vm, a, "")? {
                                Some(s) => out.push_str(&s),
                                None => out.push_str("undefined"),
                            },
                            _ => out.push_str(&inspect_plain_vm(vm, a)),
                        }
                    }
                    _ => out.push('%'),
                }
            }
            parts.push(out);
            rest = &args[i.min(args.len())..];
        }
    }
    for a in rest {
        parts.push(inspect_top_vm(vm, a));
    }
    Ok(parts.join(" "))
}

fn indented(vm: &Vm, s: &str) -> String {
    let pad = "  ".repeat(vm.console_indent);
    let mut out = String::with_capacity(s.len() + pad.len() * 2);
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if !line.is_empty() {
            out.push_str(&pad);
        }
        out.push_str(line);
    }
    out
}

fn console_line(vm: &mut Vm, args: &[Value]) -> Result<String, Value> {
    let s = format_args(vm, args)?;
    let mut s = if vm.console_indent > 0 {
        indented(vm, &s)
    } else {
        s
    };
    s.push('\n');
    Ok(s)
}

fn log(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = console_line(vm, args)?;
    vm.write_out(&s);
    Ok(Value::Undefined)
}

fn error(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let s = console_line(vm, args)?;
    vm.write_err(&s);
    Ok(Value::Undefined)
}

fn group(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if !args.is_empty() {
        let s = console_line(vm, args)?;
        vm.write_out(&s);
    }
    vm.console_indent += 1;
    Ok(Value::Undefined)
}

fn group_end(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    vm.console_indent = vm.console_indent.saturating_sub(1);
    Ok(Value::Undefined)
}

fn assert(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    if !arg(args, 0).truthy() {
        let rest = if args.len() > 1 { &args[1..] } else { &[] };
        let msg = format_args(vm, rest)?;
        let line = if msg.is_empty() {
            "Assertion failed\n".to_string()
        } else {
            format!("Assertion failed: {msg}\n")
        };
        vm.write_err(&line);
    }
    Ok(Value::Undefined)
}

fn caller_module(vm: &Vm) -> std::rc::Rc<crate::bytecode::ModuleInfo> {
    for f in vm.frames.iter().rev() {
        let m = &f.closure.proto.module;
        if !m.name.starts_with("<squirt:") && &*m.name != "<prelude>" {
            return m.clone();
        }
    }
    std::rc::Rc::new(crate::bytecode::ModuleInfo {
        name: "<require>".into(),
        dir: std::env::current_dir().unwrap_or_default(),
        source: "".into(),
        sloppy: true,
    })
}

fn require(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let spec = match arg(args, 0) {
        Value::Str(s) => s,
        other => {
            return Err(vm.type_error(format!(
                "The \"id\" argument must be of type string. Received {}",
                crate::inspect::inspect_plain(&other)
            )));
        }
    };
    if spec.is_empty() {
        return Err(vm.type_error("The argument 'id' must be a non-empty string. Received ''"));
    }
    let from = caller_module(vm);
    vm.require_module(&spec, &from)
}

fn require_resolve(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let spec = vm.to_string(&arg(args, 0))?;
    let bare = !spec.starts_with('.') && !spec.starts_with('/') && !spec.contains(':');
    if spec.starts_with("node:") || spec.starts_with("squirt:") {
        return Ok(Value::Str(spec));
    }
    if bare && vm.import_node(&spec)?.is_some() {
        return Ok(Value::Str(spec));
    }
    let from = caller_module(vm);
    match crate::module::resolve_with(&spec, &from.dir, true) {
        Ok(p) => Ok(Value::Str(p.display().to_string().into())),
        Err(msg) => {
            let e = vm.make_error(ErrorKind::Error, msg);
            if let Value::Object(o) = &e {
                o.set_own("code", "MODULE_NOT_FOUND".into());
            }
            Err(e)
        }
    }
}

fn available_parallelism(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    Ok(Value::Number(n as f64))
}

fn exit(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let code = match arg(args, 0) {
        Value::Undefined => exit_code(vm),
        v => to_integer(vm, &v)? as i32,
    };
    if let Some(Value::Object(p)) = vm.globals.get("process").cloned() {
        p.set_own("exitCode", Value::Number(code as f64));
    }
    if !vm.exiting {
        vm.exiting = true;
        vm.emit_process("exit", &[Value::Number(code as f64)])?;
    }
    let code = exit_code(vm);
    super::netio::flush_sockets(vm);
    vm.flush();
    std::process::exit(code)
}

pub fn exit_code(vm: &mut Vm) -> i32 {
    let Some(process) = vm.globals.get("process").cloned() else {
        return 0;
    };
    match vm.get_prop(&process, "exitCode") {
        Ok(Value::Number(n)) => n as i32,
        Ok(Value::Str(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

fn write_chunk(vm: &mut Vm, args: &[Value]) -> Result<Vec<u8>, Value> {
    let v = arg(args, 0);
    if let Some(b) = super::binary::buffer_source_bytes(&v) {
        return Ok(b);
    }
    let s = vm.to_string(&v)?;
    Ok(crate::value::utf8_text(&s).as_bytes().to_vec())
}

fn call_write_callback(vm: &mut Vm, args: &[Value]) {
    if let Some(cb) = args.iter().skip(1).find(|a| a.is_callable()) {
        vm.queue_microtask(cb.clone(), Vec::new());
    }
}

fn stdout_write(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bytes = write_chunk(vm, args)?;
    let _ = vm.out.write_all(&bytes);
    call_write_callback(vm, args);
    Ok(Value::Bool(true))
}

fn stderr_write(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let bytes = write_chunk(vm, args)?;
    let _ = vm.out.flush();
    let _ = vm.err.write_all(&bytes);
    let _ = vm.err.flush();
    call_write_callback(vm, args);
    Ok(Value::Bool(true))
}

fn process_on(_vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    Ok(this)
}

fn process_uid(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(super::stdlib::os_ids().0))
}

fn process_gid(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(super::stdlib::os_ids().1))
}

fn process_stdin(vm: &mut Vm, this: Value, _args: &[Value]) -> R {
    let node = super::node::load(vm)?;
    let stdin = vm.get_prop(&node, "stdin")?;
    if let Value::Object(p) = &this {
        p.set_own("stdin", stdin.clone());
    }
    Ok(stdin)
}

fn chdir(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = vm.to_string(&arg(args, 0))?;
    std::env::set_current_dir(&*p).map_err(|e| super::node::fs_error(vm, &e, "chdir", &p, None))?;
    Ok(Value::Undefined)
}

fn next_tick(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let f = arg(args, 0);
    expect_fn(vm, &f)?;
    vm.ticks
        .push_back((f, args.get(1..).unwrap_or(&[]).to_vec()));
    Ok(Value::Undefined)
}

fn hrtime(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let t = vm.start.elapsed();
    let mut secs = t.as_secs() as f64;
    let mut nanos = t.subsec_nanos() as f64;
    if let Value::Object(prev) = arg(args, 0) {
        let prev_secs = vm.get_prop(&Value::Object(prev.clone()), "0")?;
        let prev_nanos = vm.get_prop(&Value::Object(prev), "1")?;
        let prev_secs = vm.to_number(&prev_secs)?;
        let prev_nanos = vm.to_number(&prev_nanos)?;
        secs -= prev_secs;
        nanos -= prev_nanos;
        if nanos < 0.0 {
            secs -= 1.0;
            nanos += 1e9;
        }
    }
    Ok(vm.new_array(vec![Value::Number(secs), Value::Number(nanos)]))
}

fn uptime(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(vm.start.elapsed().as_secs_f64()))
}

fn memory_usage(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    let o = vm.new_object();
    for key in ["rss", "heapTotal", "heapUsed", "external", "arrayBuffers"] {
        o.set_own(key, Value::Number(0.0));
    }
    Ok(Value::Object(o))
}

fn cwd(_vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default()
        .into())
}

fn perf_now(vm: &mut Vm, _this: Value, _args: &[Value]) -> R {
    Ok(Value::Number(vm.start.elapsed().as_secs_f64() * 1000.0))
}

fn squirt(_vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let n = match arg(args, 0) {
        Value::Number(n) if n >= 1.0 => (n as usize).min(64),
        _ => 3,
    };
    Ok("\u{1f4a6}".repeat(n).into())
}

fn path_arg(vm: &mut Vm, args: &[Value]) -> Result<String, Value> {
    Ok(vm.to_string(&arg(args, 0))?.to_string())
}

fn io_error(vm: &mut Vm, e: std::io::Error, op: &str, path: &str) -> Value {
    vm.make_error(ErrorKind::Error, format!("{op} '{path}': {e}"))
}

fn fs_read(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_arg(vm, args)?;
    std::fs::read_to_string(&p)
        .map(Value::from)
        .map_err(|e| io_error(vm, e, "readFile", &p))
}

fn fs_write(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_arg(vm, args)?;
    let data = vm.to_string(&arg(args, 1))?;
    std::fs::write(&p, data.as_bytes())
        .map(|_| Value::Undefined)
        .map_err(|e| io_error(vm, e, "writeFile", &p))
}

fn fs_append(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    use std::io::Write;
    let p = path_arg(vm, args)?;
    let data = vm.to_string(&arg(args, 1))?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .and_then(|mut f| f.write_all(data.as_bytes()))
        .map(|_| Value::Undefined)
        .map_err(|e| io_error(vm, e, "appendFile", &p))
}

fn fs_exists(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_arg(vm, args)?;
    Ok(Value::Bool(Path::new(&p).exists()))
}

fn fs_remove(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_arg(vm, args)?;
    let path = Path::new(&p);
    let r = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    r.map(|_| Value::Undefined)
        .map_err(|e| io_error(vm, e, "remove", &p))
}

fn fs_read_dir(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_arg(vm, args)?;
    let entries = std::fs::read_dir(&p).map_err(|e| io_error(vm, e, "readDir", &p))?;
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    let list = names.into_iter().map(Value::from).collect();
    Ok(vm.new_array(list))
}

fn fs_mkdir(vm: &mut Vm, _this: Value, args: &[Value]) -> R {
    let p = path_arg(vm, args)?;
    std::fs::create_dir_all(&p)
        .map(|_| Value::Undefined)
        .map_err(|e| io_error(vm, e, "mkdir", &p))
}

pub fn fs_module(vm: &mut Vm) -> Value {
    let fs = vm.new_object();
    method(vm, &fs, "readFile", 1, fs_read);
    method(vm, &fs, "writeFile", 2, fs_write);
    method(vm, &fs, "appendFile", 2, fs_append);
    method(vm, &fs, "exists", 1, fs_exists);
    method(vm, &fs, "remove", 1, fs_remove);
    method(vm, &fs, "readDir", 1, fs_read_dir);
    method(vm, &fs, "mkdir", 1, fs_mkdir);
    fs.set_hidden("default", Value::Object(fs.clone()));
    Value::Object(fs)
}
