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

/// SSE 流式 chunk（含原生 tool_calls）
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: String,
    pub finish_reason: Option<String>,
    /// 累积的 tool_calls（从 SSE delta 拼接）
    pub tool_calls: Vec<AccumulatedToolCall>,
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
        let tool_choice = if self.tools.is_some() { Some("auto".to_string()) } else { None };
        let body = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            temperature: if self.thinking_enabled { None } else { Some(0.0) },
            max_tokens: self.max_tokens,
            thinking: if self.thinking_enabled {
                Some(ThinkingConfig { thinking_type: "enabled".into() })
            } else { None },
            tools: self.tools.clone(),
            tool_choice,
        };

        let url = format!("{}/v1/chat/completions", self.api_base.trim_end_matches('/'));
        let resp = self.http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

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
                let bytes = result.map_err(|e| ForgeError::Api(e.to_string()))?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                self.parse_sse_line(&text)
            });

        Ok(stream)
    }

    /// 解析 SSE 数据行（含缓存命中统计 + 原生 tool_calls）
    fn parse_sse_line(&mut self, text: &str) -> Result<StreamChunk, ForgeError> {
        let mut content = String::new();
        let mut finish_reason = None;

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

        Ok(StreamChunk { content, finish_reason, tool_calls })
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
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub tool_call_id: Option<String>,
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
        Self { role: "system".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }
    pub fn user(content: &str) -> Self {
        Self { role: "user".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }
    pub fn assistant(content: &str) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls: None, tool_call_id: None }
    }
    pub fn tool_result(tool_call_id: &str, content: &str) -> Self {
        Self { role: "tool".into(), content: content.into(), tool_call_id: Some(tool_call_id.into()), tool_calls: None }
    }
}
