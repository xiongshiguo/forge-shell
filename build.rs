/// L5: 版本号自动从 git tag 获取，无需手动同步 Cargo.toml
fn main() {
    // 优先从 git tag 读取（永远和发布版本一致）
    let version = std::process::Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_start_matches('v').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // 回退：无 git 时用 Cargo.toml
            std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into())
        });

    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into());
    let dest = std::path::Path::new(&out_dir).join("version.rs");
    std::fs::write(&dest, format!("pub const VERSION: &str = \"{}\";\n", version))
        .expect("failed to write version.rs");

    // L5: 不设 rerun-if-changed，每次 cargo build 都重新检测 git tag
    // git describe 极其轻量（<1ms），不会影响构建速度
}
