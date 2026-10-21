use std::path::{Path, PathBuf};

pub const DEPS_DIR: &str = "nude_modules";
pub const MANIFEST: &str = "squirt.json";
pub const EXT: &str = "jsq";

pub fn display_path(p: &Path) -> String {
    let s = p.display().to_string();
    let s = s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s);
    if let Ok(cwd) = std::env::current_dir() {
        let cwd = cwd.display().to_string();
        let cwd = cwd.strip_prefix(r"\\?\").unwrap_or(&cwd).to_string();
        if let Some(rest) = s.strip_prefix(&cwd) {
            let rest = rest.trim_start_matches(['/', '\\']);
            if !rest.is_empty() {
                return rest.replace('\\', "/");
            }
        }
    }
    s
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn package_main(dir: &Path) -> Option<PathBuf> {
    let manifest = dir.join(MANIFEST);
    if let Ok(text) = std::fs::read_to_string(&manifest) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(main) = v.get("main").and_then(|m| m.as_str()) {
                let p = dir.join(main);
                if p.is_file() {
                    return Some(p);
                }
                let with_ext = p.with_extension(EXT);
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }
    let index = dir.join(format!("index.{EXT}"));
    index.is_file().then_some(index)
}

fn resolve_file(p: &Path) -> Option<PathBuf> {
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    let mut with_ext = p.as_os_str().to_owned();
    with_ext.push(".");
    with_ext.push(EXT);
    let with_ext = PathBuf::from(with_ext);
    if with_ext.is_file() {
        return Some(with_ext);
    }
    if p.is_dir() {
        return package_main(p);
    }
    None
}

pub fn resolve(spec: &str, from_dir: &Path) -> Result<PathBuf, String> {
    let relative = spec.starts_with("./")
        || spec.starts_with("../")
        || spec.starts_with('/')
        || spec == "."
        || spec == ".."
        || Path::new(spec).is_absolute();
    if relative {
        let target = from_dir.join(spec);
        return resolve_file(&target)
            .map(|p| canonical(&p))
            .ok_or_else(|| format!("Cannot find module '{spec}'"));
    }
    let mut parts = spec.splitn(if spec.starts_with('@') { 3 } else { 2 }, '/');
    let mut pkg = parts.next().unwrap_or_default().to_string();
    if spec.starts_with('@') {
        if let Some(scope_name) = parts.next() {
            pkg = format!("{pkg}/{scope_name}");
        }
    }
    let sub = parts.next().unwrap_or("");
    let mut dir = Some(from_dir.to_path_buf());
    while let Some(d) = dir {
        let candidate = d.join(DEPS_DIR).join(&pkg);
        if candidate.is_dir() {
            let found = if sub.is_empty() {
                package_main(&candidate)
            } else {
                resolve_file(&candidate.join(sub))
            };
            return found.map(|p| canonical(&p)).ok_or_else(|| {
                format!("Cannot find module '{spec}': package '{pkg}' has no entry point")
            });
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Err(format!(
        "Cannot find module '{spec}'. Did you forget to run `vrotdaem gluboko --force`?"
    ))
}
