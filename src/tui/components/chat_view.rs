//! 对话视图组件 — 显示聊天历史，支持滚动

use crate::tui::app::{Message, Role};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// 渲染对话视图
pub fn render(
    f: &mut Frame,
    area: Rect,
    messages: &[Message],
    scroll_offset: usize,
    is_streaming: bool,
    streaming_buffer: &str,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 对话 ");

    let mut lines: Vec<Line> = Vec::new();

    // 跳过滚动偏移量的消息
    let visible_start = if messages.len() > scroll_offset {
        messages.len() - scroll_offset - 1
    } else {
        0
    };

    for msg in messages.iter().skip(visible_start) {
        let (role_text, role_style) = match msg.role {
            Role::System => ("系统", Style::default().fg(Color::DarkGray)),
            Role::User => ("你", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Role::Assistant => ("熔炉", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        };

        // 角色标签
        lines.push(Line::from(vec![
            Span::styled(format!("[{}] ", role_text), role_style),
        ]));

        // 消息内容（按换行拆分）
        for line_text in msg.content.lines() {
            lines.push(Line::from(Span::raw(format!("  {}", line_text))));
        }
        // 消息间空行
        lines.push(Line::from(""));
    }

    // 流式输出缓冲区
    if is_streaming && !streaming_buffer.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("[熔炉] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(streaming_buffer, Style::default().fg(Color::White)),
            Span::styled("▊", Style::default().fg(Color::Green)),
        ]));
    }

    let chat = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((0, 0));

    f.render_widget(chat, area);
}
