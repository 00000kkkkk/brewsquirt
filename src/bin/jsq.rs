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
    #[arg(long, requires = "file")]
    disasm: bool,
    file: Option<PathBuf>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run(cli))
        .expect("spawn interpreter thread");
    handle.join().unwrap_or(ExitCode::FAILURE)
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

fn run(cli: Cli) -> ExitCode {
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

    if let Some(code) = &cli.eval {
        let r = vm
            .eval("<eval>", code, Mode::Repl)
            .and_then(|v| vm.run_event_loop().map(|_| v));
        return finish(
            &mut vm,
            r.map(|v| {
                if !v.is_undefined() {
                    println_value(&v)
                } else {
                    None
                }
            }),
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
        let r = vm.run_file(file).and_then(|_| vm.run_event_loop());
        return finish(&mut vm, r.map(|_| None));
    }

    repl(&mut vm)
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
