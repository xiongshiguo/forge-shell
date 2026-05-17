//! 四级缓存上下文系统
//!
//! Level 1: System Prompt（永久，启动时加载）
//! Level 2: Project Context Block（跨会话复用，项目变更时更新）
//! Level 3: Session Persistent（最近 N 轮对话）
//! Level 4: Volatile Tail（当前指令和输出）

mod level1;
mod level2;
mod level3;
mod level4;

use crate::engine::cache::{CacheLevel, CacheManager, CacheStats};

/// 完整的上下文组装结果
#[derive(Debug, Clone)]
pub struct AssembledContext {
    pub total_tokens: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub system_prompt: String,
    pub project_context: String,
    pub session_history: Vec<(String, String)>,
    pub current_input: String,
}

impl AssembledContext {
    pub fn estimated_cost(&self, input_price_per_1m: f64) -> f64 {
        self.total_tokens as f64 * input_price_per_1m / 1_000_000.0
    }
}

/// 上下文管理器
pub struct ContextManager {
    cache: CacheManager,
    session: Vec<(String, String)>,
    max_session_rounds: usize,
}

impl ContextManager {
    pub fn new(max_entries: usize, max_session_rounds: usize) -> Self {
        Self {
            cache: CacheManager::new(max_entries),
            session: Vec::new(),
            max_session_rounds,
        }
    }

    pub fn with_disk_cache(mut self, path: std::path::PathBuf) -> Self {
        self.cache = std::mem::take(&mut self.cache).with_disk_path(path);
        self
    }

    /// 初始化系统提示词
    pub fn init_system_prompt(&mut self, prompt: &str) {
        self.cache.set_system_prompt(prompt);
    }

    /// 更新项目上下文（仅在指纹变化时重新计算）
    pub fn update_project_context(&mut self, ctx: &str, fingerprint: &str) {
        if self.cache.project_changed(fingerprint) {
            self.cache.clear_session();
            self.session.clear();
            self.cache.set_project_context(ctx, fingerprint);
        }
    }

    /// 添加一轮对话
    pub fn add_turn(&mut self, user: &str, assistant: &str) {
        // 缓存助手回复
        let cache_key = format!("turn_{}", self.session.len());
        let token_estimate = (user.len() + assistant.len()) / 4;
        self.cache.set(&cache_key, assistant, token_estimate);

        self.session.push((user.to_string(), assistant.to_string()));
        if self.session.len() > self.max_session_rounds {
            let excess = self.session.len() - self.max_session_rounds;
            self.session.drain(0..excess);
        }
    }

    /// 组装完整上下文
    pub fn assemble(&mut self, current_input: &str) -> AssembledContext {
        let system_prompt = self.cache.get_system_prompt().unwrap_or_default();
        let project_context = self.cache.get("__project_context__").unwrap_or_default();

        // 尝试从缓存获取历史
        let mut cache_hits = 0;
        let mut cache_misses = 0;
        // 检查输入是否在缓存中
        if self.cache.get(current_input).is_some() {
            cache_hits += 1;
        } else {
            cache_misses += 1;
        }

        let total = system_prompt.chars().count()
            + project_context.chars().count()
            + self.session.iter().map(|(u, a)| u.chars().count() + a.chars().count()).sum::<usize>()
            + current_input.chars().count();

        AssembledContext {
            total_tokens: total / 4,
            cache_hits,
            cache_misses,
            system_prompt,
            project_context,
            session_history: self.session.clone(),
            current_input: current_input.to_string(),
        }
    }

    /// 获得缓存统计
    pub fn cache_stats(&self) -> &CacheStats {
        self.cache.stats()
    }

    /// 缓存命中率
    pub fn hit_rate(&self) -> f64 {
        self.cache.stats().hit_rate()
    }

    /// 层级缓存命中详情
    pub fn level_hit_rates(&self) -> [f64; 4] {
        [
            self.cache.stats().level_hit_rate(CacheLevel::System),
            self.cache.stats().level_hit_rate(CacheLevel::Project),
            self.cache.stats().level_hit_rate(CacheLevel::Session),
            0.0, // Volatile 不缓存
        ]
    }
}
