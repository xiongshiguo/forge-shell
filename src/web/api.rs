//! Web API 端点

use super::SharedState;
use crate::agent::dispatcher::Dispatcher;
use crate::config::Config;
use axum::{Json, extract::State, response::sse::{Event, Sse, KeepAlive}};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::UnboundedReceiverStream;

// ---- 请求/响应类型 ----

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub mode: Option<String>,
    pub model_pref: Option<String>,
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
    pub turns: u32,
    pub total_tokens: u64,
    pub model: String,
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
    pub cache_hit_tokens: u64,
    pub cache_miss_tokens: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub ok: bool,
    pub name: String,
    pub file_count: usize,
    pub total_lines: usize,
    pub rust_files: usize,
    pub test_files: usize,
    pub files: Vec<FileItem>,
    pub recent_commits: Vec<CommitItem>,
}

#[derive(Debug, Serialize)]
pub struct FileItem {
    pub name: String,
    pub lines: usize,
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
    let current = crate::system_prompt::VERSION;
    match check_latest_version().await {
        Ok(Some(latest)) if latest != current => {
            // GitHub Actions CI 自动构建 exe，优先从 GitHub 下载
            let download_url = format!(
                "https://github.com/xiongshiguo/forge-shell/releases/download/v{0}/forge-shell.exe",
                latest
            );
            // Gitee 兜底
            let fallback_url = format!(
                "https://gitee.com/forgemaster/forge-shell/releases/download/v{0}/forge-shell.exe",
                latest
            );

            // 找到当前 exe 路径
            let current_exe = match std::env::current_exe() {
                Ok(p) => p,
                Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("找不到当前程序: {}", e)})),
            };

            // L5: 双进程替换——写临时文件 + 脚本替换 + 退出
            let new_exe = current_exe.with_file_name(format!("forge-shell-v{}.exe", latest));

            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120)).build() {
                Ok(c) => c,
                Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("创建下载客户端失败: {}", e)})),
            };

            // 尝试下载：GitHub 优先（CI 自动构建），Gitee 兜底
            let mut bytes = None;
            for url in [&download_url, &fallback_url] {
                if let Ok(resp) = client.get(url).send().await {
                    if let Ok(data) = resp.bytes().await {
                        if data.len() > 1024 && &data[0..2] == b"MZ" {
                            bytes = Some(data);
                            break;
                        }
                    }
                }
            }
            match bytes {
                Some(bytes) => {
                    // L3: 验证下载的是合法 PE 文件（已在 for 循环中预检）
                    if let Err(e) = std::fs::write(&new_exe, &bytes) {
                        return Json(serde_json::json!({"ok": false, "error": format!("写入失败: {}", e)}));
                    }
                    // 再次验证写入的文件
                    let verified = std::fs::read(&new_exe).map(|b| b.len() > 1024 && &b[0..2] == b"MZ").unwrap_or(false);
                    if !verified {
                        std::fs::remove_file(&new_exe).ok();
                        return Json(serde_json::json!({"ok": false, "error": "写入验证失败"}));
                    }
                    // 替换脚本
                    let script = format!(
                        "@echo off\r\n\
                         timeout /t 2 /nobreak >nul\r\n\
                         copy /Y \"{}\" \"{}\" >nul 2>&1\r\n\
                         if exist \"{}\" start \"\" \"{}\"\r\n\
                         del \"%~f0\"\r\n",
                        new_exe.display(), current_exe.display(),
                        current_exe.display(), current_exe.display()
                    );
                    let script_path = current_exe.with_file_name("forge-update.bat");
                    std::fs::write(&script_path, script).ok();
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", &script_path.to_string_lossy()])
                        .spawn();
                    tokio::spawn(async {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        std::process::exit(0);
                    });
                    Json(serde_json::json!({"ok": true, "message": format!("已更新到 v{}，正在重启...", latest)}))
                }
                None => Json(serde_json::json!({"ok": false, "error": "未找到可下载的有效 exe（GitHub 和 Gitee 均无）"})),
            }
        }
        _ => Json(serde_json::json!({"ok": false, "error": "已是最新版本"})),
    }
}

/// 检查更新
pub async fn update_check_handler() -> Json<serde_json::Value> {
    let current = crate::system_prompt::VERSION;
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

    // Gitee API 分页获取全部 tag（每页最多 100，超过需翻页）
    let mut all_tags: Vec<String> = Vec::new();
    for page in 1..=3 {
        let url = format!(
            "https://gitee.com/api/v5/repos/forgemaster/forge-shell/tags?per_page=100&page={}",
            page
        );
        let resp = client.get(&url).header("User-Agent", "ForgeShell-UpdateCheck").send().await?;
        let json: serde_json::Value = resp.json().await?;
        if let Some(arr) = json.as_array() {
            if arr.is_empty() { break; }
            for t in arr {
                if let Some(name) = t["name"].as_str() {
                    all_tags.push(name.trim_start_matches('v').to_string());
                }
            }
        } else { break; }
    }

    // semver 比较取最大版本号
    let tag = all_tags.into_iter().max_by(|a, b| {
        let a_parts: Vec<u32> = a.split('.').filter_map(|p| p.parse().ok()).collect();
        let b_parts: Vec<u32> = b.split('.').filter_map(|p| p.parse().ok()).collect();
        a_parts.cmp(&b_parts)
    });
    Ok(tag)
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

fn compute_fingerprint() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut hash = String::new();
    if let Ok(entries) = std::fs::read_dir(&cwd) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" { continue; }
            if let Ok(meta) = e.metadata() {
                if let Ok(modified) = meta.modified() {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    name.hash(&mut h);
                    modified.hash(&mut h);
                    hash.push_str(&format!("{:x}", h.finish()));
                }
            }
        }
    }
    hash
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

/// 始终注入的轻量项目上下文（利用 DeepSeek 1M 上下文窗口）
/// 从 AppState 读取错误日志注入上下文
fn build_error_context(state: &SharedState) -> String {
    let diag = state.error_logger.diagnose();
    let recent = state.error_logger.recent(10);
    if diag.is_empty() && recent.is_empty() { return String::new(); }
    let mut ctx = String::from("\n## 近期系统错误（你可以自查修复）\n");
    for d in &diag { ctx.push_str(&format!("- {}\n", d)); }
    if !recent.is_empty() {
        ctx.push_str("\n最近错误详情:\n");
        for e in &recent {
            ctx.push_str(&format!("- [{}] {}: {} (×{})\n", e.timestamp, e.component, e.message, e.count));
        }
    }
    ctx.push_str("\n如果以上错误属于代码问题，可用 [TOOL:edit] 修复后 [TOOL:exec:cargo build] 验证。\n");
    ctx
}

/// 检查 git 是否安装（缓存结果，只检查一次）
async fn git_available() -> bool {
    use std::sync::atomic::{AtomicI8, Ordering};
    static GIT_CHECK: AtomicI8 = AtomicI8::new(0); // 0=未检查 1=可用 -1=不可用
    match GIT_CHECK.load(Ordering::Relaxed) {
        1 => return true,
        -1 => return false,
        _ => {}
    }
    let ok = tokio::time::timeout(std::time::Duration::from_secs(1),
        tokio::process::Command::new("git").arg("--version").output()
    ).await.map(|r| r.is_ok()).unwrap_or(false);
    GIT_CHECK.store(if ok { 1 } else { -1 }, Ordering::Relaxed);
    ok
}

async fn build_project_context() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut ctx = String::new();

    // 项目类型
    let proj_type = if cwd.join("Cargo.toml").exists() { "Rust" }
        else if cwd.join("package.json").exists() { "Node.js" }
        else if cwd.join("go.mod").exists() { "Go" }
        else if cwd.join("requirements.txt").exists() || cwd.join("pyproject.toml").exists() { "Python" }
        else { "未知" };
    ctx.push_str(&format!("项目类型: {}\n", proj_type));

    // src/ 目录结构概要
    let src_dir = cwd.join("src");
    if src_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&src_dir) {
            let mut mods = Vec::new();
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".rs") || name.ends_with(".ts") || name.ends_with(".js") || name.ends_with(".py") {
                    mods.push(name);
                }
            }
            if !mods.is_empty() {
                ctx.push_str(&format!("源码模块: {}\n", mods.join(", ")));
            }
        }
    }

    // Git 信息：优雅降级——git 不可用或非仓库目录则静默跳过
    if git_available().await {
        if let Ok(Ok(output)) = tokio::time::timeout(std::time::Duration::from_secs(2),
            tokio::process::Command::new("git")
                .args(["rev-parse", "--is-inside-work-tree"]).current_dir(&cwd).output()
        ).await {
            if String::from_utf8_lossy(&output.stdout).trim() == "true" {
                // 确认在 git 仓库内，收集信息（每个命令独立超时）
                if let Ok(Ok(output)) = tokio::time::timeout(std::time::Duration::from_secs(3),
                    tokio::process::Command::new("git")
                        .args(["branch", "--show-current"]).current_dir(&cwd).output()
                ).await {
                    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !branch.is_empty() { ctx.push_str(&format!("当前分支: {}\n", branch)); }
                }
                if let Ok(Ok(output)) = tokio::time::timeout(std::time::Duration::from_secs(2),
                    tokio::process::Command::new("git")
                        .args(["log", "--oneline", "-3"]).current_dir(&cwd).output()
                ).await {
                    let commits = String::from_utf8_lossy(&output.stdout);
                    if !commits.trim().is_empty() {
                        ctx.push_str(&format!("最近提交:\n{}\n", commits.lines().map(|l| format!("  {}", l)).collect::<Vec<_>>().join("\n")));
                    }
                }
            }
        }
    }

    // 深度注入：关键配置文件内容（控制总注入量 <20K tokens）
    let key_files = ["Cargo.toml", "package.json", "README.md", ".claude/settings.json"];
    for fname in &key_files {
        let p = cwd.join(fname);
        if p.exists() {
            if let Ok(content) = std::fs::read_to_string(&p) {
                let truncated: String = content.lines().take(80).collect::<Vec<_>>().join("\n");
                if truncated.len() < 5000 {
                    ctx.push_str(&format!("\n### {} 内容:\n```\n{}\n```\n", fname, truncated));
                }
            }
        }
    }

    ctx
}

/// 规划模式：仅只读工具（物理禁止修改文件）
fn build_readonly_tools() -> Vec<crate::engine::inference::ToolDef> {
    build_tool_defs().into_iter()
        .filter(|t| matches!(t.function.name.as_str(), "read" | "search" | "glob" | "web" | "lsp" | "semantic" | "snap" | "community-fix"))
        .collect()
}

/// 构建原生 function calling 工具定义（DeepSeek V4 OpenAI 兼容格式）
fn build_tool_defs() -> Vec<crate::engine::inference::ToolDef> {
    use crate::engine::inference::{ToolDef, ToolFunction};
    vec![
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "read".into(),
            description: "读取文件内容，支持指定行范围".into(),
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string","description":"文件路径"},"start":{"type":"integer","description":"起始行"},"end":{"type":"integer","description":"结束行"}},"required":["path"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "write".into(),
            description: "创建或覆盖文件，自动备份原文件".into(),
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string","description":"文件路径"},"content":{"type":"string","description":"文件内容"}},"required":["path","content"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "edit".into(),
            description: "精确编辑文件的指定行范围，自动备份".into(),
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string","description":"文件路径"},"start":{"type":"integer"},"end":{"type":"integer"},"content":{"type":"string","description":"替换内容"}},"required":["path","start","end","content"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "search".into(),
            description: "ripgrep 全项目代码搜索".into(),
            parameters: serde_json::json!({"type":"object","properties":{"pattern":{"type":"string","description":"搜索关键词或正则"}},"required":["pattern"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "glob".into(),
            description: "文件模式匹配，如 src/**/*.rs 或 *.toml".into(),
            parameters: serde_json::json!({"type":"object","properties":{"pattern":{"type":"string","description":"glob 模式"}},"required":["pattern"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "exec".into(),
            description: "执行命令(白名单: cargo check/build/test/fmt/clippy/doc/new/init/update/tree/metadata, git status/diff/log/branch/add/commit/stash/checkout/switch/restore/rebase/clone, ls/dir/echo/type/rg/mkdir/md/rustc/rustup)".into(),
            parameters: serde_json::json!({"type":"object","properties":{"command":{"type":"string","description":"shell 命令"}},"required":["command"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "web".into(),
            description: "联网搜索最新信息".into(),
            parameters: serde_json::json!({"type":"object","properties":{"query":{"type":"string","description":"搜索词"}},"required":["query"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "lsp".into(),
            description: "cargo check 代码诊断".into(),
            parameters: serde_json::json!({"type":"object","properties":{"file":{"type":"string","description":"可选：指定文件"}}}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "semantic".into(),
            description: "语义索引查询，搜索函数/结构体定义和引用".into(),
            parameters: serde_json::json!({"type":"object","properties":{"query":{"type":"string","description":"符号名或关键词"}},"required":["query"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "snap".into(),
            description: "查看文件快照列表".into(),
            parameters: serde_json::json!({"type":"object","properties":{}}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "save".into(),
            description: "保存内容到跨会话记忆（追加模式）".into(),
            parameters: serde_json::json!({"type":"object","properties":{"content":{"type":"string","description":"要记住的内容"}},"required":["content"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "community-fix".into(),
            description: "搜索社区经验池，查找已知问题的修复方案".into(),
            parameters: serde_json::json!({"type":"object","properties":{"query":{"type":"string","description":"错误关键词"}},"required":["query"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "share-fix".into(),
            description: "匿名提交修复策略到社区经验池（格式: 错误模式|修复方案|诊断）".into(),
            parameters: serde_json::json!({"type":"object","properties":{"content":{"type":"string","description":"pattern|fix|diagnosis"}},"required":["content"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "web-fetch".into(),
            description: "抓取网页实际内容（查API文档/技术资料），URL必须完整含https://".into(),
            parameters: serde_json::json!({"type":"object","properties":{"url":{"type":"string","description":"完整URL，如 https://docs.rs/tokio/latest/tokio/"}},"required":["url"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "todo".into(),
            description: "创建/更新任务清单（复杂任务拆解跟踪）。tasks数组每项含content和status(pending/in_progress/completed)".into(),
            parameters: serde_json::json!({"type":"object","properties":{"tasks":{"type":"array","items":{"type":"object","properties":{"content":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed"]}}}}},"required":["tasks"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "ask".into(),
            description: "向用户发起多选提问，用于需求模糊时澄清。仅Flash可用".into(),
            parameters: serde_json::json!({"type":"object","properties":{"question":{"type":"string","description":"问题"},"options":{"type":"array","items":{"type":"string"},"description":"2-4个选项"}},"required":["question","options"]}),
        }},
    ]
}

/// Pro 专用精简工具集（10个核心工具，避免工具爆炸导致思考超时）
fn build_pro_tools() -> Vec<crate::engine::inference::ToolDef> {
    use crate::engine::inference::{ToolDef, ToolFunction};
    vec![
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "read".into(),
            description: "读取文件内容，支持行号范围".into(),
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string","description":"文件路径"},"start":{"type":"integer"},"end":{"type":"integer"}},"required":["path"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "write".into(),
            description: "创建新文件或完全覆写".into(),
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "edit".into(),
            description: "精确替换文件中的字符串".into(),
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}},"required":["path","old_string","new_string"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "search".into(),
            description: "ripgrep 代码搜索".into(),
            parameters: serde_json::json!({"type":"object","properties":{"pattern":{"type":"string","description":"正则或关键词"},"path":{"type":"string"}},"required":["pattern"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "exec".into(),
            description: "执行命令(白名单: cargo check/build/test/fmt/clippy/doc/new/init/update/tree/metadata, git status/diff/log/branch/add/commit/stash/checkout/switch/restore/rebase/clone, ls/dir/echo/type/rg/mkdir/md/rustc/rustup)".into(),
            parameters: serde_json::json!({"type":"object","properties":{"command":{"type":"string","description":"shell 命令"}},"required":["command"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "web".into(),
            description: "联网搜索最新信息".into(),
            parameters: serde_json::json!({"type":"object","properties":{"query":{"type":"string","description":"搜索词"}},"required":["query"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "glob".into(),
            description: "文件模式匹配，如 **/*.rs".into(),
            parameters: serde_json::json!({"type":"object","properties":{"pattern":{"type":"string","description":"glob 模式，如 src/**/*.rs"}},"required":["pattern"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "lsp".into(),
            description: "cargo check 代码诊断，返回编译错误和警告".into(),
            parameters: serde_json::json!({"type":"object","properties":{"file":{"type":"string","description":"可选：指定文件"}}}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "web-fetch".into(),
            description: "抓取网页实际内容（查API文档/技术资料），URL必须完整含https://".into(),
            parameters: serde_json::json!({"type":"object","properties":{"url":{"type":"string","description":"完整URL"}},"required":["url"]}),
        }},
        ToolDef { tool_type: "function".into(), function: ToolFunction {
            name: "todo".into(),
            description: "创建/更新任务清单。tasks数组每项含content和status(pending/in_progress/completed)".into(),
            parameters: serde_json::json!({"type":"object","properties":{"tasks":{"type":"array","items":{"type":"object","properties":{"content":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed"]}}}}},"required":["tasks"]}),
        }},
    ]
}

/// L4: 根据用户意图关键词筛选工具，减少 DeepSeek 工具解析负担
fn filter_tools_by_intent(msg: &str, tools: Vec<crate::engine::inference::ToolDef>) -> Vec<crate::engine::inference::ToolDef> {
    let msg_lower = msg.to_lowercase();
    let always_keep = ["read", "write"]; // 永远保留的核心工具
    let mut keep: Vec<&str> = always_keep.iter().copied().collect();

    // 代码生成 → write + exec + lsp
    if msg_lower.contains("写") || msg_lower.contains("生成") || msg_lower.contains("创建")
    || msg_lower.contains("write") || msg_lower.contains("create") || msg_lower.contains("html")
    || msg_lower.contains("代码") || msg_lower.contains("code") {
        keep.extend(["edit", "exec", "lsp", "web-fetch"]);
    }
    // 搜索/查找 → search + glob + web
    if msg_lower.contains("搜索") || msg_lower.contains("找") || msg_lower.contains("查")
    || msg_lower.contains("search") || msg_lower.contains("find") || msg_lower.contains("grep") {
        keep.extend(["search", "glob", "web", "semantic"]);
    }
    // 编译/测试 → exec + lsp
    if msg_lower.contains("编译") || msg_lower.contains("build") || msg_lower.contains("测试")
    || msg_lower.contains("test") || msg_lower.contains("运行") || msg_lower.contains("run")
    || msg_lower.contains("cargo") || msg_lower.contains("错误") || msg_lower.contains("error") {
        keep.extend(["exec", "lsp"]);
    }
    // 联网 → web + web-fetch
    if msg_lower.contains("联网") || msg_lower.contains("搜索最新") || msg_lower.contains("文档")
    || msg_lower.contains("docs") || msg_lower.contains("api") || msg_lower.contains("最新") {
        keep.extend(["web", "web-fetch"]);
    }
    // 多步骤/复杂 → todo + ask
    if msg_lower.contains("项目") || msg_lower.contains("project") || msg_lower.contains("系统")
    || msg_lower.contains("架构") || msg_lower.contains("设计") {
        keep.extend(["todo", "glob", "web-fetch"]);
    }

    // 过滤：保留常用 + 意图命中的工具
    let common = ["search", "glob", "exec", "web", "lsp"];
    for &c in &common { if !keep.contains(&c) { keep.push(c); } }
    // 限制 10 个以内（比全量 16 少 40%）
    keep.truncate(10);

    tools.into_iter().filter(|t| keep.contains(&t.function.name.as_str())).collect()
}

fn load_context() -> String {
    let path = context_file_path();
    if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    }
}

/// API 调用重试包装器（最多 2 次，指数退避）
async fn call_with_repair(
    client: &mut crate::engine::inference::InferenceClient,
    messages: Vec<crate::engine::inference::ChatMessage>,
) -> Result<(String, bool), String> {
    for attempt in 0u32..3 {
        match client.chat_stream(messages.clone()).await {
            Ok(mut stream) => {
                use futures::StreamExt;
                let mut text = String::new();
                while let Some(r) = stream.next().await {
                    match r {
                        Ok(c) => text.push_str(&c.content),
                        Err(e) => if attempt < 2 { break; } else { return Err(e.to_string()); },
                    }
                }
                if !text.is_empty() { return Ok((text, true)); }
            }
            Err(e) => {
                if attempt < 2 {
                    // 指数退避: 1s → 2s → 4s
                    let delay_ms = 1000u64 * (1u64 << attempt);
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                } else {
                    return Err(format!("重试3次后仍失败: {}", e));
                }
            }
        }
    }
    Err("重试3次后无内容".into())
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
        let mut client = match crate::engine::inference::InferenceClient::new(&config) {
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
                match call_with_repair(&mut client, msgs).await {
                    Ok((text, _)) => root_cause = text,
                    Err(e) => { let _ = tx.send(Ok(Event::default().data(serde_json::json!({"type":"chunk","content": format!("根因分析失败: {}", e)}).to_string()))); },
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

/// 代码推理：分析函数签名、复杂度、调用关系
pub async fn infer_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let target = req["target"].as_str().unwrap_or("");
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut findings = Vec::new();

    // 1. 搜索函数定义
    if let Ok(output) = tokio::process::Command::new("cmd").args(["/C", &format!("rg -n \"fn {}\" --type rust", target)]).current_dir(&cwd).output().await {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().take(5) {
            findings.push(format!("📍 定义: {}", line));
        }
    }

    // 2. 搜索调用者
    if let Ok(output) = tokio::process::Command::new("cmd").args(["/C", &format!("rg -n \"{}\" --type rust | grep -v \"fn {}\"", target, target)]).current_dir(&cwd).output().await {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let call_count = stdout.lines().count();
        findings.push(format!("📞 被调用 {} 次", call_count));
        for line in stdout.lines().take(8) {
            findings.push(format!("  → {}", line));
        }
    }

    // 3. 分析复杂度（简易：计数 match/if/loop/while）
    if let Ok(output) = tokio::process::Command::new("cmd").args(["/C", &format!("rg -c \"match\\|if \\|loop\\|while\\|for \" --type rust | rg \"{}\"", target)]).current_dir(&cwd).output().await {
        let stdout = String::from_utf8_lossy(&output.stdout);
        findings.push(format!("🔢 复杂度估算: {}", stdout.trim()));
    }

    // 4. 所属模块
    if let Ok(output) = tokio::process::Command::new("cmd").args(["/C", &format!("rg -l \"fn {}\" --type rust", target)]).current_dir(&cwd).output().await {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<_> = stdout.lines().collect();
        findings.push(format!("📁 所在文件: {}", files.join(", ")));
    }

    Json(serde_json::json!({"ok": true, "findings": findings}))
}

/// 项目结构图
pub async fn structure_handler() -> Json<serde_json::Value> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut modules: Vec<serde_json::Value> = Vec::new();
    let _: Vec<String> = Vec::new();

    if cwd.join("Cargo.toml").exists() {
        // 解析模块树
        for dir in ["src", "src/agent", "src/engine", "src/web", "src/tui", "src/evolution"] {
            let path = cwd.join(dir);
            if path.exists() {
                let mod_name = dir.trim_start_matches("src/");
                if let Ok(entries) = std::fs::read_dir(&path) {
                    let files: Vec<_> = entries.flatten()
                        .filter_map(|e| {
                            let n = e.file_name().to_string_lossy().to_string();
                            if n.ends_with(".rs") && n != "mod.rs" { Some(n.trim_end_matches(".rs").to_string()) } else { None }
                        }).collect();
                    modules.push(serde_json::json!({"module": mod_name, "files": files}));
                }
            }
        }
    }

    let summary = format!("{} 个模块，{} 个文件",
        modules.len(),
        modules.iter().map(|m| m["files"].as_array().map(|a| a.len()).unwrap_or(0)).sum::<usize>()
    );

    Json(serde_json::json!({"ok": true, "summary": summary, "modules": modules}))
}

/// 并行读取多个文件
pub async fn parallel_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let paths: Vec<String> = req["paths"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let mut handles = Vec::new();
    for path in paths {
        let full = cwd.join(&path);
        handles.push(tokio::spawn(async move {
            match std::fs::read_to_string(&full) {
                Ok(content) => {
                    let lines: Vec<_> = content.lines().take(100).enumerate()
                        .map(|(i, l)| format!("{:>5}  {}", i+1, l)).collect();
                    serde_json::json!({"path": path, "ok": true, "lines": lines, "total": content.lines().count()})
                }
                Err(e) => serde_json::json!({"path": path, "ok": false, "error": e.to_string()}),
            }
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        if let Ok(r) = h.await { results.push(r); }
    }

    Json(serde_json::json!({"ok": true, "results": results}))
}

/// 保存会话
pub async fn session_save_handler(
    State(state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let title = req["title"].as_str().unwrap_or("未命名");
    let msgs: Vec<String> = req["messages"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let session_id = uuid::Uuid::new_v4().to_string();
    let session = serde_json::json!({
        "id": session_id,
        "title": title,
        "date": chrono::Utc::now().format("%m-%d %H:%M").to_string(),
        "turns": msgs.len() / 2,
        "preview": msgs.last().map(|s| s.chars().take(40).collect::<String>()).unwrap_or_default(),
    });

    let dir = crate::config::forge_data_dir().join("sessions");
    std::fs::create_dir_all(&dir).ok();
    let _ = std::fs::write(dir.join(format!("session_{}.json", &session_id[..8])),
        serde_json::to_string_pretty(&session).unwrap_or_default());

    let summary = state.evolution.lock().await.summary();
    Json(serde_json::json!({"ok": true, "id": session_id, "experiences": summary.total_experiences, "sops": summary.sop_count}))
}

/// 关闭窗口时自动保存（sendBeacon 触发）
pub async fn session_auto_save_handler(
    State(state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let turns = req["turns"].as_u64().unwrap_or(0);
    let preview = req["preview"].as_str().unwrap_or("");
    let messages: Vec<serde_json::Value> = req["messages"].as_array()
        .map(|a| a.clone())
        .unwrap_or_default();

    let dir = crate::config::forge_data_dir().join("sessions");
    std::fs::create_dir_all(&dir).ok();

    // 全量覆写 latest.json（兼容旧加载逻辑，启动恢复用）
    let session_id = chrono::Utc::now().format("%y%m%d-%H%M%S").to_string();
    let latest = serde_json::json!({
        "id": session_id, "date": chrono::Utc::now().format("%m-%d %H:%M").to_string(),
        "turns": turns, "preview": preview.chars().take(80).collect::<String>(),
        "messages": messages, "auto_saved": true,
    });
    let _ = std::fs::write(dir.join("latest.json"),
        serde_json::to_string_pretty(&latest).unwrap_or_default());

    // JSONL 追加式日志（Claude Code 风格：只追加不覆写，防崩溃）
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let jsonl_path = dir.join(format!("session_{}.jsonl", today));
    let mut jsonl = std::fs::OpenOptions::new().create(true).append(true).open(&jsonl_path).unwrap();
    use std::io::Write;
    for msg in &messages {
        let mut line = serde_json::to_string(msg).unwrap_or_default();
        line.push('\n');
        let _ = jsonl.write_all(line.as_bytes());
    }
    let _ = jsonl.flush();

    {
        let mut evo = state.evolution.lock().await;
        evo.record_turn(preview, "auto_save", true);
    }
    Json(serde_json::json!({"ok": true}))
}

/// 获取当前会话（优先 JSONL，回退 latest.json）
pub async fn session_latest_handler() -> Json<serde_json::Value> {
    let dir = crate::config::forge_data_dir().join("sessions");
    // 优先读今天的 JSONL
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let jsonl_path = dir.join(format!("session_{}.jsonl", today));
    if let Ok(content) = std::fs::read_to_string(&jsonl_path) {
        let messages: Vec<serde_json::Value> = content.lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .collect();
        if !messages.is_empty() {
            let turns = messages.iter().filter(|m| m["role"].as_str() == Some("user")).count();
            let preview = messages.iter().rev()
                .find(|m| m["role"].as_str() == Some("user"))
                .and_then(|m| m["content"].as_str())
                .unwrap_or("")
                .chars().take(80).collect::<String>();
            return Json(serde_json::json!({"ok": true, "session": {
                "date": chrono::Utc::now().format("%m-%d %H:%M").to_string(),
                "turns": turns, "preview": preview, "messages": messages,
            }}));
        }
    }
    // 回退 latest.json
    let path = dir.join("latest.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(session) => Json(serde_json::json!({"ok": true, "session": session})),
            Err(_) => Json(serde_json::json!({"ok": false})),
        },
        Err(_) => Json(serde_json::json!({"ok": false})),
    }
}

/// 获取会话列表
pub async fn sessions_list_handler() -> Json<serde_json::Value> {
    let dir = crate::config::forge_data_dir().join("sessions");
    let mut sessions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if let Ok(s) = serde_json::from_str::<serde_json::Value>(&content) {
                    sessions.push(s);
                }
            }
        }
    }
    sessions.sort_by_key(|s| std::cmp::Reverse(s["date"].as_str().unwrap_or("").to_string()));
    Json(serde_json::json!({"ok": true, "sessions": sessions.iter().take(10).collect::<Vec<_>>()}))
}

/// 删除单条会话记录
pub async fn session_delete_handler(Json(req): Json<serde_json::Value>) -> Json<serde_json::Value> {
    let id = req["id"].as_str().unwrap_or("");
    if id.is_empty() { return Json(serde_json::json!({"ok": false})); }
    let dir = crate::config::forge_data_dir().join("sessions");
    // 尝试匹配文件名（session ID 只用了前8位）
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.contains(id) || fname.contains(&id[..id.len().min(8)]) {
                std::fs::remove_file(entry.path()).ok();
                break;
            }
        }
    }
    Json(serde_json::json!({"ok": true}))
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

/// 联网搜索 — Cloudflare Worker 代理（海外节点，SearXNG + DDG + GitHub）
/// 本地 Gitee 搜索兜底
pub async fn web_search_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let query = req["query"].as_str().unwrap_or("");
    if query.is_empty() {
        return Json(serde_json::json!({"ok": false, "results": [], "error": "empty query"}));
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("ForgeShell/1.0")
        .build()
    {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"ok": false, "results": [], "error": e.to_string()})),
    };

    let mut all_results: Vec<String> = Vec::new();
    let mut source: String = "none".into();

    // 1. Cloudflare Worker 搜索代理（SearXNG + DDG + GitHub，从海外节点执行）
    let worker_url = "https://forgeshell.cn/api/search";
    if let Ok(resp) = client.post(worker_url)
        .json(&serde_json::json!({"query": query}))
        .send()
        .await
    {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if data.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(results) = data["results"].as_array() {
                    for r in results {
                        if let Some(s) = r.as_str() {
                            all_results.push(s.to_string());
                        }
                    }
                }
                source = data["source"].as_str().unwrap_or("worker").to_string();
            }
        }
    }

    // 2. 本地 Gitee 兜底（Worker 不可达时）
    if all_results.is_empty() {
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
                if !all_results.is_empty() {
                    source = "gitee-fallback".to_string();
                }
            }
        }
    }

    Json(serde_json::json!({
        "ok": true,
        "results": all_results,
        "source": source,
    }))
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| {
        if c.is_alphanumeric() || c == ' ' { c.to_string() } else { format!("%{:02X}", c as u8) }
    }).collect::<String>().replace(' ', "+")
}

/// 工具调用闭环内联执行器 — 在后端直接执行工具，结果回注对话
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_native_args_read_simple() {
        let result = convert_native_args("read", r#"{"path":"src/main.rs"}"#);
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn test_convert_native_args_read_with_lines() {
        let result = convert_native_args("read", r#"{"path":"src/main.rs","start":10,"end":20}"#);
        assert_eq!(result, "src/main.rs:10:20");
    }

    #[test]
    fn test_convert_native_args_write() {
        let result = convert_native_args("write", r#"{"path":"test.rs","content":"fn main(){}"}"#);
        assert_eq!(result, "test.rs:fn main(){}");
    }

    #[test]
    fn test_convert_native_args_edit() {
        let result = convert_native_args("edit", r#"{"path":"src/lib.rs","start":5,"end":10,"content":"new code"}"#);
        assert_eq!(result, "src/lib.rs:5:10:new code");
    }

    #[test]
    fn test_convert_native_args_search() {
        let result = convert_native_args("search", r#"{"query":"async fn"}"#);
        assert_eq!(result, "async fn");
    }

    #[test]
    fn test_convert_native_args_non_json() {
        let result = convert_native_args("exec", "cargo test");
        assert_eq!(result, "cargo test");
    }

    #[test]
    fn test_convert_native_args_invalid_json() {
        let result = convert_native_args("read", "not json at all");
        assert_eq!(result, "not json at all");
    }

    #[test]
    fn test_build_tool_defs_count() {
        let defs = build_tool_defs();
        assert_eq!(defs.len(), 13, "Should have 13 tool definitions");
        let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"edit"));
        assert!(names.contains(&"exec"));
        assert!(names.contains(&"web"));
    }

    #[test]
    fn test_build_tool_defs_valid_json_schema() {
        for def in &build_tool_defs() {
            assert_eq!(def.tool_type, "function");
            assert!(!def.function.name.is_empty());
            assert!(!def.function.description.is_empty());
            let params = &def.function.parameters;
            assert!(params.get("type").is_some(), "{} missing type", def.function.name);
        }
    }
}

/// 将原生 function calling 的 JSON 参数转为文本格式
fn convert_native_args(tool: &str, json_args: &str) -> String {
    if !json_args.starts_with('{') { return json_args.to_string(); }
    let v: serde_json::Value = match serde_json::from_str(json_args) { Ok(v) => v, Err(_) => return json_args.to_string() };
    match tool {
        "read" => {
            let path = v["path"].as_str().unwrap_or("");
            let start = v["start"].as_u64().map(|n| n.to_string()).unwrap_or_default();
            let end = v["end"].as_u64().map(|n| n.to_string()).unwrap_or_default();
            if start.is_empty() { path.to_string() }
            else { format!("{}:{}:{}", path, start, end) }
        }
        "write" => format!("{}:{}", v["path"].as_str().unwrap_or(""), v["content"].as_str().unwrap_or("")),
        "edit" => format!("{}:{}:{}:{}", v["path"].as_str().unwrap_or(""), v["start"].as_u64().unwrap_or(0), v["end"].as_u64().unwrap_or(0), v["content"].as_str().unwrap_or("")),
        "search" | "web" | "semantic" => v["query"].as_str().or(v["pattern"].as_str()).unwrap_or("").to_string(),
        "glob" => v["pattern"].as_str().unwrap_or("").to_string(),
        "exec" => v["command"].as_str().unwrap_or("").to_string(),
        "lsp" => v["file"].as_str().unwrap_or("").to_string(),
        "save" => v["content"].as_str().unwrap_or("").to_string(),
        "web-fetch" => v["url"].as_str().unwrap_or("").to_string(),
        "todo" => serde_json::to_string(&v["tasks"]).unwrap_or_else(|_| json_args.to_string()),
        "ask" => format!("Q: {} [{}]", v["question"].as_str().unwrap_or(""), v["options"].as_array().map(|a| a.iter().filter_map(|o| o.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()),
        _ => json_args.to_string(),
    }
}

async fn execute_tool_inline(tool: &str, arg: &str) -> String {
    match tool {
        "web" => {
            // 调用搜索代理（优先 Worker，Gitee 兜底）
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .user_agent("ForgeShell/1.0")
                .build()
            {
                Ok(c) => c, Err(e) => return format!("创建HTTP客户端失败: {}", e),
            };
            let worker_url = "https://forgeshell.cn/api/search";
            if let Ok(resp) = client.post(worker_url)
                .json(&serde_json::json!({"query": arg}))
                .send().await
            {
                if let Ok(data) = resp.json::<serde_json::Value>().await {
                    if let Some(results) = data["results"].as_array() {
                        if !results.is_empty() {
                            let lines: Vec<String> = results.iter()
                                .filter_map(|r| r.as_str().map(|s| s.to_string()))
                                .take(6).collect();
                            return lines.join("\n");
                        }
                    }
                }
            }
            // Gitee 兜底
            let gitee_url = format!("https://gitee.com/api/v5/search/repositories?q={}&sort=stars_count&per_page=3",
                urlencoding(arg));
            if let Ok(resp) = client.get(&gitee_url).send().await {
                if let Ok(items) = resp.json::<Vec<serde_json::Value>>().await {
                    let lines: Vec<String> = items.iter().take(3).map(|item| {
                        format!("[Gitee] {} — {}",
                            item["full_name"].as_str().unwrap_or(""),
                            item["description"].as_str().unwrap_or("").chars().take(80).collect::<String>())
                    }).collect();
                    if !lines.is_empty() { return lines.join("\n"); }
                }
            }
            "搜索未返回结果，请尝试换关键词或换个问法。".to_string()
        }
        "search" => {
            match tokio::process::Command::new("rg")
                .args(["--no-heading", "-n", "--max-count=30", arg, "."])
                .output().await
            {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                    if stdout.trim().is_empty() { "项目中未找到匹配，建议检查拼写或换个关键词。".into() }
                    else { stdout }
                }
                Err(e) => format!("ripgrep 执行失败: {}", e),
            }
        }
        "read" => {
            let parts: Vec<&str> = arg.split(':').collect();
            let path = parts.first().map(|s| s.trim()).unwrap_or("");
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().take(100).collect();
                    lines.iter().enumerate()
                        .map(|(i, l)| format!("{:>5}  {}", i + 1, l))
                        .collect::<Vec<_>>().join("\n")
                }
                Err(e) => format!("读取失败: {}", e),
            }
        }
        "exec" => {
            match tokio::time::timeout(std::time::Duration::from_secs(30),
                tokio::process::Command::new("cmd").args(["/C", arg]).output()
            ).await {
                Ok(Ok(o)) => {
                    let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                    if o.status.success() {
                        format!("{}\n{}", stdout, stderr)
                    } else {
                        format!("命令失败 (exit={}):\n{}\n{}",
                            o.status.code().unwrap_or(-1), stdout, stderr)
                    }
                }
                Ok(Err(e)) => format!("命令执行失败: {}", e),
                Err(_) => "命令执行超时（30秒），已中断".to_string(),
            }
        }
        "lsp" => {
            match tokio::time::timeout(std::time::Duration::from_secs(30),
                tokio::process::Command::new("cargo")
                    .args(["check", "--message-format=json"])
                    .output()
            ).await
            {
                Ok(Ok(o)) => {
                    let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                    let errors: Vec<String> = stdout.lines().filter_map(|line| {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                            if v["reason"].as_str() == Some("compiler-message") {
                                let msg = v["message"]["rendered"].as_str().unwrap_or("");
                                if !msg.is_empty() { return Some(msg.to_string()); }
                            }
                        }
                        None
                    }).take(10).collect();
                    if errors.is_empty() { "cargo check 无错误".into() }
                    else { errors.join("\n") }
                }
                Ok(Err(e)) => format!("cargo check 执行失败: {}", e),
                Err(_) => "cargo check 超时（30秒），已中断".to_string(),
            }
        }
        "snap" => {
            // 读取快照目录
            let snap_dir = crate::config::forge_data_dir().join("snapshots");
            match std::fs::read_dir(&snap_dir) {
                Ok(entries) => {
                    let mut snaps = Vec::new();
                    for e in entries.flatten() {
                        snaps.push(e.file_name().to_string_lossy().to_string());
                    }
                    if snaps.is_empty() { "无快照".into() }
                    else { snaps.join("\n") }
                }
                Err(_) => "无快照目录".into(),
            }
        }
        "community-fix" => {
            // 从社区经验池搜索已知问题的修复
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build() { Ok(c) => c, Err(e) => return format!("{}", e) };
            let url = "https://forgeshell.cn/api/experience";
            match client.get(url).send().await {
                Ok(resp) => {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        let fixes = data["fixes"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
                        if fixes.is_empty() { "社区经验池暂无相关修复".into() }
                        else {
                            let matched: Vec<String> = fixes.iter()
                                .filter(|f| {
                                    let p = f["pattern"].as_str().unwrap_or("");
                                    arg.is_empty() || p.contains(arg)
                                })
                                .take(5)
                                .map(|f| format!("- [{}] {} → {}", f["timestamp"].as_str().unwrap_or("?"), f["pattern"].as_str().unwrap_or("?"), f["fix"].as_str().unwrap_or("?")))
                                .collect();
                            if matched.is_empty() { format!("经验池中未找到与 '{}' 匹配的修复", arg) }
                            else { format!("社区经验池匹配 ({} 条):\n{}", matched.len(), matched.join("\n")) }
                        }
                    } else { "经验池查询失败".into() }
                }
                Err(e) => format!("连接经验池失败: {}", e),
            }
        }
        "share-fix" => {
            // 匿名提交修复策略到社区经验池
            let parts: Vec<&str> = arg.splitn(3, '|').collect();
            let pattern = parts.first().unwrap_or(&"").trim();
            let fix_desc = parts.get(1).unwrap_or(&"").trim();
            let diag = parts.get(2).unwrap_or(&"").trim();
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build() { Ok(c) => c, Err(e) => return format!("{}", e) };
            let body = serde_json::json!({"pattern": pattern, "fix": fix_desc, "diagnosis": diag, "component": "shared"});
            match client.post("https://forgeshell.cn/api/experience").json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() { "经验已匿名提交到社区熔池，感谢贡献！".into() }
                    else { format!("提交失败: {}", resp.status()) }
                }
                Err(e) => format!("提交失败: {}", e),
            }
        }
        "save" => {
            let ctx_path = std::path::PathBuf::from("FORGESHELL_CONTEXT.md");
            // 追加模式：允许多次保存累积
            let entry = format!("\n## {}\n{}\n", chrono::Utc::now().format("%m-%d %H:%M"), arg);
            match std::fs::OpenOptions::new().create(true).append(true).write(true).open(&ctx_path) {
                Ok(mut f) => {
                    use std::io::Write;
                    match writeln!(f, "{}", entry) {
                        Ok(()) => format!("已追加到 FORGESHELL_CONTEXT.md ({})", arg.chars().take(30).collect::<String>()),
                        Err(e) => format!("写入失败: {}", e),
                    }
                }
                Err(e) => format!("保存失败: {}", e),
            }
        }
        "edit" => {
            let parts: Vec<&str> = arg.splitn(4, ':').collect();
            let path = parts.first().map(|s| s.trim()).unwrap_or("");
            let start = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let end = parts.get(2).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
            let content = parts.get(3).unwrap_or(&"");
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let full_path = cwd.join(path);
            match std::fs::read_to_string(&full_path) {
                Ok(original) => {
                    // 备份
                    let backup_dir = crate::config::forge_data_dir().join("backups");
                    std::fs::create_dir_all(&backup_dir).ok();
                    let safe_name = path.replace(['/', '\\'], "_");
                    let _ = std::fs::write(backup_dir.join(format!("{}_{}.bak", safe_name, chrono::Utc::now().format("%H%M%S"))), &original);
                    let lines: Vec<&str> = original.lines().collect();
                    let s = start.max(1).min(lines.len());
                    let e = if end > 0 { end.min(lines.len()) } else { s };
                    let old_lines: Vec<&str> = lines[s.saturating_sub(1)..e].to_vec();
                    let new_lines: Vec<&str> = content.lines().collect();
                    let mut result = Vec::new();
                    result.extend_from_slice(&lines[..s.saturating_sub(1)]);
                    result.extend(&new_lines);
                    result.extend_from_slice(&lines[e..]);
                    let new_content = result.join("\n");
                    // 生成 diff 摘要（前端可渲染）
                    let diff_summary: Vec<String> = old_lines.iter().map(|l| format!("-{}", l))
                        .chain(new_lines.iter().map(|l| format!("+{}", l)))
                        .collect();
                    match std::fs::write(&full_path, &new_content) {
                        Ok(()) => format!("已编辑 {} 行{}-{}\n```diff\n{}\n```", path, s, e, diff_summary.join("\n")),
                        Err(e) => format!("编辑失败: {}", e),
                    }
                }
                Err(e) => format!("读取 {} 失败: {}", path, e),
            }
        }
        "write" => {
            let parts: Vec<&str> = arg.splitn(2, ':').collect();
            let path = parts.first().map(|s| s.trim()).unwrap_or("");
            let content = parts.get(1).unwrap_or(&"");
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let full_path = cwd.join(path);
            // 如果文件已存在，先备份
            if full_path.exists() {
                let backup_dir = crate::config::forge_data_dir().join("backups");
                std::fs::create_dir_all(&backup_dir).ok();
                let safe_name = path.replace(['/', '\\'], "_");
                let _ = std::fs::copy(&full_path, backup_dir.join(format!("{}_{}.bak", safe_name, chrono::Utc::now().format("%H%M%S"))));
            }
            // 确保父目录存在
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            match std::fs::write(&full_path, content) {
                Ok(()) => format!("已写入 {} ({} 行, {} 字节)", path, content.lines().count(), content.len()),
                Err(e) => format!("写入失败: {}", e),
            }
        }
        "semantic" => {
            // 查询语义索引（函数/结构体/枚举定义和引用）
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let index = crate::engine::semantic_index::SemanticIndex::new(cwd);
            if arg.is_empty() {
                "语义索引：请提供查询关键词或符号名，如 [TOOL:semantic:main] 或 [TOOL:semantic:函数名]".into()
            } else {
                let by_kind = index.query_by_kind(arg);
                let fuzzy = index.fuzzy_search(arg);
                if by_kind.is_empty() && fuzzy.is_empty() {
                    format!("语义索引中未找到与 '{}' 相关的符号", arg)
                } else {
                    let mut out = String::new();
                    if !by_kind.is_empty() {
                        out.push_str(&format!("## 类型匹配 ({} 条)\n", by_kind.len()));
                        for s in &by_kind { out.push_str(&format!("- {}:{} {} {}\n", s.file, s.line, s.kind, s.name)); }
                    }
                    if !fuzzy.is_empty() {
                        out.push_str(&format!("\n## 模糊匹配 ({} 条)\n", fuzzy.len()));
                        for s in &fuzzy { out.push_str(&format!("- {}:{} {} {}\n", s.file, s.line, s.kind, s.name)); }
                    }
                    out
                }
            }
        }
        "glob" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let pattern = arg;
            let mut results = Vec::new();
            // 简易 glob: 支持 **/*.rs, src/**/*.rs, *.toml 等
            let (base_dir, file_pattern): (std::path::PathBuf, String) = if pattern.contains("**/") {
                let parts: Vec<&str> = pattern.split("**/").collect();
                (cwd.join(parts[0]), if parts.len() > 1 { parts[1].to_string() } else { "*".into() })
            } else if pattern.contains('/') {
                let p = std::path::Path::new(pattern);
                (cwd.join(p.parent().unwrap_or(std::path::Path::new("."))),
                 p.file_name().unwrap_or_default().to_string_lossy().to_string())
            } else {
                (cwd.clone(), pattern.to_string())
            };
            let ext_match = file_pattern.strip_prefix("*.").unwrap_or(&file_pattern).to_string();
            fn collect_files(dir: &std::path::Path, ext: &str, results: &mut Vec<String>, depth: u32) {
                if depth > 5 { return; }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        let name = p.file_name().unwrap_or_default().to_string_lossy();
                        if name.starts_with('.') || name == "target" || name == "node_modules" { continue; }
                        if p.is_dir() { collect_files(&p, ext, results, depth + 1); }
                        else if ext == "*" || name.ends_with(ext) {
                            if let Ok(meta) = p.metadata() {
                                results.push(format!("{} ({}行, {}B)", p.strip_prefix(&std::env::current_dir().unwrap_or_default()).unwrap_or(&p).display(), name.len(), meta.len()));
                            }
                        }
                    }
                }
            }
            collect_files(&base_dir, &ext_match, &mut results, 0);
            if results.is_empty() { format!("glob '{}' 无匹配文件", pattern) }
            else { format!("glob '{}' 找到 {} 个文件:\n{}", pattern, results.len(), results.join("\n")) }
        }
        "web-fetch" => {
            let url = arg.trim();
            if !url.starts_with("http") { format!("URL 必须以 http/https 开头: {}", url) }
            else {
                match reqwest::Client::new().get(url).timeout(std::time::Duration::from_secs(10))
                    .header("User-Agent", "ForgeShell/1.0").send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        match resp.text().await {
                            Ok(text) => {
                                // 只取前 8000 字符，避免超长
                                let truncated = if text.len() > 8000 { &text[..8000] } else { &text };
                                format!("[{}] {}\n{}\n(共{}字符)", status.as_u16(), url, truncated, text.len())
                            }
                            Err(e) => format!("读取响应失败: {}", e),
                        }
                    }
                    Err(e) => format!("请求失败: {}", e),
                }
            }
        }
        "todo" => {
            format!("任务清单已记录: {}", arg.chars().take(500).collect::<String>())
        }
        "ask" => {
            format!("问题已发送给用户: {}", arg.chars().take(200).collect::<String>())
        }
        _ => format!("工具 {} 不支持在工具循环中自动执行", tool),
    }
}

/// 错误日志查询
pub async fn error_logs_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let logs = state.error_logger.recent(50);
    let diag = state.error_logger.diagnose();
    Json(serde_json::json!({"ok": true, "logs": logs, "diagnosis": diag}))
}

/// 清空错误日志
pub async fn error_logs_clear_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    state.error_logger.clear();
    Json(serde_json::json!({"ok": true}))
}

/// 提示词优化器统计
pub async fn prompt_stats_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    Json(state.prompt_optimizer.lock().await.stats())
}

/// 语义索引查询
pub async fn semantic_handler(
    State(state): State<SharedState>,
    axum::extract::Query(params): axum::extract::Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let query = params.get("q").map(|s| s.as_str()).unwrap_or("");
    let kind = params.get("kind").map(|s| s.as_str()).unwrap_or("");

    let results: Vec<_> = if !query.is_empty() {
        state.semantic_index.fuzzy_search(query).into_iter().map(|e| {
            serde_json::json!({"name":e.name,"kind":e.kind,"file":e.file,"line":e.line,"sig":e.signature})
        }).take(30).collect()
    } else if !kind.is_empty() {
        state.semantic_index.query_by_kind(kind).into_iter().map(|e| {
            serde_json::json!({"name":e.name,"kind":e.kind,"file":e.file,"line":e.line,"sig":e.signature})
        }).take(30).collect()
    } else {
        vec![]
    };

    Json(serde_json::json!({"ok": true, "total": state.semantic_index.len(), "results": results}))
}

/// 缓存监控仪表盘
pub async fn cache_monitor_handler(
    State(state): State<SharedState>,
) -> Json<serde_json::Value> {
    let cache = state.cache_stats.lock().await.clone();
    let hit_rate = if cache.cache_hit_tokens + cache.cache_miss_tokens > 0 {
        cache.cache_hit_tokens as f64 / (cache.cache_hit_tokens + cache.cache_miss_tokens) as f64 * 100.0
    } else { 0.0 };
    let saved = cache.cache_hit_tokens as f64 * 0.000001; // 命中 token 免费 (¥1/M)
    let cost = cache.cache_miss_tokens as f64 * 0.000001;

    Json(serde_json::json!({
        "hit_rate_pct": format!("{:.2}", hit_rate),
        "hit_tokens": cache.cache_hit_tokens,
        "miss_tokens": cache.cache_miss_tokens,
        "prompt_tokens": cache.prompt_tokens,
        "completion_tokens": cache.completion_tokens,
        "saved_yuan": format!("{:.6}", saved),
        "cost_yuan": format!("{:.6}", cost),
        "total_requests": cache.total_tokens,
    }))
}

/// MCP JSON-RPC 端点
pub async fn mcp_handler(
    Json(req): Json<crate::engine::mcp::JsonRpcRequest>,
) -> Json<crate::engine::mcp::JsonRpcResponse> {
    Json(crate::engine::mcp::handle_mcp_request(&req).await)
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

/// 增量编辑：只改指定行，不覆写全文件
pub async fn edit_handler(
    State(state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let path = req["path"].as_str().unwrap_or("");
    let start_line = req["start"].as_u64().unwrap_or(0) as usize;
    let end_line = req["end"].as_u64().unwrap_or(0) as usize;
    let content = req["content"].as_str().unwrap_or("");

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let full_path = cwd.join(path);

    // 读原文件
    let original = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    };

    // 备份原文件
    let _ = state.backup.lock().await.backup_before_write(&full_path, &format!("edit {}-{}", start_line, end_line));

    // 行级别替换
    let lines: Vec<&str> = original.lines().collect();
    let s = start_line.max(1).min(lines.len());
    let e = if end_line > 0 { end_line.min(lines.len()) } else { s };
    let new_lines: Vec<&str> = content.lines().collect();

    let mut result = Vec::new();
    result.extend_from_slice(&lines[..s-1]);
    result.extend(&new_lines);
    result.extend_from_slice(&lines[e..]);

    let new_content = result.join("\n");
    if let Err(e) = std::fs::write(&full_path, &new_content) {
        return Json(serde_json::json!({"ok": false, "error": e.to_string()}));
    }

    Json(serde_json::json!({
        "ok": true,
        "path": path,
        "replaced": format!("行{}-{} → {}行", s, e, new_lines.len()),
        "total_lines": result.len()
    }))
}

/// 文件快照回滚（按文件粒度，SHA256 校验）
pub async fn snapshot_handler(
    State(state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let action = req["action"].as_str().unwrap_or("list");
    let backup = state.backup.lock().await;

    match action {
        "list" => {
            let snaps: Vec<_> = backup.session_backups().iter().map(|e| {
                serde_json::json!({"file": e.original_path.to_string_lossy(), "at": e.timestamp.to_rfc3339(), "desc": e.description})
            }).collect();
            Json(serde_json::json!({"ok": true, "snapshots": snaps}))
        }
        "rollback" => {
            let file = req["file"].as_str().unwrap_or("");
            let path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(file);
            match backup.rollback_file(&path) {
                Ok(true) => Json(serde_json::json!({"ok": true, "message": format!("{} 已回滚", file)})),
                Ok(false) => Json(serde_json::json!({"ok": false, "error": "未找到快照"})),
                Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
            }
        }
        _ => Json(serde_json::json!({"ok": false, "error": "未知操作"}))
    }
}

/// LSP 信息：Tree-sitter AST 解析 + 符号索引 + cargo check
pub async fn lsp_rich_handler(
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let target = req["target"].as_str().unwrap_or("");
    let file = req["file"].as_str().unwrap_or("");
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut result = serde_json::json!({"ok": true, "definitions": [], "references": [], "imports": [], "check_errors": []});

    // 1. Tree-sitter AST 符号解析
    if let Some(mut parser) = crate::engine::ast_parser::AstParser::new() {
        // 解析指定文件或搜索所有 rs 文件
        let files: Vec<PathBuf> = if !file.is_empty() {
            vec![cwd.join(file)]
        } else {
            // 找所有 .rs 文件（排除 target）
            let mut fs = Vec::new();
            if let Ok(entries) = std::fs::read_dir(cwd.join("src")) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().map(|e| e == "rs").unwrap_or(false) {
                        fs.push(p);
                    }
                }
            }
            fs
        };

        let mut all_syms = Vec::new();
        let mut all_refs = Vec::new();
        let mut all_imports = Vec::new();

        for f in &files {
            if let Ok(source) = std::fs::read_to_string(f) {
                let rel_path = f.strip_prefix(&cwd).unwrap_or(f).to_string_lossy().to_string();
                let syms = parser.parse_symbols(&source, &rel_path);
                all_syms.extend(syms);
                all_imports.extend(parser.parse_imports(&source));

                if !target.is_empty() {
                    let refs = parser.find_references(&source, target, &rel_path);
                    all_refs.extend(refs);
                }
            }
        }

        result["definitions"] = serde_json::json!(all_syms.iter().map(|s| serde_json::json!({
            "name": s.name, "kind": s.kind, "file": s.file, "line": s.line, "signature": s.signature
        })).collect::<Vec<_>>());

        result["references"] = serde_json::json!(all_refs.iter().map(|r| serde_json::json!({
            "symbol": r.symbol, "file": r.file, "line": r.line, "context": r.context
        })).collect::<Vec<_>>());

        result["imports"] = serde_json::json!(all_imports);
    }

    // 2. cargo check 错误
    if let Ok(output) = tokio::process::Command::new("cmd").args(["/C", "cargo check --message-format=json 2>&1"]).current_dir(&cwd).output().await {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let errors: Vec<_> = stdout.lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|m| m["reason"].as_str() == Some("compiler-message"))
            .filter_map(|m| {
                let msg = &m["message"];
                if msg["level"].as_str() == Some("error") {
                    Some(serde_json::json!({
                        "message": msg["message"],
                        "file": msg["spans"][0]["file_name"],
                        "line": msg["spans"][0]["line_start"],
                        "code": msg["code"],
                    }))
                } else { None }
            }).take(15).collect();
        result["check_errors"] = serde_json::json!(errors);
    }

    Json(result)
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
    State(_state): State<SharedState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let cmd = req["command"].as_str().unwrap_or("");
    let cwd = req["cwd"].as_str().unwrap_or(".");

    // L3: 统一白名单（与 sandbox::GENERAL_COMMANDS 同步）
    const ALLOWED: &[&str] = &[
        "cargo check", "cargo test", "cargo build", "cargo fmt", "cargo clippy", "cargo doc",
        "cargo update", "cargo tree", "cargo metadata", "cargo --version", "cargo new", "cargo init",
        "git status", "git diff", "git log", "git branch", "git add", "git commit", "git stash",
        "git checkout", "git switch", "git restore", "git rebase", "git clone",
        "rustc --version", "rustup", "ls", "dir", "echo", "type", "rg", "mkdir", "md",
    ];
    let cmd_trimmed = cmd.trim();
    let is_allowed = ALLOWED.iter().any(|a| cmd_trimmed.starts_with(a));

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

    let mut client = match crate::engine::inference::InferenceClient::new(&config) {
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
        let msg_len = req.message.len();
        let mode = req.mode.as_deref().unwrap_or("interactive");
        tracing::info!("[chat] 收到消息 len={} mode={}", msg_len, mode);

        // 提前 clone 用于 panic 日志
        let _ = tx.send(Ok(Event::default().data("{\"type\":\"trace\",\"msg\":\"start\"}")));
        let mut config = state_clone.config.lock().await.clone();
        let _ = tx.send(Ok(Event::default().data("{\"type\":\"trace\",\"msg\":\"post_config\"}")));

        // 模型路由：模式 × 复杂度 动态选择模型
        let router = crate::engine::router::ModelRouter::new(
            config.ai.default_model.clone(),
            config.ai.flash_model.clone(),
        );
        let mut decision = router.decide(&req.message, 0);

        // 用户模型偏好覆盖 Router（auto 时不覆盖）
        let model_pref = req.model_pref.as_deref().unwrap_or("auto");
        match model_pref {
            "pro" => { decision.model = config.ai.default_model.clone(); }
            "flash" => { decision.model = config.ai.flash_model.clone(); }
            "local" => { decision.model = "ollama".into(); }
            _ => {} // auto: Router 自动决定
        }
        // 模式覆盖：速办强制Flash（仅在 auto 模式下生效）
        if model_pref == "auto" {
            match mode {
                "quick" => {
                    decision.model = config.ai.flash_model.clone();
                    decision.estimated_cost *= 0.3;
                }
                _ => {} // interactive: Router 自动决定
            }
        }
        config.ai.default_model = decision.model.clone();

        tracing::info!("[chat] 路由 model={} complexity={:?} cost={}", decision.model, decision.complexity, decision.estimated_cost);

        // 模型选择对用户隐身（只通过顶部栏费用体现）
        let _ = tx.send(Ok(Event::default().data(
            serde_json::json!({"type": "meta", "model": decision.model, "mode": mode, "cost": decision.estimated_cost}).to_string()
        )));
        // UCB1 提示词优化器：运行时选择最优变体
        let system_variant = match tokio::time::timeout(std::time::Duration::from_secs(5), state_clone.prompt_optimizer.lock()).await {
            Ok(g) => g.select_best(),
            Err(_) => "v1-full".to_string(),
        };
        // L4: 统一用紧凑prompt——完整版4498字符导致DeepSeek Pro长文本处理极慢甚至挂死
        let system_msg = crate::system_prompt::get_system_prompt_compact();
        // 根据复杂度动态设定输出上限（DeepSeek V4 最大输出 384K）
        // Claude Code 对齐: thinking+回复共享 max_tokens（ccswitch 同逻辑）
        // 不需要单独拆分预算——UI分离已足够，DeepSeek API不支持独立thinking_budget
        // 输出上限: Simple短回答 / Moderate中等文件 / Complex大项目
        // write工具内容计入token, Moderate以上需要足够空间
        let max_out_tokens: u32 = match decision.complexity {
            crate::engine::router::Complexity::Simple => 8192, // 文件创建任务需要足够token写完整内容
            crate::engine::router::Complexity::Moderate => 65536,
            crate::engine::router::Complexity::Complex => 131072,
        };

        // L3: 项目上下文，全局超时保证永不阻塞聊天流
        let project_info = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            build_project_context(),
        ).await.unwrap_or_else(|_| {
            tracing::warn!("[chat] 项目上下文超时，跳过");
            String::new()
        });
        tracing::info!("[chat] 项目上下文完成 len={}", project_info.len());

        // L3: 会话压缩——旧轮摘要，新轮完整
        let context = load_context();
        let mut turn = state_clone.session_turn.lock().await;
        *turn += 1;
        let compressed = if *turn > 5 {
            let summaries = state_clone.session_summaries.lock().await;
            if !summaries.is_empty() {
                format!("\n\n历史摘要(前{}轮):\n{}", summaries.len(), summaries.join("\n"))
            } else { String::new() }
        } else { String::new() };

        // L3: SessionManager 原子保存
        let session_mgr = crate::engine::session::SessionManager::new(
            crate::config::forge_data_dir().join("sessions")
        );
        session_mgr.save_latest(&crate::engine::session::SessionRecord {
            date: chrono::Utc::now().format("%m-%d %H:%M").to_string(),
            turn: *turn,
            messages: vec![crate::engine::session::SessionMessage { role: "user".into(), content: req.message.clone() }],
        });

        let user_msg = format!("{}{}{}{}", req.message, project_info, compressed, if !context.is_empty() { format!("\n跨会话: {}", context) } else { String::new() });

        // 复杂任务：双模型辩论制（Pro 主攻 + Flash 审查）
        if matches!(decision.complexity, crate::engine::router::Complexity::Complex) {
            let _ = tx.send(Ok(Event::default().data(
                serde_json::json!({"type":"chunk","content":"\n⚔️ 启动双模型辩论…\n"}).to_string()
            )));

            let pro_config = config.clone();
            let mut flash_config = config.clone();
            flash_config.ai.default_model = flash_config.ai.flash_model.clone();

            let debate_msg = req.message.clone();
            let debate_msg2 = req.message.clone();
            let debate_sys = system_msg.clone();

            // Pro 和 Flash 并行
            let (pro_handle, flash_handle) = tokio::join!(
                tokio::spawn(async move {
                    let mut client = match crate::engine::inference::InferenceClient::new(&pro_config).map(|c| c.with_max_tokens(393216)) {
                        Ok(c) => c, Err(_) => return String::new(),
                    };
                    let msgs = vec![
                        crate::engine::inference::ChatMessage::system(&debate_sys),
                        crate::engine::inference::ChatMessage::user(&format!("请给出完整方案: {}", debate_msg)),
                    ];
                    let mut text = String::new();
                    if let Ok(mut stream) = client.chat_stream(msgs).await {
                        use futures::StreamExt;
                        while let Some(r) = stream.next().await {
                            if let Ok(c) = r { text.push_str(&c.content); }
                        }
                    }
                    text
                }),
                tokio::spawn(async move {
                    let mut client = match crate::engine::inference::InferenceClient::new(&flash_config).map(|c| c.with_max_tokens(16384)) {
                        Ok(c) => c, Err(_) => return String::new(),
                    };
                    let msgs = vec![
                        crate::engine::inference::ChatMessage::system("你是代码审查专家。快速指出方案的潜在问题、边界case、安全风险。用中文，只挑刺，不夸。"),
                        crate::engine::inference::ChatMessage::user(&format!("审查这个方案: {}", debate_msg2)),
                    ];
                    let mut text = String::new();
                    if let Ok(mut stream) = client.chat_stream(msgs).await {
                        use futures::StreamExt;
                        while let Some(r) = stream.next().await {
                            if let Ok(c) = r { text.push_str(&c.content); }
                        }
                    }
                    text
                }),
            );

            let pro_result = pro_handle.unwrap_or_default();
            let flash_critique = flash_handle.unwrap_or_default();

            let _ = tx.send(Ok(Event::default().data(
                serde_json::json!({"type":"chunk","content": format!("\n🧠 Pro方案:\n{}\n\n⚡ Flash审查:\n{}\n", pro_result, flash_critique)}).to_string()
            )));
            let _ = tx.send(Ok(Event::default().data(
                serde_json::json!({"type":"done","finish_reason": "debate_complete"}).to_string()
            )));
            return;
        }

        // 子任务级路由：复杂任务拆解，每个子任务独立选模型
        if matches!(decision.complexity, crate::engine::router::Complexity::Complex) {
            let oracle = crate::agent::orchestrator::Orchestrator::new(8);
            let plan = oracle.decompose(&req.message);

            if plan.tasks.len() > 1 {
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "chunk", "content": format!("\n📋 拆解为{}个子任务，并行执行\n", plan.tasks.len())}).to_string()
                )));

                let mut all_content = String::new();
                let groups = plan.parallel_groups.clone();
                let tasks = plan.tasks.clone();
                for group in groups {
                    let mut handles = Vec::new();
                    for task_id in group {
                        if let Some(task) = tasks.iter().find(|t| t.id == task_id) {
                            let task_desc = task.description.clone();
                            let task_model = if task.read_only {
                                config.ai.flash_model.clone()
                            } else {
                                config.ai.default_model.clone()
                            };
                            let mut task_config = config.clone();
                            task_config.ai.default_model = task_model;
                            let tx = tx.clone();
                            let system_msg = system_msg.clone();

                            handles.push(tokio::spawn(async move {
                                let executor = crate::agent::agent_executor::AgentExecutor::new(task_config);
                                match executor.run(&task_desc, &system_msg).await {
                                    Ok(result) => {
                                        let tools_info: Vec<String> = result.tools_used.iter()
                                            .map(|t| format!("[{}]", t.tool)).collect();
                                        let _ = tx.send(Ok(Event::default().data(
                                            serde_json::json!({"type": "chunk", "content": format!("\n✅ [{}] 用了{}个工具({})→{}轮完成\n{}\n", task_id, result.tools_used.len(), tools_info.join(""), result.rounds, result.final_output)}).to_string()
                                        )));
                                        result.final_output
                                    }
                                    Err(e) => format!("[{}] 失败: {}", task_id, e),
                                }
                            }));
                        }
                    }

                    for h in handles {
                        if let Ok(r) = h.await { all_content.push_str(&r); all_content.push('\n'); }
                    }
                }

                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "done", "finish_reason": "subtask_merge"}).to_string()
                )));

                // 记录经验
                let evo_cost = all_content.len() as f64 * 0.000001;
                {
                    let mut cost = state_clone.total_cost.lock().await;
                    *cost += evo_cost;
                }
                {
                    let mut evo = state_clone.evolution.lock().await;
                    evo.record_turn(&req.message, &all_content, !all_content.is_empty());
                    evo.try_reflect();
                }
                return;
            }
        }

        // 拉取跨轮对话历史（解决 AI 失忆问题）
        let history = {
            let mut hist = state_clone.conversation_history.lock().await;
            let h = hist.clone();
            // 保留最近 8 条消息（4 轮对话）
            if h.len() > 8 { *hist = h[h.len()-8..].to_vec(); }
            h
        };

        // 对话上下文——工具循环会往里追加工具结果
        let mut conversation = vec![
            crate::engine::inference::ChatMessage::system(&system_msg),
        ];
        // 注入历史对话
        conversation.extend(history);
        // 当前用户消息
        conversation.push(crate::engine::inference::ChatMessage::user(&user_msg));

        let mut has_content = false;
        let mut tool_round = 0u32;
        let max_tool_rounds = 5u32;

        // 工具调用闭环：AI 输出 [TOOL:xxx] → 后端执行 → 结果回注 → 再调 AI
        // 规划模式禁用写工具（物理强制执行）
        let is_pro = decision.model.contains("pro");
        let tool_defs = if is_pro {
            build_pro_tools() // Pro用10核心工具
        } else {
            // Flash: 根据意图关键词筛选工具（16→≤10），减少DeepSeek解析负担
            filter_tools_by_intent(&req.message, build_tool_defs())
        };
        // 思考模式：仅 Complex 任务启用（Pro 超时降级 Flash 时自动关闭）
        let mut use_thinking = matches!(decision.complexity, crate::engine::router::Complexity::Complex);
        loop {
            let mut client = match crate::engine::inference::InferenceClient::new(&config)
                .map(|c| c.with_max_tokens(max_out_tokens).with_thinking(use_thinking).with_tools(tool_defs.clone())) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Ok(Event::default().data(
                        serde_json::json!({"type": "error", "message": format!("客户端创建失败: {}", e)}).to_string()
                    )));
                    return;
                }
            };

            tracing::info!("[chat] API调用 round={} msgs={} tools={} thinking={}", tool_round, conversation.len(), tool_defs.len(), use_thinking);

            let mut round_text = String::new();
            let mut round_reasoning = String::new();
            let mut last_chunk_tool_calls: Vec<crate::engine::inference::AccumulatedToolCall> = Vec::new();
            // L3：三超时——连接30s + 总流 + 无内容超时。keepalive 帧无法无限续命
            let conn_timeout = std::time::Duration::from_secs(30);
            let total_timeout = std::time::Duration::from_secs(if use_thinking { 300 } else { 150 });
            let no_content_timeout = std::time::Duration::from_secs(45);
            let stream_result = tokio::time::timeout(conn_timeout, client.chat_stream(conversation.clone())).await
                .unwrap_or_else(|_| Err(crate::error::ForgeError::Api("API连接超时".into())));

            match stream_result {
                Ok(mut stream) => {
                    use futures::StreamExt;
                    let mut acc = crate::engine::stream::StreamAccumulator::new();
                    let total_deadline = tokio::time::Instant::now() + total_timeout;
                    let mut last_content = tokio::time::Instant::now();
                    loop {
                        let now = tokio::time::Instant::now();
                        // 总超时
                        if now >= total_deadline {
                            tracing::warn!("[chat] 总超时 {}s，已收={}B", total_timeout.as_secs(), acc.content.len());
                            let msg = if acc.can_degrade() { "\n⚠️ 响应超时\n" } else { "" };
                            let _ = tx.send(Ok(Event::default().data(
                                serde_json::json!({"type": "chunk", "content": msg}).to_string()
                            )));
                            if !acc.can_degrade() {
                                let _ = tx.send(Ok(Event::default().data(
                                    serde_json::json!({"type": "error", "message": format!("响应超时({}s)", total_timeout.as_secs())}).to_string()
                                )));
                            }
                            break;
                        }
                        // 无内容超时（keepalive 帧无法续命）
                        let idle = now.duration_since(last_content);
                        if idle >= no_content_timeout {
                            tracing::warn!("[chat] 无内容超时 {}s，断开", idle.as_secs());
                            if acc.can_degrade() {
                                let _ = tx.send(Ok(Event::default().data(
                                    serde_json::json!({"type": "chunk", "content": "\n⚠️ 响应中断\n"}).to_string()
                                )));
                            }
                            break;
                        }
                        // 取两者中较短的等待时间
                        let to_deadline = if now < total_deadline { total_deadline.duration_since(now) } else { std::time::Duration::ZERO };
                        let max_wait = std::cmp::min(to_deadline, no_content_timeout.saturating_sub(idle));
                        let chunk = tokio::time::timeout(max_wait, stream.next()).await;
                        let chunk_result = match chunk {
                            Ok(Some(c)) => c,
                            Ok(None) => break, // 流正常结束
                            Err(_) => {
                                // 剩余时间耗尽（等价于总超时）
                                if acc.can_degrade() {
                                    let _ = tx.send(Ok(Event::default().data(
                                        serde_json::json!({"type": "chunk", "content": "\n⚠️ 响应超时\n"}).to_string()
                                    )));
                                }
                                break;
                            }
                        };
                        match acc.ingest(chunk_result) {
                            Ok(()) => {
                                let new_content = &acc.content[round_text.len()..];
                                let new_reasoning = &acc.reasoning[round_reasoning.len()..];
                                // 思考/回答分离仅对 Pro+thinking 生效
                                // Flash（thinking=false）的输出也在 reasoning_content，必须作为 chunk 推送
                                if use_thinking && !new_reasoning.is_empty() {
                                    round_reasoning = acc.reasoning.clone();
                                    last_content = tokio::time::Instant::now();
                                    let _ = tx.send(Ok(Event::default().data(
                                        serde_json::json!({"type": "thinking", "content": new_reasoning}).to_string()
                                    )));
                                }
                                if !new_content.is_empty() {
                                    round_text = acc.content.clone();
                                    last_content = tokio::time::Instant::now();
                                    let _ = tx.send(Ok(Event::default().data(
                                        serde_json::json!({"type": "chunk", "content": new_content}).to_string()
                                    )));
                                }
                                // 非 thinking 模式下，reasoning_content 就是实际输出 → 作为 chunk 推送
                                if !use_thinking && !new_reasoning.is_empty() {
                                    round_reasoning = acc.reasoning.clone();
                                    last_content = tokio::time::Instant::now();
                                    let _ = tx.send(Ok(Event::default().data(
                                        serde_json::json!({"type": "chunk", "content": new_reasoning}).to_string()
                                    )));
                                }
                                if !acc.tool_calls.is_empty() {
                                    last_content = tokio::time::Instant::now(); // 工具调用也算进度
                                    last_chunk_tool_calls = acc.tool_calls.clone();
                                }
                                if acc.is_done() { break; }
                            }
                            Err(e) => {
                                state_clone.error_logger.log("stream", "error", &e.to_string(), &format!("round={}", tool_round));
                                let _ = tx.send(Ok(Event::default().data(
                                    serde_json::json!({"type": "error", "message": e.to_string()}).to_string()
                                )));
                                if acc.can_degrade() {
                                    let _ = tx.send(Ok(Event::default().data(
                                        serde_json::json!({"type": "chunk", "content": "\n⚠️ 响应中断，基于已接收内容继续\n"}).to_string()
                                    )));
                                    break;
                                }
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let is_network_err = e.to_string().contains("error sending request")
                        || e.to_string().contains("connection")
                        || e.to_string().contains("timeout")
                        || e.to_string().contains("dns");
                    // 网络瞬时故障：重试一次
                    if is_network_err && tool_round == 0 {
                        let retry_msg = format!("网络抖动，1秒后重试…");
                        let _ = tx.send(Ok(Event::default().data(
                            serde_json::json!({"type": "chunk", "content": retry_msg}).to_string()
                        )));
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    let error_msg = format!("API 调用失败: {}", e);
                    state_clone.error_logger.log("api", "error", &error_msg, &format!("model={} tokens={}", decision.model, max_out_tokens));
                    let _ = tx.send(Ok(Event::default().data(
                        serde_json::json!({"type": "error", "message": &error_msg}).to_string()
                    )));
                    {
                        let mut evo = state_clone.evolution.lock().await;
                        evo.record_turn(&req.message, &error_msg, false);
                    }
                    return;
                }
            }
            // stream 已 drop，client 不再被借用

            // L3: Pro 超时自动降级 Flash——保证 Complex 任务不卡死
            let pro_got_nothing = round_text.is_empty() && last_chunk_tool_calls.is_empty();
            let is_pro = config.ai.default_model.contains("pro");
            let already_fallback = config.ai.default_model.contains("flash") && !use_thinking;
            if pro_got_nothing && is_pro && !already_fallback && tool_round == 0 {
                tracing::warn!("[chat] Pro无响应，降级Flash重试");
                config.ai.default_model = config.ai.flash_model.clone();
                use_thinking = false;
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "chunk", "content": "\n⚡ Pro 响应超时，自动切换 Flash 重试…\n"}).to_string()
                )));
                continue;
            }

            // 解析工具调用：优先原生 tool_calls，文本 [TOOL:xxx] 兜底
            let was_native = !last_chunk_tool_calls.is_empty();
            let native_tool_calls: Vec<(String, String)> = last_chunk_tool_calls.iter()
                .filter(|tc| !tc.name.is_empty())
                .map(|tc| {
                    let text_arg = convert_native_args(&tc.name, &tc.arguments);
                    (tc.name.clone(), text_arg)
                })
                .collect();

            let tool_calls: Vec<(String, String)> = if !native_tool_calls.is_empty() {
                // 原生 function calling 结果
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "chunk", "content": format!("\n🤖 原生调用: {}\n", native_tool_calls.iter().map(|(n,_)| n.clone()).collect::<Vec<_>>().join(", "))}).to_string()
                )));
                native_tool_calls
            } else {
                // 文本 [TOOL:xxx] 兜底
                round_text
                    .lines()
                    .filter_map(|line| {
                        let line = line.trim();
                        if line.starts_with("[TOOL:") {
                            let inner = line.trim_start_matches("[TOOL:").trim_end_matches(']');
                            let parts: Vec<&str> = inner.splitn(2, ':').collect();
                            Some((parts[0].to_string(), parts.get(1).map(|s| s.to_string()).unwrap_or_default()))
                        } else { None }
                    }).collect()
            };

            if tool_calls.is_empty() || tool_round >= max_tool_rounds {
                break; // 无工具调用或达到最大轮次
            }

            // 发送结构化工具开始事件（前端渲染为工具卡片）
            let tc_for_event: Vec<serde_json::Value> = tool_calls.iter().map(|(t, a)| {
                serde_json::json!({"tool": t, "arg": a})
            }).collect();
            let _ = tx.send(Ok(Event::default().data(
                serde_json::json!({"type": "tool_start", "tools": tc_for_event, "round": tool_round + 1}).to_string()
            )));
            let _ = tx.send(Ok(Event::default().data(
                serde_json::json!({"type": "chunk", "content": format!("\n\n🔧 执行 {} 个工具…\n", tool_calls.len())}).to_string()
            )));

            // 执行工具：只读工具并行，写工具串行
            let read_only_tools: Vec<_> = tool_calls.iter()
                .filter(|(t, _)| matches!(t.as_str(), "read" | "search" | "web" | "lsp" | "lsp-rich" | "snap" | "glob" | "semantic"))
                .collect();
            let write_tools: Vec<_> = tool_calls.iter()
                .filter(|(t, _)| matches!(t.as_str(), "edit" | "write" | "exec" | "save" | "rollback" | "auto-fix"))
                .collect();

            // 收集每个工具的执行结果（单条记录，便于原生格式回注）
            let mut tool_results: Vec<(String, String)> = Vec::new(); // (tool, result_text)

            // 并行执行只读工具
            let ro_futures: Vec<_> = read_only_tools.iter().map(|(tool, arg)| {
                let t = tool.to_string();
                let a = arg.to_string();
                tokio::spawn(async move { (t.clone(), a.clone(), execute_tool_inline(&t, &a).await) })
            }).collect();

            for f in ro_futures {
                if let Ok((tool, arg, mut result)) = f.await {
                    let full_len = result.len();
                    if result.len() > 3000 {
                        result = format!("{}…\n[结果已截断，原始长度 {} 字符]", &result[..3000], full_len);
                    }
                    let summary: String = result.chars().take(100).collect();
                    let summary = if result.chars().count() > 100 { format!("{}…", summary) } else { result.clone() };
                    let _ = tx.send(Ok(Event::default().data(
                        serde_json::json!({"type": "tool_result", "tool": tool, "arg": arg, "success": true, "summary": summary}).to_string()
                    )));
                    let _ = tx.send(Ok(Event::default().data(
                        serde_json::json!({"type": "chunk", "content": format!("  ✓ {} — {}\n", tool, if result.len() > 60 { format!("{}…", result.chars().take(60).collect::<String>()) } else { result.clone() })}).to_string()
                    )));
                    tool_results.push((tool.clone(), result));
                }
            }

            // 串行执行写工具（保证安全顺序）
            for (tool, arg) in &write_tools {
                let mut result = execute_tool_inline(tool, arg).await;
                let full_len = result.len();
                if result.len() > 3000 {
                    result = format!("{}…\n[结果已截断，原始长度 {} 字符]", &result[..3000], full_len);
                }
                let summary: String = result.chars().take(100).collect();
                let summary = if result.chars().count() > 100 { format!("{}…", summary) } else { result.clone() };
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "tool_result", "tool": tool, "arg": arg, "success": true, "summary": summary}).to_string()
                )));
                let _ = tx.send(Ok(Event::default().data(
                    serde_json::json!({"type": "chunk", "content": format!("  ✓ {} — {}\n", tool, if result.len() > 60 { format!("{}…", result.chars().take(60).collect::<String>()) } else { result.clone() })}).to_string()
                )));
                tool_results.push(((*tool).clone(), result));
            }

            // L3: 直接追加到现有对话，不重建避免丢失 tool/assistant 消息
            // 添加当前轮 assistant（含 tool_calls 和 reasoning）
            let assistant_msg = {
                let mut msg = crate::engine::inference::ChatMessage {
                    role: "assistant".into(),
                    content: round_text.clone(),
                    reasoning_content: round_reasoning.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                };
                if !last_chunk_tool_calls.is_empty() {
                    msg.tool_calls = Some(last_chunk_tool_calls.iter().map(|tc| {
                        crate::engine::inference::ToolCallDelta {
                            id: if tc.id.is_empty() { None } else { Some(tc.id.clone()) },
                            call_type: Some("function".into()),
                            function: Some(crate::engine::inference::ToolCallFunc {
                                name: if tc.name.is_empty() { None } else { Some(tc.name.clone()) },
                                arguments: Some(tc.arguments.clone()),
                            }),
                            index: None,
                        }
                    }).collect());
                }
                msg
            };
            conversation.push(assistant_msg);

            // 添加工具结果
            for (tool_name, result_text) in &tool_results {
                let call_id = last_chunk_tool_calls.iter()
                    .find(|tc| tc.name == *tool_name)
                    .and_then(|tc| if tc.id.is_empty() { None } else { Some(tc.id.clone()) })
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| format!("call_{}", tool_name));
                conversation.push(crate::engine::inference::ChatMessage::tool_result(&call_id, result_text));
            }

            tool_round += 1;
        }

        // 保存本轮到对话历史（内存 + 磁盘）
        {
            let mut hist = state_clone.conversation_history.lock().await;
            hist.push(crate::engine::inference::ChatMessage::user(&req.message));
            // 取最后一轮 assistant 回复（不含工具调用的消息）
            for msg in conversation.iter().rev() {
                if msg.role == "assistant"
                    && !msg.content.contains("[TOOL:")
                    && msg.tool_calls.is_none() {
                    hist.push(msg.clone());
                    break;
                }
            }
            // 保留最近 8 条
            let n = hist.len();
            if n > 8 { *hist = hist.split_off(n - 8); }

            // 落盘：每次对话自动保存，重启可恢复
            let dir = crate::config::forge_data_dir().join("sessions");
            std::fs::create_dir_all(&dir).ok();
            let msg_list: Vec<serde_json::Value> = hist.iter().map(|m| {
                serde_json::json!({"role": m.role, "content": m.content})
            }).collect();
            let session_data = serde_json::json!({
                "date": chrono::Utc::now().format("%m-%d %H:%M").to_string(),
                "turn": *state_clone.session_turn.lock().await,
                "messages": msg_list,
            });
            let _ = std::fs::write(dir.join("latest.json"),
                serde_json::to_string_pretty(&session_data).unwrap_or_default());
        }

        // 发送完成
        let _ = tx.send(Ok(Event::default().data(
            serde_json::json!({"type": "done", "finish_reason": "stop"}).to_string()
        )));

        // 记录经验 + 匹配 SOP
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

        // 记录提示词优化数据
        {
            let cache = state_clone.cache_stats.lock().await;
            let complexity = decision.complexity.name().to_string();
            state_clone.prompt_optimizer.lock().await.record(
                &system_variant, has_content, cache.total_tokens, cache.cache_hit_tokens, &complexity
            );
        }

        if !has_content {
            let err = "API 返回了空内容，请检查 Key 或额度";
            state_clone.error_logger.log("api", "error", err, &format!("model={}", decision.model));
            let _ = tx.send(Ok(Event::default().data(
                serde_json::json!({"type": "error", "message": err}).to_string()
            )));
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

    let turn = *state.session_turn.lock().await;
    let cache = state.cache_stats.lock().await;
    let model = state.config.lock().await.ai.default_model.clone();

    Json(StatusResponse {
        mode: "interactive".into(),
        cost,
        hit_rate,
        active_agents: active,
        max_agents,
        memory_mb: 15.0,
        has_key: state.has_api_key,
        turns: turn as u32,
        total_tokens: cache.total_tokens,
        model,
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
    let cache = state.cache_stats.lock().await.clone();
    let hit_rate = cache.cache_hit_rate.max(*state.cache_hit_rate.lock().await);
    let cache_saved = (cache.cache_hit_tokens as f64) * 0.000001; // ¥1/M tokens
    let vs_claude = if cost > 0.0 {
        let claude_estimated = cost * 22.0;
        ((claude_estimated - cost) / claude_estimated * 100.0).min(99.0)
    } else { 0.0 };

    Json(CostResponse {
        total_cost: cost,
        cache_hit_rate: hit_rate,
        cache_saved,
        monthly_used: cost,
        monthly_budget: 100.0,
        vs_claude_savings_pct: vs_claude,
        cache_hit_tokens: cache.cache_hit_tokens,
        cache_miss_tokens: cache.cache_miss_tokens,
        prompt_tokens: cache.prompt_tokens,
        completion_tokens: cache.completion_tokens,
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

    let files: Vec<FileItem> = stats.recent_files.iter().take(10).map(|f| FileItem {
        name: f.name.clone(),
        lines: f.lines,
    }).collect();

    Json(ProjectResponse {
        ok: true,
        name: stats.project_name,
        file_count: stats.file_count,
        total_lines: stats.total_lines,
        rust_files: stats.rust_files,
        test_files: stats.test_files,
        files,
        recent_commits: commits,
    })
}
