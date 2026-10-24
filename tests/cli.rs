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
        .filter(|p| {
            p.extension()
                .is_some_and(|e| matches!(e.to_str(), Some("jsq" | "ts" | "tsx" | "jsx")))
        })
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
    assert!(err.contains("at boom (<eval>:1:25)"), "{err}");
    assert!(err.contains("at <eval>:2:1"), "{err}");
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

#[test]
fn node_flags_preload_and_env_file() {
    let dir = std::env::temp_dir().join(format!("jsq-node-flags-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("pre.cjs"), "globalThis.order = [\"require\"]\n").unwrap();
    std::fs::write(dir.join("pre.mjs"), "globalThis.order.push(\"import\")\n").unwrap();
    std::fs::write(
        dir.join(".env"),
        "GREETING=hello\nexport QUOTED=\"two\nlines\"\n# skipped\nSINGLE='raw $x'\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.js"),
        "console.log(order.join(\",\"), process.env.GREETING, JSON.stringify(process.env.QUOTED), process.env.SINGLE)\nconsole.log(JSON.stringify(process.execArgv), JSON.stringify(process.argv.slice(2)))\n",
    )
    .unwrap();
    let out = run_in(
        &dir,
        &[
            "--no-warnings",
            "-r",
            "./pre.cjs",
            "--import=./pre.mjs",
            "--env-file",
            ".env",
            "--env-file-if-exists=missing.env",
            "main.js",
            "--flag",
            "x",
        ],
    );
    assert!(out.status.success(), "{}", normalize(&out.stderr));
    assert_eq!(
        normalize(&out.stdout),
        "require,import hello \"two\\nlines\" raw $x\n[\"--no-warnings\",\"-r\",\"./pre.cjs\",\"--import=./pre.mjs\",\"--env-file\",\".env\",\"--env-file-if-exists=missing.env\"] [\"--flag\",\"x\"]\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn mask_test_report(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("at ") {
            continue;
        }
        let mut out = String::new();
        let mut rest = line;
        while let Some(i) = rest.find('(') {
            out.push_str(&rest[..i]);
            let tail = &rest[i + 1..];
            let digits = tail
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(tail.len());
            if digits > 0 && tail[digits..].starts_with("ms)") {
                out.push_str("(Xms)");
                rest = &tail[digits + 3..];
            } else {
                out.push('(');
                rest = tail;
            }
        }
        out.push_str(rest);
        if let Some(i) = out.find("duration_ms ") {
            out.truncate(i + "duration_ms ".len());
            out.push('X');
        }
        lines.push(out);
    }
    lines.join("\n")
}

#[test]
fn node_test_runner_reports_like_node() {
    let dir = std::env::temp_dir().join(format!("jsq-node-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fail.test.mjs"), "import { test, describe, it, before, after, beforeEach, afterEach } from \"node:test\"\nimport assert from \"node:assert/strict\"\nconst log = []\nbefore(() => log.push(\"before\"))\ndescribe(\"outer\", () => {\n  beforeEach(() => log.push(\"each\"))\n  it(\"ok\", () => {})\n  it(\"bad\", () => {\n    assert.equal(1, 2)\n  })\n  describe(\"inner\", () => {\n    it(\"deep\", async () => {})\n  })\n})\ntest(\"with subtests\", async (t) => {\n  await t.test(\"child one\", () => {})\n  await t.test(\"child two\", () => {\n    throw new Error(\"boom\")\n  })\n})\ntest(\"done style\", (t, done) => {\n  setTimeout(done, 1)\n})\ntest(\"skip via ctx\", (t) => {\n  t.skip(\"not now\")\n})\ntest(\"todo option\", { todo: true }, () => {\n  throw new Error(\"ignored\")\n})\nafter(() => console.log(\"log\", log.join(\",\")))\n").unwrap();
    let out = run_in(&dir, &["fail.test.mjs"]);
    assert_eq!(out.status.code(), Some(1), "{}", normalize(&out.stderr));
    assert_eq!(
        mask_test_report(&normalize(&out.stdout)),
        "▶ outer\n  ✔ ok (Xms)\n  ✖ bad (Xms)\n  ▶ inner\n    ✔ deep (Xms)\n  ✔ inner (Xms)\n✖ outer (Xms)\n▶ with subtests\n  ✔ child one (Xms)\n  ✖ child two (Xms)\n✖ with subtests (Xms)\n✔ done style (Xms)\n﹣ skip via ctx (Xms) # not now\n⚠ todo option (Xms) # TODO\nlog before,each,each,each\nℹ tests 9\nℹ suites 2\nℹ pass 4\nℹ fail 3\nℹ cancelled 0\nℹ skipped 1\nℹ todo 1\nℹ duration_ms X\n\n✖ failing tests:\n\ntest at fail.test.mjs:8:3\n✖ bad (Xms)\n  AssertionError [ERR_ASSERTION]: Expected values to be strictly equal:\n  \n  1 !== 2\n  \n    generatedMessage: true,\n    code: 'ERR_ASSERTION',\n    actual: 1,\n    expected: 2,\n    operator: 'strictEqual',\n    diff: 'simple'\n  }\n\ntest at fail.test.mjs:17:11\n✖ child two (Xms)\n  Error: boom\n\ntest at fail.test.mjs:27:1\n⚠ todo option (Xms) # TODO\n  Error: ignored"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn node_test_flag_discovers_and_aggregates() {
    let dir = std::env::temp_dir().join(format!("jsq-node-test-flag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.test.mjs"), "import { test } from \"node:test\"\nimport assert from \"node:assert\"\ntest(\"a passes\", () => {})\ntest(\"a fails\", () => assert.strictEqual(1, 2))\n").unwrap();
    std::fs::create_dir_all(dir.join("test")).unwrap();
    std::fs::write(dir.join("test/b.js"), "const { describe, it } = require(\"node:test\")\ndescribe(\"b suite\", () => {\n  it(\"b one\", () => {})\n  it.skip(\"b two\", () => {})\n})\n").unwrap();
    std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    std::fs::write(
        dir.join("node_modules/pkg/c.test.js"),
        "throw new Error(\"must not run\")\n",
    )
    .unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("helper.js"), "module.exports = 1\n").unwrap();
    let out = run_in(&dir, &["--test"]);
    assert_eq!(out.status.code(), Some(1), "{}", normalize(&out.stderr));
    assert_eq!(
        mask_test_report(&normalize(&out.stdout)),
        "✔ a passes (Xms)\n✖ a fails (Xms)\n▶ b suite\n  ✔ b one (Xms)\n  ﹣ b two (Xms) # SKIP\n✔ b suite (Xms)\nℹ tests 4\nℹ suites 1\nℹ pass 2\nℹ fail 1\nℹ cancelled 0\nℹ skipped 1\nℹ todo 0\nℹ duration_ms X\n\n✖ failing tests:\n\ntest at a.test.mjs:4:1\n✖ a fails (Xms)\n  AssertionError [ERR_ASSERTION]: Expected values to be strictly equal:\n  \n  1 !== 2\n  \n    generatedMessage: true,\n    code: 'ERR_ASSERTION',\n    actual: 1,\n    expected: 2,\n    operator: 'strictEqual',\n    diff: 'simple'\n  }"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stdin_is_a_readable_stream() {
    use std::io::Write;
    let mut child = jsq()
        .current_dir(root())
        .args([
            "-e",
            "const { Readable } = require(\"node:stream\")\nconsole.log(process.stdin instanceof Readable)\nprocess.stdin.once(\"readable\", () => {\n  let chunk\n  let text = \"\"\n  while ((chunk = process.stdin.read()) !== null) text += chunk\n  console.log(JSON.stringify(text))\n  process.stdin.pause()\n  console.log(process.getActiveResourcesInfo().includes(\"TTYWrap\"))\n})",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn jsq");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(b"abc").expect("write stdin");
    let out = child.wait_with_output().expect("wait jsq");
    drop(stdin);
    assert_eq!(normalize(&out.stdout), "true\n\"abc\"\nfalse\n");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn unsettled_top_level_await_exits_with_13() {
    let dir = std::env::temp_dir().join(format!("jsq-tla-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("main.mjs"),
        "console.log(\"before\")\nawait new Promise(() => {})\nconsole.log(\"never\")\n",
    )
    .unwrap();
    let out = run_in(&dir, &["main.mjs"]);
    assert_eq!(out.status.code(), Some(13));
    assert_eq!(normalize(&out.stdout), "before\n");
    let stderr = normalize(&out.stderr);
    assert!(
        stderr.starts_with("Warning: Detected unsettled top-level await at file:///")
            && stderr.contains("main.mjs"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn unix_domain_sockets() {
    let out = run_in(&root(), &["tests/scripts/fixtures/unix_socket.js"]);
    assert!(out.status.success(), "{}", normalize(&out.stderr));
    let text = normalize(&out.stdout);
    let mut lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("listening ") && lines[0].ends_with(".sock true"),
        "{text}"
    );
    lines.remove(0);
    lines.sort_unstable();
    assert_eq!(
        lines,
        [
            "client connected undefined undefined",
            "client got pong",
            "closed false",
            "error ENOENT connect /tmp/does-not-exist.sock",
            "http 200 text/plain hello",
            "server conn undefined undefined",
            "server conn undefined undefined",
        ],
        "{text}"
    );
}
