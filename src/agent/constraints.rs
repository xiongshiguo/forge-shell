//! 约束系统：管理代码生成的约束条件

/// 约束
#[derive(Debug, Clone)]
pub struct Constraint {
    pub description: String,
    pub must_verify: bool,
}

/// 约束类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintType {
    AllTestsPass,
    NoNewWarnings,
    ApiCompatible,
    Custom(String),
}

/// 约束检查结果
#[derive(Debug)]
pub struct ConstraintResult {
    pub constraint: ConstraintType,
    pub passed: bool,
    pub message: String,
}

/// 约束检查器
#[derive(Debug, Clone)]
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

    /// 添加约束
    pub fn add(&mut self, c: ConstraintType) {
        if !self.constraints.contains(&c) {
            self.constraints.push(c);
        }
    }

    /// 移除约束
    pub fn remove(&mut self, c: &ConstraintType) {
        self.constraints.retain(|x| x != c);
    }

    /// 列出所有约束
    pub fn list(&self) -> &[ConstraintType] {
        &self.constraints
    }

    /// 检查所有约束
    pub fn check_all(&self) -> Vec<&ConstraintType> {
        self.constraints.iter().collect()
    }

    /// 验证测试是否通过
    pub fn verify_tests(&self, test_output: &str) -> ConstraintResult {
        let passed = !test_output.contains("FAIL")
            && !test_output.contains("error:")
            && test_output.contains("test");
        ConstraintResult {
            constraint: ConstraintType::AllTestsPass,
            passed,
            message: if passed {
                "✓ 所有测试通过".into()
            } else {
                "✗ 存在失败的测试".into()
            },
        }
    }

    /// 验证无新警告（需要前后对比）
    pub fn verify_no_new_warnings(&self, before: usize, after: usize) -> ConstraintResult {
        let passed = after <= before;
        ConstraintResult {
            constraint: ConstraintType::NoNewWarnings,
            passed,
            message: if passed {
                format!("✓ 无新警告（{} → {}）", before, after)
            } else {
                format!("✗ 新增 {} 个警告（{} → {}）", after - before, before, after)
            },
        }
    }

    /// 约束描述
    pub fn describe<'a>(&self, c: &'a ConstraintType) -> &'a str {
        match c {
            ConstraintType::AllTestsPass => "所有测试必须通过",
            ConstraintType::NoNewWarnings => "不能引入新的编译器警告",
            ConstraintType::ApiCompatible => "保持 API 兼容性",
            ConstraintType::Custom(desc) => desc.as_str(),
        }
    }
}

impl Default for ConstraintChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_constraints() {
        let checker = ConstraintChecker::new();
        assert_eq!(checker.list().len(), 2);
    }

    #[test]
    fn test_verify_tests_pass() {
        let checker = ConstraintChecker::new();
        let result = checker.verify_tests("running 5 tests\n test result: ok. 5 passed");
        assert!(result.passed);
    }

    #[test]
    fn test_verify_tests_fail() {
        let checker = ConstraintChecker::new();
        let result = checker.verify_tests("running 5 tests\n FAIL: test_broken");
        assert!(!result.passed);
    }

    #[test]
    fn test_no_new_warnings() {
        let checker = ConstraintChecker::new();
        let result = checker.verify_no_new_warnings(10, 8);
        assert!(result.passed);
        let result2 = checker.verify_no_new_warnings(10, 15);
        assert!(!result2.passed);
    }
}
