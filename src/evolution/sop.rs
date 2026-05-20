//! 天工阁 (SOP 库)：存储、匹配、推荐最佳操作流程

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// SOP 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopEntry {
    pub id: String,
    pub title: String,
    pub description: String,
    /// 触发关键词
    pub triggers: Vec<String>,
    /// 操作步骤
    pub steps: Vec<SopStep>,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 使用次数
    pub usage_count: u64,
    /// 成功率
    pub success_rate: f64,
    /// 来源（reflection/manual）
    pub source: String,
    /// 数字指纹（防批量抄袭溯源）
    #[serde(default)]
    pub fingerprint: Option<String>,
}

/// SOP 操作步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopStep {
    pub order: u32,
    pub action: String,
    pub expected_result: String,
    pub tool_hint: Option<String>,
}

/// 匹配结果
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub sop_id: String,
    pub title: String,
    /// 相关性评分 0.0-1.0
    pub relevance: f64,
    pub steps: Vec<String>,
}

/// 天工阁 SOP 库
pub struct SopLibrary {
    entries: Vec<SopEntry>,
    /// 关键词 → SOP ID 倒排索引
    keyword_index: HashMap<String, Vec<String>>,
    /// 持久化文件
    storage_file: PathBuf,
}

impl SopLibrary {
    pub fn new(storage_file: PathBuf) -> Self {
        let mut lib = Self {
            entries: Vec::new(),
            keyword_index: HashMap::new(),
            storage_file,
        };
        lib.load_from_disk();
        lib
    }

    /// 添加 SOP（自动注入数字指纹）
    pub fn add(&mut self, mut sop: SopEntry) {
        // 注入数字指纹：SOP 内容 + 来源 + 时间戳的哈希，防批量抄袭可溯源
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        sop.title.hash(&mut h);
        sop.description.hash(&mut h);
        sop.created_at.to_rfc3339().hash(&mut h);
        sop.fingerprint = Some(format!("forge-sop-{:016x}", h.finish()));

        for trigger in &sop.triggers {
            let key = trigger.to_lowercase();
            self.keyword_index.entry(key).or_default().push(sop.id.clone());
        }
        self.entries.push(sop);
        self.save_to_disk();
    }

    /// 根据输入匹配最相关的 SOP
    pub fn match_input(&self, input: &str) -> Vec<MatchResult> {
        let input_lower = input.to_lowercase();
        let mut scored: Vec<(f64, &SopEntry)> = Vec::new();

        for entry in &self.entries {
            let mut score = 0.0;

            // 关键词命中
            for trigger in &entry.triggers {
                if input_lower.contains(&trigger.to_lowercase()) {
                    score += 0.3;
                }
            }

            // 标题相似度（简易 Jaccard）
            let title_words: Vec<&str> = entry.title.split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|w| w.len() > 1)
                .collect();
            let input_chars: Vec<char> = input_lower.chars().collect();

            for word in &title_words {
                let word_lower = word.to_lowercase();
                // 检查 title 中的词是否出现在 input 中
                if input_lower.contains(&word_lower) {
                    score += 0.2;
                }
            }

            // 只有有关键词或标题命中时，才考虑使用频率和成功率加成
            if score > 0.0 {
                // 使用频率加成
                if entry.usage_count > 0 {
                    score += (entry.usage_count as f64 / 100.0).min(0.2);
                }
                // 成功率加成
                score += entry.success_rate * 0.1;
                scored.push((score.min(1.0), entry));
            }
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(5);

        scored.into_iter().map(|(relevance, entry)| {
            MatchResult {
                sop_id: entry.id.clone(),
                title: entry.title.clone(),
                relevance,
                steps: entry.steps.iter()
                    .map(|s| format!("{}. {} → {}", s.order, s.action, s.expected_result))
                    .collect(),
            }
        }).collect()
    }

    /// 查找相似 SOP（按标题）
    pub fn find_similar(&self, name: &str) -> Option<&SopEntry> {
        let name_lower = name.to_lowercase();
        self.entries.iter().find(|e| {
            e.title.to_lowercase().contains(&name_lower)
                || name_lower.contains(&e.title.to_lowercase())
        })
    }

    /// SOP 总数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 获取所有 SOP
    pub fn all(&self) -> &[SopEntry] {
        &self.entries
    }

    /// 记录使用
    pub fn record_use(&mut self, sop_id: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == sop_id) {
            entry.usage_count += 1;
            self.save_to_disk();
        }
    }

    fn save_to_disk(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.entries) {
            std::fs::write(&self.storage_file, json).ok();
        }
    }

    fn load_from_disk(&mut self) {
        if let Ok(content) = std::fs::read_to_string(&self.storage_file) {
            if let Ok(entries) = serde_json::from_str::<Vec<SopEntry>>(&content) {
                self.keyword_index.clear();
                for entry in &entries {
                    for trigger in &entry.triggers {
                        self.keyword_index.entry(trigger.to_lowercase())
                            .or_default()
                            .push(entry.id.clone());
                    }
                }
                self.entries = entries;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sop(id: &str, title: &str, triggers: Vec<&str>) -> SopEntry {
        SopEntry {
            id: id.into(),
            title: title.into(),
            description: "test".into(),
            triggers: triggers.iter().map(|s| s.to_string()).collect(),
            steps: vec![SopStep { order: 1, action: "做某事".into(), expected_result: "成功".into(), tool_hint: None }],
            created_at: chrono::Utc::now(),
            usage_count: 0,
            success_rate: 0.9,
            source: "test".into(),
            fingerprint: None,
        }
    }

    #[test]
    fn test_match_by_keyword() {
        let path = std::path::PathBuf::from("/tmp/test_sop1.json");
        let _ = std::fs::remove_file(&path);
        let mut lib = SopLibrary::new(path);
        lib.add(make_sop("1", "Rust 错误处理", vec!["rust", "error", "错误"]));

        let matches = lib.match_input("如何处理 Rust 的错误");
        assert!(!matches.is_empty());
        assert!(matches[0].relevance > 0.0);
    }

    #[test]
    fn test_no_match_for_unrelated() {
        let path = std::path::PathBuf::from("/tmp/test_sop2.json");
        let _ = std::fs::remove_file(&path);
        let mut lib = SopLibrary::new(path);
        lib.add(make_sop("1", "Rust 编译", vec!["rust", "compile"]));

        let matches = lib.match_input("Python 数据分析");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_usage_boosts_score() {
        let path = std::path::PathBuf::from("/tmp/test_sop3.json");
        let _ = std::fs::remove_file(&path);
        let mut lib = SopLibrary::new(path);
        let mut sop = make_sop("1", "API 设计", vec!["api"]);
        sop.usage_count = 50;
        lib.add(sop.clone());

        let mut sop2 = make_sop("2", "数据库", vec!["api"]);
        sop2.usage_count = 0;
        lib.add(sop2);

        let matches = lib.match_input("api");
        assert_eq!(matches[0].sop_id, "1"); // 使用次数多的排前面
    }
}
