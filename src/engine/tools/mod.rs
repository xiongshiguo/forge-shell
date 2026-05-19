//! 沙箱化工具系统：执行、读、写、搜索、Git 操作
//!
//! 安全设计：
//! - 所有文件操作限制在工作目录内
//! - 命令执行走白名单机制
//! - Git 操作仅允许安全子集
//! - 写入操作先备份

pub mod backup;
mod sandbox;

pub use sandbox::Sandbox;

use crate::error::ForgeError;
use std::path::{Path, PathBuf};
use std::process::Output;

/// 工具类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    /// 执行 shell 命令
    Bash,
    /// 读取文件
    Read,
    /// 写入文件
    Write,
    /// 搜索代码
    Search,
    /// Git 操作
    Git,
}

impl ToolType {
    pub fn name(&self) -> &str {
        match self {
            ToolType::Bash => "执行",
            ToolType::Read => "读取",
            ToolType::Write => "写入",
            ToolType::Search => "搜索",
            ToolType::Git => "Git",
        }
    }
}

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub tool_type: ToolType,
}

/// 搜索匹配
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub line_content: String,
    pub column_start: usize,
    pub column_end: usize,
}

/// 工具执行器
pub struct ToolExecutor {
    work_dir: PathBuf,
    sandbox: Sandbox,
    /// 允许的命令白名单
    allowed_commands: Vec<String>,
    /// 备份目录
    backup_dir: PathBuf,
}

impl ToolExecutor {
    pub fn new(work_dir: PathBuf) -> Self {
        let backup_dir = work_dir.join(".forge-backups");
        std::fs::create_dir_all(&backup_dir).ok();

        Self {
            work_dir,
            sandbox: Sandbox::new(),
            allowed_commands: vec![
                "cargo".into(), "git".into(), "ls".into(), "find".into(),
                "grep".into(), "cat".into(), "head".into(), "tail".into(),
                "wc".into(), "diff".into(), "sort".into(), "uniq".into(),
                "npm".into(), "pnpm".into(), "yarn".into(), "node".into(),
                "python".into(), "python3".into(), "pip".into(), "rustc".into(),
                "rustfmt".into(), "clippy".into(), "go".into(), "make".into(),
                "mkdir".into(), "touch".into(), "cp".into(), "mv".into(),
                "echo".into(), "printf".into(),
            ],
            backup_dir,
        }
    }

    /// 执行工具
    pub async fn execute(&self, tool: ToolType, input: &str) -> ToolResult {
        let start = std::time::Instant::now();
        let result = match tool {
            ToolType::Bash => self.exec_bash(input).await,
            ToolType::Read => self.exec_read(input).await,
            ToolType::Write => self.exec_write(input).await,
            ToolType::Search => self.exec_search(input).await,
            ToolType::Git => self.exec_git(input).await,
        };
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => ToolResult {
                success: true,
                output,
                error: None,
                duration_ms,
                tool_type: tool,
            },
            Err(e) => ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
                duration_ms,
                tool_type: tool,
            },
        }
    }

    /// 执行 shell 命令（白名单 + 沙箱）
    async fn exec_bash(&self, command: &str) -> Result<String, ForgeError> {
        // 检查命令白名单
        let cmd_name = command.split_whitespace().next().unwrap_or("");
        let cmd_base = Path::new(cmd_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(cmd_name);

        if !self.allowed_commands.iter().any(|c| c == cmd_base) {
            return Err(ForgeError::Sandbox(format!(
                "命令 '{}' 不在白名单中", cmd_base
            )));
        }

        // 沙箱检查
        self.sandbox.validate_command(command)?;

        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .current_dir(&self.work_dir)
            .output()
            .await
            .map_err(|e| ForgeError::Tool(format!("命令执行失败: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok(format!("[退出码: {}]\n{}", output.status, stderr))
        }
    }

    /// 读取文件
    async fn exec_read(&self, path: &str) -> Result<String, ForgeError> {
        let full_path = self.resolve_path(path)?;
        self.validate_path_in_workspace(&full_path)?;

        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| ForgeError::Tool(format!("读取文件失败: {}", e)))?;

        // 行号格式化
        let numbered: Vec<String> = content.lines()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", i + 1, line))
            .collect();

        Ok(numbered.join("\n"))
    }

    /// 写入文件（含自动备份）
    async fn exec_write(&self, input: &str) -> Result<String, ForgeError> {
        // 格式: path\ncontent
        let parts: Vec<&str> = input.splitn(2, '\n').collect();
        let path = parts.first().unwrap_or(&"");
        let content = parts.get(1).unwrap_or(&"");

        let full_path = self.resolve_path(path)?;
        self.validate_path_in_workspace(&full_path)?;

        // 备份原文件
        if full_path.exists() {
            let backup_name = format!(
                "{}_{}.bak",
                full_path.file_name().unwrap().to_string_lossy(),
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            );
            tokio::fs::copy(&full_path, self.backup_dir.join(backup_name))
                .await
                .map_err(|e| ForgeError::Tool(format!("备份失败: {}", e)))?;
        }

        // 确保父目录存在
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ForgeError::Tool(format!("创建目录失败: {}", e)))?;
        }

        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| ForgeError::Tool(format!("写入文件失败: {}", e)))?;

        let size = content.len();
        Ok(format!("已写入 {} ({} 字节，已自动备份)", full_path.display(), size))
    }

    /// 搜索代码
    async fn exec_search(&self, pattern: &str) -> Result<String, ForgeError> {
        let matches = self.search_files(pattern).await?;

        if matches.is_empty() {
            return Ok("未找到匹配项".into());
        }

        let output: Vec<String> = matches.iter().map(|m| {
            format!(
                "{}:{}:{}\t{}",
                m.file_path.display(),
                m.line_number,
                m.column_start,
                m.line_content.trim()
            )
        }).collect();

        Ok(output.join("\n"))
    }

    /// Git 操作（仅安全子集）
    async fn exec_git(&self, command: &str) -> Result<String, ForgeError> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let sub = parts.first().unwrap_or(&"");

        // 仅允许安全的 Git 子命令
        let safe_commands = [
            "status", "diff", "log", "branch", "add", "commit",
            "stash", "show", "blame",
        ];

        if !safe_commands.contains(sub) {
            return Err(ForgeError::Sandbox(format!(
                "Git 子命令 '{}' 不在允许列表中（仅允许只读及安全的写操作）", sub
            )));
        }

        let output = tokio::process::Command::new("git")
            .args(parts)
            .current_dir(&self.work_dir)
            .output()
            .await
            .map_err(|e| ForgeError::Tool(format!("Git 命令失败: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(format!("{}{}", stdout, stderr))
    }

    /// 搜索文件内容
    async fn search_files(&self, pattern: &str) -> Result<Vec<SearchMatch>, ForgeError> {
        let mut results = Vec::new();
        let regex = regex::Regex::new(pattern)
            .map_err(|e| ForgeError::Tool(format!("无效的正则表达式: {}", e)))?;

        self.search_dir(&self.work_dir, &regex, &mut results).await?;
        Ok(results)
    }

    async fn search_dir(
        &self,
        dir: &Path,
        regex: &regex::Regex,
        results: &mut Vec<SearchMatch>,
    ) -> Result<(), ForgeError> {
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| ForgeError::Tool(format!("读取目录失败: {}", e)))?;

        let ignore_dirs = ["target", "node_modules", ".git", "__pycache__", ".ai", ".forge-backups"];

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();

            if name.starts_with('.') && name != "." && name != ".." {
                continue;
            }
            if ignore_dirs.contains(&name.as_ref()) {
                continue;
            }

            if path.is_dir() {
                Box::pin(self.search_dir(&path, regex, results)).await?;
            } else if path.is_file() {
                // 仅搜索文本文件（限制大小 < 1MB）
                if let Ok(meta) = tokio::fs::metadata(&path).await {
                    if meta.len() > 1024 * 1024 { continue; }
                }
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    for (line_num, line) in content.lines().enumerate() {
                        if let Some(m) = regex.find(line) {
                            results.push(SearchMatch {
                                file_path: path.clone(),
                                line_number: line_num + 1,
                                line_content: line.to_string(),
                                column_start: m.start(),
                                column_end: m.end(),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 解析相对路径为绝对路径
    fn resolve_path(&self, path: &str) -> Result<PathBuf, ForgeError> {
        let p = Path::new(path.trim());
        if p.is_absolute() {
            Ok(p.to_path_buf())
        } else {
            Ok(self.work_dir.join(p))
        }
    }

    /// 验证路径在工作目录内（防穿透）
    fn validate_path_in_workspace(&self, path: &Path) -> Result<(), ForgeError> {
        let canonical = path.canonicalize()
            .map_err(|_| ForgeError::Sandbox(format!(
                "无法解析路径: {}", path.display()
            )))?;

        let work_canon = self.work_dir.canonicalize()
            .unwrap_or_else(|_| self.work_dir.clone());

        if !canonical.starts_with(&work_canon) {
            return Err(ForgeError::Sandbox(format!(
                "路径 '{}' 不在工作目录内，操作被拒绝", path.display()
            )));
        }
        Ok(())
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}
