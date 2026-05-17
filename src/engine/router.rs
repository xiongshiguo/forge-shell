//! 模型路由器：简单任务 → Flash，复杂任务 → V4 Pro

/// 任务复杂度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Complexity {
    Simple,
    Moderate,
    Complex,
}

/// 模型路由器
pub struct ModelRouter {
    flash_model: String,
    pro_model: String,
}

impl ModelRouter {
    pub fn new(pro_model: String, flash_model: String) -> Self {
        Self {
            flash_model,
            pro_model,
        }
    }

    /// 根据任务复杂度选择模型
    pub fn route(&self, complexity: Complexity) -> &str {
        match complexity {
            Complexity::Simple => &self.flash_model,
            Complexity::Moderate | Complexity::Complex => &self.pro_model,
        }
    }

    /// 根据用户意图估算复杂度
    pub fn estimate_complexity(&self, _intent: &str) -> Complexity {
        // 阶段 5 完善：启发式分析
        // 简单规则：短指令 → Simple，长指令 → Complex
        let chars = _intent.chars().count();
        if chars < 50 {
            Complexity::Simple
        } else if chars < 500 {
            Complexity::Moderate
        } else {
            Complexity::Complex
        }
    }
}
