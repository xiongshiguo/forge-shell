//! Ctrl+Shift+C 社区大厅
//! 包含：经验熔池、天工阁、悬赏榜、锻师会动态

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
};

/// 社区大厅标签页
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityTab {
    /// 经验熔池
    Pool,
    /// 天工阁 (SOP)
    Sop,
    /// 悬赏榜
    Bounty,
    /// 锻师会
    Forge,
}

impl CommunityTab {
    pub fn name(&self) -> &str {
        match self {
            CommunityTab::Pool => "经验熔池",
            CommunityTab::Sop => "天工阁",
            CommunityTab::Bounty => "悬赏榜",
            CommunityTab::Forge => "锻师会",
        }
    }

    pub fn all() -> Vec<CommunityTab> {
        vec![
            CommunityTab::Pool,
            CommunityTab::Sop,
            CommunityTab::Bounty,
            CommunityTab::Forge,
        ]
    }
}

/// 渲染社区大厅
pub fn render(
    f: &mut Frame,
    area: Rect,
    active_tab: CommunityTab,
    _on_tab_select: impl Fn(CommunityTab),
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),   // Tab 栏
            Constraint::Min(0),      // 内容
        ])
        .split(area);

    // Tab 栏
    let tabs: Vec<Span> = CommunityTab::all()
        .iter()
        .map(|t| {
            let label = format!(" {} ", t.name());
            if *t == active_tab {
                Span::styled(label, Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(label, Style::default().fg(Color::DarkGray))
            }
        })
        .collect();

    let tabs_widget = Tabs::new(tabs)
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::Magenta)))
        .select(match active_tab {
            CommunityTab::Pool => 0,
            CommunityTab::Sop => 1,
            CommunityTab::Bounty => 2,
            CommunityTab::Forge => 3,
        })
        .divider("│");

    f.render_widget(tabs_widget, chunks[0]);

    // 内容区
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(format!(" {} ", active_tab.name()));

    let content = match active_tab {
        CommunityTab::Pool => render_pool(),
        CommunityTab::Sop => render_sop(),
        CommunityTab::Bounty => render_bounty(),
        CommunityTab::Forge => render_forge(),
    };

    let panel = Paragraph::new(content).block(content_block).wrap(Wrap { trim: true });
    f.render_widget(panel, chunks[1]);
}

fn render_pool() -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("🔥 经验熔池", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from("汇聚社区用户的脱敏复盘经验，由反思引擎每周提炼。"),
        Line::from(""),
        Line::from("本周热门策略："),
        Line::from("  1. 「模块化拆解」被 42 人使用，平均效率提升 30%"),
        Line::from("  2. 「先分析再修改」被 38 人使用，错误率降低 50%"),
        Line::from("  3. 「并行搜索」被 35 人使用，耗时缩短 60%"),
        Line::from(""),
        Line::from("你的贡献："),
        Line::from("  • 已分享 0 次复盘"),
        Line::from("  • 策略被采纳 0 次"),
        Line::from(""),
        Line::from(Span::styled(
            "按 Ctrl+S 分享你的当前复盘到经验熔池",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

fn render_sop() -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("🏗️ 天工阁（SOP 库）", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from("标准操作流程 (SOP) 库，由反思引擎自动提炼或锻师手动创建。"),
        Line::from(""),
        Line::from("热门 SOP："),
        Line::from("  📋 Rust 错误处理最佳实践 (v1.2)"),
        Line::from("  📋 API 接口重构流程 (v1.0)"),
        Line::from("  📋 数据库迁移安全清单 (v2.1)"),
        Line::from("  📋 TUI 组件开发规范 (v1.0)"),
        Line::from("  📋 性能基准测试指南 (v1.5)"),
        Line::from(""),
        Line::from("本周新增 3 条 SOP，更新 12 条"),
        Line::from(""),
        Line::from(Span::styled(
            "成为锻师后可创建和维护 SOP",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

fn render_bounty() -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("💰 悬赏榜", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from("欢迎发布悬赏任务推动项目发展！"),
        Line::from(""),
        Line::from("进行中的悬赏："),
        Line::from("  🟡 [¥200] 实现 Windows 托盘图标支持 — 0 人竞标"),
        Line::from("  🟡 [¥500] PaddleOCR 集成优化 — 1 人竞标 (锻师@张三)"),
        Line::from("  🟢 [¥100] 完善中文错误提示 — 2 人竞标"),
        Line::from("  🔴 [¥1000] 鸿蒙原生编译支持 — 0 人竞标"),
        Line::from(""),
        Line::from("规则："),
        Line::from("  • ¥500 以下免服务费，¥500 以上统一 5%"),
        Line::from("  • 锻师会审核完成后赏金自动发放"),
        Line::from(""),
        Line::from(Span::styled(
            "发布悬赏请访问 forge-shell 社区主页",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

fn render_forge() -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            Span::styled("⚒️ 锻师会", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from("项目核心贡献者治理体系"),
        Line::from(""),
        Line::from("层级："),
        Line::from("  👑 大锻师 (1人) — 最终决策权"),
        Line::from("  ⚔️ 锻师 (0人) — PR 审核、SOP 维护"),
        Line::from("  📖 学徒 (0人) — 贡献者"),
        Line::from(""),
        Line::from("近期动态："),
        Line::from("  • 项目启动，等待首批贡献者加入"),
        Line::from("  • 治理章程已发布 (GOVERNANCE.md)"),
        Line::from(""),
        Line::from("荣誉体系："),
        Line::from("  🏅 初识进化 → 🔥 锻火之心 → ⚡ 千锤百炼 → 👑 不朽锻匠"),
        Line::from(""),
        Line::from(Span::styled(
            "绑定 Gitee/GitHub 账号加入锻师会",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}
