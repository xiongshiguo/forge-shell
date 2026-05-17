//! F1 项目监控面板
//! 显示：文件数、代码行、最近提交、测试状态、性能基线

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::path::Path;
use std::process::Command;

/// 项目统计信息
#[derive(Debug, Clone, Default)]
pub struct ProjectStats {
    pub file_count: usize,
    pub total_lines: usize,
    pub rust_files: usize,
    pub test_files: usize,
    pub recent_commits: Vec<CommitInfo>,
    pub last_test_status: Option<String>,
    pub build_status: Option<String>,
    pub project_name: String,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

/// 收集项目统计信息
pub fn gather_stats(work_dir: &Path) -> ProjectStats {
    let mut stats = ProjectStats::default();

    // 项目名
    if let Some(name) = work_dir.file_name().and_then(|n| n.to_str()) {
        stats.project_name = name.to_string();
    }

    // 统计文件
    count_files(work_dir, &mut stats);

    // Git 提交历史
    if let Ok(output) = Command::new("git")
        .args(["log", "--oneline", "-10", "--format=%h|%s|%an|%ar"])
        .current_dir(work_dir)
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines().take(10) {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() >= 4 {
                stats.recent_commits.push(CommitInfo {
                    hash: parts[0].to_string(),
                    message: parts[1].to_string(),
                    author: parts[2].to_string(),
                    date: parts[3].to_string(),
                });
            }
        }
    }

    // 测试状态
    if let Ok(output) = Command::new("cargo")
        .args(["test", "--no-run"])
        .current_dir(work_dir)
        .output()
    {
        if output.status.success() {
            stats.build_status = Some("✓ 编译通过".into());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            stats.build_status = Some(format!("✗ 编译失败: {}", stderr.lines().last().unwrap_or("")));
        }
    }

    stats
}

fn count_files(dir: &Path, stats: &mut ProjectStats) {
    let ignore = ["target", "node_modules", ".git", "__pycache__", ".ai", ".forge-backups"];
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if ignore.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                count_files(&path, stats);
            } else if path.is_file() {
                stats.file_count += 1;
                if name.ends_with(".rs") {
                    stats.rust_files += 1;
                }
                if name.contains("test") || path.to_string_lossy().contains("tests/") {
                    stats.test_files += 1;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    stats.total_lines += content.lines().count();
                }
            }
        }
    }
}

/// 渲染项目监控面板
pub fn render(f: &mut Frame, area: Rect, stats: &ProjectStats) {
    let block = Block::default()
        .title(" F1 项目监控 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let mut lines: Vec<Line> = Vec::new();

    // 标题
    lines.push(Line::from(vec![
        Span::styled("📁 ", Style::default()),
        Span::styled(&stats.project_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(""));

    // 统计
    lines.push(Line::from(format!("  文件数:     {}", stats.file_count)));
    lines.push(Line::from(format!("  代码行数:   {}", stats.total_lines)));
    lines.push(Line::from(format!("  Rust 文件:  {}", stats.rust_files)));
    lines.push(Line::from(format!("  测试文件:   {}", stats.test_files)));
    lines.push(Line::from(""));

    // 构建状态
    if let Some(ref status) = stats.build_status {
        let style = if status.starts_with('✓') {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Red)
        };
        lines.push(Line::from(vec![
            Span::raw("  构建状态: "),
            Span::styled(status.as_str(), style),
        ]));
        lines.push(Line::from(""));
    }

    // 最近提交
    if !stats.recent_commits.is_empty() {
        lines.push(Line::from(Span::styled(
            "  最近提交:",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )));
        for commit in stats.recent_commits.iter().take(5) {
            lines.push(Line::from(vec![
                Span::styled(format!("    {} ", commit.hash), Style::default().fg(Color::DarkGray)),
                Span::raw(crate::utils::truncate_with_ellipsis(&commit.message, 50)),
                Span::styled(
                    format!(" ({}, {})", commit.author, commit.date),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "按 F1 关闭此面板",
        Style::default().fg(Color::DarkGray),
    )));

    let panel = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(panel, area);
}
