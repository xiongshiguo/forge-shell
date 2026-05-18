//! Web API 端点

use super::SharedState;
use crate::agent::dispatcher::Dispatcher;
use axum::{Json, extract::State, response::sse::{Event, Sse, KeepAlive}};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::UnboundedReceiverStream;

// ---- 请求/响应类型 ----

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub mode: Option<String>, // plan / assist / speed
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub reply: String,
    pub tasks_count: usize,
    pub parallel_groups: usize,
    pub parallelism_gain: f64,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub mode: String,
    pub cost: f64,
    pub hit_rate: f64,
    pub active_agents: usize,
    pub max_agents: usize,
    pub memory_mb: f64,
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

/// SSE 流式对话
pub async fn chat_handler(
    State(state): State<SharedState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let state_clone = state.clone();
    tokio::spawn(async move {
        // 编排任务
        let plan = {
            let orch = &state_clone.orchestrator;
            orch.decompose(&req.message)
        };

        // 发送编排信息
        let _ = tx.send(Ok(Event::default()
            .data(serde_json::json!({
                "type": "plan",
                "tasks": plan.tasks.len(),
                "groups": plan.parallel_groups.len(),
                "gain": plan.parallelism_gain
            }).to_string())));

        // 模拟流式响应
        let response_text = format!(
            "收到指令：「{}」。已拆解为 {} 个子任务，{} 组并行执行。预估并行增益 {:.1}x。",
            req.message, plan.tasks.len(), plan.parallel_groups.len(), plan.parallelism_gain
        );
        let words: Vec<String> = response_text
            .split_inclusive(|c: char| c == '。' || c == '，')
            .map(|s| s.to_string())
            .collect();

        for word in words {
            let _ = tx.send(Ok(Event::default().data(
                serde_json::json!({"type": "chunk", "content": word}).to_string()
            )));
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }

        // 更新成本
        {
            let mut cost = state_clone.total_cost.lock().await;
            *cost += plan.estimated_total_tokens as f64 * 0.000001;
        }

        // 调度任务
        let dispatcher = Dispatcher::new(state_clone.config.clone(), state_clone.config.engine.max_parallel_agents);
        {
            let mut agents = state_clone.active_agents.lock().await;
            *agents = plan.parallel_groups.first().map(|g| g.len()).unwrap_or(0);
        }
        let result = dispatcher.dispatch(plan).await;
        {
            let mut agents = state_clone.active_agents.lock().await;
            *agents = 0;
        }

        let _ = tx.send(Ok(Event::default().data(
            serde_json::json!({
                "type": "done",
                "success": result.success_count,
                "failure": result.failure_count,
                "tokens": result.total_tokens,
                "duration_ms": result.total_duration_ms
            }).to_string()
        )));
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

    Json(StatusResponse {
        mode: "assist".into(),
        cost,
        hit_rate,
        active_agents: active,
        max_agents: state.config.engine.max_parallel_agents,
        memory_mb: 15.0,
    })
}

/// 费用看板
pub async fn cost_handler(
    State(state): State<SharedState>,
) -> Json<CostResponse> {
    let cost = *state.total_cost.lock().await;
    let hit_rate = *state.cache_hit_rate.lock().await;

    Json(CostResponse {
        total_cost: cost,
        cache_hit_rate: hit_rate,
        cache_saved: cost * 0.3,
        monthly_used: cost,
        monthly_budget: 100.0,
        vs_claude_savings_pct: 96.0,
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
