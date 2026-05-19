//! 文件备份与回滚引擎
//! 写前自动备份 → 测试失败可一键回滚

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 一次修改操作的备份记录
#[derive(Debug, Clone)]
pub struct BackupEntry {
    /// 原文件路径
    pub original_path: PathBuf,
    /// 备份路径
    pub backup_path: PathBuf,
    /// 操作描述
    pub description: String,
    /// 时间戳
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// 备份管理器
pub struct BackupManager {
    /// 当前会话的备份记录
    session_backups: Vec<BackupEntry>,
    /// 备份目录
    backup_dir: PathBuf,
}

impl BackupManager {
    pub fn new(backup_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&backup_dir).ok();
        Self {
            session_backups: Vec::new(),
            backup_dir,
        }
    }

    /// 写前备份：复制原文件到备份目录
    pub fn backup_before_write(&mut self, file_path: &Path, description: &str) -> std::io::Result<()> {
        if !file_path.exists() {
            return Ok(()); // 新文件，无需备份
        }

        let timestamp = chrono::Utc::now();
        let backup_name = format!(
            "{}_{}.bak",
            file_path.file_name().unwrap_or_default().to_string_lossy(),
            timestamp.format("%Y%m%d_%H%M%S")
        );
        let backup_path = self.backup_dir.join(&backup_name);
        std::fs::copy(file_path, &backup_path)?;

        self.session_backups.push(BackupEntry {
            original_path: file_path.to_path_buf(),
            backup_path,
            description: description.to_string(),
            timestamp,
        });

        tracing::info!("已备份: {} → {}", file_path.display(), backup_name);
        Ok(())
    }

    /// 回滚指定文件的最后一次备份
    pub fn rollback_file(&self, file_path: &Path) -> std::io::Result<bool> {
        let entry = self.session_backups
            .iter()
            .rev()
            .find(|e| e.original_path == file_path);

        if let Some(entry) = entry {
            std::fs::copy(&entry.backup_path, &entry.original_path)?;
            tracing::info!("已回滚: {} ← {}", file_path.display(), entry.backup_path.display());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 回滚当前会话所有修改
    pub fn rollback_all(&self) -> usize {
        let mut count = 0;
        for entry in self.session_backups.iter().rev() {
            if std::fs::copy(&entry.backup_path, &entry.original_path).is_ok() {
                count += 1;
            }
        }
        tracing::info!("已回滚 {} 个文件", count);
        count
    }

    /// 获取当前会话的备份列表
    pub fn session_backups(&self) -> &[BackupEntry] {
        &self.session_backups
    }

    /// 清理备份目录中的旧文件（保留最近 50 个）
    pub fn cleanup(&self) {
        let mut backups: Vec<_> = std::fs::read_dir(&self.backup_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some((e.path(), meta.modified().ok()?))
            })
            .collect();
        backups.sort_by_key(|(_, t)| std::cmp::Reverse(*t));
        for (path, _) in backups.iter().skip(50) {
            std::fs::remove_file(path).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_new_file_noop() {
        let dir = std::env::temp_dir().join("forge_test_backup");
        let mut mgr = BackupManager::new(dir);
        let result = mgr.backup_before_write(Path::new("/nonexistent/forge_test_xyz.rs"), "test");
        assert!(result.is_ok());
        assert!(mgr.session_backups().is_empty());
    }

    #[test]
    fn test_rollback_nonexistent_file() {
        let dir = std::env::temp_dir().join("forge_test_backup2");
        let mgr = BackupManager::new(dir);
        assert_eq!(mgr.rollback_file(Path::new("/nonexistent")).unwrap(), false);
    }
}
