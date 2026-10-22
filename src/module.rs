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

pub fn display_path_abs(p: &Path) -> String {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    let s = abs.display().to_string();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

pub const EXTENSIONS: [&str; 5] = [EXT, "js", "mjs", "cjs", "json"];

fn with_extension(p: &Path, ext: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

fn package_main(dir: &Path) -> Option<PathBuf> {
    for manifest in [MANIFEST, "package.json"] {
        let Ok(text) = std::fs::read_to_string(dir.join(manifest)) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let main = v
            .get("main")
            .and_then(|m| m.as_str())
            .or_else(|| v.get("exports").and_then(|e| e.as_str()))
            .or_else(|| {
                v.get("exports").and_then(|e| e.get(".")).and_then(|e| {
                    e.as_str()
                        .or_else(|| e.get("default").and_then(|d| d.as_str()))
                })
            });
        if let Some(main) = main {
            let p = dir.join(main);
            if p.is_file() {
                return Some(p);
            }
            for ext in EXTENSIONS {
                let q = with_extension(&p, ext);
                if q.is_file() {
                    return Some(q);
                }
            }
            if p.is_dir() {
                if let Some(found) = index_file(&p) {
                    return Some(found);
                }
            }
        }
    }
    index_file(dir)
}

fn index_file(dir: &Path) -> Option<PathBuf> {
    EXTENSIONS
        .iter()
        .filter(|e| **e != "json")
        .map(|e| dir.join(format!("index.{e}")))
        .find(|p| p.is_file())
}

fn resolve_file(p: &Path) -> Option<PathBuf> {
    if p.is_file() {
        return Some(p.to_path_buf());
    }
    for ext in EXTENSIONS {
        let q = with_extension(p, ext);
        if q.is_file() {
            return Some(q);
        }
    }
    if p.is_dir() {
        return package_main(p);
    }
    None
}

pub fn has_module_syntax(src: &str) -> bool {
    src.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with("import ")
            || t.starts_with("import{")
            || t.starts_with("import(")
            || t.starts_with("export ")
            || t.starts_with("export{")
            || t.starts_with("import.meta")
    })
}

pub fn looks_like_commonjs(src: &str) -> bool {
    let has_cjs = src.lines().any(|line| {
        let t = line.trim_start();
        t.contains("module.exports") || t.contains("exports.") || t.contains("require(")
    });
    has_cjs && !has_module_syntax(src)
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
