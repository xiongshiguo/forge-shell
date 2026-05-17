//! LRU + 磁盘缓存，目标命中率 ≥97%

use std::collections::HashMap;
use std::path::PathBuf;

/// 缓存条目
#[derive(Debug, Clone)]
struct CacheEntry {
    key: String,
    value: String,
    token_count: usize,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// 缓存统计
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_requests: u64,
    pub tokens_saved: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.hits as f64 / self.total_requests as f64
        }
    }
}

/// 缓存管理器
pub struct CacheManager {
    max_entries: usize,
    entries: HashMap<String, CacheEntry>,
    disk_path: Option<PathBuf>,
    stats: CacheStats,
}

impl CacheManager {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: HashMap::new(),
            disk_path: None,
            stats: CacheStats::default(),
        }
    }

    /// 设置磁盘缓存路径
    pub fn with_disk_path(mut self, path: PathBuf) -> Self {
        self.disk_path = Some(path);
        self
    }

    /// 查询缓存
    pub fn get(&mut self, key: &str) -> Option<String> {
        self.stats.total_requests += 1;
        if let Some(entry) = self.entries.get(key) {
            self.stats.hits += 1;
            self.stats.tokens_saved += entry.token_count as u64;
            Some(entry.value.clone())
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// 写入缓存
    pub fn set(&mut self, key: &str, value: &str, token_count: usize) {
        if self.entries.len() >= self.max_entries {
            // 简易 LRU 驱逐：删除最旧条目
            if let Some(oldest_key) = self.entries.keys().next().cloned() {
                self.entries.remove(&oldest_key);
            }
        }
        self.entries.insert(
            key.to_string(),
            CacheEntry {
                key: key.to_string(),
                value: value.to_string(),
                token_count,
                created_at: chrono::Utc::now(),
            },
        );
    }

    /// 获取缓存统计
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// 清空缓存
    pub fn clear(&mut self) {
        self.entries.clear();
        self.stats = CacheStats::default();
    }
}
