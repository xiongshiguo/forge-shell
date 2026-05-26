@echo off
taskkill /F /IM forge-shell.exe >/dev/null 2>&1
timeout /t 1 /nobreak >nul
cargo build --release --bin forge-shell
if %errorlevel% equ 0 start '' target/release/forge-shell.exe
