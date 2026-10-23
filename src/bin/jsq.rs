use clap::Parser;
use javasquirt::Vm;
use javasquirt::compiler::Mode;
use javasquirt::inspect::inspect_plain;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(
    name = "jsq",
    version,
    about = "JavaSquirt: JS semantics, Rust speed",
    after_help = "Run without arguments to open the REPL.
Run `jsq test [paths] [--filter text]` to run *.test.jsq files."
)]
struct Cli {
    #[arg(
        short = 'e',
        long = "eval",
        value_name = "CODE",
        conflicts_with = "file"
    )]
    eval: Option<String>,
    #[arg(
        short = 'p',
        long = "print",
        value_name = "CODE",
        conflicts_with_all = ["file", "eval"]
    )]
    print: Option<String>,
    #[arg(long, requires = "file")]
    disasm: bool,
    #[arg(long, requires = "file", hide = true)]
    script: bool,
    #[arg(long, requires = "file", hide = true)]
    prelude: Option<PathBuf>,
    file: Option<PathBuf>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Default)]
struct NodeFlags {
    exec_argv: Vec<String>,
    require: Vec<String>,
    import: Vec<String>,
    env_files: Vec<(String, bool)>,
    test: bool,
}

fn split_node_flags(raw: Vec<String>) -> (Vec<String>, NodeFlags) {
    let mut flags = NodeFlags::default();
    let mut out = Vec::new();
    let mut it = raw.into_iter();
    if let Some(first) = it.next() {
        out.push(first);
    }
    let rest: Vec<String> = it.collect();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if !a.starts_with('-') || a == "-" {
            break;
        }
        if a == "--" {
            i += 1;
            break;
        }
        let next = rest.get(i + 1).cloned();
        match a {
            "-e" | "--eval" | "-p" | "--print" | "--prelude" => {
                out.push(rest[i].clone());
                if let Some(v) = next {
                    out.push(v);
                }
                i += 2;
                continue;
            }
            "--disasm" | "--script" | "-h" | "--help" | "-V" | "--version" => {
                out.push(rest[i].clone());
                i += 1;
                continue;
            }
            "-v" => {
                out.push("--version".into());
                i += 1;
                continue;
            }
            "--test" => {
                flags.test = true;
                flags.exec_argv.push(rest[i].clone());
                i += 1;
                continue;
            }
            "-r" | "--require" | "--import" | "--env-file" | "--env-file-if-exists" => {
                if let Some(v) = next.clone() {
                    match a {
                        "-r" | "--require" => flags.require.push(v),
                        "--import" => flags.import.push(v),
                        "--env-file" => flags.env_files.push((v, true)),
                        _ => flags.env_files.push((v, false)),
                    }
                }
                flags.exec_argv.push(rest[i].clone());
                flags.exec_argv.extend(next);
                i += 2;
                continue;
            }
            "-C"
            | "--conditions"
            | "--loader"
            | "--experimental-loader"
            | "--title"
            | "--inspect-port"
            | "--input-type"
            | "--stack-trace-limit" => {
                flags.exec_argv.push(rest[i].clone());
                flags.exec_argv.extend(next);
                i += 2;
                continue;
            }
            _ => {}
        }
        if let Some((name, value)) = a.split_once('=') {
            match name {
                "--require" => flags.require.push(value.to_string()),
                "--import" => flags.import.push(value.to_string()),
                "--env-file" => flags.env_files.push((value.to_string(), true)),
                "--env-file-if-exists" => flags.env_files.push((value.to_string(), false)),
                _ => {}
            }
            flags.exec_argv.push(rest[i].clone());
            i += 1;
            continue;
        }
        if a.starts_with("--") {
            flags.exec_argv.push(rest[i].clone());
            i += 1;
            continue;
        }
        break;
    }
    out.extend(rest[i..].iter().cloned());
    (out, flags)
}

fn load_env_file(path: &str, required: bool) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) if !required => return Ok(()),
        Err(e) => return Err(format!("{path}: {e}")),
    };
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim().to_string();
        let quote = value
            .chars()
            .next()
            .filter(|c| matches!(c, '"' | '\'' | '`'));
        if let Some(q) = quote {
            while !(value.len() > 1 && value[1..].contains(q)) {
                match lines.next() {
                    Some(more) => {
                        value.push('\n');
                        value.push_str(more);
                    }
                    None => break,
                }
            }
        }
        let value = match quote {
            Some(q) => {
                let body = &value[1..];
                let inner = body.rfind(q).map(|end| &body[..end]).unwrap_or(body);
                if q == '"' {
                    inner.replace("\\n", "\n")
                } else {
                    inner.to_string()
                }
            }
            None => value.split(" #").next().unwrap_or("").trim().to_string(),
        };
        if !key.is_empty() && std::env::var_os(key).is_none() {
            unsafe { std::env::set_var(key, value) };
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let (args, flags) = split_node_flags(std::env::args().collect());
    for (path, required) in &flags.env_files {
        if let Err(e) = load_env_file(path, *required) {
            eprintln!("jsq: {e}");
            return ExitCode::from(9);
        }
    }
    if flags.test {
        return javasquirt::runner::node_test_command(&args[1..], &flags.exec_argv);
    }
    let cli = Cli::parse_from(args);
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run(cli, flags))
        .expect("spawn interpreter thread");
    handle.join().unwrap_or(ExitCode::FAILURE)
}

fn preload(vm: &mut Vm, flags: &NodeFlags) -> Result<(), javasquirt::Value> {
    let mut exec_argv = Vec::new();
    for a in &flags.exec_argv {
        exec_argv.push(javasquirt::Value::from(a.as_str()));
    }
    let list = vm.new_array(exec_argv);
    if let Some(javasquirt::Value::Object(p)) = vm.globals.get("process").cloned() {
        p.set_own("execArgv", list);
    }
    for spec in &flags.require {
        let code = format!("require({})", serde_json::Value::String(spec.clone()));
        vm.eval("<preload>", &code, Mode::Repl)?;
    }
    for spec in &flags.import {
        let target = if spec.starts_with('.') {
            std::env::current_dir()
                .map(|d| d.join(spec).display().to_string())
                .unwrap_or_else(|_| spec.clone())
        } else {
            spec.clone()
        };
        let code = format!("import({})", serde_json::Value::String(target));
        let promise = vm.eval("<preload>", &code, Mode::Repl)?;
        vm.await_value(&promise)?;
    }
    Ok(())
}

fn test_command(args: &[String]) -> ExitCode {
    let mut paths = Vec::new();
    let mut filter = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-t" | "--filter" => filter = it.next().cloned(),
            _ => paths.push(a.clone()),
        }
    }
    let summary = javasquirt::runner::run_tests(&paths, filter.as_deref());
    if summary.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run(cli: Cli, flags: NodeFlags) -> ExitCode {
    if let Some(f) = &cli.file {
        if f.as_os_str() == "test" && !f.is_file() {
            return test_command(&cli.args);
        }
    }
    let mut argv = vec!["jsq".to_string()];
    if let Some(f) = &cli.file {
        argv.push(f.display().to_string());
    }
    argv.extend(cli.args.iter().cloned());
    let mut vm = Vm::new(argv);
    if let Err(e) = preload(&mut vm, &flags) {
        return finish(&mut vm, Err(e));
    }

    if let Some(code) = cli.eval.as_ref().or(cli.print.as_ref()) {
        let show = cli.print.is_some();
        let r = vm.eval("<eval>", code, Mode::Repl);
        let r = complete(&mut vm, r);
        return finish(
            &mut vm,
            r.map(|v| if show { println_value(&v) } else { None }),
        );
    }

    if let Some(file) = &cli.file {
        if cli.disasm {
            let src = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("jsq: cannot read {}: {e}", file.display());
                    return ExitCode::FAILURE;
                }
            };
            let dir = file.parent().map(PathBuf::from).unwrap_or_default();
            return match vm.compile(&src, &file.display().to_string(), dir, Mode::Module) {
                Ok(proto) => {
                    print!("{}", proto.disassemble());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("SyntaxError: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        if let Some(pre) = &cli.prelude {
            let src = match std::fs::read_to_string(pre) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("jsq: cannot read {}: {e}", pre.display());
                    return ExitCode::FAILURE;
                }
            };
            let name = pre.display().to_string();
            if let Err(e) = vm.run_script(&src, &name) {
                return finish(&mut vm, Err(e));
            }
        }
        if cli.script {
            let src = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("jsq: cannot read {}: {e}", file.display());
                    return ExitCode::FAILURE;
                }
            };
            let name = file.display().to_string();
            let r = vm.run_script(&src, &name);
            let r = complete(&mut vm, r);
            return finish(&mut vm, r.map(|_| None));
        }
        let r = vm.run_file(file);
        let r = complete(&mut vm, r);
        return finish(&mut vm, r.map(|_| None));
    }

    repl(&mut vm)
}

fn complete(
    vm: &mut Vm,
    r: Result<javasquirt::Value, javasquirt::Value>,
) -> Result<javasquirt::Value, javasquirt::Value> {
    match r {
        Ok(v) => vm.finish_main(Ok(())).map(|_| v),
        Err(e) => vm.finish_main(Err(e)).map(|_| javasquirt::Value::Undefined),
    }
}

fn println_value(v: &javasquirt::Value) -> Option<String> {
    Some(inspect_plain(v))
}

fn finish(vm: &mut Vm, r: Result<Option<String>, javasquirt::Value>) -> ExitCode {
    match r {
        Ok(text) => {
            if let Some(t) = text {
                vm.write_out(&t);
                vm.write_out("\n");
            }
            vm.flush();
            match javasquirt::builtins::exit_code(vm) {
                0 => ExitCode::SUCCESS,
                n => ExitCode::from(n as u8),
            }
        }
        Err(e) => {
            let msg = vm.format_uncaught(&e);
            vm.flush();
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

fn repl(vm: &mut Vm) -> ExitCode {
    println!(
        "JavaSquirt {} \u{1f4a6} type .exit to leave",
        javasquirt::VERSION
    );
    let stdin = std::io::stdin();
    let mut buffer = String::new();
    loop {
        print!("{}", if buffer.is_empty() { "jsq> " } else { "...  " });
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let trimmed = line.trim();
        if buffer.is_empty() && (trimmed == ".exit" || trimmed == ".quit") {
            break;
        }
        buffer.push_str(&line);
        let dir = std::env::current_dir().unwrap_or_default();
        match vm.compile(&buffer, "<repl>", dir, Mode::Repl) {
            Err(e) if e.is_eof() => continue,
            Err(e) => {
                vm.flush();
                eprintln!("Uncaught SyntaxError: {}", e.message);
            }
            Ok(proto) => {
                let proto_async = proto.is_async;
                let r = vm.run_proto(proto).and_then(|v| {
                    let v = if proto_async { vm.await_value(&v)? } else { v };
                    vm.run_event_loop().map(|_| v)
                });
                vm.flush();
                match r {
                    Ok(v) => {
                        if !v.is_undefined() {
                            println!("{}", inspect_plain(&v));
                        }
                    }
                    Err(e) => {
                        eprintln!("{}", vm.format_uncaught(&e));
                        vm.rejection = false;
                    }
                }
            }
        }
        buffer.clear();
    }
    vm.flush();
    ExitCode::SUCCESS
}
