use super::semver::{Range, Version};
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::JoinHandle;

pub const REGISTRY: &str = "https://registry.npmjs.org";

#[derive(Debug, Clone)]
pub struct Resolved {
    pub version: Version,
    pub tarball: String,
    pub dependencies: IndexMap<String, String>,
    pub optional_dependencies: IndexMap<String, String>,
    pub peer_dependencies: IndexMap<String, String>,
    pub platforms: (Vec<String>, Vec<String>, Vec<String>),
}

#[derive(Deserialize, Default)]
struct Entry {
    #[serde(default)]
    dependencies: serde_json::Value,
    #[serde(default, rename = "optionalDependencies")]
    optional_dependencies: serde_json::Value,
    #[serde(default, rename = "peerDependencies")]
    peer_dependencies: serde_json::Value,
    #[serde(default, rename = "peerDependenciesMeta")]
    peer_meta: serde_json::Value,
    #[serde(default)]
    os: serde_json::Value,
    #[serde(default)]
    cpu: serde_json::Value,
    #[serde(default)]
    libc: serde_json::Value,
    #[serde(default)]
    dist: serde_json::Value,
}

#[derive(Deserialize, Default)]
pub struct Packument {
    #[serde(default, rename = "dist-tags")]
    dist_tags: HashMap<String, serde_json::Value>,
    #[serde(default)]
    versions: IndexMap<String, Entry>,
}

fn string_list(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        serde_json::Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

pub fn node_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    }
}

pub fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "arm" => "arm",
        other => other,
    }
}

fn libc() -> &'static str {
    if std::path::Path::new("/etc/alpine-release").exists()
        || std::path::Path::new("/lib/ld-musl-x86_64.so.1").exists()
        || std::path::Path::new("/lib/ld-musl-aarch64.so.1").exists()
    {
        "musl"
    } else {
        "glibc"
    }
}

fn allowed(list: &[String], value: &str) -> bool {
    if list.is_empty() {
        return true;
    }
    let blocked = list.iter().any(|x| x.strip_prefix('!') == Some(value));
    let positives: Vec<&String> = list.iter().filter(|x| !x.starts_with('!')).collect();
    !blocked && (positives.is_empty() || positives.iter().any(|x| x.as_str() == value))
}

pub fn platform_ok(r: &Resolved) -> bool {
    let (os, cpu, lib) = &r.platforms;
    allowed(os, node_platform())
        && allowed(cpu, node_arch())
        && (node_platform() != "linux" || allowed(lib, libc()))
}

fn encode_name(name: &str) -> String {
    name.replacen('/', "%2F", 1)
}

fn registry_url() -> String {
    std::env::var("JSQ_NPM_REGISTRY")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| REGISTRY.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn packument(name: &str) -> Result<Packument, String> {
    let url = format!("{}/{}", registry_url(), encode_name(name));
    let bytes = crate::io::http_get(&url, Some("application/vnd.npm.install-v1+json"))
        .map_err(|e| format!("cannot fetch {name} from the npm registry: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid registry response for {name}: {e}"))
}

fn string_map(v: &serde_json::Value) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    if let Some(obj) = v.as_object() {
        for (k, val) in obj {
            if let Some(s) = val.as_str() {
                out.insert(k.clone(), s.to_string());
            }
        }
    }
    out
}

fn required_peers(peers: &serde_json::Value, meta: &serde_json::Value) -> IndexMap<String, String> {
    string_map(peers)
        .into_iter()
        .filter(|(name, _)| {
            !meta
                .get(name)
                .and_then(|v| v.get("optional"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .collect()
}

fn pick<'a>(doc: &'a Packument, name: &str, range: &Range) -> Result<(Version, &'a Entry), String> {
    if doc.versions.is_empty() {
        return Err(format!("package {name} has no versions in the registry"));
    }
    let tag = range.text();
    let tagged = |t: &str| {
        let v = doc.dist_tags.get(t)?.as_str()?;
        Some((Version::parse(v)?, doc.versions.get(v)?))
    };
    if (tag.is_empty() || tag == "latest" || Range::parse(tag).is_none())
        && let Some(found) = tagged(if tag.is_empty() { "latest" } else { tag })
    {
        return Ok(found);
    }
    if let Some((ver, entry)) = tagged("latest")
        && range.matches(&ver)
    {
        return Ok((ver, entry));
    }
    doc.versions
        .iter()
        .filter_map(|(k, v)| Version::parse(k).map(|ver| (ver, v)))
        .filter(|(ver, _)| range.matches(ver))
        .max_by(|a, b| a.0.cmp(&b.0))
        .ok_or_else(|| format!("no version of {name} satisfies '{}'", range.text()))
}

fn resolved_from(doc: &Packument, name: &str, range: &Range) -> Result<Resolved, String> {
    let (version, entry) = pick(doc, name, range)?;
    let tarball = entry
        .dist
        .get("tarball")
        .and_then(|t| t.as_str())
        .ok_or_else(|| format!("{name}@{version} has no tarball in the registry"))?
        .to_string();
    Ok(Resolved {
        version,
        tarball,
        dependencies: string_map(&entry.dependencies),
        optional_dependencies: string_map(&entry.optional_dependencies),
        peer_dependencies: required_peers(&entry.peer_dependencies, &entry.peer_meta),
        platforms: (
            string_list(&entry.os),
            string_list(&entry.cpu),
            string_list(&entry.libc),
        ),
    })
}

pub fn resolve(name: &str, range: &Range) -> Result<Resolved, String> {
    resolved_from(&packument(name)?, name, range)
}

type Job = Box<dyn FnOnce() + Send>;

struct Pool {
    tx: Option<mpsc::Sender<Job>>,
    handles: Vec<JoinHandle<()>>,
}

impl Pool {
    fn new(size: usize) -> Pool {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let handles = (0..size)
            .map(|_| {
                let rx = rx.clone();
                std::thread::spawn(move || {
                    loop {
                        let job = match rx.lock() {
                            Ok(guard) => guard.recv(),
                            Err(_) => return,
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => return,
                        }
                    }
                })
            })
            .collect();
        Pool {
            tx: Some(tx),
            handles,
        }
    }

    fn run(&self, job: impl FnOnce() + Send + 'static) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Box::new(job));
        }
    }

    fn join(&mut self) {
        self.tx.take();
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.join();
    }
}

type DocSlot = Option<Result<Arc<Packument>, String>>;

pub type Outcome = (usize, Result<(), String>);

pub struct Registry {
    docs: Arc<(Mutex<HashMap<String, DocSlot>>, Condvar)>,
    doc_pool: Pool,
    tar_pool: Pool,
    done: Arc<Mutex<Vec<Outcome>>>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry::new()
    }
}

impl Registry {
    pub fn new() -> Registry {
        Registry {
            docs: Arc::new((Mutex::new(HashMap::new()), Condvar::new())),
            doc_pool: Pool::new(16),
            tar_pool: Pool::new(16),
            done: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn prefetch(&self, name: &str) {
        let (lock, _) = &*self.docs;
        let mut docs = lock.lock().unwrap_or_else(|e| e.into_inner());
        if docs.contains_key(name) {
            return;
        }
        docs.insert(name.to_string(), None);
        drop(docs);
        let shared = self.docs.clone();
        let name = name.to_string();
        self.doc_pool.run(move || {
            let result = std::panic::catch_unwind(|| packument(&name).map(Arc::new))
                .unwrap_or_else(|_| Err(format!("cannot read the registry entry of {name}")));
            let (lock, ready) = &*shared;
            lock.lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(name, Some(result));
            ready.notify_all();
        });
    }

    fn packument(&self, name: &str) -> Result<Arc<Packument>, String> {
        self.prefetch(name);
        let (lock, ready) = &*self.docs;
        let mut docs = lock.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(Some(result)) = docs.get(name) {
                return result.clone();
            }
            docs = ready.wait(docs).unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn resolve(&self, name: &str, range: &Range) -> Result<Resolved, String> {
        resolved_from(&*self.packument(name)?, name, range)
    }

    pub fn install(&self, id: usize, name: &str, resolved: &Resolved, dest: PathBuf) {
        let url = resolved.tarball.clone();
        let label = format!("{name}@{}", resolved.version);
        let done = self.done.clone();
        self.tar_pool.run(move || {
            let result = std::panic::catch_unwind(|| download_into(&url, &label, &dest))
                .unwrap_or_else(|_| Err(format!("cannot unpack {label}")));
            done.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((id, result));
        });
    }

    pub fn finish(mut self) -> Vec<Outcome> {
        self.tar_pool.join();
        self.doc_pool.join();
        std::mem::take(&mut *self.done.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

fn download_into(url: &str, label: &str, dest: &Path) -> Result<(), String> {
    let tgz =
        crate::io::http_get(url, None).map_err(|e| format!("cannot download {label}: {e}"))?;
    std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    extract_tarball(&tgz, dest)?;
    if node_platform() == "linux" {
        let lib = package_json(dest)
            .map(|d| string_list(d.get("libc").unwrap_or(&serde_json::Value::Null)))
            .unwrap_or_default();
        if !allowed(&lib, libc()) {
            let _ = std::fs::remove_dir_all(dest);
            return Err(UNSUPPORTED.to_string());
        }
    }
    Ok(())
}

pub fn latest_version(name: &str) -> Result<Version, String> {
    Ok(resolve(name, &Range::any())?.version)
}

fn octal(field: &[u8]) -> u64 {
    let text: String = field
        .iter()
        .take_while(|b| **b != 0 && **b != b' ')
        .map(|b| *b as char)
        .collect();
    u64::from_str_radix(text.trim(), 8).unwrap_or(0)
}

fn text(field: &[u8]) -> String {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn safe_relative(path: &str) -> Option<PathBuf> {
    let mut parts = Path::new(path).components();
    parts.next()?;
    let mut out = PathBuf::new();
    for c in parts {
        match c {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn pax_path(data: &[u8]) -> Option<String> {
    let mut rest = data;
    while !rest.is_empty() {
        let space = rest.iter().position(|b| *b == b' ')?;
        let len: usize = std::str::from_utf8(&rest[..space]).ok()?.parse().ok()?;
        if len == 0 || len > rest.len() {
            return None;
        }
        let record = &rest[space + 1..len];
        let record = std::str::from_utf8(record).ok()?.trim_end_matches('\n');
        if let Some(p) = record.strip_prefix("path=") {
            return Some(p.to_string());
        }
        rest = &rest[len..];
    }
    None
}

pub fn extract_tarball(tgz: &[u8], dest: &Path) -> Result<usize, String> {
    let tar = crate::builtins::node::gunzip_members(tgz)
        .map_err(|e| format!("cannot decompress tarball: {e}"))?;
    let mut pos = 0;
    let mut long_name: Option<String> = None;
    let mut files = 0;
    while pos + 512 <= tar.len() {
        let header = &tar[pos..pos + 512];
        pos += 512;
        if header.iter().all(|b| *b == 0) {
            break;
        }
        let size = octal(&header[124..136]) as usize;
        let kind = header[156];
        let mut name = text(&header[0..100]);
        let prefix = text(&header[345..500]);
        if !prefix.is_empty() && &header[257..262] == b"ustar" {
            name = format!("{prefix}/{name}");
        }
        if let Some(n) = long_name.take() {
            name = n;
        }
        let data = &tar[pos..(pos + size).min(tar.len())];
        pos += size.div_ceil(512) * 512;
        match kind {
            b'L' => {
                long_name = Some(text(data));
                continue;
            }
            b'x' => {
                if let Some(p) = pax_path(data) {
                    long_name = Some(p);
                }
                continue;
            }
            b'g' => continue,
            b'0' | 0 | b'5' => {}
            _ => continue,
        }
        let Some(rel) = safe_relative(&name) else {
            continue;
        };
        let target = dest.join(&rel);
        if kind == b'5' {
            std::fs::create_dir_all(&target)
                .map_err(|e| format!("cannot create {}: {e}", target.display()))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&target, data)
            .map_err(|e| format!("cannot write {}: {e}", target.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if octal(&header[100..108]) & 0o111 != 0 {
                let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
            }
        }
        files += 1;
    }
    Ok(files)
}

pub const UNSUPPORTED: &str = "unsupported platform";

fn package_json(dir: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn installed_version(dir: &Path) -> Option<Version> {
    let doc = package_json(dir)?;
    Version::parse(doc.get("version")?.as_str()?)
}

pub type DependencySets = (
    IndexMap<String, String>,
    IndexMap<String, String>,
    IndexMap<String, String>,
);

pub fn package_dependency_sets(dir: &Path) -> DependencySets {
    let Some(doc) = package_json(dir) else {
        return Default::default();
    };
    let null = serde_json::Value::Null;
    let field = |k: &str| doc.get(k).unwrap_or(&null);
    (
        string_map(field("dependencies")),
        string_map(field("optionalDependencies")),
        required_peers(field("peerDependencies"), field("peerDependenciesMeta")),
    )
}

pub fn split_name_range(spec: &str) -> (String, String) {
    let at = match spec.strip_prefix('@') {
        Some(rest) => rest.find('@').map(|i| i + 1),
        None => spec.find('@'),
    };
    match at {
        Some(i) => (spec[..i].to_string(), spec[i + 1..].to_string()),
        None => (spec.to_string(), String::new()),
    }
}
