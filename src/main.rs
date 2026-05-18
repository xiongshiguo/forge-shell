mod config;
mod error;
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

    let file_appender = tracing_appender::rolling::daily(
        config::forge_data_dir().join("logs"),
        "forge.log",
    );
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_file(true)
        .with_line_number(true);

    let file_layer = fmt::layer()
        .with_target(true)
        .with_ansi(false)
        .with_writer(file_writer);

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

    if cli.tui {
        let mut app = tui::App::new(cfg)?;
        app.run()?;
    } else {
        tracing::info!("🌐 启动 Web UI 模式");
        web::run_web(cfg).await?;
    }

    Ok(())
}
