use crate::token::Token;
use std::path::{Path, PathBuf};

pub const DEPS_DIR: &str = "nude_modules";
pub const NODE_DEPS_DIR: &str = "node_modules";
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

pub fn canonical(p: &Path) -> PathBuf {
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

pub const EXTENSIONS: [&str; 10] = [
    EXT, "js", "mjs", "cjs", "ts", "mts", "cts", "tsx", "jsx", "json",
];

pub fn is_typescript(name: &str) -> bool {
    (name.ends_with(".ts")
        || name.ends_with(".mts")
        || name.ends_with(".cts")
        || name.ends_with(".tsx"))
        && !name.ends_with(".d.ts")
}

pub fn is_jsx(name: &str) -> bool {
    name.ends_with(".jsx") || name.ends_with(".tsx")
}

fn typescript_sibling(p: &Path) -> Option<PathBuf> {
    let ext = p.extension()?.to_str()?;
    let candidates: &[&str] = match ext {
        "js" => &["ts", "tsx"],
        "jsx" => &["tsx"],
        "mjs" => &["mts"],
        "cjs" => &["cts"],
        _ => return None,
    };
    candidates
        .iter()
        .map(|e| p.with_extension(e))
        .find(|q| q.is_file())
}

pub fn ts_options(dir: &Path) -> crate::parser::TsOptions {
    let mut opts = crate::parser::TsOptions::default();
    let mut cur = Some(dir.to_path_buf());
    while let Some(d) = cur {
        if d.file_name()
            .is_some_and(|n| n == DEPS_DIR || n == NODE_DEPS_DIR)
        {
            break;
        }
        let file = ["tsconfig.json", "jsconfig.json"]
            .iter()
            .map(|n| d.join(n))
            .find(|f| f.is_file());
        if let Some(f) = file {
            let mut chain = Vec::new();
            read_config_chain(&f, 0, &mut chain);
            let get = |key: &str| {
                chain.iter().find_map(|(_, v)| {
                    v.get("compilerOptions")
                        .and_then(|o| o.get(key))
                        .and_then(|x| x.as_bool())
                })
            };
            opts.legacy_decorators = get("experimentalDecorators").unwrap_or(false);
            opts.emit_metadata =
                opts.legacy_decorators && get("emitDecoratorMetadata").unwrap_or(false);
            break;
        }
        cur = d.parent().map(Path::to_path_buf);
    }
    opts
}

pub fn jsx_options(dir: &Path) -> crate::parser::JsxOptions {
    let mut opts = crate::parser::JsxOptions::default();
    let mut cur = Some(dir.to_path_buf());
    while let Some(d) = cur {
        if d.file_name()
            .is_some_and(|n| n == DEPS_DIR || n == NODE_DEPS_DIR)
        {
            break;
        }
        let file = ["tsconfig.json", "jsconfig.json"]
            .iter()
            .map(|n| d.join(n))
            .find(|f| f.is_file());
        if let Some(f) = file {
            let mut chain = Vec::new();
            read_config_chain(&f, 0, &mut chain);
            let get = |key: &str| {
                chain.iter().find_map(|(_, v)| {
                    v.get("compilerOptions")
                        .and_then(|o| o.get(key))
                        .and_then(|x| x.as_str())
                        .map(str::to_string)
                })
            };
            if let Some(mode) = get("jsx") {
                opts.automatic = mode != "react";
            }
            if let Some(f) = get("jsxFactory") {
                opts.factory = f;
            }
            if let Some(f) = get("jsxFragmentFactory") {
                opts.fragment = f;
            }
            if let Some(s) = get("jsxImportSource") {
                opts.import_source = s;
            }
            break;
        }
        cur = d.parent().map(Path::to_path_buf);
    }
    opts
}

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
    if let Some(q) = typescript_sibling(p) {
        return Some(q);
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
    has_module_syntax_in(src, false, false)
}

fn has_module_syntax_in(src: &str, jsx: bool, ts: bool) -> bool {
    if !src.contains("import") && !src.contains("export") {
        return false;
    }
    let Ok(tokens) = crate::lexer::tokenize_jsx(src, false, jsx, ts) else {
        return src.lines().any(|line| {
            let t = line.trim_start();
            t.starts_with("import ")
                || t.starts_with("import{")
                || t.starts_with("export ")
                || t.starts_with("export{")
                || t.starts_with("import.meta")
        });
    };
    use crate::token::{P, Tok};
    let ident = |t: &Token, name: &str| matches!(&t.tok, Tok::Ident(n) if &**n == name);
    let punct = |t: &Token, p: P| matches!(&t.tok, Tok::Punct(q) if *q == p);
    for (i, t) in tokens.iter().enumerate() {
        let prev_dot = i > 0 && punct(&tokens[i - 1], P::Dot);
        if prev_dot {
            continue;
        }
        let next = tokens.get(i + 1);
        let after = tokens.get(i + 2);
        if ident(t, "import") {
            let Some(next) = next else {
                continue;
            };
            if punct(next, P::LBrace) || punct(next, P::Star) || matches!(next.tok, Tok::Str(_)) {
                return true;
            }
            if punct(next, P::Dot) && after.is_some_and(|a| ident(a, "meta")) {
                return true;
            }
            if matches!(next.tok, Tok::Ident(_))
                && after.is_some_and(|a| ident(a, "from") || punct(a, P::Comma))
            {
                return true;
            }
        } else if ident(t, "export") {
            let Some(next) = next else {
                continue;
            };
            if punct(next, P::LBrace) || punct(next, P::Star) {
                return true;
            }
            if let Tok::Ident(n) = &next.tok {
                if matches!(
                    &**n,
                    "default" | "const" | "let" | "var" | "function" | "class" | "async"
                ) {
                    return true;
                }
            }
        }
    }
    false
}

thread_local! {
    static CJS_CACHE: std::cell::RefCell<std::collections::HashMap<PathBuf, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn package_type(dir: &Path) -> Option<String> {
    let mut cur = Some(dir.to_path_buf());
    while let Some(d) = cur {
        if let Some(doc) = package_json(&d) {
            return Some(
                doc.get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("commonjs")
                    .to_string(),
            );
        }
        cur = d.parent().map(Path::to_path_buf);
    }
    None
}

pub fn mark_esm(path: &Path) {
    CJS_CACHE.with(|c| c.borrow_mut().insert(path.to_path_buf(), false));
}

pub fn esm_only_error(message: &str) -> bool {
    message.contains("top level of modules")
        || message.contains("import.meta")
        || message.contains("Cannot use import statement")
        || message.contains("Unexpected token 'export'")
}

pub fn redeclares_wrapper(message: &str) -> bool {
    message
        .strip_prefix("Identifier '")
        .and_then(|m| m.strip_suffix("' has already been declared"))
        .is_some_and(|name| {
            matches!(
                name,
                "exports" | "require" | "module" | "__filename" | "__dirname"
            )
        })
}

pub fn explicit_commonjs(dir: &Path) -> bool {
    matches!(package_type(dir).as_deref(), Some("commonjs"))
}

pub fn is_commonjs(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "cjs" | "cts" | "json" => return true,
        "js" | "ts" | "jsx" | "tsx" => {}
        _ => return false,
    }
    if let Some(v) = CJS_CACHE.with(|c| c.borrow().get(path).copied()) {
        return v;
    }
    let src = std::fs::read_to_string(path).unwrap_or_default();
    let name = path.display().to_string();
    let result = if has_module_syntax_in(&src, is_jsx(&name), is_typescript(&name)) {
        false
    } else if looks_like_commonjs(&src) {
        true
    } else {
        let dir = path.parent().unwrap_or(Path::new("."));
        !matches!(package_type(dir).as_deref(), Some("module"))
    };
    CJS_CACHE.with(|c| c.borrow_mut().insert(path.to_path_buf(), result));
    result
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

struct PathConfig {
    paths: Vec<(String, Vec<String>)>,
    paths_base: PathBuf,
    base_url: Option<PathBuf>,
}

thread_local! {
    static PATH_CONFIGS: std::cell::RefCell<std::collections::HashMap<PathBuf, Option<std::rc::Rc<PathConfig>>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

pub fn lenient_json(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if matches!(chars.get(j), Some('}') | Some(']')) {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn read_config_chain(file: &Path, depth: u32, out: &mut Vec<(PathBuf, serde_json::Value)>) {
    if depth > 8 {
        return;
    }
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&lenient_json(&text)) else {
        return;
    };
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let parents: Vec<String> = match v.get("extends") {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    out.push((dir.clone(), v));
    for parent in parents {
        let target = if parent.starts_with('.') || Path::new(&parent).is_absolute() {
            dir.join(&parent)
        } else {
            let mut found = None;
            let mut d = Some(dir.clone());
            while let Some(cur) = d {
                for deps in [DEPS_DIR, NODE_DEPS_DIR] {
                    let c = cur.join(deps).join(&parent);
                    if c.exists() {
                        found = Some(c);
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
                d = cur.parent().map(Path::to_path_buf);
            }
            match found {
                Some(f) => f,
                None => continue,
            }
        };
        let target = if target.is_dir() {
            target.join("tsconfig.json")
        } else if target.extension().is_none() {
            with_extension(&target, "json")
        } else {
            target
        };
        read_config_chain(&target, depth + 1, out);
    }
}

fn load_path_config(file: &Path) -> Option<PathConfig> {
    let mut chain = Vec::new();
    read_config_chain(file, 0, &mut chain);
    let mut paths = None;
    let mut paths_base = None;
    let mut base_url = None;
    for (dir, v) in &chain {
        let Some(opts) = v.get("compilerOptions") else {
            continue;
        };
        if base_url.is_none() {
            if let Some(b) = opts.get("baseUrl").and_then(|b| b.as_str()) {
                base_url = Some(dir.join(b));
            }
        }
        if paths.is_none() {
            if let Some(map) = opts.get("paths").and_then(|p| p.as_object()) {
                let list: Vec<(String, Vec<String>)> = map
                    .iter()
                    .map(|(k, v)| {
                        let targets = v
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|t| t.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (k.clone(), targets)
                    })
                    .collect();
                paths = Some(list);
                paths_base = Some(dir.clone());
            }
        }
    }
    if paths.is_none() && base_url.is_none() {
        return None;
    }
    let paths_base = base_url.clone().or(paths_base).unwrap_or_default();
    Some(PathConfig {
        paths: paths.unwrap_or_default(),
        paths_base,
        base_url,
    })
}

fn path_config_for(dir: &Path) -> Option<std::rc::Rc<PathConfig>> {
    if let Some(hit) = PATH_CONFIGS.with(|c| c.borrow().get(dir).cloned()) {
        return hit;
    }
    let mut result = None;
    let mut cur = Some(dir.to_path_buf());
    while let Some(d) = cur {
        if d.file_name()
            .is_some_and(|n| n == DEPS_DIR || n == NODE_DEPS_DIR)
        {
            break;
        }
        let ts = d.join("tsconfig.json");
        let js = d.join("jsconfig.json");
        let file = if ts.is_file() {
            Some(ts)
        } else if js.is_file() {
            Some(js)
        } else {
            None
        };
        if let Some(f) = file {
            result = load_path_config(&f).map(std::rc::Rc::new);
            break;
        }
        cur = d.parent().map(Path::to_path_buf);
    }
    PATH_CONFIGS.with(|c| c.borrow_mut().insert(dir.to_path_buf(), result.clone()));
    result
}

fn config_paths_resolve(spec: &str, from_dir: &Path) -> Option<PathBuf> {
    let config = path_config_for(from_dir)?;
    let mut best: Option<(usize, &str, &Vec<String>)> = None;
    for (pattern, targets) in &config.paths {
        let captured = match pattern.split_once('*') {
            Some((prefix, suffix)) => {
                if spec.len() >= prefix.len() + suffix.len()
                    && spec.starts_with(prefix)
                    && spec.ends_with(suffix)
                {
                    Some((prefix.len(), &spec[prefix.len()..spec.len() - suffix.len()]))
                } else {
                    None
                }
            }
            None => (pattern == spec).then_some((usize::MAX, "")),
        };
        if let Some((rank, cap)) = captured {
            if best.is_none_or(|(r, _, _)| rank > r) {
                best = Some((rank, cap, targets));
            }
        }
    }
    if let Some((_, cap, targets)) = best {
        for t in targets {
            let target = config.paths_base.join(t.replace('*', cap));
            if let Some(found) = resolve_file(&target) {
                return Some(found);
            }
        }
    }
    let base = config.base_url.as_ref()?;
    resolve_file(&base.join(spec))
}

pub fn resolve(spec: &str, from_dir: &Path) -> Result<PathBuf, String> {
    resolve_with(spec, from_dir, false)
}

pub fn resolve_with(spec: &str, from_dir: &Path, cjs: bool) -> Result<PathBuf, String> {
    if spec.starts_with('#') {
        return resolve_import_alias(spec, from_dir, cjs).or_else(|e| {
            config_paths_resolve(spec, from_dir)
                .map(|p| canonical(&p))
                .ok_or(e)
        });
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
    if let Some(found) = config_paths_resolve(spec, from_dir) {
        return Ok(canonical(&found));
    }
    let mut dir = Some(from_dir.to_path_buf());
    while let Some(d) = dir {
        for deps in [DEPS_DIR, NODE_DEPS_DIR] {
            let candidate = d.join(deps).join(&pkg);
            if candidate.is_dir() {
                let found = package_subpath(&candidate, sub, cjs);
                return found.map(|p| canonical(&p)).ok_or_else(|| {
                    format!("Cannot find module '{spec}': package '{pkg}' has no entry point")
                });
            }
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Err(format!(
        "Cannot find module '{spec}'. Did you forget to run `vrotdaem gluboko --force`?"
    ))
}
