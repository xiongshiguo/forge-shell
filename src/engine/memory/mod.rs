//! 记忆系统：短期/长期记忆 + 自动摘要 + 会话恢复
//!
//! 短期记忆：当前会话的最近交互，存储在内存中
//! 长期记忆：跨会话持久化到 .ai/memory/ 目录
//! 自动摘要：定期将长对话压缩为摘要，减少 token 消耗

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 记忆类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    ShortTerm,
    LongTerm,
    Summary,
}

/// 记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub access_count: u64,
    pub last_accessed: Option<chrono::DateTime<chrono::Utc>>,
    /// 重要性评分 0.0-1.0
    pub importance: f64,
}

/// 会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub message_count: usize,
    pub summary_text: String,
    pub key_decisions: Vec<String>,
    pub files_modified: Vec<String>,
    pub total_tokens_used: u64,
}

/// 自动摘要配置
#[derive(Debug, Clone)]
pub struct SummaryConfig {
    /// 触发摘要的对话轮数阈值
    pub trigger_rounds: usize,
    /// 触发摘要的 token 数阈值
    pub trigger_tokens: u64,
    /// 保留的最近轮数（不被摘要覆盖）
    pub keep_recent_rounds: usize,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            trigger_rounds: 20,
            trigger_tokens: 50000,
            keep_recent_rounds: 5,
        }
    }
}

/// 记忆管理器
pub struct MemoryManager {
    /// 短期记忆（当前会话对话轮次）
    short_term: Vec<MemoryEntry>,
    /// 长期记忆索引
    long_term: Vec<MemoryEntry>,
    /// 会话摘要历史
    summaries: Vec<SessionSummary>,
    /// 关键词 → 记忆 ID 索引
    tag_index: HashMap<String, Vec<String>>,
    /// 持久化目录
    storage_path: Option<PathBuf>,
    /// 摘要配置
    summary_config: SummaryConfig,
    /// 当前会话 ID
    session_id: String,
    /// 当前会话开始时间
    session_start: chrono::DateTime<chrono::Utc>,
    /// 累计 token 数
    total_tokens: u64,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self {
            short_term: Vec::new(),
            long_term: Vec::new(),
            summaries: Vec::new(),
            tag_index: HashMap::new(),
            storage_path: None,
            summary_config: SummaryConfig::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
            session_start: chrono::Utc::now(),
            total_tokens: 0,
        }
    }

    /// 设置持久化目录
    pub fn with_storage(mut self, path: PathBuf) -> Self {
        self.storage_path = Some(path);
        self.load_from_disk();
        self
    }

    /// 保存当前会话
    pub fn save_session(&self) -> std::io::Result<()> {
        if let Some(ref dir) = self.storage_path {
            std::fs::create_dir_all(dir)?;

            // 保存短期记忆
            let short_path = dir.join("short_term.json");
            let json = serde_json::to_string_pretty(&self.short_term)?;
            std::fs::write(&short_path, json)?;

            // 保存摘要
            let summary_path = dir.join("summaries.json");
            let json = serde_json::to_string_pretty(&self.summaries)?;
            std::fs::write(&summary_path, json)?;
        }
        Ok(())
    }

    /// 从磁盘加载
    fn load_from_disk(&mut self) {
        let (long_path, summary_path) = if let Some(ref dir) = self.storage_path {
            (dir.join("long_term.json"), dir.join("summaries.json"))
        } else {
            return;
        };

        // 加载长期记忆
        if let Ok(data) = std::fs::read_to_string(&long_path) {
            if let Ok(entries) = serde_json::from_str::<Vec<MemoryEntry>>(&data) {
                self.long_term = entries;
            }
        }
        // 加载摘要
        if let Ok(data) = std::fs::read_to_string(&summary_path) {
            if let Ok(sums) = serde_json::from_str::<Vec<SessionSummary>>(&data) {
                self.summaries = sums;
            }
        }
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.tag_index.clear();
        for entry in &self.long_term {
            for tag in &entry.tags {
                self.tag_index
                    .entry(tag.clone())
                    .or_default()
                    .push(entry.id.clone());
            }
        }
    }

    /// 添加记忆
    pub fn add(&mut self, memory_type: MemoryType, content: &str, tags: &[&str], importance: f64) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let entry = MemoryEntry {
            id: id.clone(),
            memory_type,
            content: content.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            created_at: chrono::Utc::now(),
            access_count: 0,
            last_accessed: None,
            importance,
        };

        // 索引标签
        for tag in &entry.tags {
            self.tag_index.entry(tag.clone()).or_default().push(id.clone());
        }

        match memory_type {
            MemoryType::ShortTerm => self.short_term.push(entry),
            MemoryType::LongTerm => {
                self.long_term.push(entry);
                // 限制长期记忆数量
                if self.long_term.len() > 1000 {
                    self.long_term.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));
                    self.long_term.truncate(1000);
                }
            }
            MemoryType::Summary => {
                // 摘要不单独存储，而是记录在 summaries 中
            }
        }

        id
    }

    /// 按关键词搜索
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<&MemoryEntry> = Vec::new();

        // 搜索标签索引
        for (tag, ids) in &self.tag_index {
            if tag.to_lowercase().contains(&query_lower) {
                for id in ids {
                    if let Some(entry) = self.long_term.iter().find(|e| &e.id == id) {
                        if !results.iter().any(|r| r.id == entry.id) {
                            results.push(entry);
                        }
                    }
                }
            }
        }

        // 全文搜索短期记忆
        for entry in &self.short_term {
            if entry.content.to_lowercase().contains(&query_lower) {
                results.push(entry);
            }
        }

        // 全文搜索长期记忆
        for entry in &self.long_term {
            if entry.content.to_lowercase().contains(&query_lower)
                && !results.iter().any(|r| r.id == entry.id)
            {
                results.push(entry);
            }
        }

        // 按重要性排序
        results.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// 检查是否需要生成摘要
    pub fn should_summarize(&self) -> bool {
        let rounds = self.short_term.len();
        rounds >= self.summary_config.trigger_rounds
            || self.total_tokens >= self.summary_config.trigger_tokens
    }

    /// 生成自动摘要
    pub fn summarize(&mut self) -> SessionSummary {
        let recent_count = self.summary_config.keep_recent_rounds;
        let to_summarize: Vec<&MemoryEntry> = self.short_term
            .iter()
            .take(self.short_term.len().saturating_sub(recent_count))
            .collect();

        // 生成摘要文本（精简版：提取关键信息）
        let summary_parts: Vec<String> = to_summarize.iter().map(|e| {
            crate::utils::truncate_with_ellipsis(&e.content, 100)
        }).collect();

        let summary = SessionSummary {
            session_id: self.session_id.clone(),
            start_time: self.session_start,
            end_time: Some(chrono::Utc::now()),
            message_count: to_summarize.len(),
            summary_text: summary_parts.join("\n"),
            key_decisions: self.extract_key_decisions(&to_summarize),
            files_modified: Vec::new(),
            total_tokens_used: self.total_tokens,
        };

        // 将被摘要的短期记忆转为长期记忆
        let summary_id = uuid::Uuid::new_v4().to_string();
        let summary_entry = MemoryEntry {
            id: summary_id,
            memory_type: MemoryType::LongTerm,
            content: summary.summary_text.clone(),
            tags: vec!["auto-summary".into(), "session".into()],
            created_at: chrono::Utc::now(),
            access_count: 0,
            last_accessed: None,
            importance: 0.7,
        };
        self.long_term.push(summary_entry);
        self.summaries.push(summary.clone());

        // 清理已摘要的短期记忆
        let keep_start = self.short_term.len().saturating_sub(recent_count);
        self.short_term = self.short_term.split_off(keep_start);

        // 保存
        let _ = self.save_session();

        summary
    }

    /// 提取关键决策（简易版：寻找"决定"、"选择"等关键词）
    fn extract_key_decisions(&self, entries: &[&MemoryEntry]) -> Vec<String> {
        let keywords = ["决定", "选择", "采用", "修改", "创建", "删除", "重构"];
        let mut decisions = Vec::new();
        for entry in entries {
            for kw in &keywords {
                if entry.content.contains(kw) {
                    decisions.push(entry.content.clone());
                    break;
                }
            }
        }
        decisions.truncate(10);
        decisions
    }

    /// 获取最近摘要
    pub fn last_summary(&self) -> Option<&SessionSummary> {
        self.summaries.last()
    }

    /// 获取所有摘要
    pub fn summaries(&self) -> &[SessionSummary] {
        &self.summaries
    }

    /// 累积 token
    pub fn add_tokens(&mut self, tokens: u64) {
        self.total_tokens += tokens;
    }

    /// 获取记忆统计
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            short_term_count: self.short_term.len(),
            long_term_count: self.long_term.len(),
            summary_count: self.summaries.len(),
            total_tokens: self.total_tokens,
            index_size: self.tag_index.len(),
        }
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 记忆统计
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub short_term_count: usize,
    pub long_term_count: usize,
    pub summary_count: usize,
    pub total_tokens: u64,
    pub index_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_search() {
        let mut mm = MemoryManager::new();
        mm.add(MemoryType::ShortTerm, "使用 ratatui 实现 TUI 界面", &["rust", "tui"], 0.8);
        mm.add(MemoryType::LongTerm, "Rust 的内存模型基于所有权", &["rust", "memory"], 0.9);

        let results = mm.search("rust");
        assert!(!results.is_empty());

        let results2 = mm.search("python");
        assert!(results2.is_empty());
    }

    #[test]
    fn test_should_summarize_with_rounds() {
        let mut mm = MemoryManager::new();
        mm.summary_config.trigger_rounds = 5;

        for i in 0..6 {
            mm.add(MemoryType::ShortTerm, &format!("message {}", i), &[], 0.5);
        }
        assert!(mm.should_summarize());
    }

    #[test]
    fn test_summarize_preserves_recent() {
        let mut mm = MemoryManager::new();
        mm.summary_config.trigger_rounds = 5;
        mm.summary_config.keep_recent_rounds = 2;

        for i in 0..7 {
            mm.add(MemoryType::ShortTerm, &format!("message {}", i), &[], 0.5);
        }

        let summary = mm.summarize();
        assert_eq!(summary.message_count, 5);
        // 应保留最近 2 条
        assert_eq!(mm.stats().short_term_count, 2);
    }
}
