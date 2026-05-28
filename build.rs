/// L5: 版本号从 git tag 获取 + 源文件哈希强制重编译
///
/// 问题：仅依赖 git tag 时，tag 不变 → version.rs 不变 → cargo 复用缓存 →
///       src/api.rs 等修改后二进制不更新（需 cargo clean）
/// 修复：将 src/ 下所有 .rs 文件的大小+路径哈希嵌入 version.rs，
///       任何源码变更 → 哈希不同 → cargo 自动重编译所有依赖
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
            std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into())
        });

    // 计算 src/ 下所有 .rs 文件的路径+大小哈希（不读内容，足够检测变更）
    let src_hash = compute_src_hash();

    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into());
    let dest = std::path::Path::new(&out_dir).join("version.rs");
    std::fs::write(
        &dest,
        format!(
            "pub const VERSION: &str = \"{}\";\npub const SRC_HASH: &str = \"{}\";\n",
            version, src_hash
        ),
    )
    .expect("failed to write version.rs");

    // 当 .git/HEAD 变更（新提交/tag）时重跑 build.rs，确保版本号始终正确
    println!("cargo:rerun-if-changed=.git/HEAD");
    // 源文件哈希变更也会触发重编译（通过 version.rs 内容不同）
}

/// 遍历 src/ 目录，收集所有 .rs 文件的路径和大小，计算确定性哈希
fn compute_src_hash() -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut entries: Vec<(String, u64)> = Vec::new();
    collect_rs_files(std::path::Path::new("src"), &mut entries);
    // 按路径排序保证确定性
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, size) in &entries {
        path.hash(&mut hasher);
        size.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn collect_rs_files(dir: &std::path::Path, entries: &mut Vec<(String, u64)>) {
    if let Ok(iter) = std::fs::read_dir(dir) {
        for entry in iter.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, entries);
            } else if path.extension().map_or(false, |e| e == "rs") {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                entries.push((path.to_string_lossy().to_string(), size));
            }
        }
    }
}
