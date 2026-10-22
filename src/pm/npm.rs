use super::semver::{Range, Version};
use indexmap::IndexMap;
use std::path::{Component, Path, PathBuf};

pub const REGISTRY: &str = "https://registry.npmjs.org";

#[derive(Debug, Clone)]
pub struct Resolved {
    pub version: Version,
    pub tarball: String,
    pub dependencies: IndexMap<String, String>,
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

fn packument(name: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/{}", registry_url(), encode_name(name));
    let bytes = crate::io::http_get(&url, Some("application/vnd.npm.install-v1+json"))
        .map_err(|e| format!("cannot fetch {name} from the npm registry: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid registry response for {name}: {e}"))
}

fn deps_of(v: &serde_json::Value) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    if let Some(obj) = v.get("dependencies").and_then(|d| d.as_object()) {
        for (k, val) in obj {
            if let Some(s) = val.as_str() {
                out.insert(k.clone(), s.to_string());
            }
        }
    }
    out
}

fn pick<'a>(
    doc: &'a serde_json::Value,
    name: &str,
    range: &Range,
) -> Result<(Version, &'a serde_json::Value), String> {
    let versions = doc
        .get("versions")
        .and_then(|v| v.as_object())
        .ok_or_else(|| format!("package {name} has no versions in the registry"))?;
    let tag = range.text();
    if let Some(tagged) = doc
        .get("dist-tags")
        .and_then(|t| t.get(if tag.is_empty() { "latest" } else { tag }))
        .and_then(|v| v.as_str())
    {
        if tag.is_empty() || Range::parse(tag).is_none() || tag == "latest" {
            if let (Some(ver), Some(entry)) = (Version::parse(tagged), versions.get(tagged)) {
                return Ok((ver, entry));
            }
        }
    }
    let mut parsed: Vec<(Version, &serde_json::Value)> = versions
        .iter()
        .filter_map(|(k, v)| Version::parse(k).map(|ver| (ver, v)))
        .collect();
    parsed.sort_by(|a, b| a.0.cmp(&b.0));
    let best = parsed
        .iter()
        .rev()
        .find(|(ver, _)| range.matches(ver))
        .ok_or_else(|| format!("no version of {name} satisfies '{}'", range.text()))?;
    Ok((best.0.clone(), best.1))
}

pub fn resolve(name: &str, range: &Range) -> Result<Resolved, String> {
    let doc = packument(name)?;
    let (version, entry) = pick(&doc, name, range)?;
    let tarball = entry
        .get("dist")
        .and_then(|d| d.get("tarball"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| format!("{name}@{version} has no tarball in the registry"))?
        .to_string();
    Ok(Resolved {
        version,
        tarball,
        dependencies: deps_of(entry),
    })
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
        files += 1;
    }
    Ok(files)
}

pub fn install(name: &str, range: &Range, dest: &Path) -> Result<Resolved, String> {
    let resolved = resolve(name, range)?;
    let tgz = crate::io::http_get(&resolved.tarball, None)
        .map_err(|e| format!("cannot download {name}@{}: {e}", resolved.version))?;
    std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    extract_tarball(&tgz, dest)?;
    Ok(resolved)
}

fn package_json(dir: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn installed_version(dir: &Path) -> Option<Version> {
    let doc = package_json(dir)?;
    Version::parse(doc.get("version")?.as_str()?)
}

pub fn package_dependencies(dir: &Path) -> IndexMap<String, String> {
    package_json(dir).map(|d| deps_of(&d)).unwrap_or_default()
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
