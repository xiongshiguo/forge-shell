//! 红-绿-重构工作流
//!
//! 红 (Red)：    编写失败测试，定义期望行为
//! 绿 (Green)：  最小实现让测试通过
//! 重构 (Refactor)：优化代码结构，保持测试全绿
//!
//! 每完成一个周期，检查所有约束条件。

use crate::agent::constraints::{Constraint, ConstraintChecker};

/// 工作流阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Red,
    Green,
    Refactor,
}

impl Phase {
    pub fn name(&self) -> &str {
        match self {
            Phase::Red => "红",
            Phase::Green => "绿",
            Phase::Refactor => "重构",
        }
    }

    pub fn emoji(&self) -> &str {
        match self {
            Phase::Red => "🔴",
            Phase::Green => "🟢",
            Phase::Refactor => "🔵",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Phase::Red => "编写失败测试，定义期望行为",
            Phase::Green => "最小实现让测试通过",
            Phase::Refactor => "优化代码结构，保持测试全绿",
        }
    }
}

/// 工作流状态
#[derive(Debug, Clone)]
pub struct WorkflowState {
    pub current_phase: Phase,
    pub cycle_count: u32,
    /// 当前阶段的约束
    pub constraints: Vec<Constraint>,
    /// 约束检查器
    pub checker: ConstraintChecker,
    /// 测试是否已编写
    pub test_written: bool,
    /// 实现是否已完成
    pub implementation_done: bool,
    /// 是否已通过所有检查
    pub all_checks_passed: bool,
}

/// 红-绿-重构工作流引擎
pub struct Workflow {
    state: WorkflowState,
    /// 工作流历史
    history: Vec<WorkflowStep>,
}

#[derive(Debug, Clone)]
struct WorkflowStep {
    phase: Phase,
    action: String,
    result: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl Workflow {
    pub fn new() -> Self {
        Self {
            state: WorkflowState {
                current_phase: Phase::Red,
                cycle_count: 0,
                constraints: Vec::new(),
                checker: ConstraintChecker::new(),
                test_written: false,
                implementation_done: false,
                all_checks_passed: false,
            },
            history: Vec::new(),
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }

    /// 获取当前阶段
    pub fn current_phase(&self) -> Phase {
        self.state.current_phase
    }

    /// 添加约束
    pub fn add_constraint(&mut self, desc: &str, must_verify: bool) {
        self.state.constraints.push(Constraint {
            description: desc.to_string(),
            must_verify,
        });
        self.state.checker.add(crate::agent::constraints::ConstraintType::Custom(desc.to_string()));
    }

    /// 标记测试已编写（红 → 绿的前提）
    pub fn tests_written(&mut self) {
        self.state.test_written = true;
        self.log(Phase::Red, "测试已编写", "等待验证测试失败");
    }

    /// 验证测试确实失败（红阶段检查）
    pub fn verify_tests_fail(&mut self, test_output: &str) -> bool {
        let fail = test_output.contains("FAIL") || test_output.contains("error");
        self.log(Phase::Red, "验证测试失败", if fail { "✓ 测试按预期失败" } else { "⚠ 测试未失败，请检查" });
        fail
    }

    /// 推进到绿阶段
    pub fn advance_to_green(&mut self) -> Result<(), String> {
        if !self.state.test_written {
            return Err("必须先编写测试才能进入绿阶段".into());
        }
        self.state.current_phase = Phase::Green;
        self.log(Phase::Green, "进入绿阶段", "开始最小实现");
        Ok(())
    }

    /// 标记实现完成（绿 → 重构的前提）
    pub fn implementation_done(&mut self) {
        self.state.implementation_done = true;
        self.log(Phase::Green, "实现完成", "等待验证测试通过");
    }

    /// 验证测试通过（绿阶段检查）
    pub fn verify_tests_pass(&mut self, test_output: &str) -> bool {
        let pass = !test_output.contains("FAIL") && !test_output.contains("error:") && test_output.contains("test");
        self.log(Phase::Green, "验证测试通过", if pass { "✓ 测试全部通过" } else { "✗ 测试存在失败" });
        pass
    }

    /// 推进到重构阶段
    pub fn advance_to_refactor(&mut self) -> Result<(), String> {
        if !self.state.implementation_done {
            return Err("必须先完成实现才能进入重构阶段".into());
        }
        self.state.current_phase = Phase::Refactor;
        self.log(Phase::Refactor, "进入重构阶段", "优化代码结构");
        Ok(())
    }

    /// 完成一个完整周期
    pub fn complete_cycle(&mut self) {
        self.state.cycle_count += 1;
        self.state.test_written = false;
        self.state.implementation_done = false;
        self.state.current_phase = Phase::Red;
        self.log(Phase::Refactor, "周期完成", &format!("第 {} 轮结束", self.state.cycle_count));
    }

    /// 执行约束检查
    pub fn check_all_constraints(&self) -> Vec<String> {
        let mut violations = Vec::new();
        for constraint in &self.state.constraints {
            if constraint.must_verify {
                violations.push(format!("[待验证] {}", constraint.description));
            }
        }
        violations
    }

    /// 获取工作流摘要
    pub fn summary(&self) -> String {
        let phase_info = format!(
            "当前阶段: {} {}\n   {}\n  周期数: {}\n  测试: {} | 实现: {} | 检查: {}",
            self.state.current_phase.emoji(),
            self.state.current_phase.name(),
            self.state.current_phase.description(),
            self.state.cycle_count,
            if self.state.test_written { "✓" } else { "—" },
            if self.state.implementation_done { "✓" } else { "—" },
            if self.state.all_checks_passed { "✓" } else { "—" },
        );
        phase_info
    }

    fn log(&mut self, phase: Phase, action: &str, result: &str) {
        self.history.push(WorkflowStep {
            phase,
            action: action.to_string(),
            result: result.to_string(),
            timestamp: chrono::Utc::now(),
        });
    }
}

impl Default for Workflow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_starts_in_red() {
        let wf = Workflow::new();
        assert_eq!(wf.current_phase(), Phase::Red);
    }

    #[test]
    fn test_cannot_go_green_without_tests() {
        let mut wf = Workflow::new();
        assert!(wf.advance_to_green().is_err());
    }

    #[test]
    fn test_normal_flow() {
        let mut wf = Workflow::new();
        wf.tests_written();
        assert!(wf.advance_to_green().is_ok());
        wf.implementation_done();
        assert!(wf.advance_to_refactor().is_ok());
        wf.complete_cycle();
        assert_eq!(wf.state.cycle_count, 1);
        assert_eq!(wf.current_phase(), Phase::Red);
    }

    #[test]
    fn test_verify_test_failure() {
        let mut wf = Workflow::new();
        assert!(wf.verify_tests_fail("test result: FAIL. 3 passed; 1 failed"));
        assert!(!wf.verify_tests_fail("test result: ok. 4 passed"));
    }

    #[test]
    fn test_verify_test_pass() {
        let mut wf = Workflow::new();
        assert!(wf.verify_tests_pass("running 4 tests\n test result: ok. 4 passed"));
        assert!(!wf.verify_tests_pass("running 4 tests\n FAIL: test_broken"));
    }
}
