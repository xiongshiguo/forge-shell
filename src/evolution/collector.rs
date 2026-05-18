//! 经验采集器：自动从对话中提取匿名策略记录
//! 不上传代码、路径、变量名、API Key

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 一条匿名经验记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceRecord {
    pub id: String,
    /// 用户意图类型（搜索/重构/测试/分析/通用）
    pub intent_type: String,
    /// 任务复杂度（简单/中等/复杂）
    pub complexity: String,
    /// 用户消息长度
    pub input_length: usize,
    /// 回复长度
    pub output_length: usize,
    /// 是否成功（无错误即成功）
    pub success: bool,
    /// 使用的工具列表（不包含参数值）
    pub tools_used: Vec<String>,
    /// 对话轮次
    pub turn_count: u64,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 策略标签（自动提取）
    pub tags: Vec<String>,
    /// 脱敏的策略描述
    pub strategy_note: String,
}

/// 经验采集器
pub struct ExperienceCollector {
    /// 存储目录
    storage_dir: PathBuf,
    /// 当前会话轮次计数
    session_turns: u64,
    /// 当前会话使用的工具
    session_tools: Vec<String>,
}

impl ExperienceCollector {
    pub fn new(storage_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&storage_dir).ok();
        Self {
            storage_dir,
            session_turns: 0,
            session_tools: Vec::new(),
        }
    }

    /// 记录一轮对话
    pub fn record(&mut self, user_input: &str, assistant_output: &str, success: bool) {
        self.session_turns += 1;

        let record = ExperienceRecord {
            id: uuid::Uuid::new_v4().to_string(),
            intent_type: self.classify_intent(user_input),
            complexity: self.classify_complexity(user_input),
            input_length: user_input.len(),
            output_length: assistant_output.len(),
            success,
            tools_used: self.session_tools.clone(),
            turn_count: self.session_turns,
            timestamp: chrono::Utc::now(),
            tags: self.extract_tags(user_input),
            strategy_note: self.extract_strategy(user_input, assistant_output, success),
        };

        self.save_record(&record);
    }

    /// 记录工具使用
    pub fn record_tool(&mut self, tool_name: &str) {
        if !self.session_tools.contains(&tool_name.to_string()) {
            self.session_tools.push(tool_name.to_string());
        }
    }

    /// 会话结束，保存摘要
    pub fn end_session(&self) -> SessionDigest {
        let files = self.load_all_records();
        let success_rate = if files.is_empty() { 1.0 }
            else { files.iter().filter(|r| r.success).count() as f64 / files.len() as f64 };

        SessionDigest {
            total_turns: self.session_turns,
            success_rate,
            tools_used: self.session_tools.len(),
            top_tags: self.top_tags(&files, 5),
            timestamp: chrono::Utc::now(),
        }
    }

    fn classify_intent(&self, input: &str) -> String {
        let s = input.to_lowercase();
        if s.contains("搜索") || s.contains("找") { "search".into() }
        else if s.contains("重构") || s.contains("改") || s.contains("优化") { "refactor".into() }
        else if s.contains("测试") || s.contains("test") { "test".into() }
        else if s.contains("分析") || s.contains("检查") { "analyze".into() }
        else if s.contains("解释") || s.contains("什么是") { "explain".into() }
        else { "general".into() }
    }

    fn classify_complexity(&self, input: &str) -> String {
        let len = input.chars().count();
        if len < 50 { "simple".into() }
        else if len < 500 { "moderate".into() }
        else { "complex".into() }
    }

    fn extract_tags(&self, input: &str) -> Vec<String> {
        let keywords = ["rust", "api", "数据库", "前端", "后端", "测试", "部署",
            "性能", "安全", "重构", "架构", "tui", "web", "git", "配置"];
        keywords.iter()
            .filter(|k| input.to_lowercase().contains(*k))
            .map(|k| k.to_string())
            .collect()
    }

    fn extract_strategy(&self, input: &str, output: &str, success: bool) -> String {
        if success {
            let intent = self.classify_intent(input);
            let complexity = self.classify_complexity(input);
            format!("{} 任务（{}），{} 字符输入，{} 字符输出，成功",
                intent, complexity, input.len(), output.len())
        } else {
            format!("任务失败，输入 {} 字符", input.len())
        }
    }

    fn save_record(&self, record: &ExperienceRecord) {
        let file_path = self.storage_dir.join(format!("exp_{}.json", &record.id[..8]));
        if let Ok(json) = serde_json::to_string_pretty(record) {
            std::fs::write(file_path, json).ok();
        }
    }

    pub fn load_all_records(&self) -> Vec<ExperienceRecord> {
        let mut records = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.storage_dir) {
            for entry in entries.flatten() {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(record) = serde_json::from_str::<ExperienceRecord>(&content) {
                        records.push(record);
                    }
                }
            }
        }
        records.sort_by_key(|r| r.timestamp);
        records
    }

    fn top_tags(&self, records: &[ExperienceRecord], n: usize) -> Vec<String> {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for r in records {
            for tag in &r.tags {
                *counts.entry(tag.clone()).or_default() += 1;
            }
        }
        let mut pairs: Vec<_> = counts.into_iter().collect();
        pairs.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        pairs.truncate(n);
        pairs.into_iter().map(|(k, _)| k).collect()
    }
}

/// 会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDigest {
    pub total_turns: u64,
    pub success_rate: f64,
    pub tools_used: usize,
    pub top_tags: Vec<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
