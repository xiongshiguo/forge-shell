#!/usr/bin/env bash
# 熔炉 ForgeShell 多平台构建脚本
# 用法: ./build.sh [windows|linux|ohos|all]

set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$PROJECT_DIR"

build_windows() {
    echo "🔨 构建 Windows 静态二进制 (x86_64-pc-windows-msvc)..."
    cargo build --release --target x86_64-pc-windows-msvc
    echo "✅ Windows 构建完成: target/x86_64-pc-windows-msvc/release/forge-shell.exe"
}

build_linux() {
    echo "🔨 构建 Linux Musl 静态二进制 (x86_64-unknown-linux-musl)..."
    rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
    cargo build --release --target x86_64-unknown-linux-musl
    echo "✅ Linux 构建完成: target/x86_64-unknown-linux-musl/release/forge-shell"
}

build_ohos() {
    echo "🔨 构建鸿蒙兼容 Linux 静态二进制..."
    # 鸿蒙 PC 端通过"融合开发引擎"直接运行 Linux 静态二进制
    rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
    cargo build --release --target x86_64-unknown-linux-musl
    echo "✅ 鸿蒙构建完成: target/x86_64-unknown-linux-musl/release/forge-shell"
}

build_all() {
    echo "🔥 开始全平台构建..."
    build_linux
    build_windows
    build_ohos
    echo "✅ 全平台构建完成"
}

case "${1:-all}" in
    windows) build_windows ;;
    linux)   build_linux ;;
    ohos)    build_ohos ;;
    all)     build_all ;;
    *)
        echo "用法: $0 [windows|linux|ohos|all]"
        exit 1
        ;;
esac
