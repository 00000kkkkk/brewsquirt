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
