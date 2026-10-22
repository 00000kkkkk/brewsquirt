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
    let c = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let text = c.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix("UNC\\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        return PathBuf::from(rest);
    }
    c
}

pub const EXTENSIONS: [&str; 5] = [EXT, "js", "mjs", "cjs", "json"];

fn with_extension(p: &Path, ext: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

const IMPORT_CONDITIONS: [&str; 4] = ["import", "node", "module-sync", "default"];
const REQUIRE_CONDITIONS: [&str; 4] = ["require", "node", "module-sync", "default"];

fn conditions(cjs: bool) -> &'static [&'static str] {
    if cjs {
        &REQUIRE_CONDITIONS
    } else {
        &IMPORT_CONDITIONS
    }
}

fn export_target(value: &serde_json::Value, cjs: bool) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(items) => items.iter().find_map(|v| export_target(v, cjs)),
        serde_json::Value::Object(map) => {
            let active = conditions(cjs);
            for (key, v) in map {
                if active.contains(&key.as_str()) {
                    if let Some(t) = export_target(v, cjs) {
                        return Some(t);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn exports_lookup(exports: &serde_json::Value, sub: &str, cjs: bool) -> Option<String> {
    let key = if sub.is_empty() {
        ".".to_string()
    } else {
        format!("./{sub}")
    };
    match exports {
        serde_json::Value::Object(map) => {
            let is_map = map.keys().any(|k| k.starts_with('.'));
            if !is_map {
                return if sub.is_empty() {
                    export_target(exports, cjs)
                } else {
                    None
                };
            }
            if let Some(v) = map.get(&key) {
                return export_target(v, cjs);
            }
            for (pattern, v) in map {
                if let Some((prefix, suffix)) = pattern.split_once('*') {
                    if key.starts_with(prefix)
                        && key.ends_with(suffix)
                        && key.len() >= prefix.len() + suffix.len()
                    {
                        let star = &key[prefix.len()..key.len() - suffix.len()];
                        return export_target(v, cjs).map(|t| t.replace('*', star));
                    }
                }
            }
            None
        }
        _ => {
            if sub.is_empty() {
                export_target(exports, cjs)
            } else {
                None
            }
        }
    }
}

fn package_json(dir: &Path) -> Option<serde_json::Value> {
    for manifest in [MANIFEST, "package.json"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(manifest)) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                return Some(v);
            }
        }
    }
    None
}

fn resolve_in_package(dir: &Path, target: &str) -> Option<PathBuf> {
    let p = dir.join(target);
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
        return index_file(&p);
    }
    None
}

pub fn package_subpath(dir: &Path, sub: &str, cjs: bool) -> Option<PathBuf> {
    if let Some(v) = package_json(dir) {
        if let Some(exports) = v.get("exports") {
            if let Some(target) = exports_lookup(exports, sub, cjs) {
                if let Some(found) = resolve_in_package(dir, &target) {
                    return Some(found);
                }
            }
        }
    }
    if sub.is_empty() {
        package_main(dir, cjs)
    } else {
        resolve_file(&dir.join(sub))
    }
}

fn package_main(dir: &Path, cjs: bool) -> Option<PathBuf> {
    if let Some(v) = package_json(dir) {
        let mut candidates: Vec<String> = Vec::new();
        if let Some(target) = v.get("exports").and_then(|e| exports_lookup(e, "", cjs)) {
            candidates.push(target);
        }
        for key in ["main", "module"] {
            if let Some(m) = v.get(key).and_then(|m| m.as_str()) {
                candidates.push(m.to_string());
            }
        }
        for main in candidates {
            if let Some(found) = resolve_in_package(dir, &main) {
                return Some(found);
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
        return package_main(p, false);
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

pub fn is_commonjs(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    ext == "cjs"
        || ext == "json"
        || (ext == "js"
            && std::fs::read_to_string(path)
                .map(|s| looks_like_commonjs(&s))
                .unwrap_or(false))
}

pub fn looks_like_commonjs(src: &str) -> bool {
    let has_cjs = src.lines().any(|line| {
        let t = line.trim_start();
        t.contains("module.exports") || t.contains("exports.") || t.contains("require(")
    });
    has_cjs && !has_module_syntax(src)
}

fn imports_lookup(imports: &serde_json::Value, spec: &str, cjs: bool) -> Option<String> {
    let map = imports.as_object()?;
    if let Some(v) = map.get(spec) {
        return export_target(v, cjs);
    }
    for (pattern, v) in map {
        if let Some((prefix, suffix)) = pattern.split_once('*') {
            if spec.starts_with(prefix)
                && spec.ends_with(suffix)
                && spec.len() >= prefix.len() + suffix.len()
            {
                let star = &spec[prefix.len()..spec.len() - suffix.len()];
                return export_target(v, cjs).map(|t| t.replace('*', star));
            }
        }
    }
    None
}

fn resolve_import_alias(spec: &str, from_dir: &Path, cjs: bool) -> Result<PathBuf, String> {
    let mut dir = Some(from_dir.to_path_buf());
    while let Some(d) = dir {
        if d.join("package.json").is_file() || d.join(MANIFEST).is_file() {
            let target = package_json(&d)
                .and_then(|v| v.get("imports").cloned())
                .and_then(|imports| imports_lookup(&imports, spec, cjs));
            if let Some(target) = target {
                if target.starts_with("./") || target.starts_with("../") {
                    if let Some(found) = resolve_in_package(&d, &target) {
                        return Ok(canonical(&found));
                    }
                } else {
                    return resolve_with(&target, &d, cjs);
                }
            }
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Err(format!("Cannot find module '{spec}'"))
}

pub fn resolve(spec: &str, from_dir: &Path) -> Result<PathBuf, String> {
    resolve_with(spec, from_dir, false)
}

pub fn resolve_with(spec: &str, from_dir: &Path, cjs: bool) -> Result<PathBuf, String> {
    if spec.starts_with('#') {
        return resolve_import_alias(spec, from_dir, cjs);
    }
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
            let found = package_subpath(&candidate, sub, cjs);
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
