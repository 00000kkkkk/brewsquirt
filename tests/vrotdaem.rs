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

#[test]
fn bins_and_trusted_install_scripts() {
    let base = temp_dir("scripts");
    write(
        base.join("helper").join("package.json"),
        r#"{"name":"helper","version":"1.0.0","bin":{"helper-say":"say.js"}}"#,
    );
    write(
        base.join("helper").join("say.js"),
        "#!/usr/bin/env node\nrequire(\"fs\").appendFileSync(process.argv[2], \"said \" + process.argv.slice(3).join(\" \") + \"\\n\")\n",
    );
    write(
        base.join("hooked").join("package.json"),
        r#"{"name":"hooked","version":"2.0.0","scripts":{"preinstall":"node step.js pre","postinstall":"node step.js post && helper-say log.txt from hooked"}}"#,
    );
    write(
        base.join("hooked").join("step.js"),
        "require(\"fs\").appendFileSync(\"log.txt\", [process.argv[2], process.env.npm_lifecycle_event, process.env.npm_package_name, process.env.npm_package_version].join(\" \") + \"\\n\")\n",
    );
    let app = base.join("app");
    write(
        app.join("squirt.json"),
        r#"{"name":"app","version":"0.1.0","main":"index.jsq","dependencies":{"helper":"file:../helper","hooked":"file:../hooked"}}"#,
    );
    let log = app.join("nude_modules").join("hooked").join("log.txt");
    let expected = "pre preinstall hooked 2.0.0\npost postinstall hooked 2.0.0\nsaid from hooked\n";

    let out = vrotdaem(&app, &["gluboko"]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{stdout}{}", text(&out.stderr));
    assert!(
        stdout.contains("hooked has install scripts that were not run"),
        "{stdout}"
    );
    assert!(!log.exists());
    let bin = app.join("nude_modules").join(".bin");
    let shim = if cfg!(windows) {
        "helper-say.cmd"
    } else {
        "helper-say"
    };
    assert!(bin.join(shim).is_file());

    let out = vrotdaem(&app, &["trust", "hooked"]);
    assert!(
        out.status.success(),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    assert_eq!(text(&std::fs::read(&log).unwrap()), expected);
    let manifest = std::fs::read_to_string(app.join("squirt.json")).unwrap();
    assert!(manifest.contains("\"trustedDependencies\""), "{manifest}");

    let out = vrotdaem(&app, &["gluboko", "--force"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!text(&out.stdout).contains("were not run"));
    assert_eq!(text(&std::fs::read(&log).unwrap()), expected);

    write(
        base.join("hooked").join("package.json"),
        r#"{"name":"hooked","version":"2.0.1","scripts":{"install":"node -e \"process.exit(3)\""}}"#,
    );
    let out = vrotdaem(&app, &["gluboko", "--force"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("the install script of hooked failed with exit code 3"),
        "{}",
        text(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&base);
}

fn tar_gz(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut tar = Vec::new();
    for (name, body) in files {
        let mut header = [0u8; 512];
        let path = format!("package/{name}");
        header[..path.len()].copy_from_slice(path.as_bytes());
        header[100..107].copy_from_slice(b"0000644");
        header[108..115].copy_from_slice(b"0000000");
        header[116..123].copy_from_slice(b"0000000");
        let size = format!("{:011o}", body.len());
        header[124..135].copy_from_slice(size.as_bytes());
        header[136..147].copy_from_slice(b"00000000000");
        header[156] = b'0';
        header[257..262].copy_from_slice(b"ustar");
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|b| *b as u32).sum();
        let sum = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(sum.as_bytes());
        tar.extend_from_slice(&header);
        tar.extend_from_slice(body.as_bytes());
        let pad = (512 - body.len() % 512) % 512;
        tar.extend(std::iter::repeat_n(0u8, pad));
    }
    tar.extend(std::iter::repeat_n(0u8, 1024));
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar).unwrap();
    gz.finish().unwrap()
}

struct Package {
    name: &'static str,
    manifest: serde_json::Value,
    tarball: Vec<u8>,
}

fn serve_registry(packages: Vec<Package>) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let root = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            let path = path
                .trim_start_matches('/')
                .replace("%2F", "/")
                .replace("%2f", "/");
            let version_of = |p: &Package| p.manifest["version"].as_str().unwrap().to_string();
            let (status, body): (&str, Vec<u8>) = if let Some(file) = path.strip_suffix(".tgz") {
                match packages
                    .iter()
                    .find(|p| format!("{}@{}", p.name, version_of(p)) == file)
                {
                    Some(p) => ("200 OK", p.tarball.clone()),
                    None => ("404 Not Found", Vec::new()),
                }
            } else {
                let matching: Vec<&Package> = packages.iter().filter(|p| p.name == path).collect();
                match matching.last() {
                    Some(last) => {
                        let mut versions = serde_json::Map::new();
                        for p in &matching {
                            let version = version_of(p);
                            let mut entry = p.manifest.clone();
                            entry["dist"] = serde_json::json!({
                                "tarball": format!("{root}/{}@{version}.tgz", p.name)
                            });
                            versions.insert(version, entry);
                        }
                        let doc = serde_json::json!({
                            "name": last.name,
                            "dist-tags": { "latest": version_of(last) },
                            "versions": versions,
                        });
                        ("200 OK", doc.to_string().into_bytes())
                    }
                    None => ("404 Not Found", b"{}".to_vec()),
                }
            };
            let mut out = stream;
            let _ = write!(
                out,
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = out.write_all(&body);
        }
    });
    base
}

fn npm_package(name: &'static str, manifest: serde_json::Value, files: &[(&str, &str)]) -> Package {
    let mut all = vec![("package.json", manifest.to_string())];
    for (n, b) in files {
        all.push((n, b.to_string()));
    }
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(n, b)| (*n, b.as_str())).collect();
    Package {
        name,
        manifest,
        tarball: tar_gz(&borrowed),
    }
}

#[test]
fn peer_dependencies_and_node_modules_link() {
    let registry = serve_registry(vec![
        npm_package(
            "uses-peer",
            serde_json::json!({
                "name": "uses-peer",
                "version": "1.0.0",
                "main": "index.js",
                "peerDependencies": { "the-peer": "^2.0.0", "optional-peer": "^1.0.0" },
                "peerDependenciesMeta": { "optional-peer": { "optional": true } }
            }),
            &[(
                "index.js",
                "module.exports = () => require(\"the-peer\") + \" via peer\"",
            )],
        ),
        npm_package(
            "the-peer",
            serde_json::json!({ "name": "the-peer", "version": "2.1.0", "main": "index.js" }),
            &[("index.js", "module.exports = \"peer 2.1.0\"")],
        ),
        npm_package(
            "optional-peer",
            serde_json::json!({ "name": "optional-peer", "version": "1.0.0" }),
            &[],
        ),
    ]);
    let app = temp_dir("peers");
    write(
        app.join("squirt.json"),
        r#"{"name":"app","version":"0.1.0","main":"index.jsq","dependencies":{"uses-peer":"npm:uses-peer@^1.0.0"}}"#,
    );
    write(
        app.join("index.jsq"),
        "import usePeer from \"uses-peer\"\nconsole.log(usePeer())\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_vrotdaem"))
        .current_dir(&app)
        .env("JSQ_NPM_REGISTRY", &registry)
        .arg("gluboko")
        .output()
        .expect("run vrotdaem");
    assert!(
        out.status.success(),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    assert!(app.join("nude_modules/the-peer/index.js").is_file());
    assert!(!app.join("nude_modules/optional-peer").exists());
    assert!(app.join("node_modules/the-peer/index.js").is_file());
    let out = vrotdaem(&app, &["run"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "peer 2.1.0 via peer\n");
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn cyclic_dependencies_resolve_through_ancestors() {
    let pkg = |name: &'static str, version: &str, deps: serde_json::Value, main: &str| {
        npm_package(
            name,
            serde_json::json!({ "name": name, "version": version, "main": "index.js", "dependencies": deps }),
            &[("index.js", main)],
        )
    };
    let map = "exports.version = require(\"./package.json\").version\nexports.peer = () => require(PEER)\n";
    let registry = serve_registry(vec![
        pkg(
            "srcmap",
            "1.0.0",
            serde_json::json!({ "symbol": "1.0.0" }),
            &map.replace("PEER", "\"symbol\""),
        ),
        pkg(
            "srcmap",
            "2.0.0",
            serde_json::json!({ "symbol": "^2.0.0" }),
            &map.replace("PEER", "\"symbol\""),
        ),
        pkg(
            "symbol",
            "1.0.0",
            serde_json::json!({ "srcmap": "1.0.0" }),
            &map.replace("PEER", "\"srcmap\""),
        ),
        pkg(
            "symbol",
            "2.0.0",
            serde_json::json!({ "srcmap": "^2.0.0" }),
            &map.replace("PEER", "\"srcmap\""),
        ),
        pkg(
            "bundler",
            "1.0.0",
            serde_json::json!({ "srcmap": "1.0.0" }),
            "const m = require(\"srcmap\")\nmodule.exports = [m.version, m.peer().version, m.peer().peer().version].join(\",\")\n",
        ),
    ]);
    let app = temp_dir("cycles");
    write(
        app.join("squirt.json"),
        r#"{"name":"app","version":"0.1.0","main":"index.jsq","dependencies":{"srcmap":"npm:srcmap@^2.0.0","bundler":"npm:bundler@^1.0.0"}}"#,
    );
    write(
        app.join("index.jsq"),
        "import srcmap from \"srcmap\"\nimport bundler from \"bundler\"\nconsole.log(srcmap.version, srcmap.peer().version, bundler)\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_vrotdaem"))
        .current_dir(&app)
        .env("JSQ_NPM_REGISTRY", &registry)
        .arg("gluboko")
        .output()
        .expect("run vrotdaem");
    assert!(
        out.status.success(),
        "{}{}",
        text(&out.stdout),
        text(&out.stderr)
    );
    let nested = app.join("nude_modules/bundler/nude_modules/srcmap");
    assert!(nested.join("nude_modules/symbol/index.js").is_file());
    assert!(!nested.join("nude_modules/symbol/nude_modules").exists());
    assert!(
        !app.join("nude_modules/bundler/nude_modules/symbol")
            .exists()
    );
    let out = vrotdaem(&app, &["run"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "2.0.0 2.0.0 1.0.0,1.0.0,1.0.0\n");
    let _ = std::fs::remove_dir_all(&app);
}
