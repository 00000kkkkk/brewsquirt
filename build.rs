fn main() {
    println!("cargo:rerun-if-changed=src/napi/exports.txt");
    let list = std::fs::read_to_string("src/napi/exports.txt").unwrap_or_default();
    let names: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let target = std::env::var("TARGET").unwrap_or_default();
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    if target.contains("windows") && target.contains("msvc") {
        for n in &names {
            println!("cargo:rustc-link-arg-bin=jsq=/EXPORT:{n}");
        }
    } else if target.contains("apple") {
        let file = out.join("napi_exports.txt");
        let body: String = names.iter().map(|n| format!("_{n}\n")).collect();
        std::fs::write(&file, body).expect("napi export list");
        println!(
            "cargo:rustc-link-arg-bin=jsq=-Wl,-exported_symbols_list,{}",
            file.display()
        );
    } else if target.contains("linux") || target.contains("bsd") {
        let file = out.join("napi_exports.list");
        let body: String = format!(
            "{{\n{}}};\n",
            names
                .iter()
                .map(|n| format!("  {n};\n"))
                .collect::<String>()
        );
        std::fs::write(&file, body).expect("napi export list");
        println!(
            "cargo:rustc-link-arg-bin=jsq=-Wl,--dynamic-list={}",
            file.display()
        );
    }
}
