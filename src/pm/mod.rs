use crate::module::{DEPS_DIR, MANIFEST};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_main")]
    pub main: String,
    #[serde(default)]
    pub dependencies: IndexMap<String, String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_main() -> String {
    "index.jsq".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    File(PathBuf),
    Git { url: String, rev: Option<String> },
}

impl Spec {
    pub fn parse(s: &str) -> Result<Spec, String> {
        if let Some(path) = s.strip_prefix("file:") {
            return Ok(Spec::File(PathBuf::from(path)));
        }
        if let Some(rest) = s.strip_prefix("git:") {
            let (url, rev) = match rest.rsplit_once('#') {
                Some((u, r)) if !r.is_empty() => (u.to_string(), Some(r.to_string())),
                _ => (rest.to_string(), None),
            };
            return Ok(Spec::Git { url, rev });
        }
        Err(format!(
            "unsupported dependency spec '{s}', use file:<path> or git:<url>[#ref]"
        ))
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub injected: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        if d.join(MANIFEST).is_file() {
            return Some(d);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

pub fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let path = root.join(MANIFEST);
    let text =
        fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("invalid {}: {e}", path.display()))
}

pub fn write_manifest(root: &Path, m: &Manifest) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::write(root.join(MANIFEST), text).map_err(|e| format!("cannot write {MANIFEST}: {e}"))
}

pub fn init(root: &Path, name: Option<String>) -> Result<Manifest, String> {
    if root.join(MANIFEST).exists() {
        return Err(format!(
            "{MANIFEST} already exists, it is already deep enough"
        ));
    }
    let name = name.unwrap_or_else(|| {
        root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "squirt-app".to_string())
    });
    let m = Manifest {
        name,
        version: default_version(),
        description: None,
        main: default_main(),
        dependencies: IndexMap::new(),
    };
    write_manifest(root, &m)?;
    let main = root.join(&m.main);
    if !main.exists() {
        fs::write(&main, "console.log(`hello from ${squirt()} JavaSquirt`)\n")
            .map_err(|e| format!("cannot write {}: {e}", main.display()))?;
    }
    Ok(m)
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("cannot create {}: {e}", dst.display()))?;
    let entries = fs::read_dir(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == DEPS_DIR || name == ".git" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("cannot copy {}: {e}", from.display()))?;
        }
    }
    Ok(())
}

fn fetch(spec: &Spec, base: &Path, dest: &Path) -> Result<(), String> {
    match spec {
        Spec::File(p) => {
            let src = if p.is_absolute() {
                p.clone()
            } else {
                base.join(p)
            };
            if !src.is_dir() {
                return Err(format!("{} is not a directory", src.display()));
            }
            copy_dir(&src, dest)
        }
        Spec::Git { url, rev } => {
            let mut cmd = Command::new("git");
            cmd.arg("clone").arg("--depth").arg("1").arg("--quiet");
            if let Some(r) = rev {
                cmd.arg("--branch").arg(r);
            }
            cmd.arg(url).arg(dest);
            let status = cmd.status().map_err(|e| format!("cannot run git: {e}"))?;
            if !status.success() {
                return Err(format!("git clone {url} failed"));
            }
            let _ = fs::remove_dir_all(dest.join(".git"));
            Ok(())
        }
    }
}

pub fn gluboko(root: &Path, force: bool, mut log: impl FnMut(&str)) -> Result<Report, String> {
    let manifest = read_manifest(root)?;
    let nude = root.join(DEPS_DIR);
    if force && nude.exists() {
        fs::remove_dir_all(&nude).map_err(|e| format!("cannot wipe {DEPS_DIR}: {e}"))?;
    }
    fs::create_dir_all(&nude).map_err(|e| format!("cannot create {DEPS_DIR}: {e}"))?;
    let mut queue: VecDeque<(String, String, PathBuf)> = manifest
        .dependencies
        .iter()
        .map(|(n, s)| (n.clone(), s.clone(), root.to_path_buf()))
        .collect();
    let mut seen = Vec::new();
    let mut report = Report::default();
    while let Some((name, raw, base)) = queue.pop_front() {
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        let dest = nude.join(&name);
        let spec = Spec::parse(&raw)?;
        let origin = match &spec {
            Spec::File(p) if p.is_absolute() => p.clone(),
            Spec::File(p) => base.join(p),
            Spec::Git { .. } => dest.clone(),
        };
        if dest.exists() {
            report.skipped.push(name.clone());
        } else {
            log(&format!("injecting {name} ({raw})"));
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if let Err(e) = fetch(&spec, &base, &dest) {
                let _ = fs::remove_dir_all(&dest);
                return Err(format!("{name}: {e}"));
            }
            report.injected.push(name.clone());
        }
        if dest.join(MANIFEST).is_file() {
            let sub = read_manifest(&dest)?;
            for (n, s) in sub.dependencies {
                queue.push_back((n, s, origin.clone()));
            }
        }
    }
    Ok(report)
}

pub fn add(root: &Path, name: &str, spec: &str) -> Result<(), String> {
    Spec::parse(spec)?;
    let mut m = read_manifest(root)?;
    m.dependencies.insert(name.to_string(), spec.to_string());
    write_manifest(root, &m)
}

pub fn remove(root: &Path, name: &str) -> Result<bool, String> {
    let mut m = read_manifest(root)?;
    let existed = m.dependencies.shift_remove(name).is_some();
    write_manifest(root, &m)?;
    let dir = root.join(DEPS_DIR).join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
    }
    Ok(existed)
}
