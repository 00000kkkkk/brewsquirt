use crate::module::DEPS_DIR;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const EVENTS: [&str; 3] = ["preinstall", "install", "postinstall"];

fn package_json(dir: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(dir.join("package.json")).ok()?).ok()
}

pub fn jsq_path() -> PathBuf {
    let name = format!("jsq{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

fn runs_with_jsq(target: &Path) -> bool {
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("");
    if matches!(ext, "js" | "cjs" | "mjs" | "jsq" | "ts" | "mts" | "cts") {
        return true;
    }
    let Ok(bytes) = fs::read(target) else {
        return false;
    };
    match bytes.strip_prefix(b"#!") {
        Some(rest) => {
            let line = rest.split(|b| *b == b'\n').next().unwrap_or_default();
            let line = String::from_utf8_lossy(line);
            line.contains("node") || line.contains("jsq")
        }
        None => !bytes.is_empty() && !bytes.contains(&0),
    }
}

fn write_shim(dir: &Path, name: &str, target: &Path, jsq: &Path) -> Result<(), String> {
    let with_jsq = runs_with_jsq(target);
    if cfg!(windows) {
        let body = if with_jsq {
            format!("@\"{}\" \"{}\" %*\r\n", jsq.display(), target.display())
        } else {
            format!("@\"{}\" %*\r\n", target.display())
        };
        fs::write(dir.join(format!("{name}.cmd")), body)
            .map_err(|e| format!("cannot write the {name} command: {e}"))
    } else {
        let body = if with_jsq {
            format!(
                "#!/bin/sh\nexec '{}' '{}' \"$@\"\n",
                jsq.display(),
                target.display()
            )
        } else {
            format!("#!/bin/sh\nexec '{}' \"$@\"\n", target.display())
        };
        let path = dir.join(name);
        fs::write(&path, body).map_err(|e| format!("cannot write the {name} command: {e}"))?;
        make_executable(&path);
        if !with_jsq {
            make_executable(target);
        }
        Ok(())
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = fs::metadata(path) {
        let mut perm = meta.permissions();
        perm.set_mode(perm.mode() | 0o755);
        let _ = fs::set_permissions(path, perm);
    }
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn bin_dir_for(dest: &Path) -> Option<PathBuf> {
    dest.ancestors()
        .skip(1)
        .find(|a| a.file_name().is_some_and(|n| n == DEPS_DIR))
        .map(|d| d.join(".bin"))
}

fn bins_of(name: &str, doc: &serde_json::Value) -> Vec<(String, String)> {
    match doc.get("bin") {
        Some(serde_json::Value::String(s)) => {
            let short = name.rsplit('/').next().unwrap_or(name);
            vec![(short.to_string(), s.clone())]
        }
        Some(serde_json::Value::Object(m)) => m
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect(),
        _ => Vec::new(),
    }
}

fn safe_bin_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\', ':', '"', '\'']) && name != "." && name != ".."
}

pub fn link_bins(name: &str, dest: &Path) -> Result<usize, String> {
    let Some(doc) = package_json(dest) else {
        return Ok(0);
    };
    let bins = bins_of(name, &doc);
    if bins.is_empty() {
        return Ok(0);
    }
    let Some(dir) = bin_dir_for(dest) else {
        return Ok(0);
    };
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let jsq = jsq_path();
    let mut count = 0;
    for (bin, rel) in bins {
        if !safe_bin_name(&bin) {
            continue;
        }
        let target = dest.join(rel.trim_start_matches("./"));
        if !target.starts_with(dest) || !target.is_file() {
            continue;
        }
        write_shim(&dir, &bin, &target, &jsq)?;
        count += 1;
    }
    Ok(count)
}

fn node_shims(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(DEPS_DIR).join(".vrotdaem").join("node");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let jsq = jsq_path();
    if cfg!(windows) {
        fs::write(
            dir.join("node.cmd"),
            format!("@\"{}\" %*\r\n", jsq.display()),
        )
        .map_err(|e| e.to_string())?;
    } else {
        let path = dir.join("node");
        fs::write(
            &path,
            format!("#!/bin/sh\nexec '{}' \"$@\"\n", jsq.display()),
        )
        .map_err(|e| e.to_string())?;
        make_executable(&path);
    }
    Ok(dir)
}

pub fn lifecycle_scripts(dest: &Path) -> Vec<(&'static str, String)> {
    let Some(doc) = package_json(dest) else {
        return Vec::new();
    };
    let Some(scripts) = doc.get("scripts").and_then(|s| s.as_object()) else {
        return Vec::new();
    };
    EVENTS
        .iter()
        .filter_map(|ev| {
            let cmd = scripts.get(*ev)?.as_str()?.trim();
            (!cmd.is_empty()).then(|| (*ev, cmd.to_string()))
        })
        .collect()
}

pub fn needs_scripts(dest: &Path) -> bool {
    let scripts = lifecycle_scripts(dest);
    if scripts.is_empty() {
        return false;
    }
    let prebuilt = dest.join("prebuilds").is_dir();
    !scripts.iter().all(|(_, cmd)| {
        prebuilt
            && matches!(
                cmd.as_str(),
                "node-gyp-build" | "node-gyp-build-optional-packages" | "node-gyp-build-test"
            )
    })
}

fn search_path(root: &Path, dest: &Path) -> Result<std::ffi::OsString, String> {
    let mut dirs = vec![node_shims(root)?];
    for anc in dest.ancestors() {
        let bin = anc.join(DEPS_DIR).join(".bin");
        if bin.is_dir() && !dirs.contains(&bin) {
            dirs.push(bin);
        }
        if anc == root {
            break;
        }
    }
    if let Some(p) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    std::env::join_paths(dirs).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn shell(cmd: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into()));
    c.raw_arg(format!("/d /s /c \"{cmd}\""));
    c
}

#[cfg(not(windows))]
fn shell(cmd: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(cmd);
    c
}

pub fn run_lifecycle(
    root: &Path,
    name: &str,
    dest: &Path,
    log: &mut dyn FnMut(&str),
) -> Result<usize, String> {
    let scripts = lifecycle_scripts(dest);
    if scripts.is_empty() {
        return Ok(0);
    }
    let doc = package_json(dest).unwrap_or_default();
    let version = doc.get("version").and_then(|v| v.as_str()).unwrap_or("");
    let path = search_path(root, dest)?;
    let jsq = jsq_path();
    let agent = format!(
        "vrotdaem/{} node/v22.12.0 {} {}",
        crate::VERSION,
        crate::pm::npm::node_platform(),
        crate::pm::npm::node_arch()
    );
    for (event, cmd) in &scripts {
        log(&format!("running {name} {event}: {cmd}"));
        let status = shell(cmd)
            .current_dir(dest)
            .env("PATH", &path)
            .env("npm_lifecycle_event", event)
            .env("npm_lifecycle_script", cmd)
            .env("npm_package_name", name)
            .env("npm_package_version", version)
            .env("npm_package_json", dest.join("package.json"))
            .env("npm_node_execpath", &jsq)
            .env("npm_execpath", std::env::current_exe().unwrap_or_default())
            .env("npm_config_user_agent", &agent)
            .env("npm_command", "install")
            .env("NODE", &jsq)
            .env("INIT_CWD", root)
            .status()
            .map_err(|e| format!("cannot run the {event} script of {name}: {e}"))?;
        if !status.success() {
            return Err(format!(
                "the {event} script of {name} failed with {}",
                status
                    .code()
                    .map(|c| format!("exit code {c}"))
                    .unwrap_or_else(|| "a signal".to_string())
            ));
        }
    }
    Ok(scripts.len())
}

pub fn find_installed(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.join(DEPS_DIR)];
    while let Some(nude) = stack.pop() {
        let candidate = nude.join(name);
        if candidate.join("package.json").is_file() {
            out.push(candidate);
        }
        let Ok(entries) = fs::read_dir(&nude) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let file = e.file_name();
            let file = file.to_string_lossy();
            if file.starts_with('.') || !p.is_dir() {
                continue;
            }
            if file.starts_with('@') {
                if let Ok(scoped) = fs::read_dir(&p) {
                    for s in scoped.flatten() {
                        let nested = s.path().join(DEPS_DIR);
                        if nested.is_dir() {
                            stack.push(nested);
                        }
                    }
                }
            } else {
                let nested = p.join(DEPS_DIR);
                if nested.is_dir() {
                    stack.push(nested);
                }
            }
        }
    }
    out
}
