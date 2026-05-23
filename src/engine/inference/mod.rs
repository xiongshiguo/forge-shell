//! AI 推理客户端：流式 API 调用 + Token 计数

use crate::config::Config;
use crate::error::ForgeError;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

/// Token 用量统计（含缓存命中）
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    /// Prefix Cache 命中 token 数（免费）
    pub cache_hit_tokens: u64,
    /// 未命中缓存的 token 数（收费）
    pub cache_miss_tokens: u64,
    /// 命中率
    pub cache_hit_rate: f64,
}

/// SSE 流式 chunk（含原生 tool_calls + reasoning）
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: String,
    pub finish_reason: Option<String>,
    /// 累积的 tool_calls（从 SSE delta 拼接）
    pub tool_calls: Vec<AccumulatedToolCall>,
    /// DeepSeek V4 thinking 推理内容（必须回传）
    pub reasoning_content: String,
}

#[derive(Debug, Clone)]
pub struct AccumulatedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 推理客户端
pub struct InferenceClient {
    api_key: String,
    api_base: String,
    model: String,
    http: reqwest::Client,
    pub total_usage: TokenUsage,
    max_tokens: u32,
    thinking_enabled: bool,
    /// 原生 tool_calls 累积器（按 index 分组）
    tool_acc: std::collections::HashMap<u32, AccumulatedToolCall>,
    /// 工具定义列表（发送给 API）
    tools: Option<Vec<ToolDef>>,
}

impl InferenceClient {
    pub fn new(config: &Config) -> Result<Self, ForgeError> {
        let api_key = config.effective_api_key();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.ai.timeout_secs))
            .build()?;

        Ok(Self {
            api_key,
            api_base: config.ai.api_base.clone(),
            model: config.ai.default_model.clone(),
            http,
            total_usage: TokenUsage::default(),
            max_tokens: 8192,
            thinking_enabled: false,
            tool_acc: std::collections::HashMap::new(),
            tools: None,
        })
    }

    /// 设置最大输出 token 数
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }

    /// 启用深度思考（DeepSeek V4 思维链）
    pub fn with_thinking(mut self, enabled: bool) -> Self {
        self.thinking_enabled = enabled;
        self
    }

    /// 设置原生 function calling 工具定义
    pub fn with_tools(mut self, tools: Vec<ToolDef>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// 流式聊天（返回 SSE stream）
    pub async fn chat_stream(
        &mut self,
        messages: Vec<ChatMessage>,
    ) -> Result<impl Stream<Item = Result<StreamChunk, ForgeError>>, ForgeError> {
        self.tool_acc.clear();
        let is_ollama = self.model == "ollama";
        let tool_choice = if !is_ollama && self.tools.is_some() { Some("auto".to_string()) } else { None };
        // 消息格式校验+自动修复（防止之前反复出现的 400 错误）
        let messages = validate_messages(messages, self.thinking_enabled);
        let body = ChatRequest {
            model: if is_ollama { "deepseek-r1:latest".into() } else { self.model.clone() },
            messages,
            stream: true,
            temperature: if self.thinking_enabled { None } else { Some(0.0) },
            max_tokens: self.max_tokens,
            thinking: if self.thinking_enabled && !is_ollama {
                Some(ThinkingConfig { thinking_type: "enabled".into() })
            } else { None },
            tools: if is_ollama { None } else { self.tools.clone() },
            tool_choice,
        };

        let is_ollama = self.model == "ollama";
        let url = if is_ollama {
            "http://localhost:11434/v1/chat/completions".to_string()
        } else {
            format!("{}/v1/chat/completions", self.api_base.trim_end_matches('/'))
        };
        let mut req = self.http.post(&url);
        if !is_ollama {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }
        let resp = req.json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            let friendly = match status.as_u16() {
                401 => "❌ API Key 无效，请检查配置".to_string(),
                402 => "💰 API 额度不足，请登录 DeepSeek 平台充值".to_string(),
                403 => "🚫 API 访问被拒，请检查 Key 权限".to_string(),
                429 => "⏳ 请求过于频繁，请稍后再试".to_string(),
                500..=599 => "🔧 DeepSeek 服务暂时不可用，请稍后重试".to_string(),
                _ => format!("API 错误 ({}): {}", status, text.chars().take(200).collect::<String>()),
            };
            return Err(ForgeError::Api(friendly));
        }

        use futures::StreamExt;
        let stream = resp
            .bytes_stream()
            .map(|result| {
                let bytes = match result {
                    Ok(b) => b,
                    Err(e) => {
                        // 区分流错误类型，给出有用信息
                        let detail = if e.is_timeout() { "连接超时"
                        } else if e.is_connect() { "无法连接"
                        } else if e.is_body() { "响应体解码失败（可能对话过大导致服务端截断）"
                        } else { "网络错误" };
                        return Err(ForgeError::Api(format!("{}: {}", detail, e)));
                    }
                };
                let text = String::from_utf8_lossy(&bytes).to_string();
                self.parse_sse_line(&text)
            });

        Ok(stream)
    }

    /// 解析 SSE 数据行（含缓存命中统计 + 原生 tool_calls + reasoning_content）
    fn parse_sse_line(&mut self, text: &str) -> Result<StreamChunk, ForgeError> {
        let mut content = String::new();
        let mut finish_reason = None;
        let mut reasoning = String::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    finish_reason = Some("stop".into());
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(choices) = parsed["choices"].as_array() {
                        for choice in choices {
                            if let Some(delta) = choice["delta"]["content"].as_str() {
                                content.push_str(delta);
                            }
                            // 捕获 reasoning_content（DeepSeek V4 thinking 模式，必须回传）
                            if let Some(rc) = choice["delta"]["reasoning_content"].as_str() {
                                reasoning.push_str(rc);
                            }
                            // 捕获原生 tool_calls
                            if let Some(tc_array) = choice["delta"]["tool_calls"].as_array() {
                                for tc in tc_array {
                                    let idx = tc["index"].as_u64().unwrap_or(0) as u32;
                                    let entry = self.tool_acc.entry(idx).or_insert_with(|| AccumulatedToolCall {
                                        id: String::new(), name: String::new(), arguments: String::new(),
                                    });
                                    if let Some(id) = tc["id"].as_str() { entry.id = id.to_string(); }
                                    if let Some(name) = tc["function"]["name"].as_str() { entry.name = name.to_string(); }
                                    if let Some(args) = tc["function"]["arguments"].as_str() { entry.arguments.push_str(args); }
                                }
                            }
                            if let Some(reason) = choice["finish_reason"].as_str() {
                                finish_reason = Some(reason.to_string());
                            }
                        }
                    }
                    // 捕获 usage 数据（含 prefix cache 命中）
                    if let Some(usage) = parsed["usage"].as_object() {
                        let pt = usage.get("prompt_tokens").and_then(|v| v.as_u64());
                        let ct = usage.get("completion_tokens").and_then(|v| v.as_u64());
                        let tt = usage.get("total_tokens").and_then(|v| v.as_u64());
                        let cache = usage.get("prompt_cache_hit_tokens").and_then(|v| v.as_u64());
                        let miss = usage.get("prompt_cache_miss_tokens").and_then(|v| v.as_u64());
                        if let (Some(pt), Some(ct)) = (pt, ct) {
                            self.total_usage.prompt_tokens += pt;
                            self.total_usage.completion_tokens += ct;
                            self.total_usage.total_tokens += tt.unwrap_or(pt + ct);
                            if let Some(c) = cache { self.total_usage.cache_hit_tokens += c; }
                            if let Some(m) = miss { self.total_usage.cache_miss_tokens += m; }
                            let total = self.total_usage.cache_hit_tokens + self.total_usage.cache_miss_tokens;
                            if total > 0 {
                                self.total_usage.cache_hit_rate = self.total_usage.cache_hit_tokens as f64 / total as f64;
                            }
                        }
                    }
                }
            }
        }

        // 刷新累积的 tool_calls
        let tool_calls: Vec<AccumulatedToolCall> = if finish_reason.is_some() {
            let mut tcs: Vec<_> = self.tool_acc.drain().map(|(_, v)| v).collect();
            tcs.sort_by_key(|t| t.id.clone());
            tcs
        } else { Vec::new() };

        Ok(StreamChunk { content, finish_reason, tool_calls, reasoning_content: reasoning })
    }

    /// 获取累计 Token 用量
    pub fn total_usage(&self) -> &TokenUsage {
        &self.total_usage
    }

    /// 估算费用
    pub fn estimate_cost(&self, model: &str) -> f64 {
        let (in_price, out_price) = if model.contains("flash") {
            (0.1 / 1_000_000.0, 0.4 / 1_000_000.0)
        } else {
            (1.0 / 1_000_000.0, 4.0 / 1_000_000.0)
        };
        self.total_usage.prompt_tokens as f64 * in_price
            + self.total_usage.completion_tokens as f64 * out_price
    }
}

// ---- DeepSeek API 类型 ----

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Debug, Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    thinking_type: String,
}

// -- 原生函数调用（DeepSeek V4 OpenAI 兼容格式） --

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub tool_call_id: Option<String>,
    /// DeepSeek V4 thinking 模式的推理内容（必须回传）
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    pub function: Option<ToolCallFunc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunc {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

impl ChatMessage {
    pub fn system(content: &str) -> Self {
        Self { role: "system".into(), content: content.into(), tool_calls: None, tool_call_id: None, reasoning_content: None }
    }
    pub fn user(content: &str) -> Self {
        Self { role: "user".into(), content: content.into(), tool_calls: None, tool_call_id: None, reasoning_content: None }
    }
    pub fn assistant(content: &str) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls: None, tool_call_id: None, reasoning_content: None }
    }
    pub fn assistant_with_reasoning(content: &str, reasoning: &str) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls: None, tool_call_id: None, reasoning_content: if reasoning.is_empty() { None } else { Some(reasoning.into()) } }
    }
    pub fn tool_result(tool_call_id: &str, content: &str) -> Self {
        Self { role: "tool".into(), content: content.into(), tool_call_id: Some(tool_call_id.into()), tool_calls: None, reasoning_content: None }
    }
}

/// 消息格式校验+自动修复。在每次 API 调用前运行，防止：
/// 1. tool_calls 后缺 tool 消息 → 补齐空结果
/// 2. content 字段缺失 → 补空字符串
/// 3. thinking 关闭但有 reasoning_content → 清除
/// 4. tool_call_id 为空 → 生成占位 ID
fn validate_messages(mut msgs: Vec<ChatMessage>, thinking_enabled: bool) -> Vec<ChatMessage> {
    // 修复 3: thinking 关闭时清除所有 reasoning_content
    if !thinking_enabled {
        for m in &mut msgs {
            if m.role == "assistant" { m.reasoning_content = None; }
        }
    }

    // 修复 4: tool 消息的 tool_call_id 为空时补充
    for m in &mut msgs {
        if m.role == "tool" && m.tool_call_id.as_deref().unwrap_or("").is_empty() {
            m.tool_call_id = Some("call_fixed".into());
        }
    }

    // 修复 1: 检查每个 assistant+tool_calls 后是否有足够 tool 消息
    let mut fixed = Vec::new();
    let mut pending_tool_ids: Vec<String> = Vec::new();
    for m in msgs {
        let is_assistant_with_tools = m.role == "assistant" && m.tool_calls.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
        let is_tool = m.role == "tool";

        if is_assistant_with_tools {
            // 先把上一轮的 pending 补齐
            for id in pending_tool_ids.drain(..) {
                fixed.push(ChatMessage::tool_result(&id, "(auto-fixed)"));
            }
            // 收集本轮需要的 tool_call_ids
            if let Some(ref tc) = m.tool_calls {
                pending_tool_ids = tc.iter().filter_map(|t| t.id.clone()).collect();
            }
        } else if is_tool {
            if let Some(ref id) = m.tool_call_id {
                pending_tool_ids.retain(|x| x != id);
            }
        }
        fixed.push(m);
    }
    // 末尾补齐
    for id in pending_tool_ids.drain(..) {
        fixed.push(ChatMessage::tool_result(&id, "(auto-fixed)"));
    }

    // 修复 2: 确保所有消息有 content 字段
    for m in &mut fixed {
        if m.content.is_empty() && m.tool_calls.is_none() && m.role != "tool" {
            m.content = String::new(); // 空串但存在
        }
    }

    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    // === SSE 解析测试 ===

    #[test]
    fn test_parse_content_chunk() {
        let mut client = InferenceClient {
            api_key: String::new(), api_base: String::new(), model: String::new(),
            http: reqwest::Client::new(), total_usage: TokenUsage::default(),
            max_tokens: 8192, thinking_enabled: false,
            tool_acc: std::collections::HashMap::new(), tools: None,
        };
        let sse = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
        let chunk = client.parse_sse_line(sse).unwrap();
        assert_eq!(chunk.content, "Hello");
        assert!(chunk.reasoning_content.is_empty());
        assert!(chunk.tool_calls.is_empty());
    }

    #[test]
    fn test_parse_reasoning_content() {
        let mut client = InferenceClient {
            api_key: String::new(), api_base: String::new(), model: String::new(),
            http: reqwest::Client::new(), total_usage: TokenUsage::default(),
            max_tokens: 8192, thinking_enabled: true,
            tool_acc: std::collections::HashMap::new(), tools: None,
        };
        let sse = r#"data: {"choices":[{"delta":{"reasoning_content":"Let me think...","content":""}}]}"#;
        let chunk = client.parse_sse_line(sse).unwrap();
        assert_eq!(chunk.reasoning_content, "Let me think...");
    }

    #[test]
    fn test_parse_tool_call_delta() {
        let mut client = InferenceClient {
            api_key: String::new(), api_base: String::new(), model: String::new(),
            http: reqwest::Client::new(), total_usage: TokenUsage::default(),
            max_tokens: 8192, thinking_enabled: false,
            tool_acc: std::collections::HashMap::new(), tools: None,
        };
        // Tool call accumulates in tool_acc, only flushed on finish_reason
        let sse = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read","arguments":"{\"path\":\"src/main.rs\"}"}}]},"finish_reason":"tool_calls"}]}"#;
        let chunk = client.parse_sse_line(sse).unwrap();
        assert_eq!(chunk.tool_calls.len(), 1);
        let tc = &chunk.tool_calls[0];
        assert_eq!(tc.name, "read");
        assert_eq!(tc.id, "call_1");
        assert_eq!(tc.arguments, "{\"path\":\"src/main.rs\"}");
    }

    #[test]
    fn test_parse_tool_call_streaming_args() {
        let mut client = InferenceClient {
            api_key: String::new(), api_base: String::new(), model: String::new(),
            http: reqwest::Client::new(), total_usage: TokenUsage::default(),
            max_tokens: 8192, thinking_enabled: false,
            tool_acc: std::collections::HashMap::new(), tools: None,
        };
        // First chunk: id and name
        client.parse_sse_line(r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"write","arguments":""}}]}}]}"#).ok();
        // Second chunk: partial arguments
        client.parse_sse_line(r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":\"f"}}]}}]}"#).ok();
        // Third chunk: remaining arguments
        client.parse_sse_line(r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"oo.rs\",\"content\":\"fn main(){}\"}"}}]}}]}"#).ok();
        // Final chunk with finish_reason to flush
        let chunk = client.parse_sse_line(r#"data: {"choices":[{"finish_reason":"tool_calls"}]}"#).unwrap();
        assert!(!chunk.tool_calls.is_empty(), "Tool calls should be flushed on finish");
        let tc = &chunk.tool_calls[0];
        assert_eq!(tc.name, "write");
        assert_eq!(tc.id, "call_x");
        assert!(tc.arguments.contains("foo.rs"), "Arguments should accumulate across chunks: {}", tc.arguments);
    }

    #[test]
    fn test_parse_usage_stats() {
        let mut client = InferenceClient {
            api_key: String::new(), api_base: String::new(), model: String::new(),
            http: reqwest::Client::new(), total_usage: TokenUsage::default(),
            max_tokens: 8192, thinking_enabled: false,
            tool_acc: std::collections::HashMap::new(), tools: None,
        };
        let sse = r#"data: {"choices":[{"delta":{"content":"ok"}}],"usage":{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150,"prompt_cache_hit_tokens":80,"prompt_cache_miss_tokens":20}}"#;
        client.parse_sse_line(sse).ok();
        assert_eq!(client.total_usage.prompt_tokens, 100);
        assert_eq!(client.total_usage.cache_hit_tokens, 80);
    }

    #[test]
    fn test_parse_done_signal() {
        let mut client = InferenceClient {
            api_key: String::new(), api_base: String::new(), model: String::new(),
            http: reqwest::Client::new(), total_usage: TokenUsage::default(),
            max_tokens: 8192, thinking_enabled: false,
            tool_acc: std::collections::HashMap::new(), tools: None,
        };
        let chunk = client.parse_sse_line("data: [DONE]").unwrap();
        assert_eq!(chunk.finish_reason, Some("stop".into()));
    }

    // === ChatMessage 序列化测试 ===

    #[test]
    fn test_message_always_has_content_field() {
        // Bug v0.16.5: tool_calls 消息的 content 不能缺失
        let msg = ChatMessage {
            role: "assistant".into(), content: String::new(),
            tool_calls: Some(vec![ToolCallDelta {
                id: Some("call_1".into()), call_type: Some("function".into()),
                function: Some(ToolCallFunc { name: Some("read".into()), arguments: Some("{}".into()) }),
                index: None,
            }]),
            tool_call_id: None, reasoning_content: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"content\""), "content must always be in JSON: {}", json);
        assert!(json.contains("tool_calls"), "tool_calls must be present: {}", json);
    }

    #[test]
    fn test_tool_result_has_tool_call_id() {
        let msg = ChatMessage::tool_result("call_abc", "result text");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("tool_call_id"));
        assert!(json.contains("call_abc"));
    }

    #[test]
    fn test_assistant_reasoning_preserved() {
        let msg = ChatMessage::assistant_with_reasoning("answer", "I need to think...");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("reasoning_content"));
        assert!(json.contains("I need to think..."));
    }

    #[test]
    fn test_tool_def_json_schema() {
        let def = ToolDef {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "read".into(),
                description: "Read a file".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            },
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("function"));
        assert!(json.contains("read"));
        assert!(json.contains("parameters"));
    }

    #[test]
    fn test_chat_request_includes_tools_when_set() {
        let tools = vec![ToolDef {
            tool_type: "function".into(),
            function: ToolFunction {
                name: "search".into(), description: "search".into(),
                parameters: serde_json::json!({"type":"object","properties":{}}),
            },
        }];
        let req = ChatRequest {
            model: "deepseek-v4-pro".into(),
            messages: vec![ChatMessage::user("hello")],
            stream: true,
            temperature: None,
            max_tokens: 8192,
            thinking: None,
            tools: Some(tools),
            tool_choice: Some("auto".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"tools\""));
        assert!(json.contains("\"tool_choice\""));
        assert!(json.contains("\"auto\""));
    }
}
