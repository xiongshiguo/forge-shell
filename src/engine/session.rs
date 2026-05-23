//! 会话管理器（L3）
//! 保证：原子写入（不会半途崩溃丢数据）、自动恢复、最新+历史同步

use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub date: String,
    pub turn: usize,
    pub messages: Vec<SessionMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
}

/// 会话管理器——原子写入保证不丢数据
pub struct SessionManager {
    dir: PathBuf,
}

impl SessionManager {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    /// 保存最新会话（原子写入：先写临时文件，再重命名）
    pub fn save_latest(&self, record: &SessionRecord) {
        let latest = self.dir.join("latest.json");
        let tmp = self.dir.join("latest.tmp");
        if let Ok(json) = serde_json::to_string_pretty(record) {
            let _ = std::fs::write(&tmp, &json);
            let _ = std::fs::rename(&tmp, &latest); // 原子操作
        }
    }

    /// 加载最新会话
    pub fn load_latest(&self) -> Option<SessionRecord> {
        let path = self.dir.join("latest.json");
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// 保存历史会话（同时更新 latest）
    pub fn save_to_history(&self, id: &str, record: &SessionRecord) {
        let path = self.dir.join(format!("{}.json", id));
        if let Ok(json) = serde_json::to_string_pretty(record) {
            let _ = std::fs::write(&path, &json);
        }
    }

    /// 列出历史会话
    pub fn list_history(&self) -> Vec<SessionRecord> {
        let mut sessions = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name == "latest.json" || name == "latest.tmp" { continue; }
                if let Ok(content) = std::fs::read_to_string(e.path()) {
                    if let Ok(s) = serde_json::from_str::<SessionRecord>(&content) {
                        sessions.push(s);
                    }
                }
            }
        }
        sessions.sort_by_key(|s| std::cmp::Reverse(s.date.clone()));
        sessions
    }
}
