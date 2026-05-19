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

/// 检查更新
pub async fn update_check_handler() -> Json<serde_json::Value> {
    let current = env!("CARGO_PKG_VERSION");
    match check_latest_version().await {
        Ok(Some(latest)) if latest != current => {
            Json(serde_json::json!({
                "update_available": true,
                "current": current,
                "latest": latest,
                "download_url": format!("https://gitee.com/forgemaster/forge-shell/releases/download/v{}/forge-shell.exe", latest),
            }))
        }
        _ => Json(serde_json::json!({"update_available": false, "current": current})),
    }
}

async fn check_latest_version() -> Result<Option<String>, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let resp = client
        .get("https://gitee.com/api/v5/repos/forgemaster/forge-shell/releases/latest")
        .header("User-Agent", "ForgeShell-UpdateCheck")
        .send()
        .await?;
    let json: serde_json::Value = resp.json().await?;
    Ok(json["tag_name"].as_str().map(|s| s.trim_start_matches('v').to_string()))
}

/// 进化状态
pub async fn evolution_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let evolution = state.evolution.lock().await;
    let summary = evolution.summary();
    let sops: Vec<serde_json::Value> = evolution.sop_library.all().iter().map(|s| {
        serde_json::json!({
            "id": s.id,
            "title": s.title,
            "triggers": s.triggers,
            "usage_count": s.usage_count,
            "success_rate": s.success_rate,
            "source": s.source,
        })
    }).collect();

    Json(serde_json::json!({
        "summary": {
            "total_experiences": summary.total_experiences,
            "success_rate": summary.success_rate,
            "sop_count": summary.sop_count,
        },
        "sops": sops,
    }))
}

/// 保存跨会话记忆
pub async fn save_context_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let content = req["content"].as_str().unwrap_or("");
    let path = context_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    match std::fs::write(&path, content) {
        Ok(_) => Json(serde_json::json!({"ok": true, "path": path.to_string_lossy()})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

fn context_file_path() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("FORGESHELL_CONTEXT.md")
}

fn load_context() -> String {
    let path = context_file_path();
    if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    }
}

/// 检查是否已配置 API Key（动态读取，不依赖启动时快照）
pub async fn check_key_handler(
    State(state): State<SharedState>,
) -> Json<CheckKeyResponse> {
    let config = state.config.lock().await;
    let has_key = !config.effective_api_key().is_empty();
    Json(CheckKeyResponse { has_key })
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

/// 执行沙箱命令（cargo check/test/build 等白名单命令）
pub async fn exec_handler(
    State(state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let cmd = req["command"].as_str().unwrap_or("");
    let cwd = req["cwd"].as_str().unwrap_or(".");

    // 白名单检查
    let allowed = ["cargo check", "cargo test", "cargo build", "cargo fmt", "cargo clippy",
                   "git status", "git diff", "git log", "git branch", "rustc --version", "cargo --version"];
    let cmd_trimmed = cmd.trim();
    let is_allowed = allowed.iter().any(|a| cmd_trimmed.starts_with(a));

    if !is_allowed {
        return Json(serde_json::json!({
            "ok": false, "stdout": "", "stderr": "命令不在白名单中。允许: cargo check/test/build/fmt/clippy, git status/diff/log/branch",
            "exit_code": -1
        }));
    }

    // 用 state 获取当前工作目录
    let work_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(cwd));

    match tokio::process::Command::new("cmd")
        .args(["/C", cmd_trimmed])
        .current_dir(&work_dir)
        .output()
        .await
    {
        Ok(output) => {
            Json(serde_json::json!({
                "ok": output.status.success(),
                "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                "exit_code": output.status.code().unwrap_or(-1),
            }))
        }
        Err(e) => Json(serde_json::json!({
            "ok": false, "stdout": "", "stderr": e.to_string(), "exit_code": -1
        })),
    }
}

/// 诊断：测试 API 连通性
pub async fn ping_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let config = state.config.lock().await.clone();
    let key_ok = !config.effective_api_key().is_empty();
    if !key_ok {
        return Json(serde_json::json!({"ok": false, "error": "未配置 API Key"}));
    }

    let client = match crate::engine::inference::InferenceClient::new(&config) {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("客户端创建失败: {}", e)})),
    };

    let messages = vec![
        crate::engine::inference::ChatMessage::system(&crate::system_prompt::get_system_prompt()),
        crate::engine::inference::ChatMessage::user("你好，请回复 OK"),
    ];

    match client.chat_stream(messages).await {
        Ok(mut stream) => {
            use futures::StreamExt;
            let mut text = String::new();
            while let Some(r) = stream.next().await {
                match r {
                    Ok(c) => text.push_str(&c.content),
                    Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
                }
            }
            Json(serde_json::json!({"ok": true, "response": text}))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
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
        let mut config = state_clone.config.lock().await.clone();

        // 模型路由：根据意图复杂度动态选择模型
        let router = crate::engine::router::ModelRouter::new(
            config.ai.default_model.clone(),
            config.ai.flash_model.clone(),
        );
        let decision = router.decide(&req.message, 0);
        config.ai.default_model = decision.model.clone();

        let emoji = if matches!(decision.complexity, crate::engine::router::Complexity::Simple) { "⚡" } else { "🧠" };
        let _ = tx.send(Ok(Event::default().data(
            serde_json::json!({"type": "chunk", "content": format!("🔌 {} → {} ({}复杂度，预计¥{:.4})", decision.model, emoji, decision.complexity.name(), decision.estimated_cost)}).to_string()
        )));
        let client = match crate::engine::inference::InferenceClient::new(&config) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "error", "message": format!("创建客户端失败: {}", e)}).to_string()
                )));
                return;
            }
        };

        // 加载跨会话上下文
        let context = load_context();
        let system_msg = if context.is_empty() {
            crate::system_prompt::get_system_prompt()
        } else {
            format!("{}\n\n## 跨会话记忆\n以下是之前会话中你记住的重要内容：\n{}", crate::system_prompt::get_system_prompt(), context)
        };

        let messages = vec![
            crate::engine::inference::ChatMessage::system(&system_msg),
            crate::engine::inference::ChatMessage::user(&req.message),
        ];

        match client.chat_stream(messages).await {
            Ok(mut stream) => {
                use futures::StreamExt;
                let mut has_content = false;
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            if !chunk.content.is_empty() {
                                has_content = true;
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
                                serde_json::json!({"type": "error", "message": format!("流式错误: {}", e)}).to_string()
                            )));
                        }
                    }
                }
                // 记录成功经验 + 匹配 SOP
                {
                    let mut evo = state_clone.evolution.lock().await;
                    let dummy_output = "[流式响应已完成]";
                    evo.record_turn(&req.message, dummy_output, has_content);
                    if has_content {
                        let _ = evo.match_sop(&req.message);
                    }
                }

                if !has_content {
                    let _ = tx.send(Ok(Event::default().data(
                        serde_json::json!({"type": "error", "message": "API 返回了空内容，请检查 Key 是否正确"}).to_string()
                    )));
                }
            }
            Err(e) => {
                let error_msg = format!("API 调用失败: {}", e);
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "error", "message": &error_msg}).to_string()
                )));
                // 记录失败经验
                let mut evo = state_clone.evolution.lock().await;
                evo.record_turn(&req.message, &error_msg, false);
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
