pub mod npm;
pub mod scripts;
pub mod semver;

use crate::module::{DEPS_DIR, MANIFEST, NODE_DEPS_DIR};
use indexmap::IndexMap;
use semver::Range;
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
    #[serde(
        default,
        rename = "trustedDependencies",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub trusted: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_main() -> String {
    "index.jsq".to_string()
}

#[derive(Debug, Clone)]
pub enum Spec {
    File(PathBuf),
    Git { url: String, rev: Option<String> },
    Npm { name: String, range: Range },
}

fn looks_like_package_name(s: &str) -> bool {
    let (name, _) = npm::split_name_range(s);
    if name.is_empty() || name.len() > 214 {
        return false;
    }
    let body = name.strip_prefix('@').unwrap_or(&name);
    let mut parts = body.split('/');
    let first = parts.next().unwrap_or("");
    let valid = |p: &str| {
        !p.is_empty()
            && p.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.')
            })
    };
    if name.starts_with('@') {
        matches!(parts.next(), Some(second) if valid(first) && valid(second))
            && parts.next().is_none()
    } else {
        valid(first) && parts.next().is_none()
    }
}

impl Spec {
    pub fn parse(s: &str) -> Result<Spec, String> {
        Spec::parse_for(None, s)
    }

    pub fn parse_for(dep_name: Option<&str>, s: &str) -> Result<Spec, String> {
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
        let npm_spec = s.strip_prefix("npm:").unwrap_or(s);
        if npm_spec.contains("://") {
            return Err(format!(
                "unsupported dependency spec '{s}', use a package name like lodash@^4, npm:<name>@<range>, file:<path> or git:<url>[#ref]"
            ));
        }
        let (name, range_text) = if looks_like_package_name(npm_spec) {
            npm::split_name_range(npm_spec)
        } else if let Some(dep) = dep_name {
            (dep.to_string(), npm_spec.to_string())
        } else {
            return Err(format!(
                "unsupported dependency spec '{s}', use a package name like lodash@^4, npm:<name>@<range>, file:<path> or git:<url>[#ref]"
            ));
        };
        let range = Range::parse(&range_text)
            .ok_or_else(|| format!("invalid version range '{range_text}' for {name}"))?;
        Ok(Spec::Npm { name, range })
    }

    pub fn is_npm(&self) -> bool {
        matches!(self, Spec::Npm { .. })
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub injected: Vec<String>,
    pub skipped: Vec<String>,
    pub untrusted: Vec<String>,
    pub scripts: Vec<String>,
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
        trusted: Vec::new(),
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

fn fetch(spec: &Spec, base: &Path, dest: &Path, optional: bool) -> Result<Option<String>, String> {
    match spec {
        Spec::Npm { name, range } => {
            let resolved = npm::install_checked(name, range, dest, optional)?;
            Ok(Some(resolved.version.to_string()))
        }
        Spec::File(p) => {
            let src = if p.is_absolute() {
                p.clone()
            } else {
                base.join(p)
            };
            if !src.is_dir() {
                return Err(format!("{} is not a directory", src.display()));
            }
            copy_dir(&src, dest)?;
            Ok(None)
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
            Ok(None)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DepKind {
    Regular,
    Optional,
    Peer,
}

fn dependencies_of(dir: &Path) -> Result<IndexMap<String, (String, DepKind)>, String> {
    if dir.join(MANIFEST).is_file() {
        return Ok(read_manifest(dir)?
            .dependencies
            .into_iter()
            .map(|(k, v)| (k, (v, DepKind::Regular)))
            .collect());
    }
    let mut out = IndexMap::new();
    let optional = npm::package_optional_dependencies(dir);
    let peers = npm::package_peer_dependencies(dir);
    let mut all = npm::package_dependencies(dir);
    for (k, v) in &optional {
        all.insert(k.clone(), v.clone());
    }
    for (k, v) in &peers {
        if !all.contains_key(k) {
            all.insert(k.clone(), v.clone());
        }
    }
    for (name, raw) in all {
        let spec = if let Some(alias) = raw.strip_prefix("npm:") {
            format!("npm:{alias}")
        } else if Range::parse(&raw).is_some() {
            format!("npm:{name}@{raw}")
        } else {
            continue;
        };
        let kind = if optional.contains_key(&name) {
            DepKind::Optional
        } else if peers.contains_key(&name) && !npm::package_dependencies(dir).contains_key(&name) {
            DepKind::Peer
        } else {
            DepKind::Regular
        };
        out.insert(name, (spec, kind));
    }
    Ok(out)
}

pub fn gluboko(root: &Path, force: bool, mut log: impl FnMut(&str)) -> Result<Report, String> {
    let manifest = read_manifest(root)?;
    let nude = root.join(DEPS_DIR);
    if force && nude.exists() {
        fs::remove_dir_all(&nude).map_err(|e| format!("cannot wipe {DEPS_DIR}: {e}"))?;
    }
    fs::create_dir_all(&nude).map_err(|e| format!("cannot create {DEPS_DIR}: {e}"))?;
    let mut queue: VecDeque<(String, String, PathBuf, PathBuf, DepKind)> = manifest
        .dependencies
        .iter()
        .map(|(n, s)| {
            (
                n.clone(),
                s.clone(),
                root.to_path_buf(),
                root.to_path_buf(),
                DepKind::Regular,
            )
        })
        .collect();
    let mut peers: VecDeque<(String, String, PathBuf, PathBuf, DepKind)> = VecDeque::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut all: Vec<(String, PathBuf)> = Vec::new();
    let mut fresh: Vec<(String, PathBuf, bool)> = Vec::new();
    let mut report = Report::default();
    while let Some((name, raw, base, holder, kind)) =
        queue.pop_front().or_else(|| peers.pop_front())
    {
        let spec = Spec::parse_for(Some(&name), &raw)?;
        let top = nude.join(&name);
        if kind == DepKind::Peer && top.exists() {
            continue;
        }
        let optional = kind == DepKind::Optional;
        let (dest, satisfied) = match &spec {
            Spec::Npm { range, .. } if top.exists() => match npm::installed_version(&top) {
                Some(v) if range.matches(&v) => (top.clone(), true),
                _ if holder.as_path() == root => (top.clone(), false),
                _ => {
                    let nested = holder.join(DEPS_DIR).join(&name);
                    let ok = nested.exists()
                        && npm::installed_version(&nested).is_some_and(|v| range.matches(&v));
                    (nested, ok)
                }
            },
            _ => {
                let exists = top.exists();
                (top.clone(), exists)
            }
        };
        if seen.contains(&dest) {
            continue;
        }
        seen.push(dest.clone());
        all.push((name.clone(), dest.clone()));
        let origin = match &spec {
            Spec::File(p) if p.is_absolute() => p.clone(),
            Spec::File(p) => base.join(p),
            _ => dest.clone(),
        };
        if satisfied {
            report.skipped.push(name.clone());
        } else {
            if !optional {
                log(&format!("injecting {name} ({raw})"));
            }
            if dest.exists() {
                fs::remove_dir_all(&dest)
                    .map_err(|e| format!("cannot replace {}: {e}", dest.display()))?;
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            match fetch(&spec, &base, &dest, optional) {
                Ok(Some(version)) => {
                    if optional {
                        log(&format!("injecting {name} ({raw})"));
                    }
                    fresh.push((name.clone(), dest.clone(), optional));
                    let nested = dest != top;
                    report.injected.push(if nested {
                        format!("{name}@{version} (nested)")
                    } else {
                        format!("{name}@{version}")
                    });
                }
                Ok(None) => {
                    fresh.push((name.clone(), dest.clone(), optional));
                    report.injected.push(name.clone());
                }
                Err(e) => {
                    let _ = fs::remove_dir_all(&dest);
                    if optional {
                        if e != npm::UNSUPPORTED {
                            log(&format!("skipping optional {name}: {e}"));
                        }
                        continue;
                    }
                    return Err(format!("{name}: {e}"));
                }
            }
        }
        for (n, (s, k)) in dependencies_of(&dest)? {
            if k == DepKind::Peer {
                peers.push_back((n, s, root.to_path_buf(), root.to_path_buf(), k));
            } else {
                queue.push_back((n, s, origin.clone(), dest.clone(), k));
            }
        }
    }
    for (name, dest) in &all {
        if dest.is_dir() {
            scripts::link_bins(name, dest)?;
            link_node_modules(dest);
        }
    }
    link_node_modules(root);
    for (name, dest, optional) in fresh.iter().rev() {
        if !dest.is_dir() || !scripts::needs_scripts(dest) {
            continue;
        }
        if !manifest.trusted.iter().any(|t| t == name) {
            if !report.untrusted.contains(name) {
                report.untrusted.push(name.clone());
            }
            continue;
        }
        match scripts::run_lifecycle(root, name, dest, &mut log) {
            Ok(_) => report.scripts.push(name.clone()),
            Err(e) if *optional => {
                log(&format!("skipping optional {name}: {e}"));
                let _ = fs::remove_dir_all(dest);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(report)
}

pub fn link_node_modules(dir: &Path) {
    let nude = dir.join(DEPS_DIR);
    let node = dir.join(NODE_DEPS_DIR);
    if !nude.is_dir() || node.symlink_metadata().is_ok() {
        return;
    }
    #[cfg(unix)]
    {
        let _ = std::os::unix::fs::symlink(DEPS_DIR, &node);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into()))
            .raw_arg(format!(
                "/d /c mklink /J \"{}\" \"{}\"",
                node.display(),
                nude.display()
            ))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

pub fn trust(root: &Path, names: &[String], mut log: impl FnMut(&str)) -> Result<usize, String> {
    let mut m = read_manifest(root)?;
    for n in names {
        if !m.trusted.contains(n) {
            m.trusted.push(n.clone());
        }
    }
    write_manifest(root, &m)?;
    let mut ran = 0;
    for n in names {
        for dir in scripts::find_installed(root, n) {
            ran += scripts::run_lifecycle(root, n, &dir, &mut log)?;
        }
    }
    Ok(ran)
}

pub fn add(root: &Path, name: &str, spec: Option<&str>) -> Result<(String, String), String> {
    let raw = spec.unwrap_or(name);
    let parsed = Spec::parse_for(Some(name), raw)?;
    let (dep_name, stored) = match &parsed {
        Spec::Npm { name: pkg, range } => {
            let text = range.text();
            let stored = if text.is_empty() || text == "*" || text == "latest" {
                let latest = npm::latest_version(pkg)?;
                format!("npm:{pkg}@^{latest}")
            } else {
                format!("npm:{pkg}@{text}")
            };
            (pkg.clone(), stored)
        }
        _ => (name.to_string(), raw.to_string()),
    };
    let mut m = read_manifest(root)?;
    m.dependencies.insert(dep_name.clone(), stored.clone());
    write_manifest(root, &m)?;
    Ok((dep_name, stored))
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
