//! 记忆系统：短期/长期/自动摘要

/// 记忆类型
#[derive(Debug, Clone)]
pub enum MemoryType {
    /// 短期记忆（当前会话）
    ShortTerm,
    /// 长期记忆（跨会话持久化）
    LongTerm,
    /// 自动摘要
    Summary,
}

/// 记忆条目
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub access_count: u64,
}

/// 记忆管理器
pub struct MemoryManager {
    entries: Vec<MemoryEntry>,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self { entries: vec![] }
    }

    /// 添加记忆
    pub fn add(&mut self, memory_type: MemoryType, content: &str) {
        self.entries.push(MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            memory_type,
            content: content.to_string(),
            created_at: chrono::Utc::now(),
            access_count: 0,
        });
    }

    /// 搜索相关记忆
    pub fn search(&self, _query: &str) -> Vec<&MemoryEntry> {
        // 阶段 5 完善：向量相似度搜索
        self.entries.iter().collect()
    }

    /// 生成摘要
    pub fn summarize(&self) -> String {
        // 阶段 5 完善：调用 AI 生成摘要
        format!("共 {} 条记忆", self.entries.len())
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}
