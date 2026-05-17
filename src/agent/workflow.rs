//! 红-绿-重构工作流
//!
//! 红：先写失败测试
//! 绿：最小实现让测试通过
//! 重构：优化代码结构，保持测试绿色

/// 工作流阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Red,
    Green,
    Refactor,
}

/// 约束标注
#[derive(Debug, Clone)]
pub struct Constraint {
    pub description: String,
    pub must_verify: bool,
}

/// 红-绿-重构工作流
pub struct Workflow {
    pub current_phase: Phase,
    pub constraints: Vec<Constraint>,
    pub cycle_count: u32,
}

impl Workflow {
    pub fn new() -> Self {
        Self {
            current_phase: Phase::Red,
            constraints: vec![],
            cycle_count: 0,
        }
    }

    /// 进入下一阶段
    pub fn advance(&mut self) {
        self.current_phase = match self.current_phase {
            Phase::Red => Phase::Green,
            Phase::Green => Phase::Refactor,
            Phase::Refactor => {
                self.cycle_count += 1;
                Phase::Red
            }
        };
    }

    /// 添加约束
    pub fn add_constraint(&mut self, desc: &str, must_verify: bool) {
        self.constraints.push(Constraint {
            description: desc.to_string(),
            must_verify,
        });
    }
}

impl Default for Workflow {
    fn default() -> Self {
        Self::new()
    }
}
