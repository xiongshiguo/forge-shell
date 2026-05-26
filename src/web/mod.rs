//! 熔炉 Web UI 模块
//! 本地地址: http://localhost:9527
//! 技术: axum + rust-embed，静态文件编译进二进制

pub mod api;
pub mod static_files;

use crate::config::Config;
use crate::agent::orchestrator::Orchestrator;
use crate::engine::context::ContextManager;
use crate::engine::tools::backup::BackupManager;
use crate::evolution::EvolutionCoordinator;
use std::sync::Arc;
use tokio::sync::Mutex;
use axum::{Router, routing::get};
use tower_http::cors::CorsLayer;

/// 限流计数器
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

struct RateLimiter {
    counts: Mutex<HashMap<String, (u64, std::time::Instant)>>,
    max_per_minute: u64,
}

impl RateLimiter {
    fn new(max_per_minute: u64) -> Self {
        Self { counts: Mutex::new(HashMap::new()), max_per_minute }
    }

    async fn check(&self, key: &str) -> bool {
        let mut map = self.counts.lock().await;
        let now = std::time::Instant::now();
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1).as_secs() > 60 {
            *entry = (1, now);
            true
        } else if entry.0 >= self.max_per_minute {
            false
        } else {
            entry.0 += 1;
            true
        }
    }
}

/// Web 应用共享状态
pub struct AppState {
    pub config: Mutex<Config>,
    pub context_manager: Mutex<ContextManager>,
    pub orchestrator: Orchestrator,
    pub total_cost: Mutex<f64>,
    pub cache_hit_rate: Mutex<f64>,
    pub active_agents: Mutex<usize>,
    pub has_api_key: bool,
    pub cache_stats: Mutex<crate::engine::inference::TokenUsage>,
    pub project_fingerprint: Mutex<String>,
    pub session_turn: Mutex<usize>,
    pub session_summaries: Mutex<Vec<String>>,
    pub conversation_history: Mutex<Vec<crate::engine::inference::ChatMessage>>,
    pub evolution: Mutex<EvolutionCoordinator>,
    pub backup: Mutex<BackupManager>,
    pub semantic_index: crate::engine::semantic_index::SemanticIndex,
    pub prompt_optimizer: Mutex<crate::engine::prompt_optimizer::PromptOptimizer>,
    pub rate_limiter: RateLimiter,
    pub error_logger: crate::error_log::ErrorLogger,
}

pub type SharedState = Arc<AppState>;

/// 启动 Web 服务
pub async fn run_web(config: Config) -> anyhow::Result<()> {
    let max_agents = config.engine.max_parallel_agents;
    let cache_entries = config.engine.session_cache_rounds * 20 + 10;
    let session_rounds = config.engine.session_cache_rounds;
    let mut ctx_mgr = ContextManager::new(cache_entries, session_rounds);
    ctx_mgr.init_system_prompt(&crate::system_prompt::get_system_prompt());

    let has_key = !config.effective_api_key().is_empty();
    let state = Arc::new(AppState {
        config: Mutex::new(config),
        context_manager: Mutex::new(ctx_mgr),
        orchestrator: Orchestrator::new(max_agents),
        total_cost: Mutex::new(0.0),
        cache_hit_rate: Mutex::new(0.0),
        active_agents: Mutex::new(0),
        has_api_key: has_key,
        cache_stats: Mutex::new(crate::engine::inference::TokenUsage::default()),
        project_fingerprint: Mutex::new(String::new()),
        session_turn: Mutex::new(0),
        session_summaries: Mutex::new(Vec::new()),
        conversation_history: Mutex::new(Vec::new()),
        evolution: Mutex::new(EvolutionCoordinator::new(
            crate::config::forge_data_dir().join("evolution")
        )),
        backup: Mutex::new(BackupManager::new(
            crate::config::forge_data_dir().join("backups")
        )),
        semantic_index: crate::engine::semantic_index::SemanticIndex::new(
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        ),
        prompt_optimizer: Mutex::new(crate::engine::prompt_optimizer::PromptOptimizer::new()),
        rate_limiter: RateLimiter::new(50),
        error_logger: crate::error_log::ErrorLogger::new(crate::config::forge_data_dir().join("logs")),
    });

    let app = Router::new()
        // API 路由
        .route("/api/chat", axum::routing::post(api::chat_handler))
        .route("/api/status", get(api::status_handler))
        .route("/api/cost", get(api::cost_handler))
        .route("/api/project", get(api::project_handler))
        .route("/api/setup", axum::routing::post(api::setup_handler))
        .route("/api/check-key", get(api::check_key_handler))
        .route("/api/ping", get(api::ping_handler))
        .route("/api/evolution", get(api::evolution_handler))
        .route("/api/update-check", get(api::update_check_handler))
        .route("/api/update-now", axum::routing::post(api::update_now_handler))
        .route("/api/read", axum::routing::post(api::read_handler))
        .route("/api/search", axum::routing::post(api::search_handler))
        .route("/api/web-search", axum::routing::post(api::web_search_handler))
        .route("/api/lsp", axum::routing::post(api::lsp_handler))
        .route("/api/lsp-rich", axum::routing::post(api::lsp_rich_handler))
        .route("/api/edit", axum::routing::post(api::edit_handler))
        .route("/api/snapshot", axum::routing::post(api::snapshot_handler))
        .route("/api/files", get(api::files_handler))
        .route("/api/explore", get(api::explore_handler))
        .route("/api/parallel", axum::routing::post(api::parallel_handler))
        .route("/api/logs", get(api::error_logs_handler))
        .route("/api/logs/clear", axum::routing::post(api::error_logs_clear_handler))
        .route("/api/session/auto-save", axum::routing::post(api::session_auto_save_handler))
        .route("/api/session/latest", get(api::session_latest_handler))
        .route("/api/session/save", axum::routing::post(api::session_save_handler))
        .route("/api/sessions", get(api::sessions_list_handler))
        .route("/api/session/delete", axum::routing::post(api::session_delete_handler))
        .route("/api/infer", axum::routing::post(api::infer_handler))
        .route("/api/structure", get(api::structure_handler))
        .route("/api/mcp", axum::routing::post(api::mcp_handler))
        .route("/api/cache-monitor", get(api::cache_monitor_handler))
        .route("/api/semantic", get(api::semantic_handler))
        .route("/api/prompt-stats", get(api::prompt_stats_handler))
        .route("/api/exec", axum::routing::post(api::exec_handler))
        .route("/api/auto-fix", get(api::auto_fix_handler))
        .route("/api/rollback", axum::routing::post(api::rollback_handler))
        .route("/api/save-context", axum::routing::post(api::save_context_handler))
        .route("/api/review/submit", axum::routing::post(api::review_submit_handler))
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
