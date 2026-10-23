use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn jsq() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jsq"))
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn normalize(s: &[u8]) -> String {
    String::from_utf8_lossy(s).replace("\r\n", "\n")
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    jsq().current_dir(dir).args(args).output().expect("run jsq")
}

#[test]
fn scripts_match_expected_output() {
    let dir = root().join("tests").join("scripts");
    let mut failures = Vec::new();
    let mut count = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("scripts dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsq"))
        .collect();
    entries.sort();
    for script in entries {
        count += 1;
        let expected_path = script.with_extension("out");
        let expected = std::fs::read(&expected_path)
            .map(|b| normalize(&b))
            .unwrap_or_else(|_| panic!("missing {}", expected_path.display()));
        let name = script.file_name().unwrap().to_string_lossy().to_string();
        let out = run_in(&dir, &[&name]);
        let actual = normalize(&out.stdout);
        if !out.status.success() || actual != expected {
            failures.push(format!(
                "{name}\n--- expected\n{expected}\n--- actual\n{actual}\n--- stderr\n{}",
                normalize(&out.stderr)
            ));
        }
    }
    assert!(count > 0, "no scripts found");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn modules_resolve_and_link() {
    let dir = root().join("tests").join("modules");
    let out = run_in(&dir, &["main.jsq"]);
    assert!(out.status.success(), "{}", normalize(&out.stderr));
    let expected = normalize(&std::fs::read(dir.join("main.out")).expect("main.out"));
    assert_eq!(normalize(&out.stdout), expected);
}

#[test]
fn print_shows_expression_value() {
    let out = run_in(&root(), &["-p", "[1, 2, 3].map((x) => x * 2)"]);
    assert!(out.status.success());
    assert_eq!(normalize(&out.stdout), "[ 2, 4, 6 ]\n");
}

#[test]
fn eval_is_silent() {
    let out = run_in(&root(), &["-e", "[1, 2, 3].map((x) => x * 2)"]);
    assert!(out.status.success());
    assert_eq!(normalize(&out.stdout), "");
}

#[test]
fn uncaught_error_reports_and_fails() {
    let out = run_in(
        &root(),
        &[
            "-e",
            "function boom() { throw new TypeError('nope') }\nboom()",
        ],
    );
    assert!(!out.status.success());
    let err = normalize(&out.stderr);
    assert!(err.starts_with("Uncaught TypeError: nope"), "{err}");
    assert!(err.contains("at boom (<eval>:1)"), "{err}");
    assert!(err.contains("at <main> (<eval>:2)"), "{err}");
}

#[test]
fn syntax_error_has_position() {
    let out = run_in(&root(), &["-e", "let x = (1 + "]);
    assert!(!out.status.success());
    let err = normalize(&out.stderr);
    assert!(err.contains("SyntaxError"), "{err}");
}

#[test]
fn script_arguments_reach_process_argv() {
    let dir = std::env::temp_dir().join(format!("jsq-argv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("args.jsq"),
        "console.log(process.argv.slice(2).join(','))\n",
    )
    .unwrap();
    let out = run_in(&dir, &["args.jsq", "deep", "--force"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "{}", normalize(&out.stderr));
    assert_eq!(normalize(&out.stdout), "deep,--force\n");
}

#[test]
fn disasm_dumps_bytecode() {
    let dir = root().join("benches");
    let out = run_in(&dir, &["--disasm", "fib.jsq"]);
    assert!(out.status.success());
    let text = normalize(&out.stdout);
    assert!(text.contains("== <main>"), "{text}");
    assert!(text.contains("== fib"), "{text}");
    assert!(text.contains("Call"), "{text}");
}

#[test]
fn deep_recursion_is_a_range_error() {
    let out = run_in(
        &root(),
        &[
            "-p",
            "function r() { return r() }\nlet n\ntry { r() } catch (e) { n = e.name }\nn",
        ],
    );
    assert!(out.status.success(), "{}", normalize(&out.stderr));
    assert_eq!(normalize(&out.stdout), "'RangeError'\n");
}

#[test]
fn unhandled_rejection_fails_the_process() {
    let out = run_in(
        &root(),
        &[
            "-e",
            "async function f() { await null; throw new TypeError('lost') }
f()",
        ],
    );
    assert!(!out.status.success());
    let err = normalize(&out.stderr);
    assert!(
        err.starts_with("Uncaught (in promise) TypeError: lost"),
        "{err}"
    );
}

#[test]
fn rejected_entry_module_is_uncaught() {
    let out = run_in(&root(), &["tests/scripts/fixtures/main_tla_reject.mjs"]);
    assert!(!out.status.success());
    assert_eq!(normalize(&out.stdout), "");
    let err = normalize(&out.stderr);
    assert!(err.starts_with("Uncaught TypeError: entry failed"), "{err}");
}

#[test]
fn top_level_await_in_eval() {
    let out = run_in(
        &root(),
        &[
            "-p",
            "const v = await sleep(5).then(() => 41)
v + 1",
        ],
    );
    assert!(out.status.success(), "{}", normalize(&out.stderr));
    assert_eq!(
        normalize(&out.stdout),
        "42
"
    );
}

#[test]
fn test_runner_reports_results() {
    let dir = root().join("tests").join("testing");
    let out = run_in(&dir, &["test", "pass.test.jsq"]);
    let stdout = normalize(&out.stdout);
    assert!(out.status.success(), "{stdout}{}", normalize(&out.stderr));
    assert!(stdout.contains("\u{2713} math > adds"), "{stdout}");
    assert!(
        stdout.contains("tests: 7 passed, 1 skipped, 1 todo (1 file"),
        "{stdout}"
    );

    let out = run_in(&dir, &["test"]);
    let stdout = normalize(&out.stdout);
    assert!(!out.status.success(), "{stdout}");
    assert!(stdout.contains("\u{2717} bad equality"), "{stdout}");
    assert!(stdout.contains("received: { a: 1 }"), "{stdout}");
    assert!(stdout.contains("8 passed, 3 failed"), "{stdout}");

    let out = run_in(&dir, &["test", "--filter", "objects"]);
    let stdout = normalize(&out.stdout);
    assert!(stdout.contains("tests: 2 passed (2 files"), "{stdout}");
}

#[test]
fn stdin_readline_and_exit_code() {
    use std::io::Write;
    let mut child = jsq()
        .current_dir(root())
        .args([
            "-e",
            "import readline from \"node:readline\"\nconst rl = readline.createInterface({ input: process.stdin })\nlet n = 0\nfor await (const line of rl) { n++; console.log(n, line.toUpperCase()) }\nvoid (process.exitCode = n)",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn jsq");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"one\r\ntwo\nthree")
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait jsq");
    assert_eq!(normalize(&out.stdout), "1 ONE\n2 TWO\n3 THREE\n");
    assert_eq!(out.status.code(), Some(3));
}
