//! 约束系统：管理代码生成的约束条件

/// 约束类型
#[derive(Debug, Clone)]
pub enum ConstraintType {
    /// 必须通过所有测试
    AllTestsPass,
    /// 不能引入新的 clippy 警告
    NoNewWarnings,
    /// 保持 API 兼容
    ApiCompatible,
    /// 自定义约束
    Custom(String),
}

/// 约束检查器
pub struct ConstraintChecker {
    constraints: Vec<ConstraintType>,
}

impl ConstraintChecker {
    pub fn new() -> Self {
        Self {
            constraints: vec![
                ConstraintType::AllTestsPass,
                ConstraintType::NoNewWarnings,
            ],
        }
    }

    pub fn add(&mut self, c: ConstraintType) {
        self.constraints.push(c);
    }

    pub fn check_all(&self) -> Vec<&ConstraintType> {
        self.constraints.iter().collect()
    }
}

impl Default for ConstraintChecker {
    fn default() -> Self {
        Self::new()
    }
}
