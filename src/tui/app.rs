//! TUI 主应用 — 全中文界面，支持规划/助手/极速三种模式

use crate::config::Config;
use crate::locale::Locale;
use crate::error::ForgeError;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame,
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
};
use std::io;
use std::time::Instant;

use super::components::{chat_view, mode_bar, stats_bar};
use super::keybindings::{self, KeyEvent};

/// 应用模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 规划模式：只分析，不修改
    Plan,
    /// 助手模式：逐步执行，需确认
    Assist,
    /// 极速模式：自动执行，事后汇总
    Speed,
}

impl Mode {
    pub fn name(&self) -> &str {
        match self {
            Mode::Plan => "规划",
            Mode::Assist => "助手",
            Mode::Speed => "极速",
        }
    }

    pub fn desc(&self) -> &str {
        match self {
            Mode::Plan => "只分析，不修改",
            Mode::Assist => "逐步执行，需确认",
            Mode::Speed => "自动执行，事后汇总",
        }
    }

    pub fn shortcut(&self) -> &str {
        match self {
            Mode::Plan => "Ctrl+P",
            Mode::Assist => "Ctrl+A",
            Mode::Speed => "Ctrl+Y",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Mode::Plan => Color::Cyan,
            Mode::Assist => Color::Green,
            Mode::Speed => Color::Yellow,
        }
    }
}

/// 消息角色
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

/// 聊天消息
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub timestamp: Instant,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), timestamp: Instant::now() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), timestamp: Instant::now() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), timestamp: Instant::now() }
    }
}

/// 应用运行状态
enum RunState {
    Running,
    Quit,
}

/// TUI 主应用
pub struct App {
    config: Config,
    locale: Locale,
    mode: Mode,
    /// 输入缓冲区
    input: String,
    /// 聊天消息
    messages: Vec<Message>,
    /// 游标字符位置
    cursor_char_pos: usize,
    /// 累计费用（元）
    total_cost: f64,
    /// 缓存命中率
    cache_hit_rate: f64,
    /// 当前活跃子 Agent 数
    active_agents: usize,
    /// 最大并行数
    max_agents: usize,
    /// 当前内存使用（字节）
    memory_usage: u64,
    /// 启动时间
    start_time: Instant,
    /// 是否显示项目面板
    show_project_panel: bool,
    /// 是否显示费用面板
    show_cost_panel: bool,
    /// 是否显示社区面板
    show_community_panel: bool,
    /// 是否显示分享弹窗
    show_share_dialog: bool,
    /// 滚动偏移
    scroll_offset: usize,
    /// 流式响应缓冲区
    streaming_buffer: String,
    /// 是否正在流式响应
    is_streaming: bool,
}

impl App {
    pub fn new(config: Config) -> Result<Self, ForgeError> {
        let mode = match config.ui.default_mode.as_str() {
            "plan" => Mode::Plan,
            "speed" => Mode::Speed,
            _ => Mode::Assist,
        };
        let max_agents = config.engine.max_parallel_agents;

        let messages = vec![
            Message::system("🔥 熔炉已就绪。以意为炉，以语为锤，铸代码之剑。"),
            Message::system(format!(
                "当前模式：「{}」— {} | 输入指令开始，Ctrl+C 退出",
                mode.name(),
                mode.desc()
            )),
        ];

        Ok(Self {
            config,
            locale: Locale::default(),
            mode,
            input: String::new(),
            messages,
            cursor_char_pos: 0,
            total_cost: 0.0,
            cache_hit_rate: 0.0,
            active_agents: 0,
            max_agents,
            memory_usage: 0,
            start_time: Instant::now(),
            show_project_panel: false,
            show_cost_panel: false,
            show_community_panel: false,
            show_share_dialog: false,
            scroll_offset: 0,
            streaming_buffer: String::new(),
            is_streaming: false,
        })
    }

    /// 运行 TUI 主循环
    pub fn run(&mut self) -> Result<(), ForgeError> {
        enable_raw_mode().map_err(|e| ForgeError::Tui(e.to_string()))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| ForgeError::Tui(e.to_string()))?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)
            .map_err(|e| ForgeError::Tui(e.to_string()))?;

        let result = self.event_loop(&mut terminal);

        disable_raw_mode().map_err(|e| ForgeError::Tui(e.to_string()))?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .map_err(|e| ForgeError::Tui(e.to_string()))?;
        terminal.show_cursor()
            .map_err(|e| ForgeError::Tui(e.to_string()))?;

        result
    }

    fn event_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), ForgeError> {
        let mut state = RunState::Running;

        while matches!(state, RunState::Running) {
            // 更新内存使用
            self.memory_usage = self.estimate_memory();

            terminal
                .draw(|f| self.draw(f))
                .map_err(|e| ForgeError::Tui(e.to_string()))?;

            if let Event::Key(key) = event::read().map_err(|e| ForgeError::Tui(e.to_string()))? {
                if let Some(kb_event) = keybindings::map_key(key) {
                    match kb_event {
                        KeyEvent::ModePlan => self.switch_mode(Mode::Plan),
                        KeyEvent::ModeAssist => self.switch_mode(Mode::Assist),
                        KeyEvent::ModeSpeed => self.switch_mode(Mode::Speed),
                        KeyEvent::ProjectPanel => {
                            self.show_project_panel = !self.show_project_panel;
                            self.show_cost_panel = false;
                            self.show_community_panel = false;
                        }
                        KeyEvent::CostPanel => {
                            self.show_cost_panel = !self.show_cost_panel;
                            self.show_project_panel = false;
                            self.show_community_panel = false;
                        }
                        KeyEvent::CommunityPanel => {
                            self.show_community_panel = !self.show_community_panel;
                            self.show_project_panel = false;
                            self.show_cost_panel = false;
                        }
                        KeyEvent::ShareReview => {
                            self.show_share_dialog = !self.show_share_dialog;
                        }
                        KeyEvent::Quit => state = RunState::Quit,
                        KeyEvent::Enter => self.submit_input(),
                        KeyEvent::Backspace => {
                            if self.cursor_char_pos > 0 {
                                self.cursor_char_pos -= 1;
                                self.input.remove(self.cursor_char_pos);
                            }
                        }
                        KeyEvent::Char(c) => {
                            self.input.insert(self.cursor_char_pos, c);
                            self.cursor_char_pos += 1;
                        }
                        KeyEvent::ScrollUp => {
                            self.scroll_offset = self.scroll_offset.saturating_add(1);
                        }
                        KeyEvent::ScrollDown => {
                            self.scroll_offset = self.scroll_offset.saturating_sub(1);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 绘制界面
    fn draw(&self, f: &mut Frame) {
        let area = f.area();

        // 主布局：标题栏 → 内容区 → 底部状态栏
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),   // 标题/模式栏
                Constraint::Min(3),      // 内容区
                Constraint::Length(1),   // 底部状态栏
            ])
            .split(area);

        self.draw_title_bar(f, main_chunks[0]);
        self.draw_content(f, main_chunks[1]);
        self.draw_status_bar(f, main_chunks[2]);

        // 弹窗面板
        if self.show_share_dialog {
            self.draw_share_dialog(f, area);
        }
    }

    /// 绘制顶部标题栏 + 模式标签
    fn draw_title_bar(&self, f: &mut Frame, area: Rect) {
        let title_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),  // 标题
                Constraint::Min(0),      // 模式标签
                Constraint::Length(25),  // 快捷键提示
            ])
            .split(area);

        // 标题
        let title = Paragraph::new(Line::from(vec![
            Span::styled("熔炉", Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD)),
            Span::raw(" ForgeShell"),
        ]));
        f.render_widget(title, title_chunks[0]);

        // 模式标签
        mode_bar::render(f, title_chunks[1], self.mode);

        // 快捷键提示
        let hints = Line::from(vec![
            Span::styled("F1", Style::default().fg(Color::DarkGray)),
            Span::raw("项目 "),
            Span::styled("F2", Style::default().fg(Color::DarkGray)),
            Span::raw("费用 "),
            Span::styled("C-S-C", Style::default().fg(Color::DarkGray)),
            Span::raw("社区"),
        ]);
        let hints_p = Paragraph::new(hints).alignment(ratatui::layout::Alignment::Right);
        f.render_widget(hints_p, title_chunks[2]);
    }

    /// 绘制内容区
    fn draw_content(&self, f: &mut Frame, area: Rect) {
        if self.show_project_panel {
            self.draw_project_panel(f, area);
        } else if self.show_cost_panel {
            self.draw_cost_panel(f, area);
        } else if self.show_community_panel {
            self.draw_community_panel(f, area);
        } else {
            // 默认：对话视图
            let content_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),      // 聊天区域
                    Constraint::Length(3),   // 输入区域
                ])
                .split(area);

            self.draw_chat(f, content_chunks[0]);
            self.draw_input(f, content_chunks[1]);
        }
    }

    /// 绘制对话历史
    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        chat_view::render(f, area, &self.messages, self.scroll_offset, self.is_streaming, &self.streaming_buffer);
    }

    /// 绘制输入框
    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.mode.color()))
            .title(format!(" 输入指令（{}模式） ", self.mode.name()));

        let display_text = if self.input.is_empty() {
            vec![Line::from(Span::styled(
                "输入你的指令…",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            vec![Line::from(vec![
                Span::raw(&self.input),
                Span::styled("▊", Style::default().fg(self.mode.color())),
            ])]
        };

        let input_p = Paragraph::new(display_text)
            .block(input_block)
            .wrap(Wrap { trim: false });

        f.render_widget(input_p, area);
    }

    /// 绘制底部状态栏
    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let cost_text = crate::utils::format_cost(self.total_cost);
        let hit_text = crate::utils::format_percent(self.cache_hit_rate);
        let parallel_text = format!("{}/{}", self.active_agents, self.max_agents);
        let mem_text = crate::utils::format_bytes(self.memory_usage);

        stats_bar::render(f, area, &cost_text, &hit_text, &parallel_text, &mem_text);
    }

    /// 绘制项目监控面板
    fn draw_project_panel(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" F1 项目监控 ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let info = vec![
            Line::from("📁 项目监控面板"),
            Line::from(""),
            Line::from("此面板将在阶段 4 完善，显示："),
            Line::from("  • 文件数与代码行数"),
            Line::from("  • 最近提交记录"),
            Line::from("  • 测试状态"),
            Line::from("  • 性能基线"),
        ];

        let panel = Paragraph::new(info).block(block);
        f.render_widget(panel, area);
    }

    /// 绘制费用看板
    fn draw_cost_panel(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" F2 费用看板 ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        let info = vec![
            Line::from("💰 费用看板"),
            Line::from(""),
            Line::from(format!("  累计费用: {}", crate::utils::format_cost(self.total_cost))),
            Line::from(format!("  缓存命中: {}", crate::utils::format_percent(self.cache_hit_rate))),
            Line::from(""),
            Line::from("此面板将在阶段 4 完善，显示："),
            Line::from("  • Token 用量明细"),
            Line::from("  • 缓存节省金额"),
            Line::from("  • 与 Claude Code 对比"),
            Line::from("  • 月度预算预警"),
        ];

        let panel = Paragraph::new(info).block(block);
        f.render_widget(panel, area);
    }

    /// 绘制社区大厅
    fn draw_community_panel(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" 社区大厅 ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));

        let info = vec![
            Line::from("🌐 社区大厅"),
            Line::from(""),
            Line::from("此面板将在阶段 4 完善，包含："),
            Line::from("  • 经验熔池"),
            Line::from("  • 天工阁 (SOP)"),
            Line::from("  • 悬赏榜"),
            Line::from("  • 锻师会动态"),
        ];

        let panel = Paragraph::new(info).block(block);
        f.render_widget(panel, area);
    }

    /// 绘制分享弹窗
    fn draw_share_dialog(&self, f: &mut Frame, area: Rect) {
        let dialog_area = centered_rect(60, 40, area);

        let block = Block::default()
            .title(" 复盘分享 (Ctrl+S 关闭) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let lines = vec![
            Line::from("📤 分享复盘到经验熔池"),
            Line::from(""),
            Line::from("以下内容将被上传（脱敏后）："),
            Line::from("  • 任务策略和决策逻辑"),
            Line::from("  • 工具调用时序"),
            Line::from("  • 成功/失败模式"),
            Line::from(""),
            Line::from(Span::styled(
                "以下内容绝不会上传：",
                Style::default().fg(Color::Red),
            )),
            Line::from("  • 源代码、文件路径、变量名"),
            Line::from("  • API Key 或任何密钥"),
            Line::from("  • 个人身份信息"),
            Line::from(""),
            Line::from("按 Enter 确认分享，Esc 取消"),
        ];

        // 先清空弹窗区域再绘制内容
        let clear_block = Block::default().style(Style::default().bg(Color::Black));
        f.render_widget(clear_block, dialog_area);
        let panel = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
        f.render_widget(panel, dialog_area);
    }

    /// 处理用户提交
    fn submit_input(&mut self) {
        let msg = std::mem::take(&mut self.input);
        self.cursor_char_pos = 0;
        if msg.trim().is_empty() {
            return;
        }
        self.messages.push(Message::user(msg.clone()));

        // 模拟流式响应
        let response = match self.mode {
            Mode::Plan => format!("📋 [规划模式] 收到指令：「{}」\n分析中，不会执行任何修改操作。此功能将在阶段 2 完善。", msg),
            Mode::Assist => format!("🤖 [助手模式] 收到指令：「{}」\n将逐步执行，每步需要你的确认。此功能将在阶段 2 完善。", msg),
            Mode::Speed => format!("⚡ [极速模式] 收到指令：「{}」\n自动执行中，完成后将汇总结果。此功能将在阶段 2 完善。", msg),
        };
        self.messages.push(Message::assistant(response));

        // 模拟费用累计
        self.total_cost += 0.0012;
        // 更新缓存命中率（模拟）
        self.cache_hit_rate = 0.94;
        self.active_agents = 2;
    }

    /// 切换模式
    fn switch_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.messages.push(Message::system(format!(
                "已切换到「{}」模式 — {}",
                mode.name(),
                mode.desc()
            )));
        }
    }

    /// 估算当前内存使用
    fn estimate_memory(&self) -> u64 {
        // 简易估算：消息数 × 平均消息长度 + 固定开销
        let msg_bytes: usize = self.messages.iter().map(|m| m.content.len()).sum();
        (msg_bytes + 1024 * 1024) as u64 // 加 1MB 基础开销
    }
}

/// 在屏幕中央创建矩形区域
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_width = (r.width * percent_x / 100).min(80);
    let popup_height = (r.height * percent_y / 100).min(24);
    let x = r.x + (r.width.saturating_sub(popup_width)) / 2;
    let y = r.y + (r.height.saturating_sub(popup_height)) / 2;
    Rect::new(x, y, popup_width, popup_height)
}

