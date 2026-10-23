use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_addon() -> PathBuf {
    let target = root().join("target").join("napi-addon");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--quiet", "--manifest-path"])
        .arg(root().join("tests").join("napi-addon").join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("cargo build addon");
    assert!(status.success(), "addon build failed");
    let name = if cfg!(windows) {
        "napi_test_addon.dll"
    } else if cfg!(target_vendor = "apple") {
        "libnapi_test_addon.dylib"
    } else {
        "libnapi_test_addon.so"
    };
    target.join("release").join(name)
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jsq-napi-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, script: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsq"))
        .current_dir(dir)
        .arg(script)
        .output()
        .expect("run jsq");
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "jsq failed\n{stdout}\n{stderr}");
    stdout
}

#[test]
fn native_addon_runs() {
    let lib = build_addon();
    let dir = temp_dir();
    std::fs::copy(&lib, dir.join("addon.node")).unwrap();
    let fixture = root().join("tests").join("napi-addon");
    for f in ["main.js", "main.out", "loader.js"] {
        std::fs::copy(fixture.join(f), dir.join(f)).unwrap();
    }
    let expected = std::fs::read_to_string(dir.join("main.out"))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(run(&dir, "main.js"), expected);
    assert_eq!(run(&dir, "loader.js"), "dlopen 5 hello dl\n");
    let _ = std::fs::remove_dir_all(&dir);
}
