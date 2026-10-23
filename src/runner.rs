use crate::module::{EXT, display_path};
use crate::value::Value;
use crate::vm::Vm;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Default)]
pub struct Summary {
    pub files: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub todo: usize,
    pub broken: usize,
}

impl Summary {
    pub fn ok(&self) -> bool {
        self.failed == 0 && self.broken == 0
    }
}

fn is_test_file(p: &Path) -> bool {
    let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(stem) = name.strip_suffix(&format!(".{EXT}")) else {
        return false;
    };
    stem.ends_with(".test") || stem.ends_with("_test") || stem.ends_with(".spec")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if name.starts_with('.') || matches!(name, "target" | "node_modules" | "nude_modules") {
                continue;
            }
            walk(&p, out);
        } else if is_test_file(&p) {
            out.push(p);
        }
    }
}

pub fn discover(paths: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if paths.is_empty() {
        walk(Path::new("."), &mut out);
        return out
            .into_iter()
            .map(|p| p.strip_prefix(".").map(Path::to_path_buf).unwrap_or(p))
            .collect();
    }
    for p in paths {
        let p = PathBuf::from(p);
        if p.is_dir() {
            walk(&p, &mut out);
        } else {
            out.push(p);
        }
    }
    out
}

fn count(vm: &mut Vm, v: &Value, key: &str) -> usize {
    vm.get_prop(v, key)
        .ok()
        .and_then(|n| vm.to_number(&n).ok())
        .unwrap_or(0.0) as usize
}

fn run_file(path: &Path, filter: Option<&str>, total: &mut Summary) {
    let name = display_path(path)
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    let mut vm = Vm::new(vec!["jsq".to_string(), name.clone()]);
    vm.test_harness = true;
    let loaded = vm.run_file(path).and_then(|_| vm.run_event_loop());
    if let Err(e) = loaded {
        let msg = vm.format_uncaught(&e);
        vm.write_out(&format!("{name}\n  \u{2717} failed to load\n"));
        for line in msg.lines() {
            vm.write_out(&format!("      {line}\n"));
        }
        vm.flush();
        total.broken += 1;
        return;
    }
    let result = vm.import_builtin("test").and_then(|module| {
        let run = vm.get_prop(&module, "run")?;
        let filter = filter.map_or(Value::Undefined, |f| f.into());
        let p = vm.call_value(&run, Value::Undefined, &[name.as_str().into(), filter])?;
        let r = vm.await_value(&p)?;
        vm.run_event_loop()?;
        Ok(r)
    });
    match result {
        Ok(r) => {
            total.passed += count(&mut vm, &r, "passed");
            total.failed += count(&mut vm, &r, "failed");
            total.skipped += count(&mut vm, &r, "skipped");
            total.todo += count(&mut vm, &r, "todo");
        }
        Err(e) => {
            let msg = vm.format_uncaught(&e);
            vm.write_out(&format!("  \u{2717} {msg}\n"));
            total.broken += 1;
        }
    }
    vm.flush();
}

pub fn run_tests(paths: &[String], filter: Option<&str>) -> Summary {
    let started = Instant::now();
    let files = discover(paths);
    let mut total = Summary {
        files: files.len(),
        ..Summary::default()
    };
    for f in &files {
        run_file(f, filter, &mut total);
    }
    let mut parts = vec![format!("{} passed", total.passed)];
    if total.failed > 0 {
        parts.push(format!("{} failed", total.failed));
    }
    if total.skipped > 0 {
        parts.push(format!("{} skipped", total.skipped));
    }
    if total.todo > 0 {
        parts.push(format!("{} todo", total.todo));
    }
    if total.broken > 0 {
        parts.push(format!("{} broken files", total.broken));
    }
    println!();
    println!(
        "tests: {} ({} file{}, {}ms)",
        parts.join(", "),
        total.files,
        if total.files == 1 { "" } else { "s" },
        started.elapsed().as_millis()
    );
    total
}

const NODE_TEST_EXTENSIONS: [&str; 6] = ["js", "mjs", "cjs", "ts", "mts", "cts"];

fn is_node_test_file(path: &Path, in_test_dir: bool) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    if !NODE_TEST_EXTENSIONS.contains(&ext) {
        return false;
    }
    if in_test_dir {
        return true;
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    stem == "test"
        || stem.ends_with(".test")
        || stem.ends_with("-test")
        || stem.ends_with("_test")
        || stem.starts_with("test-")
}

fn walk_node_tests(dir: &Path, in_test_dir: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if name == "node_modules" || name == "nude_modules" || name.starts_with('.') {
                continue;
            }
            walk_node_tests(&p, in_test_dir || name == "test", out);
        } else if is_node_test_file(&p, in_test_dir) {
            out.push(p);
        }
    }
}

#[derive(Default)]
struct NodeTestTotals {
    stats: [u64; 7],
    diagnostics: Vec<String>,
    failures: Vec<Vec<String>>,
}

const NODE_TEST_KEYS: [&str; 7] = [
    "tests",
    "suites",
    "pass",
    "fail",
    "cancelled",
    "skipped",
    "todo",
];

fn run_node_test_file(path: &Path, exec_argv: &[String], totals: &mut NodeTestTotals) {
    use std::io::{BufRead, Write};
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("jsq"));
    let started = Instant::now();
    let child = std::process::Command::new(exe)
        .args(exec_argv.iter().filter(|a| a.as_str() != "--test"))
        .arg(path)
        .env("JSQ_TEST_CHILD", "1")
        .stdout(std::process::Stdio::piped())
        .spawn();
    let display = path.display().to_string();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            totals.stats[0] += 1;
            totals.stats[3] += 1;
            totals.failures.push(vec![
                format!("test at {display}"),
                format!("\u{2716} {display}"),
                format!("  {e}"),
            ]);
            return;
        }
    };
    let mut reported = false;
    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        let mut out = std::io::stdout();
        for line in reader.lines().map_while(Result::ok) {
            if let Some(json) = line.strip_prefix("\u{1}jsq-test-report ") {
                if let Ok(report) = serde_json::from_str::<serde_json::Value>(json) {
                    reported = true;
                    for (i, key) in NODE_TEST_KEYS.iter().enumerate() {
                        totals.stats[i] += report["stats"][key].as_u64().unwrap_or(0);
                    }
                    for d in report["diagnostics"].as_array().into_iter().flatten() {
                        totals
                            .diagnostics
                            .push(d.as_str().unwrap_or("").to_string());
                    }
                    for f in report["failures"].as_array().into_iter().flatten() {
                        totals.failures.push(
                            f.as_array()
                                .into_iter()
                                .flatten()
                                .map(|l| l.as_str().unwrap_or("").to_string())
                                .collect(),
                        );
                    }
                }
                continue;
            }
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }
    let status = child.wait();
    let ok = status.as_ref().is_ok_and(|s| s.success());
    if !reported && !ok {
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let code = status.ok().and_then(|s| s.code()).unwrap_or(1);
        println!("\u{2716} {display} ({ms:.4}ms)");
        totals.stats[0] += 1;
        totals.stats[3] += 1;
        totals.failures.push(vec![
            format!("test at {display}"),
            format!("\u{2716} {display} ({ms:.4}ms)"),
            format!("  'test failed with exit code {code}'"),
        ]);
    }
}

pub fn node_test_command(args: &[String], exec_argv: &[String]) -> std::process::ExitCode {
    let started = Instant::now();
    let mut files = Vec::new();
    let patterns: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if patterns.is_empty() {
        walk_node_tests(Path::new("."), false, &mut files);
    } else {
        for p in patterns {
            let path = PathBuf::from(p);
            if path.is_dir() {
                walk_node_tests(&path, false, &mut files);
            } else {
                files.push(path);
            }
        }
    }
    let files: Vec<PathBuf> = files
        .into_iter()
        .map(|f| f.strip_prefix(".").map(Path::to_path_buf).unwrap_or(f))
        .collect();
    let mut totals = NodeTestTotals::default();
    for f in &files {
        run_node_test_file(f, exec_argv, &mut totals);
    }
    for d in &totals.diagnostics {
        println!("\u{2139} {d}");
    }
    for (i, key) in NODE_TEST_KEYS.iter().enumerate() {
        println!("\u{2139} {key} {}", totals.stats[i]);
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    println!("\u{2139} duration_ms {}", (ms * 1000.0).round() / 1000.0);
    if !totals.failures.is_empty() {
        println!();
        println!("\u{2716} failing tests:");
        for f in &totals.failures {
            println!();
            for line in f {
                println!("{line}");
            }
        }
    }
    if totals.stats[3] > 0 {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
