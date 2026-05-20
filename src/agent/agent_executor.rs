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

    /// 执行一个 Agent 任务（带工具循环）
    pub async fn run(&self, task_desc: &str, system_prompt: &str) -> Result<AgentResult, ForgeError> {
        let mut client = InferenceClient::new(&self.config)?;
        let mut tools_used = Vec::new();
        let mut conversation = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(&format!(
                "执行以下子任务。你可以使用工具:\n\
                 [TOOL:read:文件路径] 或 [TOOL:read:文件:起始行:结束行] - 读取文件\n\
                 [TOOL:search:关键字] - 全项目搜索\n\
                 [TOOL:exec:命令] - 执行白名单命令(cargo/git)\n\
                 [TOOL:infer:函数名] - 分析函数签名和调用链\n\
                 \n\
                 每次回复可以包含工具调用，工具返回后你会收到结果，然后可以继续或给出最终答案。\n\
                 子任务: {}", task_desc
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
            _ => ToolResult { tool: tool.into(), arg: arg.into(), output: format!("未知工具: {}", tool), success: false },
        }
    }
}
