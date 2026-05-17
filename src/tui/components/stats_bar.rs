//! 底部状态栏组件
//! 显示格式: 💰 ¥0.0032 | 命中: 94% | 并行: 2/8 | 内存: 15MB

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// 渲染底部状态栏
pub fn render(
    f: &mut Frame,
    area: Rect,
    cost: &str,
    hit_rate: &str,
    parallel: &str,
    memory: &str,
) {
    let segments = vec![
        Span::styled("💰 ", Style::default().fg(Color::Yellow)),
        Span::raw(cost),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled("命中: ", Style::default().fg(Color::DarkGray)),
        Span::styled(hit_rate, Style::default().fg(Color::Green)),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled("并行: ", Style::default().fg(Color::DarkGray)),
        Span::styled(parallel, Style::default().fg(Color::Cyan)),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled("内存: ", Style::default().fg(Color::DarkGray)),
        Span::styled(memory, Style::default().fg(Color::Magenta)),
    ];

    let line = Line::from(segments);
    let para = Paragraph::new(line);
    f.render_widget(para, area);
}
