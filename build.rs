/// 强制 Cargo.toml 或前端文件变化时重新编译
fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=assets/web/");
}
