//! F2 费用看板
//! 显示：Token 用量、实时费用、缓存节省、与 Claude Code 对比、月度预算预警

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};

/// 费用统计
#[derive(Debug, Clone, Default)]
pub struct CostData {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cache_hit_tokens: u64,
    pub total_cost_yuan: f64,
    pub cache_saved_yuan: f64,
    /// 与 Claude Code 估算费用对比
    pub claude_estimated_yuan: f64,
    /// 月度预算（元）
    pub monthly_budget: f64,
    /// 本月已用（元）
    pub monthly_used: f64,
}

impl CostData {
    pub fn cache_savings_pct(&self) -> f64 {
        if self.total_cost_yuan + self.cache_saved_yuan > 0.0 {
            self.cache_saved_yuan / (self.total_cost_yuan + self.cache_saved_yuan) * 100.0
        } else {
            0.0
        }
    }

    pub fn vs_claude_savings_pct(&self) -> f64 {
        if self.claude_estimated_yuan > 0.0 {
            (self.claude_estimated_yuan - self.total_cost_yuan) / self.claude_estimated_yuan * 100.0
        } else {
            0.0
        }
    }

    pub fn budget_pct(&self) -> f64 {
        if self.monthly_budget > 0.0 {
            (self.monthly_used / self.monthly_budget * 100.0).min(100.0)
        } else {
            0.0
        }
    }
}

/// 渲染费用看板
pub fn render(f: &mut Frame, area: Rect, data: &CostData, hit_rate: f64) {
    let block = Block::default()
        .title(" F2 费用看板 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let mut lines: Vec<Line> = Vec::new();

    // 费用概览
    lines.push(Line::from(vec![
        Span::styled("💰 累计费用: ", Style::default()),
        Span::styled(
            crate::utils::format_cost(data.total_cost_yuan),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // Token 用量
    lines.push(Line::from(vec![
        Span::styled("📊 Token 用量", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(format!("  输入 Token:  {}", data.prompt_tokens)));
    lines.push(Line::from(format!("  输出 Token:  {}", data.completion_tokens)));
    lines.push(Line::from(format!("  总计 Token:  {}", data.total_tokens)));
    lines.push(Line::from(""));

    // 缓存节省
    lines.push(Line::from(vec![
        Span::styled("💾 缓存节省", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(format!("  命中 Token:  {}", data.cache_hit_tokens)));
    lines.push(Line::from(format!("  缓存命中率: {:.1}%", hit_rate * 100.0)));
    lines.push(Line::from(format!(
        "  节省费用:   {}",
        crate::utils::format_cost(data.cache_saved_yuan)
    )));
    lines.push(Line::from(format!(
        "  节省比例:   {:.1}%",
        data.cache_savings_pct()
    )));
    lines.push(Line::from(""));

    // 与 Claude Code 对比
    lines.push(Line::from(vec![
        Span::styled("🆚 与 Claude Code 对比", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(format!(
        "  熔炉费用:   {}",
        crate::utils::format_cost(data.total_cost_yuan)
    )));
    lines.push(Line::from(format!(
        "  Claude 估算: {}",
        crate::utils::format_cost(data.claude_estimated_yuan)
    )));
    let savings = data.vs_claude_savings_pct();
    let savings_style = if savings > 50.0 {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Yellow)
    };
    lines.push(Line::from(vec![
        Span::raw("  节省:       "),
        Span::styled(format!("{:.1}%", savings), savings_style.add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(""));

    // 月度预算
    let budget_pct = data.budget_pct();
    let budget_color = if budget_pct > 90.0 {
        Color::Red
    } else if budget_pct > 70.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    lines.push(Line::from(vec![
        Span::styled("📅 月度预算", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(format!(
        "  预算: {} | 已用: {} | 剩余: {}",
        crate::utils::format_cost(data.monthly_budget),
        crate::utils::format_cost(data.monthly_used),
        crate::utils::format_cost((data.monthly_budget - data.monthly_used).max(0.0)),
    )));

    // 预算进度条
    let gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(budget_color))
        .percent(budget_pct as u16)
        .label(format!("{:.1}%", budget_pct));
    f.render_widget(gauge, Rect::new(area.x + 2, area.y + 17, area.width.saturating_sub(4), 1));

    let panel = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(panel, area);
}
