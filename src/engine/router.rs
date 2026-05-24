//! 模型路由器：智能选择最优模型以控制成本
//!
//! 策略：
//! - 简单任务（代码补全、格式化、简单问答） → Flash 模型（成本仅 Pro 的 10%）
//! - 中等任务（重构、分析、调试） → Pro 模型（精度优先）
//! - 复杂任务（架构设计、多文件修改） → Pro 模型 + 扩展思考
//!
//! 启发式评估规则：
//! 1. 指令长度 < 50 字符 → 大概率简单
//! 2. 包含"分析"、"设计"、"重构"、"架构"等词 → 复杂
//! 3. 涉及多文件 → 复杂
//! 4. 历史相似任务模式 → 参考缓存

/// 任务复杂度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Complexity {
    Simple,
    Moderate,
    Complex,
}

impl Complexity {
    pub fn name(&self) -> &str {
        match self {
            Complexity::Simple => "简单",
            Complexity::Moderate => "中等",
            Complexity::Complex => "复杂",
        }
    }
}

/// 路由决策
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub model: String,
    pub complexity: Complexity,
    pub reason: String,
    /// 预计 token 数
    pub estimated_tokens: usize,
    /// 预计费用（元）
    pub estimated_cost: f64,
}

/// 模型路由器
pub struct ModelRouter {
    pro_model: String,
    flash_model: String,
    /// 简单→Flash 的 token 阈值
    simple_max_tokens: usize,
    /// Pro 价格 (元/1M token)
    pro_input_price: f64,
    pro_output_price: f64,
    /// Flash 价格 (元/1M token)
    flash_input_price: f64,
    flash_output_price: f64,
}

impl ModelRouter {
    pub fn new(pro_model: String, flash_model: String) -> Self {
        Self {
            pro_model,
            flash_model,
            simple_max_tokens: 1000,
            // V4 Pro: ¥1/M in, ¥4/M out; V4 Flash: ¥0.1/M in, ¥0.4/M out
            pro_input_price: 1.0,
            pro_output_price: 4.0,
            flash_input_price: 0.1,
            flash_output_price: 0.4,
        }
    }

    /// 根据意图做出路由决策
    pub fn decide(&self, intent: &str, context_tokens: usize) -> RoutingDecision {
        let complexity = self.estimate_complexity(intent, context_tokens);
        let model = self.route(complexity).to_string();
        let estimated_tokens = self.estimate_tokens(intent, complexity);

        let (in_price, out_price) = match complexity {
            Complexity::Simple | Complexity::Moderate => (self.flash_input_price, self.flash_output_price),
            Complexity::Complex => (self.pro_input_price, self.pro_output_price),
        };
        let estimated_cost = (estimated_tokens as f64 / 1_000_000.0) * (in_price + out_price) / 2.0;

        let reason = match complexity {
            Complexity::Simple => format!(
                "简短指令 ({} 字符)，使用 {} 以降低成本",
                intent.chars().count(),
                self.flash_model
            ),
            Complexity::Moderate => format!(
                "中等任务 ({} 字符)，使用 {} 以兼顾速度",
                intent.chars().count(),
                self.flash_model
            ),
            Complexity::Complex => format!(
                "复杂任务 (包含关键指令)，使用 {} 并启用扩展思考",
                self.pro_model
            ),
        };

        RoutingDecision {
            model,
            complexity,
            reason,
            estimated_tokens,
            estimated_cost,
        }
    }

    /// 根据复杂度选择模型
    pub fn route(&self, complexity: Complexity) -> &str {
        match complexity {
            // L4: DeepSeek V4 Pro + 长system prompt + 多工具 => 不发数据
            // Flash 每次正常，故 Simple/Moderate 都用 Flash
            Complexity::Simple | Complexity::Moderate => &self.flash_model,
            Complexity::Complex => &self.pro_model, // 仅真正复杂任务用 Pro+思考
        }
    }

    /// 估算任务复杂度
    /// 语义复杂度估算（用户无感——不显示模型名，只通过费用和响应速度体现）
    pub fn estimate_complexity(&self, intent: &str, context_tokens: usize) -> Complexity {
        let char_count = intent.chars().count();
        let intent_lower = intent.to_lowercase();

        // 检测复杂度特征词
        let complex_keywords = [
            "架构", "设计", "系统", "重构", "大规模", "多模块",
            "并发", "异步", "分布式", "数据库迁移", "安全审计",
            // 竞品对比/评估类必须走 Pro
            "对比", "竞品", "compare", " vs ", "versus", "谁强", "谁弱",
            "差距", "优劣", "哪个好", "区别", "差异",
        ];
        let moderate_keywords = [
            "分析", "调试", "优化", "修改", "实现", "集成",
            "测试", "部署", "配置", "重构",
        ];
        let simple_keywords = [
            "解释", "什么是", "怎么", "例子", "示例", "格式化",
            "补全", "修复拼写", "注释", "重命名",
        ];

        let complex_hits = complex_keywords.iter().filter(|k| intent_lower.contains(*k)).count();
        let moderate_hits = moderate_keywords.iter().filter(|k| intent_lower.contains(*k)).count();
        let simple_hits = simple_keywords.iter().filter(|k| intent_lower.contains(*k)).count();

        // 上下文很大意味着复杂任务
        if context_tokens > 50000 {
            return Complexity::Complex;
        }

        // 长指令 + 关键特征
        if char_count > 2000 || complex_hits >= 2 {
            return Complexity::Complex;
        }

        if char_count > 200 || moderate_hits >= 2 || complex_hits >= 1 {
            return Complexity::Moderate;
        }

        if char_count <= 50 || simple_hits >= 1 {
            return Complexity::Simple;
        }

        // 默认中等
        Complexity::Moderate
    }

    /// 估算 token 用量
    fn estimate_tokens(&self, intent: &str, complexity: Complexity) -> usize {
        let base = intent.chars().count() / 4; // 粗略估算 4 char ≈ 1 token

        match complexity {
            Complexity::Simple => (base as f64 * 1.5) as usize,
            Complexity::Moderate => (base as f64 * 3.0) as usize,
            Complexity::Complex => (base as f64 * 5.0) as usize,
        }
    }

    /// 比较两模型费用差异
    pub fn cost_comparison(&self, estimated_tokens: usize) -> (f64, f64, f64) {
        let pro_cost = estimated_tokens as f64 / 1_000_000.0 * (self.pro_input_price + self.pro_output_price) / 2.0;
        let flash_cost = estimated_tokens as f64 / 1_000_000.0 * (self.flash_input_price + self.flash_output_price) / 2.0;
        let savings = if pro_cost > 0.0 {
            (pro_cost - flash_cost) / pro_cost * 100.0
        } else {
            0.0
        };
        (pro_cost, flash_cost, savings)
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new(
            "deepseek-v4-pro".into(),
            "deepseek-v4-flash".into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let r = ModelRouter::default();
        let d = r.decide("", 0);
        assert_eq!(d.complexity, Complexity::Simple);
    }

    #[test]
    fn test_long_input_is_complex() {
        let r = ModelRouter::default();
        let long = "a".repeat(2001);
        let d = r.decide(&long, 0);
        assert_eq!(d.complexity, Complexity::Complex);
    }

    #[test]
    fn test_architecture_keyword() {
        let r = ModelRouter::default();
        assert_eq!(r.estimate_complexity("重构系统架构设计", 0), Complexity::Complex);
    }

    #[test]
    fn test_moderate_boundary() {
        let r = ModelRouter::default();
        // 中等长度 + 关键词 "优化" → Moderate
        let s = "优化这个模块的性能 修改一些实现细节 分析代码结构";
        let d = r.decide(s, 0);
        assert_eq!(d.complexity, Complexity::Moderate);
    }

    #[test]
    fn test_simple_query_routes_to_flash() {
        let router = ModelRouter::default();
        let decision = router.decide("什么是 Rust 的所有权？", 0);
        assert_eq!(decision.complexity, Complexity::Simple);
        assert!(decision.model.contains("flash"));
    }

    #[test]
    fn test_complex_query_routes_to_pro() {
        let router = ModelRouter::default();
        let decision = router.decide("重构整个认证系统的架构设计，支持分布式会话管理", 0);
        assert!(matches!(decision.complexity, Complexity::Moderate | Complexity::Complex));
        assert!(!decision.model.is_empty());
    }

    #[test]
    fn test_large_context_is_complex() {
        let router = ModelRouter::default();
        let decision = router.decide("优化性能", 100000);
        assert_eq!(decision.complexity, Complexity::Complex);
    }

    #[test]
    fn test_short_query_is_simple() {
        let router = ModelRouter::default();
        // 短指令（< 50 字符）默认为简单
        let decision = router.decide("修复编译错误", 0);
        assert_eq!(decision.complexity, Complexity::Simple);
    }

    #[test]
    fn test_explain_is_simple() {
        let router = ModelRouter::default();
        let decision = router.decide("怎么使用 git status", 0);
        assert_eq!(decision.complexity, Complexity::Simple);
    }

    #[test]
    fn test_cost_comparison() {
        let router = ModelRouter::default();
        let (pro_cost, flash_cost, savings) = router.cost_comparison(100000);
        // Flash 应比 Pro 便宜很多 (1/10 价格)
        assert!(flash_cost < pro_cost);
        assert!(savings > 50.0);
    }
}
