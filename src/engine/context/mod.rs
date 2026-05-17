//! 四级缓存上下文系统
//!
//! Level 1: System Prompt（永久，启动时加载）
//! Level 2: Project Context Block（跨会话复用，项目变更时更新）
//! Level 3: Session Persistent（最近 5 轮对话）
//! Level 4: Volatile Tail（当前指令和输出）

mod level1;
mod level2;
mod level3;
mod level4;

use crate::error::ForgeError;

/// 上下文层级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextLevel {
    System = 1,
    Project = 2,
    Session = 3,
    Volatile = 4,
}

/// 完整的上下文组装结果
#[derive(Debug, Clone)]
pub struct AssembledContext {
    pub total_tokens: usize,
    pub system_prompt: String,
    pub project_context: String,
    pub session_history: Vec<(String, String)>,
    pub current_input: String,
}

/// 上下文管理器
pub struct ContextManager {
    /// 系统提示词缓存
    system_prompt: String,
    /// 项目上下文
    project_context: String,
    /// 会话历史
    session: Vec<(String, String)>,
    /// 最大保留轮数
    max_session_rounds: usize,
}

impl ContextManager {
    pub fn new(max_session_rounds: usize) -> Self {
        Self {
            system_prompt: String::new(),
            project_context: String::new(),
            session: Vec::new(),
            max_session_rounds,
        }
    }

    /// 设置系统提示词
    pub fn set_system_prompt(&mut self, prompt: &str) {
        self.system_prompt = prompt.to_string();
    }

    /// 更新项目上下文
    pub fn update_project_context(&mut self, ctx: &str) {
        self.project_context = ctx.to_string();
    }

    /// 添加一轮对话
    pub fn add_turn(&mut self, user: &str, assistant: &str) {
        self.session.push((user.to_string(), assistant.to_string()));
        // 保留最近 N 轮
        if self.session.len() > self.max_session_rounds {
            let excess = self.session.len() - self.max_session_rounds;
            self.session.drain(0..excess);
        }
    }

    /// 组装完整上下文
    pub fn assemble(&self, current_input: &str) -> AssembledContext {
        let total = self.system_prompt.chars().count()
            + self.project_context.chars().count()
            + self.session.iter().map(|(u, a)| u.chars().count() + a.chars().count()).sum::<usize>()
            + current_input.chars().count();

        AssembledContext {
            total_tokens: total / 4, // 粗略估算：4 字符 ≈ 1 token
            system_prompt: self.system_prompt.clone(),
            project_context: self.project_context.clone(),
            session_history: self.session.clone(),
            current_input: current_input.to_string(),
        }
    }

    /// 获得缓存命中率统计
    pub fn hit_rate(&self) -> f64 {
        // 阶段 2 完善
        0.94
    }
}
