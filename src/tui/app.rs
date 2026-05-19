//! TUI 主应用 — 全中文界面，支持规划/助手/极速三种模式
//! 集成四级缓存上下文和并行任务调度

use crate::agent::dispatcher::{Dispatcher, MergedResult};
use crate::agent::orchestrator::{OrchestrationPlan, Orchestrator};
use crate::config::Config;
use crate::engine::cache::CacheStats;
use crate::engine::context::ContextManager;
use crate::error::ForgeError;
use crate::locale::Locale;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    Frame,
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;

use super::components::cost_panel::CostData;
use super::components::community_panel::{self, CommunityTab};
use super::components::project_panel::{self, ProjectStats};
use super::components::{chat_view, cost_panel, mode_bar, stats_bar};
use super::keybindings::{self, KeyEvent};

// ---- 模式定义 ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode { Plan, Assist, Speed }

impl Mode {
    pub fn name(&self) -> &str {
        match self { Mode::Plan => "规划", Mode::Assist => "助手", Mode::Speed => "极速" }
    }

    pub fn desc(&self) -> &str {
        match self { Mode::Plan => "只分析，不修改", Mode::Assist => "逐步执行，需确认", Mode::Speed => "自动执行，事后汇总" }
    }

    pub fn shortcut(&self) -> &str {
        match self { Mode::Plan => "Ctrl+P", Mode::Assist => "Ctrl+A", Mode::Speed => "Ctrl+Y" }
    }

    pub fn color(&self) -> Color {
        match self { Mode::Plan => Color::Cyan, Mode::Assist => Color::Green, Mode::Speed => Color::Yellow }
    }
}

// ---- 消息类型 ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role { System, User, Assistant }

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

// ---- 异步后台事件 ----

enum BackgroundEvent {
    DispatchResult(MergedResult),
    CacheUpdated(CacheStats),
}

// ---- 应用状态 ----

enum RunState { Running, Quit }

/// TUI 主应用
pub struct App {
    config: Config,
    locale: Locale,
    mode: Mode,
    input: String,
    messages: Vec<Message>,
    cursor_char_pos: usize,
    total_cost: f64,
    cache_hit_rate: f64,
    active_agents: usize,
    max_agents: usize,
    memory_usage: u64,
    start_time: Instant,
    show_project_panel: bool,
    show_cost_panel: bool,
    show_community_panel: bool,
    show_share_dialog: bool,
    scroll_offset: usize,
    streaming_buffer: String,
    is_streaming: bool,
    context_manager: ContextManager,
    orchestrator: Orchestrator,
    /// 后台任务结果（Arc<Mutex> 便于同步读取）
    pending_results: Arc<Mutex<Vec<MergedResult>>>,
    /// Tokio 运行时句柄
    runtime_handle: tokio::runtime::Handle,
    /// 项目统计
    project_stats: ProjectStats,
    /// 费用数据
    cost_data: CostData,
    /// 社区大厅当前标签
    community_tab: CommunityTab,
}

impl App {
    pub fn new(config: Config) -> Result<Self, ForgeError> {
        let mode = match config.ui.default_mode.as_str() {
            "plan" => Mode::Plan,
            "speed" => Mode::Speed,
            _ => Mode::Assist,
        };
        let max_agents = config.engine.max_parallel_agents;
        let cache_entries = config.engine.session_cache_rounds * 20 + 10;
        let session_rounds = config.engine.session_cache_rounds;

        let mut ctx_mgr = ContextManager::new(cache_entries, session_rounds);
        ctx_mgr.init_system_prompt(&crate::system_prompt::get_system_prompt());

        let orchestrator = Orchestrator::new(max_agents);
        let runtime_handle = tokio::runtime::Handle::current();

        let messages = vec![
            Message::system("🔥 熔炉已就绪。以意为炉，以语为锤，铸代码之剑。"),
            Message::system(format!(
                "当前模式：「{}」— {} | 输入指令开始，Ctrl+C 退出",
                mode.name(), mode.desc()
            )),
            Message::system(format!(
                "四级缓存已初始化 | 最大并行: {} | 目标命中率: ≥97%",
                max_agents
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
            context_manager: ctx_mgr,
            orchestrator,
            pending_results: Arc::new(Mutex::new(Vec::new())),
            runtime_handle,
            project_stats: ProjectStats::default(),
            cost_data: CostData {
                monthly_budget: 100.0,
                ..Default::default()
            },
            community_tab: CommunityTab::Pool,
        })
    }

    /// 运行 TUI 主循环（同步，使用 tokio handle 分发后台任务）
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

    /// 事件循环
    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), ForgeError> {
        let mut state = RunState::Running;

        while matches!(state, RunState::Running) {
            // 更新指标
            self.memory_usage = self.estimate_memory();
            self.cache_hit_rate = self.context_manager.hit_rate();

            // 消费后台结果
            self.consume_background_results();

            terminal
                .draw(|f| self.draw(f))
                .map_err(|e| ForgeError::Tui(e.to_string()))?;

            if event::poll(std::time::Duration::from_millis(50))
                .map_err(|e| ForgeError::Tui(e.to_string()))?
            {
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
                                if self.show_project_panel {
                                    self.project_stats = project_panel::gather_stats(
                                        &std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                                    );
                                }
                            }
                            KeyEvent::CostPanel => {
                                self.show_cost_panel = !self.show_cost_panel;
                                self.show_project_panel = false;
                                self.show_community_panel = false;
                                if self.show_cost_panel {
                                    self.update_cost_data();
                                }
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
                            KeyEvent::TabLeft => {
                                if self.show_community_panel {
                                    let tabs = CommunityTab::all();
                                    let idx = tabs.iter().position(|t| *t == self.community_tab).unwrap_or(0);
                                    self.community_tab = tabs[(idx + tabs.len() - 1) % tabs.len()];
                                }
                            }
                            KeyEvent::TabRight => {
                                if self.show_community_panel {
                                    let tabs = CommunityTab::all();
                                    let idx = tabs.iter().position(|t| *t == self.community_tab).unwrap_or(0);
                                    self.community_tab = tabs[(idx + 1) % tabs.len()];
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 消费后台分发结果
    fn consume_background_results(&mut self) {
        let drained: Vec<MergedResult> = {
            if let Ok(mut guard) = self.pending_results.lock() {
                std::mem::take(&mut *guard)
            } else {
                Vec::new()
            }
        };
        for result in drained {
            self.handle_dispatch_result(result);
        }
    }

    // ---- 绘制 ----

    fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(area);

        self.draw_title_bar(f, main_chunks[0]);
        self.draw_content(f, main_chunks[1]);
        self.draw_status_bar(f, main_chunks[2]);

        if self.show_share_dialog {
            self.draw_share_dialog(f, area);
        }
    }

    fn draw_title_bar(&self, f: &mut Frame, area: Rect) {
        let title_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(20),
                Constraint::Min(0),
                Constraint::Length(25),
            ])
            .split(area);

        let title = Paragraph::new(Line::from(vec![
            Span::styled("熔炉", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" ForgeShell"),
        ]));
        f.render_widget(title, title_chunks[0]);

        mode_bar::render(f, title_chunks[1], self.mode);

        let hints = Line::from(vec![
            Span::styled("F1", Style::default().fg(Color::DarkGray)), Span::raw("项目 "),
            Span::styled("F2", Style::default().fg(Color::DarkGray)), Span::raw("费用 "),
            Span::styled("C-S-C", Style::default().fg(Color::DarkGray)), Span::raw("社区"),
        ]);
        let hints_p = Paragraph::new(hints).alignment(ratatui::layout::Alignment::Right);
        f.render_widget(hints_p, title_chunks[2]);
    }

    fn draw_content(&self, f: &mut Frame, area: Rect) {
        if self.show_project_panel {
            self.draw_project_panel(f, area);
        } else if self.show_cost_panel {
            self.draw_cost_panel(f, area);
        } else if self.show_community_panel {
            self.draw_community_panel(f, area);
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(area);
            self.draw_chat(f, chunks[0]);
            self.draw_input(f, chunks[1]);
        }
    }

    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        chat_view::render(f, area, &self.messages, self.scroll_offset, self.is_streaming, &self.streaming_buffer);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.mode.color()))
            .title(format!(" 输入指令（{}模式） ", self.mode.name()));

        let display_text = if self.input.is_empty() {
            vec![Line::from(Span::styled("输入你的指令…", Style::default().fg(Color::DarkGray)))]
        } else {
            vec![Line::from(vec![
                Span::raw(&self.input),
                Span::styled("▊", Style::default().fg(self.mode.color())),
            ])]
        };

        let input_p = Paragraph::new(display_text).block(input_block).wrap(Wrap { trim: false });
        f.render_widget(input_p, area);
    }

    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let cost_text = crate::utils::format_cost(self.total_cost);
        let hit_text = crate::utils::format_percent(self.cache_hit_rate);
        let parallel_text = format!("{}/{}", self.active_agents, self.max_agents);
        let mem_text = crate::utils::format_bytes(self.memory_usage);
        stats_bar::render(f, area, &cost_text, &hit_text, &parallel_text, &mem_text);
    }

    fn draw_project_panel(&self, f: &mut Frame, area: Rect) {
        project_panel::render(f, area, &self.project_stats);
    }

    fn draw_cost_panel(&self, f: &mut Frame, area: Rect) {
        cost_panel::render(f, area, &self.cost_data, self.cache_hit_rate);
    }

    fn draw_community_panel(&self, f: &mut Frame, area: Rect) {
        community_panel::render(f, area, self.community_tab, |_tab| {
            // Tab switching handled via key events
        });
    }

    fn draw_share_dialog(&self, f: &mut Frame, area: Rect) {
        let dialog_area = centered_rect(60, 40, area);
        let block = Block::default().title(" 复盘分享 (Ctrl+S 关闭) ").borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let lines = vec![
            Line::from("📤 分享复盘到经验熔池"), Line::from(""),
            Line::from("以下内容将被上传（脱敏后）："),
            Line::from("  • 任务策略和决策逻辑"),
            Line::from("  • 工具调用时序"),
            Line::from("  • 成功/失败模式"), Line::from(""),
            Line::from(Span::styled("以下内容绝不会上传：", Style::default().fg(Color::Red))),
            Line::from("  • 源代码、文件路径、变量名"),
            Line::from("  • API Key 或任何密钥"),
            Line::from("  • 个人身份信息"), Line::from(""),
            Line::from("按 Enter 确认分享，Esc 取消"),
        ];

        let clear_block = Block::default().style(Style::default().bg(Color::Black));
        f.render_widget(clear_block, dialog_area);
        f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: true }), dialog_area);
    }

    // ---- 交互 ----

    fn submit_input(&mut self) {
        let msg = std::mem::take(&mut self.input);
        self.cursor_char_pos = 0;
        if msg.trim().is_empty() { return; }

        self.messages.push(Message::user(msg.clone()));

        // 编排任务
        let plan = self.orchestrator.decompose(&msg);
        self.context_manager.add_turn(&msg, &format!("编排: {} 子任务", plan.tasks.len()));

        // 显示编排结果
        let task_summary: Vec<String> = plan.tasks.iter().map(|t| {
            format!("  {} [{}] {}",
                if t.read_only { "📖" } else { "✏️" }, t.id, t.description)
        }).collect();

        self.messages.push(Message::assistant(format!(
            "任务拆解完成（{} 个子任务，{} 组并行，预估并行增益 {:.1}x）：\n{}",
            plan.tasks.len(), plan.parallel_groups.len(), plan.parallelism_gain, task_summary.join("\n")
        )));

        // 更新指标
        self.cache_hit_rate = self.context_manager.hit_rate();
        self.total_cost += plan.estimated_total_tokens as f64 * 0.000001;
        self.active_agents = plan.parallel_groups.first().map(|g| g.len()).unwrap_or(0);

        // 异步分发
        let config = self.config.clone();
        let dispatcher = Dispatcher::new(config, self.max_agents);
        let results = self.pending_results.clone();
        self.runtime_handle.spawn(async move {
            let result = dispatcher.dispatch(plan).await;
            if let Ok(mut guard) = results.lock() {
                guard.push(result);
            }
        });
    }

    fn handle_dispatch_result(&mut self, result: MergedResult) {
        self.active_agents = 0;
        let summary = format!(
            "分发完成: {} 成功, {} 失败 | 总 Token: {} | 耗时: {}ms",
            result.success_count, result.failure_count, result.total_tokens, result.total_duration_ms,
        );
        self.messages.push(Message::assistant(summary));

        let details: Vec<String> = result.results.iter().map(|r| {
            format!("  [{}] {} → {} ({}ms, {} tokens)",
                if r.success { "✓" } else { "✗" }, r.task_id,
                if r.success { &r.output } else { r.error.as_deref().unwrap_or("未知错误") },
                r.duration_ms, r.tokens_used)
        }).collect();
        self.messages.push(Message::system(details.join("\n")));
        self.total_cost += result.total_tokens as f64 * 0.000001;
    }

    fn switch_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.messages.push(Message::system(format!("已切换到「{}」模式 — {}", mode.name(), mode.desc())));
        }
    }

    /// 更新费用数据
    fn update_cost_data(&mut self) {
        let stats = self.context_manager.cache_stats();
        // Claude Code 估算：每 1M token 约 $3，折合 ¥21
        let claude_estimate = stats.total_requests() as f64 * 0.003;
        let cache_saved = stats.tokens_saved as f64 * 0.000001;

        self.cost_data = CostData {
            prompt_tokens: stats.tokens_saved + stats.hits * 100,
            completion_tokens: stats.total_requests() * 200,
            total_tokens: stats.total_requests() * 300 + stats.tokens_saved,
            cache_hit_tokens: stats.tokens_saved,
            total_cost_yuan: self.total_cost,
            cache_saved_yuan: cache_saved,
            claude_estimated_yuan: claude_estimate,
            monthly_budget: self.cost_data.monthly_budget,
            monthly_used: self.total_cost,
        };
    }

    fn estimate_memory(&self) -> u64 {
        let msg_bytes: usize = self.messages.iter().map(|m| m.content.len()).sum();
        let cache_bytes = self.context_manager.cache_stats().tokens_saved as usize * 4;
        (msg_bytes + cache_bytes + 1024 * 1024) as u64
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_width = (r.width * percent_x / 100).min(80);
    let popup_height = (r.height * percent_y / 100).min(24);
    let x = r.x + (r.width.saturating_sub(popup_width)) / 2;
    let y = r.y + (r.height.saturating_sub(popup_height)) / 2;
    Rect::new(x, y, popup_width, popup_height)
}

