/// 生成版本号文件（解决增量编译时 CARGO_PKG_VERSION 不更新的问题）
fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into());
    let dest = std::path::Path::new(&out_dir).join("version.rs");
    std::fs::write(&dest, format!("pub const VERSION: &str = \"{}\";\n", version))
        .expect("failed to write version.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets/web/");
}
