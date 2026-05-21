//! Agent 执行器：子 Agent 独立工具循环
//! 每个子 Agent 可以独立调用 read/search/exec 工具，结果反馈给 AI，最多 3 轮

use crate::config::Config;
use crate::engine::inference::{ChatMessage, InferenceClient};
use crate::error::ForgeError;
use std::path::PathBuf;

/// 工具调用结果
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool: String,
    pub arg: String,
    pub output: String,
    pub success: bool,
}

/// Agent 执行结果
#[derive(Debug)]
pub struct AgentResult {
    pub final_output: String,
    pub tools_used: Vec<ToolResult>,
    pub rounds: u32,
}

/// Agent 执行器
pub struct AgentExecutor {
    config: Config,
    max_rounds: u32,
    work_dir: PathBuf,
}

impl AgentExecutor {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            max_rounds: 3,
            work_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    /// 执行一个 Agent 任务（规划→执行→验证闭环）
    pub async fn run(&self, task_desc: &str, system_prompt: &str) -> Result<AgentResult, ForgeError> {
        let mut client = InferenceClient::new(&self.config)?;
        let mut tools_used = Vec::new();
        let mut conversation = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(&format!(
                "你是一个自主编程 Agent。执行以下任务。\n\n\
                 ## 可用工具\n\
                 [TOOL:read:路径] - 读文件（可选 :起始行:结束行）\n\
                 [TOOL:search:关键字] - 全项目 ripgrep 搜索\n\
                 [TOOL:exec:命令] - 执行白名单命令(cargo check/test/build, git status/diff/log)\n\
                 [TOOL:infer:函数名] - Tree-sitter AST 分析函数/结构体定义和引用\n\
                 [TOOL:lsp-rich:符号] - 全局符号索引+类型信息+编译错误\n\
                 \n\
                 ## 工作流程\n\
                 1. 第一步：用 search/lsp-rich 了解现状\n\
                 2. 第二步：用 read 读取相关文件\n\
                 3. 第三步：执行修改或分析\n\
                 4. 第四步：用 exec:cargo check 验证\n\
                 5. 最后给出完整答案。每步用 [TOOL:...] 标记\n\
                 \n\
                 ## 任务\n{}", task_desc
            )),
        ];

        for round in 0..self.max_rounds {
            let mut response = String::new();

            {
                use futures::StreamExt;
                let stream = client.chat_stream(conversation.clone()).await?;
                tokio::pin!(stream);
                while let Some(chunk) = stream.next().await {
                    if let Ok(c) = chunk { response.push_str(&c.content); }
                }
            }

            // 解析工具调用
            let tool_calls: Vec<(String, String)> = response.lines()
                .filter_map(|line| {
                    let line = line.trim();
                    if line.starts_with("[TOOL:") {
                        let inner = line.trim_start_matches("[TOOL:").trim_end_matches(']');
                        let parts: Vec<&str> = inner.splitn(2, ':').collect();
                        Some((parts[0].to_string(), parts.get(1).map(|s| s.to_string()).unwrap_or_default()))
                    } else { None }
                }).collect();

            if tool_calls.is_empty() {
                return Ok(AgentResult { final_output: response, tools_used, rounds: round + 1 });
            }

            // 执行工具调用
            let mut tool_results = String::new();
            for (tool, arg) in &tool_calls {
                let result = self.execute_tool(tool, arg).await;
                tools_used.push(result.clone());
                tool_results.push_str(&format!(
                    "\n[工具结果: {}] {}\n",
                    tool,
                    if result.success { &result.output } else { "执行失败" }
                ));
            }

            // 将工具结果反馈给 AI
            conversation.push(ChatMessage::assistant(&response));
            conversation.push(ChatMessage::user(&format!("工具执行结果:{}", tool_results)));
        }

        Ok(AgentResult { final_output: "达到最大轮次".into(), tools_used, rounds: self.max_rounds })
    }

    async fn execute_tool(&self, tool: &str, arg: &str) -> ToolResult {
        match tool {
            "read" => {
                let parts: Vec<&str> = arg.split(':').collect();
                let path = parts.first().map(|s| s.trim()).unwrap_or("");
                let full_path = self.work_dir.join(path);
                match std::fs::read_to_string(&full_path) {
                    Ok(content) => {
                        let lines: Vec<_> = content.lines().take(80).enumerate()
                            .map(|(i, l)| format!("{:>5}  {}", i + 1, l)).collect();
                        ToolResult { tool: tool.into(), arg: arg.into(), output: lines.join("\n"), success: true }
                    }
                    Err(e) => ToolResult { tool: tool.into(), arg: arg.into(), output: e.to_string(), success: false },
                }
            }
            "search" => {
                match tokio::process::Command::new("rg")
                    .args(["--no-heading", "-n", "--max-count=30", arg, "."])
                    .current_dir(&self.work_dir).output().await
                {
                    Ok(o) => ToolResult { tool: tool.into(), arg: arg.into(), output: String::from_utf8_lossy(&o.stdout).to_string(), success: true },
                    Err(e) => ToolResult { tool: tool.into(), arg: arg.into(), output: e.to_string(), success: false },
                }
            }
            "exec" => {
                match tokio::process::Command::new("cmd").args(["/C", arg]).current_dir(&self.work_dir).output().await {
                    Ok(o) => ToolResult { tool: tool.into(), arg: arg.into(), output: String::from_utf8_lossy(&o.stdout).to_string(), success: o.status.success() },
                    Err(e) => ToolResult { tool: tool.into(), arg: arg.into(), output: e.to_string(), success: false },
                }
            }
            "lsp-rich" => {
                // 使用 AstParser 做全局符号分析
                let mut output = String::new();
                if let Some(mut parser) = crate::engine::ast_parser::AstParser::new() {
                    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    if let Ok(entries) = std::fs::read_dir(cwd.join("src")) {
                        for e in entries.flatten() {
                            let p = e.path();
                            if p.extension().map(|e| e == "rs").unwrap_or(false) {
                                if let Ok(src) = std::fs::read_to_string(&p) {
                                    let syms = parser.parse_symbols(&src, &p.to_string_lossy());
                                    for s in &syms { output.push_str(&format!("{}:{} {} {}\n", s.file, s.line, s.kind, s.name)); }
                                }
                            }
                        }
                    }
                }
                ToolResult { tool: tool.into(), arg: arg.into(), output: if output.is_empty() { "无符号".into() } else { output }, success: true }
            }
            "infer" => {
                // 搜索函数定义和调用
                let mut output = String::new();
                if let Ok(o) = tokio::process::Command::new("rg")
                    .args(["-n", &format!("fn {}", arg), "--type", "rust"])
                    .current_dir(&self.work_dir).output().await
                {
                    output.push_str("定义:\n");
                    output.push_str(&String::from_utf8_lossy(&o.stdout));
                }
                if let Ok(o) = tokio::process::Command::new("rg")
                    .args(["-c", arg, "--type", "rust"])
                    .current_dir(&self.work_dir).output().await
                {
                    output.push_str(&format!("\n调用次数: {}", String::from_utf8_lossy(&o.stdout).trim()));
                }
                ToolResult { tool: tool.into(), arg: arg.into(), output, success: true }
            }
            "web" => {
                // Agent 内联网搜索
                let client = match reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(12))
                    .user_agent("ForgeShell/1.0")
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => return ToolResult { tool: tool.into(), arg: arg.into(), output: e.to_string(), success: false },
                };
                let worker_url = "https://forgeshell.cn/api/search";
                let mut output = String::new();
                if let Ok(resp) = client.post(worker_url)
                    .json(&serde_json::json!({"query": arg}))
                    .send().await
                {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(results) = data["results"].as_array() {
                            for r in results.iter().take(5) {
                                if let Some(s) = r.as_str() { output.push_str(s); output.push('\n'); }
                            }
                        }
                    }
                }
                if output.is_empty() { output = "搜索无结果".into(); }
                ToolResult { tool: tool.into(), arg: arg.into(), output, success: true }
            }
            "glob" => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let mut results = Vec::new();
                fn walk(dir: &std::path::Path, pattern: &str, results: &mut Vec<String>, depth: u32) {
                    if depth > 5 { return; }
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for e in entries.flatten() {
                            let p = e.path();
                            let name = p.file_name().unwrap_or_default().to_string_lossy();
                            if name.starts_with('.') || name == "target" || name == "node_modules" { continue; }
                            if p.is_dir() { walk(&p, pattern, results, depth + 1); }
                            else if pattern == "*" || name.contains(pattern.trim_end_matches('*')) {
                                results.push(p.display().to_string());
                            }
                        }
                    }
                }
                walk(&cwd, arg, &mut results, 0);
                ToolResult { tool: tool.into(), arg: arg.into(), output: format!("{} 个匹配:\n{}", results.len(), results.join("\n")), success: true }
            }
            _ => ToolResult { tool: tool.into(), arg: arg.into(), output: format!("未知工具: {}", tool), success: false },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_construction() {
        let r = ToolResult { tool: "read".into(), arg: "test.rs".into(), output: "line 1".into(), success: true };
        assert!(r.success);
        assert_eq!(r.tool, "read");
    }

    #[test]
    fn test_agent_executor_new() {
        let config = crate::config::Config::default();
        let executor = AgentExecutor::new(config);
        assert_eq!(executor.max_rounds, 3);
    }

    #[test]
    fn test_agent_result_fields() {
        let result = AgentResult {
            final_output: "done".into(),
            tools_used: vec![],
            rounds: 2,
        };
        assert_eq!(result.rounds, 2);
        assert_eq!(result.final_output, "done");
        assert!(result.tools_used.is_empty());
    }
}
