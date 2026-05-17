@echo off
REM 熔炉 ForgeShell 多平台构建脚本 (Windows)
REM 用法: build.bat [windows|linux|ohos|all]

cd /d "%~dp0"

if "%~1"=="" goto :build_all
if "%~1"=="windows" goto :build_windows
if "%~1"=="linux" goto :build_linux
if "%~1"=="ohos" goto :build_ohos
if "%~1"=="all" goto :build_all
echo 用法: build.bat [windows^|linux^|ohos^|all]
exit /b 1

:build_windows
echo 🔨 构建 Windows 静态二进制...
cargo build --release --target x86_64-pc-windows-msvc
echo ✅ Windows 构建完成
goto :end

:build_linux
echo 🔨 构建 Linux Musl 静态二进制...
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
echo ✅ Linux 构建完成
goto :end

:build_ohos
echo 🔨 构建鸿蒙兼容 Linux 静态二进制...
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
echo ✅ 鸿蒙构建完成
goto :end

:build_all
echo 🔥 开始全平台构建...
call :build_linux
call :build_windows
call :build_ohos
echo ✅ 全平台构建完成
goto :end

:end
