//! TUI 主应用

use crate::config::Config;
use crate::locale::{self, Locale};
use crate::error::ForgeError;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

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
    pub fn name(&self) -> &'static str {
        match self {
            Mode::Plan => "规划",
            Mode::Assist => "助手",
            Mode::Speed => "极速",
        }
    }
}

/// 应用状态
pub enum AppState {
    Running,
    Quitting,
}

/// TUI 主应用
pub struct App {
    config: Config,
    locale: Locale,
    mode: Mode,
    state: AppState,
    /// 输入缓冲区
    input: String,
    /// 对话历史
    messages: Vec<(String, String)>,
    /// 游标位置
    cursor_pos: usize,
    /// 累计费用
    total_cost: f64,
    /// 缓存命中率
    cache_hit_rate: f64,
    /// 当前并行数
    active_agents: usize,
    /// 最大并行数
    max_agents: usize,
}

impl App {
    pub fn new(config: Config) -> Result<Self, ForgeError> {
        let mode = match config.ui.default_mode.as_str() {
            "plan" => Mode::Plan,
            "speed" => Mode::Speed,
            _ => Mode::Assist,
        };

        Ok(Self {
            config,
            locale: Locale::default(),
            mode,
            state: AppState::Running,
            input: String::new(),
            messages: vec![
                ("系统".into(), "🔥 熔炉已就绪。输入你的指令开始编程。".into()),
            ],
            cursor_pos: 0,
            total_cost: 0.0,
            cache_hit_rate: 0.0,
            active_agents: 0,
            max_agents: 8,
        })
    }

    /// 运行 TUI 主循环
    pub async fn run(&mut self) -> Result<(), ForgeError> {
        // 阶段 1 完善：初始化 crossterm，启动事件循环
        println!("🔥 熔炉 (ForgeShell) v{}", env!("CARGO_PKG_VERSION"));
        println!("模式: {}", self.mode.name());
        println!("按 Ctrl+C 退出");
        println!("---");
        println!("阶段 1 将实现完整 TUI 界面。当前为占位模式。");
        Ok(())
    }

    /// 切换模式
    pub fn switch_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.messages.push((
            "系统".into(),
            format!("已切换到「{}」模式", mode.name()),
        ));
    }

    /// 处理用户输入
    pub fn handle_input(&mut self, key: char) {
        if key == '\n' {
            let msg = std::mem::take(&mut self.input);
            if !msg.trim().is_empty() {
                self.messages.push(("用户".into(), msg));
                self.messages.push((
                    "熔炉".into(),
                    format!("收到指令，当前处于「{}」模式", self.mode.name()),
                ));
            }
        } else if key == '\u{8}' {
            self.input.pop();
        } else {
            self.input.push(key);
        }
    }
}
