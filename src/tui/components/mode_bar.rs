//! 模式栏组件 — 显示三种模式标签，高亮当前模式

use crate::tui::app::Mode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::Tabs,
};

/// 渲染模式栏
pub fn render(f: &mut Frame, area: Rect, current_mode: Mode) {
    let modes = [Mode::Plan, Mode::Assist, Mode::Speed];
    let labels: Vec<Span> = modes
        .iter()
        .map(|m| {
            let label = format!(" {} {} ", m.shortcut(), m.name());
            if *m == current_mode {
                Span::styled(
                    label,
                    Style::default()
                        .fg(Color::Black)
                        .bg(m.color())
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(label, Style::default().fg(Color::DarkGray))
            }
        })
        .collect();

    let tabs = Tabs::new(labels)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .select(match current_mode {
            Mode::Plan => 0,
            Mode::Assist => 1,
            Mode::Speed => 2,
        })
        .divider("│");

    f.render_widget(tabs, area);
}
