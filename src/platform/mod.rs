//! 多平台适配模块
//!
//! 使用 Rust 条件编译隔离平台特定代码：
//! - Windows: x86_64-pc-windows-msvc
//! - Linux:   x86_64-unknown-linux-musl (静态编译)
//! - 鸿蒙:    aarch64-unknown-linux-ohos (通过融合开发引擎运行 Linux 二进制)

/// 获取平台名称
pub fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "ohos") {
        "鸿蒙"
    } else {
        "未知"
    }
}

/// 获取架构名称
pub fn arch_name() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

/// 获取数据目录（平台相关）
pub fn data_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
            .join("forge-shell")
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("forge-shell")
    }
}

/// 获取配置目录（平台相关）
pub fn config_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")))
            .join("forge-shell")
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("forge-shell")
    }
}

/// 获取临时目录
pub fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("forge-shell")
}

/// 清除终端（平台相关）
pub fn clear_terminal() {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/c", "cls"]).status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        print!("\x1B[2J\x1B[1;1H");
    }
}

/// 获取系统内存信息（平台相关）
pub fn system_memory_mb() -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        // Windows API 获取内存信息
        None // 简化实现
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| {
                        l.split_whitespace()
                            .nth(1)
                            .and_then(|n| n.parse::<u64>().ok())
                            .map(|kb| kb / 1024)
                    })
            })
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_name_not_empty() {
        assert!(!platform_name().is_empty());
    }

    #[test]
    fn test_arch_name_not_empty() {
        assert!(!arch_name().is_empty());
    }
}
