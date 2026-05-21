//! 自优化提示词引擎
//! 记录每次调用的提示词版本、成功率、Token 成本
//! 用多臂老虎机算法自动选最优变体

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 提示词变体记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVariant {
    pub id: String,
    pub description: String,
    /// 调用次数
    pub calls: u64,
    /// 成功次数
    pub successes: u64,
    /// 总 Token 成本
    pub total_tokens: u64,
    /// 总缓存命中 Token
    pub cache_hit_tokens: u64,
    /// 当前权重（多臂老虎机 UCB1 算法）
    pub weight: f64,
}

/// 单次调用记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecord {
    pub variant_id: String,
    pub success: bool,
    pub tokens: u64,
    pub cache_hits: u64,
    pub timestamp: String,
    pub complexity: String,
}

/// 提示词优化器
pub struct PromptOptimizer {
    variants: HashMap<String, PromptVariant>,
    history: Vec<CallRecord>,
    storage_path: std::path::PathBuf,
    total_calls: u64,
}

impl PromptOptimizer {
    pub fn new() -> Self {
        let path = crate::config::forge_data_dir().join("prompt_variants.json");
        let mut opt = Self {
            variants: HashMap::new(),
            history: Vec::new(),
            storage_path: path,
            total_calls: 0,
        };
        opt.init_defaults();
        opt.load();
        opt
    }

    fn init_defaults(&mut self) {
        // 三种系统提示词变体
        self.variants.insert("v1-full".into(), PromptVariant {
            id: "v1-full".into(), description: "完整版系统提示词（含社区、架构、工具描述）".into(),
            calls: 0, successes: 0, total_tokens: 0, cache_hit_tokens: 0, weight: 1.0,
        });
        self.variants.insert("v2-compact".into(), PromptVariant {
            id: "v2-compact".into(), description: "精简版（只含工具定义和核心规则）".into(),
            calls: 0, successes: 0, total_tokens: 0, cache_hit_tokens: 0, weight: 1.0,
        });
        self.variants.insert("v3-tool-first".into(), PromptVariant {
            id: "v3-tool-first".into(), description: "工具优先版（先列出工具，再给规则）".into(),
            calls: 0, successes: 0, total_tokens: 0, cache_hit_tokens: 0, weight: 1.0,
        });
    }

    /// UCB1 算法选择最优变体
    pub fn select_best(&self) -> String {
        let mut best_id = "v1-full".to_string();
        let mut best_score = f64::NEG_INFINITY;

        for (id, v) in &self.variants {
            if v.calls == 0 {
                return id.clone(); // 优先探索未试过的
            }
            // UCB1: 成功率 + sqrt(2*ln(总次数)/该变体次数)
            let success_rate = v.successes as f64 / v.calls.max(1) as f64;
            let exploration = if self.total_calls > 0 {
                (2.0 * (self.total_calls as f64).ln() / v.calls as f64).sqrt()
            } else { 1.0 };
            let score = success_rate + 0.1 * exploration - (v.total_tokens as f64 / v.calls.max(1) as f64) * 0.0001;
            if score > best_score {
                best_score = score;
                best_id = id.clone();
            }
        }
        best_id
    }

    /// 记录一次调用结果
    pub fn record(&mut self, variant_id: &str, success: bool, tokens: u64, cache_hits: u64, complexity: &str) {
        self.total_calls += 1;
        if let Some(v) = self.variants.get_mut(variant_id) {
            v.calls += 1;
            if success { v.successes += 1; }
            v.total_tokens += tokens;
            v.cache_hit_tokens += cache_hits;
            let success_rate = v.successes as f64 / v.calls.max(1) as f64;
            let avg_tokens = v.total_tokens as f64 / v.calls.max(1) as f64;
            v.weight = success_rate * 100.0 - avg_tokens * 0.0001;
        }

        self.history.push(CallRecord {
            variant_id: variant_id.into(), success, tokens, cache_hits,
            timestamp: chrono::Utc::now().to_rfc3339(), complexity: complexity.into(),
        });

        // 每 50 次调用保存一次
        if self.total_calls % 50 == 0 { self.save(); }
    }

    /// 获取最优变体的统计数据
    pub fn stats(&self) -> serde_json::Value {
        let mut variants = Vec::new();
        for (id, v) in &self.variants {
            let success_rate = if v.calls > 0 { v.successes as f64 / v.calls as f64 * 100.0 } else { 0.0 };
            variants.push(serde_json::json!({
                "id": id, "desc": v.description, "calls": v.calls,
                "success_rate": format!("{:.1}%", success_rate),
                "avg_tokens": v.total_tokens / v.calls.max(1),
                "weight": format!("{:.2}", v.weight),
                "is_best": id == &self.select_best(),
            }));
        }
        serde_json::json!({"total_calls": self.total_calls, "variants": variants})
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string(&self.variants) {
            std::fs::write(&self.storage_path, json).ok();
        }
    }

    fn load(&mut self) {
        if let Ok(data) = std::fs::read_to_string(&self.storage_path) {
            if let Ok(variants) = serde_json::from_str::<HashMap<String, PromptVariant>>(&data) {
                self.variants = variants;
                self.total_calls = self.variants.values().map(|v| v.calls).sum();
            }
        }
    }
}
