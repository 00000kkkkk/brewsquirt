use clap::{Parser, Subcommand};
use javasquirt::Vm;
use javasquirt::module::MANIFEST;
use javasquirt::pm;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(
    name = "vrotdaem",
    version,
    about = "The JavaSquirt package manager. Injects dependencies directly into your nude_modules"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Create squirt.json and index.jsq in the current directory")]
    Init {
        #[arg(long)]
        name: Option<String>,
    },
    #[command(
        about = "Add a dependency and inject it: a package from npm (lodash, lodash@^4), npm:<name>@<range>, file:<path> or git:<url>[#ref]"
    )]
    Add { name: String, spec: Option<String> },
    #[command(about = "Remove a dependency from squirt.json and nude_modules")]
    Remove { name: String },
    #[command(
        about = "Allow the install scripts of these packages, record them in trustedDependencies and run them now"
    )]
    Trust {
        #[arg(required = true)]
        names: Vec<String>,
    },
    #[command(about = "Inject every dependency deep into nude_modules")]
    Gluboko {
        #[arg(long, help = "Wipe nude_modules and inject everything again")]
        force: bool,
    },
    #[command(about = "Run the main script from squirt.json")]
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    #[command(about = "Run every *.test.jsq file in the package")]
    Test {
        #[arg(
            short = 't',
            long,
            help = "Only run tests whose name contains this text"
        )]
        filter: Option<String>,
        paths: Vec<String>,
    },
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn root() -> Result<PathBuf, String> {
    pm::find_root(&cwd()).ok_or_else(|| {
        format!("no {MANIFEST} found here or in any parent directory, run `vrotdaem init` first")
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli.command) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("vrotdaem: {e}");
            ExitCode::FAILURE
        }
    }
}

fn install(root: &Path, force: bool) -> Result<(), String> {
    let report = pm::gluboko(root, force, |line| println!("\u{1f4a6} {line}"))?;
    for name in &report.skipped {
        println!("   {name} is already deep inside, use --force to go again");
    }
    for name in &report.untrusted {
        println!(
            "   {name} has install scripts that were not run, `vrotdaem trust {name}` runs them"
        );
    }
    let n = report.injected.len();
    println!(
        "done: {n} dependenc{} injected deep into nude_modules",
        if n == 1 { "y" } else { "ies" }
    );
    Ok(())
}

fn execute(command: Command) -> Result<ExitCode, String> {
    match command {
        Command::Init { name } => {
            let m = pm::init(&cwd(), name)?;
            println!("created {MANIFEST} for '{}', main is {}", m.name, m.main);
        }
        Command::Add { name, spec } => {
            let root = root()?;
            let before = pm::read_manifest(&root)?;
            let (dep, stored) = pm::add(&root, &name, spec.as_deref())?;
            println!("added {dep} -> {stored}");
            if let Err(e) = install(&root, false) {
                pm::write_manifest(&root, &before)?;
                return Err(format!("{e}\n{dep} was not added to {MANIFEST}"));
            }
        }
        Command::Remove { name } => {
            let root = root()?;
            if pm::remove(&root, &name)? {
                println!("pulled {name} out of nude_modules");
            } else {
                println!("{name} was not a dependency");
            }
        }
        Command::Gluboko { force } => {
            let root = root()?;
            install(&root, force)?;
        }
        Command::Trust { names } => {
            let root = root()?;
            let ran = pm::trust(&root, &names, |line| println!("\u{1f4a6} {line}"))?;
            println!(
                "trusted {}, {ran} install script{} ran",
                names.join(", "),
                if ran == 1 { "" } else { "s" }
            );
        }
        Command::Run { args } => {
            let root = root()?;
            let manifest = pm::read_manifest(&root)?;
            let main = root.join(&manifest.main);
            let handle = std::thread::Builder::new()
                .stack_size(256 * 1024 * 1024)
                .spawn(move || {
                    let mut argv = vec!["jsq".to_string(), main.display().to_string()];
                    argv.extend(args);
                    let mut vm = Vm::new(argv);
                    let r = vm.run_file(&main).map(|_| ());
                    let r = vm.finish_main(r);
                    vm.flush();
                    match r {
                        Ok(_) => match javasquirt::builtins::exit_code(&mut vm) {
                            0 => ExitCode::SUCCESS,
                            n => ExitCode::from(n as u8),
                        },
                        Err(e) => {
                            eprintln!("{}", vm.format_uncaught(&e));
                            ExitCode::FAILURE
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            return Ok(handle.join().unwrap_or(ExitCode::FAILURE));
        }
        Command::Test { filter, paths } => {
            let root = root()?;
            std::env::set_current_dir(&root).map_err(|e| e.to_string())?;
            let handle = std::thread::Builder::new()
                .stack_size(256 * 1024 * 1024)
                .spawn(move || {
                    if javasquirt::runner::run_tests(&paths, filter.as_deref()).ok() {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                })
                .map_err(|e| e.to_string())?;
            return Ok(handle.join().unwrap_or(ExitCode::FAILURE));
        }
    }
    Ok(ExitCode::SUCCESS)
}
