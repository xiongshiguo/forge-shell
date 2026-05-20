//! Web API 端点

use super::SharedState;
use crate::agent::dispatcher::Dispatcher;
use crate::config::Config;
use axum::{Json, extract::State, response::sse::{Event, Sse, KeepAlive}};
use std::path::{Path, PathBuf};
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
    pub evolution: EvolutionStatus,
}

#[derive(Debug, Serialize)]
pub struct EvolutionStatus {
    pub experiences: usize,
    pub sops: usize,
    pub success_rate: f64,
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

/// 一键更新：下载新版本 → 替换 → 重启
pub async fn update_now_handler() -> Json<serde_json::Value> {
    let current = env!("CARGO_PKG_VERSION");
    match check_latest_version().await {
        Ok(Some(latest)) if latest != current => {
            let download_url = format!(
                "https://gitee.com/forgemaster/forge-shell/releases/download/v{0}/forge-shell.exe",
                latest
            );

            // 找到当前 exe 路径
            let current_exe = match std::env::current_exe() {
                Ok(p) => p,
                Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("找不到当前程序: {}", e)})),
            };

            let new_exe = current_exe.with_file_name(format!("forge-shell-v{}.exe", latest));
            let backup_exe = current_exe.with_extension("exe.old");

            // 下载新版本
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
            {
                Ok(c) => c,
                Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("创建下载客户端失败: {}", e)})),
            };

            match client.get(&download_url).send().await {
                Ok(resp) => {
                    match resp.bytes().await {
                        Ok(bytes) => {
                            // 写到临时文件
                            if let Err(e) = std::fs::write(&new_exe, &bytes) {
                                return Json(serde_json::json!({"ok": false, "error": format!("写入新版本失败: {}", e)}));
                            }

                            // 备份旧版本
                            std::fs::rename(&current_exe, &backup_exe).ok();

                            // 替换为新版本
                            if let Err(e) = std::fs::rename(&new_exe, &current_exe) {
                                // 恢复旧版本
                                std::fs::rename(&backup_exe, &current_exe).ok();
                                return Json(serde_json::json!({"ok": false, "error": format!("替换失败: {}", e)}));
                            }

                            // 启动新进程并退出当前进程
                            let _ = std::process::Command::new(&current_exe)
                                .arg("--web")
                                .spawn();

                            // 延迟退出，让响应先发出去
                            tokio::spawn(async {
                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                std::process::exit(0);
                            });

                            Json(serde_json::json!({"ok": true, "message": format!("已更新到 v{}，正在重启...", latest)}))
                        }
                        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("下载失败: {}", e)})),
                    }
                }
                Err(e) => Json(serde_json::json!({"ok": false, "error": format!("请求失败: {}", e)})),
            }
        }
        _ => Json(serde_json::json!({"ok": false, "error": "已是最新版本"})),
    }
}

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

/// 提交复盘到经验熔池
pub async fn review_submit_handler(
    State(state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let turns: Vec<String> = req["turns"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let project = req["project"].as_str().unwrap_or("");

    let review_id = uuid::Uuid::new_v4().to_string();
    let review = serde_json::json!({
        "id": review_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "turns": turns.len(),
        "project": project,
        "patterns": extract_patterns(&turns),
    });

    let review_dir = crate::config::forge_data_dir().join("reviews");
    std::fs::create_dir_all(&review_dir).ok();
    if let Ok(json) = serde_json::to_string_pretty(&review) {
        std::fs::write(review_dir.join(format!("review_{}.json", &review_id[..8])), json).ok();
    }

    let review_count = {
        let mut evo = state.evolution.lock().await;
        let summary: String = turns.iter().take(3).map(|t| t.chars().take(50).collect::<String>()).collect::<Vec<_>>().join(" | ");
        evo.record_turn(&summary, "session_review", true);
        evo.try_reflect();
        evo.summary()
    };

    Json(serde_json::json!({"ok": true, "id": review_id, "experiences": review_count.total_experiences, "sops": review_count.sop_count}))
}

fn extract_patterns(turns: &[String]) -> Vec<String> {
    let keywords = ["rust", "修复", "重构", "测试", "编译", "部署", "性能", "api"];
    let mut patterns = Vec::new();
    for kw in keywords {
        if turns.iter().any(|t| t.to_lowercase().contains(kw)) {
            patterns.push(kw.to_string());
        }
    }
    patterns
}

/// 回滚所有修改
pub async fn rollback_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let backup = state.backup.lock().await;
    let count = backup.rollback_all();
    Json(serde_json::json!({"ok": true, "rolled_back": count}))
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

fn scan_project() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut info = format!("\n\n## 当前项目\n路径: {}\n", cwd.display());
    if let Ok(entries) = std::fs::read_dir(&cwd) {
        let mut dirs = vec![];
        let mut files = vec![];
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" { continue; }
            match e.file_type() {
                Ok(t) if t.is_dir() => dirs.push(name),
                _ => files.push(name),
            }
        }
        dirs.sort(); files.sort();
        info.push_str(&format!("目录: {}\n", dirs.join(", ")));
        if !files.is_empty() {
            info.push_str(&format!("文件: {}\n", files.iter().take(20).cloned().collect::<Vec<_>>().join(", ")));
        }
    }
    // 检查是否有 Rust 项目
    if cwd.join("Cargo.toml").exists() {
        info.push_str("\n这是一个 Rust 项目（Cargo.toml 存在）。");
    }
    if cwd.join("package.json").exists() {
        info.push_str("\n这是一个 Node 项目（package.json 存在）。");
    }
    if cwd.join("FORGESHELL_CONTEXT.md").exists() {
        info.push_str("\nFORGESHELL_CONTEXT.md 存在，包含跨会话记忆。");
    }
    info
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

/// 自动修复循环：跑测试 → 失败 → AI 分析 → 改代码 → 重跑 (最多3轮)
pub async fn auto_fix_handler(
    State(state): State<SharedState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let work_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let config = state_clone.config.lock().await.clone();
        let client = match crate::engine::inference::InferenceClient::new(&config) {
            Ok(c) => c,
            Err(e) => { let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"error","message":e.to_string()}).to_string()))); return; }
        };

        for round in 0..3u32 {
            let msg = format!("\n🔄 第{}轮：运行 cargo test...\n", round+1);
            let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"chunk","content": msg}).to_string())));

            let output = match tokio::process::Command::new("cmd").args(["/C", "cargo test"]).current_dir(&work_dir).output().await {
                Ok(o) => o,
                Err(e) => { let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"error","message":e.to_string()}).to_string()))); return; }
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"done","message":"✓ 全部测试通过！"}).to_string())));
                return;
            }

            // 全量错误输出 + 根因分析
            let full_err = format!("STDOUT:\n{}\nSTDERR:\n{}", stdout, stderr);
            let err_preview = format!("\n❌ 第{}轮测试失败:\n{}\n", round+1, &full_err[..full_err.len().min(3000)]);
            let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"chunk","content": err_preview}).to_string())));

            // 第一步：根因分析
            let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"chunk","content": "\n🔍 分析根本原因...\n"}).to_string())));
            let root_cause_prompt = format!(
                "分析以下 cargo test 失败的根本原因。只输出1-3句话，格式：'根因: ...' \n{}",
                &full_err[..full_err.len().min(4000)]
            );
            let mut root_cause = String::from("未确定");
            {
                let msgs = vec![
                    crate::engine::inference::ChatMessage::system("你是 Rust 编译器专家。分析测试失败的根本原因。"),
                    crate::engine::inference::ChatMessage::user(&root_cause_prompt),
                ];
                if let Ok(mut stream) = client.chat_stream(msgs).await {
                    use futures::StreamExt;
                    while let Some(r) = stream.next().await {
                        if let Ok(c) = r { root_cause.push_str(&c.content); }
                    }
                }
            }
            let msg_rc = format!("根因: {}\n\n🔧 生成修复方案...\n", root_cause);
            let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"chunk","content": msg_rc}).to_string())));

            // 第二步：带根因的修复
            let fix_prompt = format!(
                "根因: {}\n\n完整错误:\n{}\n\n根据根因修复代码。只输出修复后的完整文件，用```rust:文件路径 包裹。不要解释。",
                root_cause, &full_err[..full_err.len().min(5000)]
            );
            let msgs = vec![
                crate::engine::inference::ChatMessage::system("你是 Rust 修复专家。基于根因分析修复代码，不是盲目打补丁。用```rust:路径 格式输出。"),
                crate::engine::inference::ChatMessage::user(&fix_prompt),
            ];

            match client.chat_stream(msgs).await {
                Ok(mut stream) => {
                    use futures::StreamExt;
                    let mut fix_code = String::new();
                    while let Some(r) = stream.next().await {
                        if let Ok(c) = r { fix_code.push_str(&c.content); }
                    }
                    let applied = apply_fix_code(&fix_code, &work_dir, &state_clone).await;
                    let fix_msg = format!("\n📝 第{}轮：已修改{}个文件，基于根因「{}」\n", round+1, applied, &root_cause[..root_cause.len().min(100)]);
                    let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"chunk","content": fix_msg}).to_string())));
                }
                Err(e) => {
                    let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"error","message":format!("AI 调用失败: {}", e)}).to_string())));
                    return;
                }
            }
        }

        let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"done","message":"⚠ 3轮自动修复后仍有失败，请手动检查"}).to_string())));
    });

    Sse::new(UnboundedReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

async fn apply_fix_code(fix_code: &str, work_dir: &Path, state: &SharedState) -> usize {
    let mut count = 0;
    let mut current_path: Option<String> = None;
    let mut current_code = String::new();

    for line in fix_code.lines() {
        if line.starts_with("```rust:") {
            if let Some(path) = &current_path {
                let full_path = work_dir.join(path);
                let _ = state.backup.lock().await.backup_before_write(&full_path, "auto-fix");
                std::fs::write(&full_path, current_code.trim()).ok();
                count += 1;
            }
            current_path = Some(line.trim_start_matches("```rust:").trim().to_string());
            current_code = String::new();
        } else if line == "```" {
            if let Some(path) = &current_path {
                let full_path = work_dir.join(path);
                let _ = state.backup.lock().await.backup_before_write(&full_path, "auto-fix");
                std::fs::write(&full_path, current_code.trim()).ok();
                count += 1;
                current_path = None;
            }
        } else if current_path.is_some() {
            current_code.push_str(line);
            current_code.push('\n');
        }
    }
    count
}

/// 探索工具：自动扫描项目结构、文档、最近提交
pub async fn explore_handler() -> Json<serde_json::Value> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut findings = Vec::new();

    // 1. 扫描文档目录
    for doc_dir in ["docs", "doc", "documentation", ".github"] {
        let path = cwd.join(doc_dir);
        if path.exists() && path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                let files: Vec<_> = entries.flatten()
                    .filter_map(|e| {
                        let n = e.file_name().to_string_lossy().to_string();
                        if n.ends_with(".md") || n.ends_with(".txt") || n.ends_with(".yml") { Some(n) } else { None }
                    }).collect();
                if !files.is_empty() {
                    findings.push(format!("📁 {}/: {}", doc_dir, files.join(", ")));
                }
            }
        }
    }

    // 2. 检查 README
    for readme in ["README.md", "README.txt", "README"] {
        if cwd.join(readme).exists() {
            if let Ok(content) = std::fs::read_to_string(cwd.join(readme)) {
                findings.push(format!("📄 {} ({} 字符): {}", readme, content.len(), &content[..content.len().min(500)]));
            }
            break;
        }
    }

    // 3. 最近 git log
    if let Ok(output) = tokio::process::Command::new("cmd").args(["/C", "git log --oneline -5"]).current_dir(&cwd).output().await {
        let log = String::from_utf8_lossy(&output.stdout);
        if !log.trim().is_empty() {
            findings.push(format!("📋 最近提交:\n{}", log.trim()));
        }
    }

    // 4. 项目类型判断
    if cwd.join("Cargo.toml").exists() { findings.push("🦀 Rust 项目".into()); }
    if cwd.join("package.json").exists() { findings.push("📦 Node 项目".into()); }
    if cwd.join("go.mod").exists() { findings.push("🔵 Go 项目".into()); }

    Json(serde_json::json!({"ok": true, "findings": findings}))
}

/// 读取文件内容（支持行范围）
pub async fn read_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let path = req["path"].as_str().unwrap_or("");
    let start = req["start"].as_u64().unwrap_or(0) as usize;
    let end = req["end"].as_u64().unwrap_or(0) as usize;

    let full_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path);

    match std::fs::read_to_string(&full_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let (s, e) = if end > 0 && end > start {
                (start.min(total).saturating_sub(1), end.min(total))
            } else {
                (0, total)
            };
            let selected: Vec<_> = lines[s..e].iter().enumerate().map(|(i, l)| format!("{:>5}  {}", s+i+1, l)).collect();
            Json(serde_json::json!({"ok": true, "path": path, "total_lines": total, "lines": selected}))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

/// 联网搜索 — 三层引擎：GitHub → Gitee → DuckDuckGo（全部零配置）
pub async fn web_search_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let query = req["query"].as_str().unwrap_or("");
    if query.is_empty() {
        return Json(serde_json::json!({"ok": false, "results": []}));
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("ForgeShell/0.8")
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };

    let mut all_results: Vec<String> = Vec::new();

    // 1. GitHub 代码搜索（公开 API，60次/小时免认证）
    let gh_url = format!(
        "https://api.github.com/search/repositories?q={}&sort=stars&per_page=5",
        urlencoding(&query)
    );
    if let Ok(resp) = client.get(&gh_url).send().await {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(items) = json["items"].as_array() {
                for item in items.iter().take(5) {
                    let name = item["full_name"].as_str().unwrap_or("");
                    let desc = item["description"].as_str().unwrap_or("").chars().take(80).collect::<String>();
                    let stars = item["stargazers_count"].as_u64().unwrap_or(0);
                    all_results.push(format!("[GitHub] {} ⭐{} — {}", name, stars, desc));
                }
            }
        }
    }

    // 2. Gitee 仓库搜索（公开 API，免认证）
    let gitee_url = format!(
        "https://gitee.com/api/v5/search/repositories?q={}&sort=stars_count&per_page=5",
        urlencoding(&query)
    );
    if let Ok(resp) = client.get(&gitee_url).send().await {
        if let Ok(items) = resp.json::<Vec<serde_json::Value>>().await {
            for item in items.iter().take(5) {
                let name = item["full_name"].as_str().unwrap_or("");
                let desc = item["description"].as_str().unwrap_or("").chars().take(80).collect::<String>();
                all_results.push(format!("[Gitee] {} — {}", name, desc));
            }
        }
    }

    // 3. DuckDuckGo 兜底
    let ddg_url = format!("https://lite.duckduckgo.com/lite/?q={}", urlencoding(&query));
    if let Ok(resp) = client.get(&ddg_url).send().await {
        let html = resp.text().await.unwrap_or_default();
        let snippets: Vec<&str> = html
            .split("result__snippet")
            .skip(1)
            .filter_map(|s| s.split("</").next())
            .map(|s| s.trim_start_matches('>').trim())
            .filter(|s| s.len() > 10)
            .take(5)
            .collect();
        for s in snippets {
            all_results.push(format!("[Web] {}", s));
        }
    }

    Json(serde_json::json!({
        "ok": true,
        "results": all_results,
        "sources": ["github", "gitee", "duckduckgo"]
    }))
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| {
        if c.is_alphanumeric() || c == ' ' { c.to_string() } else { format!("%{:02X}", c as u8) }
    }).collect::<String>().replace(' ', "+")
}

/// 获取项目文件树（排除 .git/target/node_modules）
pub async fn files_handler() -> Json<serde_json::Value> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let tree = build_file_tree(&cwd, &cwd, 0, 3);
    Json(serde_json::json!({"ok": true, "files": tree}))
}

fn build_file_tree(root: &Path, current: &Path, depth: usize, max_depth: usize) -> Vec<serde_json::Value> {
    if depth > max_depth { return vec![]; }
    let mut items = Vec::new();
    let skip = ["target", ".git", "node_modules", ".ai", "__pycache__", ".rustup", "debug_screenshots", "logs"];

    if let Ok(entries) = std::fs::read_dir(current) {
        let mut children: Vec<_> = entries.flatten().collect();
        children.sort_by_key(|e| (!e.file_type().map(|t| t.is_dir()).unwrap_or(false), e.file_name().to_string_lossy().to_string()));

        for entry in children {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || skip.contains(&name.as_str()) { continue; }

            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                let sub = build_file_tree(root, &entry.path(), depth + 1, max_depth);
                items.push(serde_json::json!({"name": name, "dir": true, "children": sub}));
            } else {
                let ext = std::path::Path::new(&name).extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                items.push(serde_json::json!({"name": name, "dir": false, "ext": ext}));
            }
        }
    }
    items
}

/// LSP 信息：运行 cargo check 并解析错误
pub async fn lsp_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let file_path = req["file"].as_str().unwrap_or("");

    let work_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let output = match tokio::process::Command::new("cmd")
        .args(["/C", "cargo check --message-format=json 2>&1"])
        .current_dir(&work_dir)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let errors: Vec<serde_json::Value> = stdout.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|msg| msg["reason"].as_str() == Some("compiler-message"))
        .filter_map(|msg| {
            let m = &msg["message"];
            if m["level"].as_str() == Some("error") {
                let spans = &m["spans"];
                Some(serde_json::json!({
                    "message": m["message"],
                    "file": spans[0]["file_name"],
                    "line": spans[0]["line_start"],
                    "column": spans[0]["column_start"],
                }))
            } else { None }
        })
        .take(10)
        .collect();

    Json(serde_json::json!({"ok": true, "errors": errors, "count": errors.len()}))
}

/// 搜索代码（ripgrep）
pub async fn search_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let pattern = req["pattern"].as_str().unwrap_or("");
    let path = req["path"].as_str().unwrap_or(".");

    if pattern.is_empty() {
        return Json(serde_json::json!({"ok": false, "matches": []}));
    }

    let work_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let search_path = work_dir.join(path);

    match tokio::process::Command::new("rg")
        .args(["--no-heading", "-n", "--max-count=50", pattern])
        .arg(&search_path)
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let matches: Vec<&str> = stdout.lines().take(20).collect();
            Json(serde_json::json!({"ok": true, "matches": matches, "count": matches.len()}))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string(), "matches": []})),
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

        // 加载项目上下文 + 跨会话记忆
        let context = load_context();
        let project_info = scan_project();
        let mut system_msg = crate::system_prompt::get_system_prompt();
        system_msg.push_str(&project_info);
        if !context.is_empty() {
            system_msg.push_str(&format!("\n\n## 跨会话记忆\n{}", context));
        }

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
                // 记录经验 + 匹配 SOP + 自动反思
                {
                    let mut evo = state_clone.evolution.lock().await;
                    evo.record_turn(&req.message, "[OK]", has_content);
                    if has_content {
                        let matches = evo.match_sop(&req.message);
                        if !matches.is_empty() {
                            let _ = tx.send(Ok(Event::default().data(
                                serde_json::json!({"type": "chunk", "content": format!("\n💡 天工阁匹配到 {} 条 SOP\n", matches.len())}).to_string()
                            )));
                        }
                        evo.try_reflect();
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
    let evo_summary = state.evolution.lock().await.summary();

    Json(StatusResponse {
        mode: "assist".into(),
        cost,
        hit_rate,
        active_agents: active,
        max_agents,
        memory_mb: 15.0,
        has_key: state.has_api_key,
        evolution: EvolutionStatus {
            experiences: evo_summary.total_experiences,
            sops: evo_summary.sop_count,
            success_rate: evo_summary.success_rate,
        },
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
