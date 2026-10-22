use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vrotdaem-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn vrotdaem(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vrotdaem"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run vrotdaem")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace("\r\n", "\n")
}

fn write(path: PathBuf, content: &str) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

#[test]
fn init_add_gluboko_run() {
    let base = temp_dir("flow");
    let pad = base.join("leftsquirt");
    write(
        pad.join("squirt.json"),
        r#"{"name":"leftsquirt","version":"1.0.0","main":"lib.jsq","dependencies":{"repeat":"file:../repeat"}}"#,
    );
    write(
        pad.join("lib.jsq"),
        "import { times } from \"repeat\"\nexport function pad(s, n) { return times(\" \", n - s.length) + s }\n",
    );
    write(
        base.join("repeat").join("index.jsq"),
        "export const times = (s, n) => s.repeat(Math.max(0, n))\n",
    );
    let app = base.join("app");
    std::fs::create_dir_all(&app).unwrap();

    let out = vrotdaem(&app, &["init", "--name", "app"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(app.join("squirt.json").is_file());
    assert!(app.join("index.jsq").is_file());

    let again = vrotdaem(&app, &["init"]);
    assert!(!again.status.success());

    let out = vrotdaem(&app, &["add", "leftsquirt", "file:../leftsquirt"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(app.join("nude_modules/leftsquirt/lib.jsq").is_file());
    assert!(app.join("nude_modules/repeat/index.jsq").is_file());

    write(
        app.join("index.jsq"),
        "import { pad } from \"leftsquirt\"\nconsole.log(`[${pad(\"jsq\", 6)}]`, process.argv.slice(2))\n",
    );
    let out = vrotdaem(&app, &["run", "x"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "[   jsq] [ 'x' ]\n");

    let out = vrotdaem(&app, &["gluboko"]);
    assert!(out.status.success());
    assert!(text(&out.stdout).contains("already deep inside"));

    std::fs::remove_file(app.join("nude_modules/repeat/index.jsq")).unwrap();
    let out = vrotdaem(&app, &["gluboko", "--force"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("2 dependencies injected"));
    assert!(app.join("nude_modules/repeat/index.jsq").is_file());

    let out = vrotdaem(&app, &["remove", "leftsquirt"]);
    assert!(out.status.success());
    assert!(!app.join("nude_modules/leftsquirt").exists());
    let manifest = std::fs::read_to_string(app.join("squirt.json")).unwrap();
    assert!(!manifest.contains("leftsquirt"));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn rejects_unknown_spec() {
    let base = temp_dir("spec");
    let out = vrotdaem(&base, &["init"]);
    assert!(out.status.success());
    let out = vrotdaem(&base, &["add", "thing", "nope://thing"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("unsupported dependency spec"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn missing_manifest_is_reported() {
    let base = temp_dir("none");
    let out = vrotdaem(&base, &["gluboko", "--force"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("vrotdaem init"));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn test_command_runs_package_tests() {
    let base = temp_dir("test");
    write(
        base.join("squirt.json"),
        r#"{"name":"t","version":"0.1.0","main":"lib.jsq","dependencies":{}}"#,
    );
    write(base.join("lib.jsq"), "export const double = (n) => n * 2\n");
    write(
        base.join("spec").join("lib.test.jsq"),
        "import { test, expect } from \"squirt:test\"\nimport { double } from \"../lib.jsq\"\ntest(\"doubles\", () => expect(double(21)).toBe(42))\ntest(\"fails\", () => expect(double(1)).toBe(3))\n",
    );
    let nested = base.join("spec");
    let out = vrotdaem(&nested, &["test", "--filter", "doubles"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}{}", text(&out.stderr));
    assert!(stdout.contains("spec/lib.test.jsq"), "{stdout}");
    assert!(stdout.contains("tests: 1 passed (1 file"), "{stdout}");

    let out = vrotdaem(&nested, &["test"]);
    let stdout = text(&out.stdout);
    assert!(!out.status.success(), "{stdout}");
    assert!(stdout.contains("1 passed, 1 failed"), "{stdout}");
    let _ = std::fs::remove_dir_all(&base);
}
