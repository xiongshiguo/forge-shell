//! AI 推理客户端：流式 API 调用 + Token 计数

use crate::config::Config;
use crate::error::ForgeError;

/// Token 用量统计
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// 推理客户端
pub struct InferenceClient {
    config: Config,
    http: reqwest::Client,
    total_usage: TokenUsage,
}

impl InferenceClient {
    pub fn new(config: Config) -> Result<Self, ForgeError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.ai.timeout_secs))
            .build()?;

        Ok(Self {
            config,
            http,
            total_usage: TokenUsage::default(),
        })
    }

    /// 获取累计 Token 用量
    pub fn total_usage(&self) -> &TokenUsage {
        &self.total_usage
    }

    /// 估算费用（DeepSeek 价格）
    /// V4 Pro: ¥1/M input tokens, ¥4/M output tokens
    /// Flash: ¥0.1/M input tokens, ¥0.4/M output tokens
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
