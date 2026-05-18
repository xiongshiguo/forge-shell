//! Web API 端点

use super::SharedState;
use crate::agent::dispatcher::Dispatcher;
use crate::config::Config;
use axum::{Json, extract::State, response::sse::{Event, Sse, KeepAlive}};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::UnboundedReceiverStream;

// ---- 请求/响应类型 ----

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub reply: String,
    pub tasks_count: usize,
    pub parallel_groups: usize,
    pub parallelism_gain: f64,
}

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    pub api_key: String,
}

#[derive(Debug, Serialize)]
pub struct SetupResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct CheckKeyResponse {
    pub has_key: bool,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub mode: String,
    pub cost: f64,
    pub hit_rate: f64,
    pub active_agents: usize,
    pub max_agents: usize,
    pub memory_mb: f64,
    pub has_key: bool,
}

#[derive(Debug, Serialize)]
pub struct CostResponse {
    pub total_cost: f64,
    pub cache_hit_rate: f64,
    pub cache_saved: f64,
    pub monthly_used: f64,
    pub monthly_budget: f64,
    pub vs_claude_savings_pct: f64,
}

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub name: String,
    pub file_count: usize,
    pub total_lines: usize,
    pub rust_files: usize,
    pub test_files: usize,
    pub recent_commits: Vec<CommitItem>,
}

#[derive(Debug, Serialize)]
pub struct CommitItem {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

// ---- Handler ----

/// 检查是否已配置 API Key
pub async fn check_key_handler(
    State(state): State<SharedState>,
) -> Json<CheckKeyResponse> {
    Json(CheckKeyResponse { has_key: state.has_api_key })
}

/// 首次启动：保存用户输入的 API Key 到本地配置
pub async fn setup_handler(
    State(state): State<SharedState>,
    Json(req): Json<SetupRequest>,
) -> Json<SetupResponse> {
    let key = req.api_key.trim().to_string();
    if key.is_empty() {
        return Json(SetupResponse { success: false, message: "API Key 不能为空".into() });
    }
    if !key.starts_with("sk-") {
        return Json(SetupResponse { success: false, message: "API Key 格式错误，应以 sk- 开头".into() });
    }

    // 保存到配置
    let config_path = crate::config::forge_config_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let mut config = state.config.lock().await;
    config.ai.api_key = key;
    // 也写到环境变量（当前进程）
    unsafe { std::env::set_var("DEEPSEEK_API_KEY", &config.ai.api_key); }

    if let Ok(toml_str) = toml::to_string_pretty(&*config) {
        if std::fs::write(&config_path, toml_str).is_ok() {
            drop(config);
            Json(SetupResponse {
                success: true,
                message: "API Key 已保存！熔炉就绪 🔥".into(),
            })
        } else {
            Json(SetupResponse { success: false, message: "保存配置文件失败，请检查磁盘权限".into() })
        }
    } else {
        Json(SetupResponse { success: false, message: "配置序列化失败".into() })
    }
}

/// SSE 流式对话（调用真实 DeepSeek API）
pub async fn chat_handler(
    State(state): State<SharedState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let state_clone = state.clone();

    tokio::spawn(async move {
        // 构建消息
        let config = state_clone.config.lock().await.clone();
        let client = match crate::engine::inference::InferenceClient::new(&config) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "error", "message": e.to_string()}).to_string()
                )));
                return;
            }
        };

        let messages = vec![
            crate::engine::inference::ChatMessage::system("你是熔炉 ForgeShell，一个中文 AI 编程助手。回答简洁实用，代码示例用 Rust。"),
            crate::engine::inference::ChatMessage::user(&req.message),
        ];

        // 调用 DeepSeek API 流式
        match client.chat_stream(messages).await {
            Ok(mut stream) => {
                use futures::StreamExt;
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            if !chunk.content.is_empty() {
                                let _ = tx.send(Ok(Event::default().data(
                                    serde_json::json!({"type": "chunk", "content": chunk.content}).to_string()
                                )));
                            }
                            if chunk.finish_reason.is_some() {
                                let _ = tx.send(Ok(Event::default().data(
                                    serde_json::json!({"type": "done", "finish_reason": chunk.finish_reason}).to_string()
                                )));
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Ok(Event::default().data(
                                serde_json::json!({"type": "error", "message": e.to_string()}).to_string()
                            )));
                        }
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "error", "message": e.to_string()}).to_string()
                )));
            }
        }
    });

    Sse::new(UnboundedReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
}

/// 状态查询
pub async fn status_handler(
    State(state): State<SharedState>,
) -> Json<StatusResponse> {
    let cost = *state.total_cost.lock().await;
    let hit_rate = *state.cache_hit_rate.lock().await;
    let active = *state.active_agents.lock().await;
    let max_agents = state.config.lock().await.engine.max_parallel_agents;

    Json(StatusResponse {
        mode: "assist".into(),
        cost,
        hit_rate,
        active_agents: active,
        max_agents,
        memory_mb: 15.0,
        has_key: state.has_api_key,
    })
}

/// 费用看板
pub async fn cost_handler(
    State(state): State<SharedState>,
) -> Json<CostResponse> {
    let cost = *state.total_cost.lock().await;
    let hit_rate = *state.cache_hit_rate.lock().await;
    // DeepSeek 缓存命中 token 免费，估算节省为 cost * hit_rate
    let cache_saved = cost * hit_rate;
    // Claude Code 估算：相比 DeepSeek，同任务费用高约 20-25 倍
    let vs_claude = if cost > 0.0 {
        let claude_estimated = cost * 22.0;
        ((claude_estimated - cost) / claude_estimated * 100.0).min(99.0)
    } else {
        0.0
    };

    Json(CostResponse {
        total_cost: cost,
        cache_hit_rate: hit_rate,
        cache_saved,
        monthly_used: cost,
        monthly_budget: 100.0,
        vs_claude_savings_pct: vs_claude,
    })
}

/// 项目信息
pub async fn project_handler(
    State(state): State<SharedState>,
) -> Json<ProjectResponse> {
    let work_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let stats = crate::tui::components::project_panel::gather_stats(&work_dir);

    let commits: Vec<CommitItem> = stats.recent_commits.iter().take(5).map(|c| CommitItem {
        hash: c.hash.clone(),
        message: c.message.clone(),
        author: c.author.clone(),
        date: c.date.clone(),
    }).collect();

    Json(ProjectResponse {
        name: stats.project_name,
        file_count: stats.file_count,
        total_lines: stats.total_lines,
        rust_files: stats.rust_files,
        test_files: stats.test_files,
        recent_commits: commits,
    })
}
