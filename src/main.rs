mod config;
mod error;
mod locale;
mod utils;
mod agent;
mod engine;
mod tui;
#[cfg(feature = "evolution")]
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

    let cfg = config::Config::load()?;
    tracing::info!("配置加载完成，工作目录: {}", cli.dir);
    tracing::info!("AI 后端: {}", cfg.ai.default_model);
    tracing::info!("缓存命中率目标: ≥97%");

    let mut app = tui::App::new(cfg)?;
    app.run()?;

    Ok(())
}
