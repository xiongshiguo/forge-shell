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

/// SSE 流式 chunk
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub content: String,
    pub finish_reason: Option<String>,
}

/// 推理客户端
pub struct InferenceClient {
    api_key: String,
    api_base: String,
    model: String,
    http: reqwest::Client,
    pub total_usage: TokenUsage,
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
        })
    }

    /// 流式聊天（返回 SSE stream）
    pub async fn chat_stream(
        &mut self,
        messages: Vec<ChatMessage>,
    ) -> Result<impl Stream<Item = Result<StreamChunk, ForgeError>>, ForgeError> {
        let body = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            temperature: 0.0,
            max_tokens: 8192,
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
            return Err(ForgeError::Api(format!("API 调用失败 ({}): {}", status, text)));
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

    /// 解析 SSE 数据行（含缓存命中统计）
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

        Ok(StreamChunk { content, finish_reason })
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
    temperature: f64,
    max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: &str) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: &str) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: &str) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}
