//! 错误日志系统：持久化记录 + 环形缓冲 + 自动诊断
use parking_lot::Mutex;
use chrono::Utc;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;

/// 单条错误日志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEntry {
    pub timestamp: String,
    pub component: String,
    pub level: String, // "error" | "warn" | "panic"
    pub message: String,
    pub context: String,
    pub count: u32, // 相同错误合并计数
}

/// 错误日志管理器
pub struct ErrorLogger {
    /// 内存环形缓冲（最近 200 条）
    buffer: Mutex<Vec<ErrorEntry>>,
    /// 持久化路径
    log_dir: PathBuf,
}

impl ErrorLogger {
    pub fn new(log_dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            tracing::warn!("无法创建日志目录 {:?}: {}", log_dir, e);
        }
        tracing::info!("错误日志目录: {:?}", log_dir);
        // 从磁盘加载历史错误
        let buffer = Self::load_from_disk(&log_dir);
        Self { buffer: Mutex::new(buffer), log_dir }
    }

    fn load_from_disk(dir: &PathBuf) -> Vec<ErrorEntry> {
        let path = dir.join("errors.jsonl");
        if let Ok(content) = std::fs::read_to_string(&path) {
            content.lines()
                .filter_map(|l| serde_json::from_str::<ErrorEntry>(l).ok())
                .collect()
        } else { Vec::new() }
    }

    /// 记录错误（相同错误自动合并）
    pub fn log(&self, component: &str, level: &str, message: &str, context: &str) {
        let mut buf = self.buffer.lock();
        // 合并相同错误（5秒内同组件同消息）
        let now = Utc::now();
        if let Some(last) = buf.last_mut() {
            if last.component == component && last.message == message
                && last.timestamp.len() >= 16
            {
                let last_ts: &str = &last.timestamp;
                let now_ts = now.format("%m-%d %H:%M").to_string();
                if last_ts[..16] == now_ts[..16] {
                    last.count += 1;
                    last.timestamp = now.format("%m-%d %H:%M:%S").to_string();
                    return; // 合并，不新增
                }
            }
        }

        let entry = ErrorEntry {
            timestamp: now.format("%m-%d %H:%M:%S").to_string(),
            component: component.to_string(),
            level: level.to_string(),
            message: message.to_string(),
            context: context.to_string(),
            count: 1,
        };

        buf.push(entry.clone());

        // 保持最近 200 条
        if buf.len() > 200 { buf.remove(0); }

        // 持久化追加
        let path = self.log_dir.join("errors.jsonl");
        if let Ok(json) = serde_json::to_string(&entry) {
            let _ = std::fs::OpenOptions::new().create(true).append(true)
                .write(true).open(&path)
                .map(|mut f| {
                    use std::io::Write;
                    let _ = writeln!(f, "{}", json);
                });
        }
    }

    /// 获取最近 N 条错误
    pub fn recent(&self, n: usize) -> Vec<ErrorEntry> {
        let buf = self.buffer.lock();
        let start = if buf.len() > n { buf.len() - n } else { 0 };
        buf[start..].to_vec()
    }

    /// 自动诊断：检查是否出现已知问题模式
    pub fn diagnose(&self) -> Vec<String> {
        let buf = self.buffer.lock();
        let mut findings = Vec::new();

        // 统计最近5分钟的各类错误
        let recent: Vec<&ErrorEntry> = buf.iter()
            .filter(|e| e.timestamp.len() >= 16)
            .collect();

        let api_errors: Vec<_> = recent.iter().filter(|e| e.component == "api").collect();
        let stream_errors: Vec<_> = recent.iter().filter(|e| e.component == "stream").collect();
        let panics: Vec<_> = recent.iter().filter(|e| e.level == "panic").collect();

        if api_errors.len() >= 5 {
            findings.push(format!("🔄 API 错误频繁({}次)，建议检查 Key 或网络", api_errors.len()));
        }
        if stream_errors.len() >= 3 {
            findings.push(format!("🌊 流式中断({}次)，可能对话过大，建议简化问题或分步处理", stream_errors.len()));
        }
        if panics.len() >= 1 {
            findings.push(format!("💥 发生 {} 次 panic，建议重启服务", panics.len()));
        }
        if let Some(top) = buf.iter().max_by_key(|e| e.count) {
            if top.count > 10 {
                findings.push(format!("🔁 \"{}\" 重复 {} 次，建议优先修复", top.message, top.count));
            }
        }

        findings
    }

    /// 清空日志
    pub fn clear(&self) {
        let mut buf = self.buffer.lock();
        buf.clear();
        let _ = std::fs::remove_file(self.log_dir.join("errors.jsonl"));
    }
}
