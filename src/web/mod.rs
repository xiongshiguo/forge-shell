//! 熔炉 Web UI 模块
//! 本地地址: http://localhost:9527
//! 技术: axum + rust-embed，静态文件编译进二进制

pub mod api;
pub mod static_files;

use crate::config::Config;
use crate::agent::orchestrator::Orchestrator;
use crate::agent::dispatcher::Dispatcher;
use crate::engine::context::ContextManager;
use std::sync::Arc;
use tokio::sync::Mutex;
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;

/// Web 应用共享状态
pub struct AppState {
    pub config: Config,
    pub context_manager: Mutex<ContextManager>,
    pub orchestrator: Orchestrator,
    pub total_cost: Mutex<f64>,
    pub cache_hit_rate: Mutex<f64>,
    pub active_agents: Mutex<usize>,
}

pub type SharedState = Arc<AppState>;

/// 启动 Web 服务
pub async fn run_web(config: Config) -> anyhow::Result<()> {
    let max_agents = config.engine.max_parallel_agents;
    let cache_entries = config.engine.session_cache_rounds * 20 + 10;
    let session_rounds = config.engine.session_cache_rounds;
    let mut ctx_mgr = ContextManager::new(cache_entries, session_rounds);
    ctx_mgr.init_system_prompt(crate::tui::app::SYSTEM_PROMPT);

    let state = Arc::new(AppState {
        config,
        context_manager: Mutex::new(ctx_mgr),
        orchestrator: Orchestrator::new(max_agents),
        total_cost: Mutex::new(0.0),
        cache_hit_rate: Mutex::new(0.94),
        active_agents: Mutex::new(0),
    });

    let app = Router::new()
        // API 路由
        .route("/api/chat", axum::routing::post(api::chat_handler))
        .route("/api/status", get(api::status_handler))
        .route("/api/cost", get(api::cost_handler))
        .route("/api/project", get(api::project_handler))
        // 静态文件
        .route("/", get(static_files::index_html))
        .route("/style.css", get(static_files::style_css))
        .route("/app.js", get(static_files::app_js))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "127.0.0.1:9527";
    let url = format!("http://{}", addr);

    tracing::info!("🌐 熔炉 Web UI 启动: {}", url);

    // 自动打开浏览器
    if webbrowser::open(&url).is_ok() {
        tracing::info!("📂 浏览器已打开");
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
