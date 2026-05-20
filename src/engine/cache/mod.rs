//! LRU + 磁盘缓存，目标命中率 ≥97%
//!
//! 四级缓存层级：
//! Level 1: System Prompt（永久，启动时加载）
//! Level 2: Project Context Block（跨会话复用，项目变更时清除）
//! Level 3: Session Persistent（最近 N 轮对话，LRU 淘汰）
//! Level 4: Volatile Tail（当前指令，不缓存）

use std::collections::HashMap;
use std::path::PathBuf;

/// 缓存层级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLevel {
    System = 1,
    Project = 2,
    Session = 3,
    Volatile = 4,
}

/// 缓存条目
#[derive(Debug, Clone)]
struct CacheEntry {
    key: String,
    value: String,
    token_count: usize,
    level: CacheLevel,
    created_at: chrono::DateTime<chrono::Utc>,
    last_accessed: chrono::DateTime<chrono::Utc>,
}

/// 缓存统计
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub tokens_saved: u64,
    pub level_hits: [u64; 4],
}

impl CacheStats {
    pub fn total_requests(&self) -> u64 {
        self.hits + self.misses
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }

    pub fn level_hit_rate(&self, level: CacheLevel) -> f64 {
        let total = self.total_requests();
        if total == 0 { 0.0 } else { self.level_hits[level as usize - 1] as f64 / total as f64 }
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new(100)
    }
}

/// LRU 缓存管理器
pub struct CacheManager {
    max_entries: usize,
    entries: HashMap<String, CacheEntry>,
    /// 访问顺序（LRU 用）
    access_order: Vec<String>,
    disk_path: Option<PathBuf>,
    stats: CacheStats,
    /// 系统提示词缓存
    system_prompt: Option<String>,
    /// 项目上下文缓存
    project_context: Option<String>,
    /// 项目上下文指纹（用于判断是否需要重算）
    project_fingerprint: Option<String>,
}

impl CacheManager {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: HashMap::new(),
            access_order: Vec::new(),
            disk_path: None,
            stats: CacheStats::default(),
            system_prompt: None,
            project_context: None,
            project_fingerprint: None,
        }
    }

    pub fn with_disk_path(mut self, path: PathBuf) -> Self {
        self.disk_path = Some(path);
        self
    }

    /// 获取缓存统计
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// 设置系统提示词
    pub fn set_system_prompt(&mut self, prompt: &str) {
        let now = chrono::Utc::now();
        let entry = CacheEntry {
            key: "__system_prompt__".into(),
            value: prompt.to_string(),
            token_count: prompt.len() / 4,
            level: CacheLevel::System,
            created_at: now,
            last_accessed: now,
        };
        self.entries.insert("__system_prompt__".into(), entry);
        self.system_prompt = Some(prompt.to_string());
    }

    /// 获取系统提示词
    pub fn get_system_prompt(&mut self) -> Option<String> {
        self.get_internal("__system_prompt__", CacheLevel::System)
    }

    /// 设置项目上下文
    pub fn set_project_context(&mut self, ctx: &str, fingerprint: &str) {
        self.project_context = Some(ctx.to_string());
        self.project_fingerprint = Some(fingerprint.to_string());
        let now = chrono::Utc::now();
        let entry = CacheEntry {
            key: "__project_context__".into(),
            value: ctx.to_string(),
            token_count: ctx.len() / 4,
            level: CacheLevel::Project,
            created_at: now,
            last_accessed: now,
        };
        self.entries.insert("__project_context__".into(), entry);
    }

    /// 检查项目指纹是否变化
    pub fn project_changed(&self, new_fingerprint: &str) -> bool {
        self.project_fingerprint.as_deref() != Some(new_fingerprint)
    }

    /// 查询缓存
    pub fn get(&mut self, key: &str) -> Option<String> {
        self.get_internal(key, CacheLevel::Session)
    }

    /// 内部查询实现（带 LRU 更新）
    fn get_internal(&mut self, key: &str, level: CacheLevel) -> Option<String> {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_accessed = chrono::Utc::now();
            // LRU: 移到队尾
            if let Some(pos) = self.access_order.iter().position(|k| k == key) {
                self.access_order.remove(pos);
            }
            self.access_order.push(key.to_string());

            self.stats.hits += 1;
            self.stats.tokens_saved += entry.token_count as u64;
            self.stats.level_hits[level as usize - 1] += 1;
            Some(entry.value.clone())
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// 写入缓存
    pub fn set(&mut self, key: &str, value: &str, token_count: usize) {
        self.set_internal(key, value, token_count, CacheLevel::Session);
    }

    fn set_internal(&mut self, key: &str, value: &str, token_count: usize, level: CacheLevel) {
        // LRU 驱逐
        while self.entries.len() >= self.max_entries && !self.entries.contains_key(key) {
            if let Some(oldest_key) = self.access_order.first().cloned() {
                // 不驱逐系统级和项目级缓存
                if self.entries.get(&oldest_key).map(|e| e.level).unwrap_or(CacheLevel::Volatile)
                   == CacheLevel::Session
                {
                    self.entries.remove(&oldest_key);
                    self.access_order.retain(|k| k != &oldest_key);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // 如果 key 已存在，先移除旧的访问顺序
        self.access_order.retain(|k| k != key);

        let now = chrono::Utc::now();
        self.entries.insert(
            key.to_string(),
            CacheEntry {
                key: key.to_string(),
                value: value.to_string(),
                token_count,
                level,
                created_at: now,
                last_accessed: now,
            },
        );
        self.access_order.push(key.to_string());
    }

    /// 清空会话级缓存
    pub fn clear_session(&mut self) {
        self.entries.retain(|_, e| e.level != CacheLevel::Session);
        self.access_order.clear();
        for key in self.entries.keys() {
            self.access_order.push(key.clone());
        }
    }

    /// 清空所有缓存
    pub fn clear(&mut self) {
        self.entries.clear();
        self.access_order.clear();
        self.system_prompt = None;
        self.project_context = None;
        self.project_fingerprint = None;
        self.stats = CacheStats::default();
    }

    /// 获取缓存条目数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 获取各层级缓存数量
    pub fn level_counts(&self) -> [usize; 4] {
        let mut counts = [0; 4];
        for entry in self.entries.values() {
            counts[entry.level as usize - 1] += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_test_cache(max: usize) -> CacheManager { CacheManager::new(max) }

    #[test]
    fn test_cache_hit_and_miss() {
        let mut cache = new_test_cache(10);
        cache.set("k1", "v1", 5);
        assert!(cache.get("k1").is_some());
        assert!(cache.get("k2").is_none());
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = new_test_cache(3);
        cache.set("a", "1", 1);
        cache.set("b", "2", 1);
        cache.set("c", "3", 1);
        cache.set("d", "4", 1); // 应驱逐 a
        assert!(cache.get("a").is_none());
        assert!(cache.get("d").is_some());
    }

    #[test]
    fn test_lru_reorder_on_get() {
        let mut cache = new_test_cache(2);
        cache.set("a", "1", 1);
        cache.set("b", "2", 1);
        cache.get("a"); // a 变最新
        cache.set("c", "3", 1); // 应驱逐 b
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none());
    }

    #[test]
    fn test_system_level_not_evicted() {
        let mut cache = new_test_cache(2);
        cache.set("sys", "v", 5);
        // 更新 level 为 L1
        cache.entries.get_mut("sys").unwrap().level = CacheLevel::System;
        cache.set("a", "1", 1);
        cache.set("b", "2", 1);
        assert!(cache.get("sys").is_some());
    }

    #[test]
    fn test_level_counts() {
        let mut cache = new_test_cache(10);
        cache.set("a", "1", 1);
        cache.set("b", "2", 1);
        cache.set("c", "3", 1);
        cache.entries.get_mut("a").unwrap().level = CacheLevel::Session;
        cache.entries.get_mut("b").unwrap().level = CacheLevel::Project;
        let counts = cache.level_counts();
        assert!(counts[0] + counts[1] + counts[2] + counts[3] == 3);
    }

    #[test]
    fn test_clear() {
        let mut cache = new_test_cache(10);
        cache.set("a", "1", 1);
        cache.clear();
        assert!(cache.get("a").is_none());
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = new_test_cache(10);
        cache.set("a", "1", 5);
        cache.get("a"); // hit
        cache.get("b"); // miss
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_max_entries_enforced() {
        let mut cache = new_test_cache(5);
        for i in 0..10 { cache.set(&format!("k{}", i), "v", 1); }
        assert!(cache.len() <= 5);
    }
}
