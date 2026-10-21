use clap::{Parser, Subcommand};
use javasquirt::Vm;
use javasquirt::module::MANIFEST;
use javasquirt::pm;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
    #[command(about = "Add a dependency (file:<path> or git:<url>[#ref]) and inject it")]
    Add { name: String, spec: String },
    #[command(about = "Remove a dependency from squirt.json and nude_modules")]
    Remove { name: String },
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
            pm::add(&root, &name, &spec)?;
            println!("added {name} -> {spec}");
            install(&root, false)?;
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
                    let r = vm.run_file(&main).and_then(|_| vm.run_event_loop());
                    vm.flush();
                    match r {
                        Ok(_) => ExitCode::SUCCESS,
                        Err(e) => {
                            eprintln!("{}", vm.format_uncaught(&e));
                            ExitCode::FAILURE
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            return Ok(handle.join().unwrap_or(ExitCode::FAILURE));
        }
    }
    Ok(ExitCode::SUCCESS)
}
