//! 差异对比视图 — 使用 similar 库实现代码 diff 展示

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use similar::{ChangeTag, TextDiff};

/// 渲染差异对比视图
pub fn render(f: &mut Frame, area: Rect, old_text: &str, new_text: &str, title: &str) {
    let block = Block::default()
        .title(format!(" 差异对比 — {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let diff = TextDiff::from_lines(old_text, new_text);
    let mut lines: Vec<Line> = Vec::new();

    for change in diff.iter_all_changes() {
        let (sign, style) = match change.tag() {
            ChangeTag::Delete => ("-", Style::default().fg(Color::Red)),
            ChangeTag::Insert => ("+", Style::default().fg(Color::Green)),
            ChangeTag::Equal => (" ", Style::default().fg(Color::DarkGray)),
        };

        let line_num = if let ChangeTag::Delete = change.tag() {
            change.old_index().map(|i| i + 1)
        } else {
            change.new_index().map(|i| i + 1)
        };

        let num_text = line_num.map(|n| format!("{:>4}", n)).unwrap_or_else(|| "    ".into());

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", sign), style),
            Span::styled(num_text, Style::default().fg(Color::DarkGray)),
            Span::styled(" ", style),
            Span::styled(change.value().to_string(), style),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from("无差异"));
    }

    let panel = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    f.render_widget(panel, area);
}

/// 并排差异（左侧旧，右侧新）
pub fn render_side_by_side(
    f: &mut Frame,
    area: Rect,
    old_text: &str,
    new_text: &str,
    left_title: &str,
    right_title: &str,
) {
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(50),
            ratatui::layout::Constraint::Percentage(50),
        ])
        .split(area);

    let left_block = Block::default()
        .title(format!(" {} ", left_title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let right_block = Block::default()
        .title(format!(" {} ", right_title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let diff = TextDiff::from_lines(old_text, new_text);

    let mut left_lines: Vec<Line> = Vec::new();
    let mut right_lines: Vec<Line> = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => {
                let num = change.old_index().map(|i| i + 1).unwrap_or(0);
                left_lines.push(Line::from(vec![
                    Span::styled(format!("- {:>4} ", num), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::styled(change.value().to_string(), Style::default().fg(Color::Red)),
                ]));
            }
            ChangeTag::Insert => {
                let num = change.new_index().map(|i| i + 1).unwrap_or(0);
                right_lines.push(Line::from(vec![
                    Span::styled(format!("+ {:>4} ", num), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::styled(change.value().to_string(), Style::default().fg(Color::Green)),
                ]));
            }
            ChangeTag::Equal => {
                let num = change.new_index().map(|i| i + 1).unwrap_or(0);
                let text = format!("  {:>4} {}", num, change.value());
                left_lines.push(Line::from(Span::styled(text.clone(), Style::default().fg(Color::DarkGray))));
                right_lines.push(Line::from(Span::styled(text, Style::default().fg(Color::DarkGray))));
            }
        }
    }

    let left_para = Paragraph::new(left_lines).block(left_block).wrap(Wrap { trim: false });
    let right_para = Paragraph::new(right_lines).block(right_block).wrap(Wrap { trim: false });

    f.render_widget(left_para, chunks[0]);
    f.render_widget(right_para, chunks[1]);
}
