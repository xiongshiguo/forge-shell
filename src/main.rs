mod config;
mod error;
mod error_log;
mod locale;
mod utils;
mod platform;
mod system_prompt;
mod agent;
mod engine;
mod tui;
mod web;
mod evolution;

use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// 熔炉 (ForgeShell) — 下一代 AI 编程终端
#[derive(Parser, Debug)]
#[command(name = "forge", about = "熔炉AI编程终端", version)]
struct Cli {
    /// 工作目录
    #[arg(short, long, default_value = ".")]
    dir: String,

    /// 日志级别 (trace/debug/info/warn/error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// 启动 TUI 终端模式（默认启动 Web UI）
    #[arg(long, default_value_t = false)]
    tui: bool,

    /// DeepSeek API Key（也可通过环境变量 DEEPSEEK_API_KEY 设置）
    #[arg(short, long)]
    key: Option<String>,
}

fn setup_logging(log_level: &str) -> anyhow::Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    let log_dir = config::forge_data_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let log_path = log_dir.join(format!("forge-{}.log", today));

    // 验证文件可创建
    std::fs::OpenOptions::new().create(true).append(true).open(&log_path)?;

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_file(true)
        .with_line_number(true);

    let file_layer = fmt::layer()
        .with_target(true)
        .with_ansi(false)
        .with_writer(move || {
            std::fs::OpenOptions::new()
                .create(true).append(true)
                .open(&log_path)
                .expect("日志文件打开失败")
        });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .with(file_layer)
        .init();

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    setup_logging(&cli.log_level)?;

    // L3: 崩溃日志——panic 时写 crash.log，不再静默死亡
    let crash_log = config::forge_data_dir().join("logs").join("crash.log");
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!(
            "PANIC {} {}\n{:?}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            info,
            std::backtrace::Backtrace::capture()
        );
        let _ = std::fs::OpenOptions::new().create(true).append(true)
            .open(&crash_log).map(|mut f| {
                use std::io::Write;
                let _ = f.write_all(msg.as_bytes());
            });
        eprintln!("{}", msg);
    }));

    tracing::info!("🔥 熔炉 (ForgeShell) 启动中...");

    let mut cfg = config::Config::load()?;

    // 命令行传入的 key 优先级最高
    if let Some(ref key) = cli.key {
        cfg.ai.api_key = key.clone();
        tracing::info!("🔑 已从命令行读取 API Key");
    }

    // 检查是否配置了 API Key
    let has_key = !cfg.effective_api_key().is_empty();
    if !has_key {
        if !cli.tui {
            tracing::warn!("⚠ 未配置 API Key，Web UI 将显示欢迎配置页");
        } else {
            anyhow::bail!(
                "未设置 DeepSeek API Key！\n\n请通过以下方式之一设置：\n  forge --key sk-你的key      # 命令行传参（推荐）\n  forge -k sk-你的key          # 简写\n  forge --tui --key sk-你的key  # TUI 终端模式\n  setx DEEPSEEK_API_KEY sk-你的key  # 环境变量（永久）"
            );
        }
    }

    tracing::info!("配置加载完成，工作目录: {}", cli.dir);
    tracing::info!("AI 后端: {}", cfg.ai.default_model);
    tracing::info!("缓存命中率目标: ≥97%");

    // 启动前自动清理旧进程，解决"更新后仍在跑旧版"问题
    kill_old_instance().await;

    if cli.tui {
        let mut app = tui::App::new(cfg)?;
        app.run()?;
    } else {
        tracing::info!("🌐 启动 Web UI 模式");
        web::run_web(cfg).await?;
    }

    Ok(())
}

/// 检查并杀掉之前遗留的 forge-shell 进程
async fn kill_old_instance() {
    let pid_path = config::forge_data_dir().join("forge.pid");
    let mut killed = false;

    // 方式 1：通过 PID 文件杀旧进程
    if let Ok(old_pid) = std::fs::read_to_string(&pid_path) {
        let old_pid = old_pid.trim();
        if !old_pid.is_empty() && old_pid != std::process::id().to_string() {
            tracing::info!("发现旧进程 PID={}，尝试关闭", old_pid);
            kill_pid(old_pid);
            killed = true;
        }
    }

    // 方式 2：无 PID 文件时，通过端口反查进程（解决首次安装问题）
    if !killed {
        if let Some(pid) = find_process_on_port(9527).await {
            tracing::info!("端口 9527 被 PID={} 占用，尝试关闭", pid);
            kill_pid(&pid);
            killed = true;
        }
    }

    if killed {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    }

    // 写入当前 PID
    let _ = std::fs::write(&pid_path, std::process::id().to_string());
}

fn kill_pid(pid: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", pid, "/F"]).output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill").args([pid]).output();
    }
}

#[cfg(windows)]
async fn find_process_on_port(port: u16) -> Option<String> {
    let output = tokio::process::Command::new("cmd")
        .args(["/C", &format!("netstat -ano | findstr :{}", port)])
        .output().await.ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // 提取最后一列的 PID
    text.lines().next()?.split_whitespace().last().map(|s| s.to_string())
}

#[cfg(not(windows))]
async fn find_process_on_port(port: u16) -> Option<String> {
    let output = tokio::process::Command::new("lsof")
        .args(["-t", &format!("-i:{}", port)])
        .output().await.ok()?;
    let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if pid.is_empty() { None } else { Some(pid) }
}
