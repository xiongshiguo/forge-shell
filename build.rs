/// 强制 Cargo.toml 版本号变化时重新编译
fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
}
