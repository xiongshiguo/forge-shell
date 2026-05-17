//! 进化引擎（feature-gated: `evolution`）
//!
//! 进化闭环：
//! ```
//! 用户使用 → 脱敏复盘 → 一键分享 → 公共池
//!                                      ↓
//! 用户变聪明 ← 自动匹配 SOP ← 天工阁 ← 反思引擎提炼
//! ```
//!
//! 代码变异仅在 `self-evolve/` 分支进行，需全测试通过 + 锻师会审核。

use serde::{Deserialize, Serialize};

// ---- 反思引擎 ----

/// 复盘记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub id: String,
    /// 脱敏后的策略描述
    pub strategy_description: String,
    /// 任务类别标签
    pub tags: Vec<String>,
    /// 成功/失败
    pub outcome: ReviewOutcome,
    /// 效率评分 (0.0-1.0)
    pub efficiency_score: f64,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 被采纳次数
    pub adoption_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewOutcome {
    Success,
    PartialSuccess,
    Failure,
}

/// 反思引擎
///
/// 由 Gitee Go / GitHub Actions 每周运行一次（调用 DeepSeek Flash API）
/// 从经验熔池中提炼优秀策略，生成 SOP
pub struct ReflectionEngine {
    /// 反思间隔（天）
    interval_days: u32,
    /// 最小置信度阈值
    min_confidence: f64,
}

impl ReflectionEngine {
    pub fn new() -> Self {
        Self {
            interval_days: 7,
            min_confidence: 0.7,
        }
    }

    /// 设置反思间隔
    pub fn with_interval(mut self, days: u32) -> Self {
        self.interval_days = days;
        self
    }

    /// 分析复盘记录，提炼策略模式
    ///
    /// 实际实现需调用 AI API 进行聚类和模式识别
    pub fn analyze(&self, _records: &[ReviewRecord]) -> Vec<StrategyPattern> {
        // 预留：调用 DeepSeek Flash API 分析复盘
        // 返回提炼出的策略模式
        Vec::new()
    }

    /// 生成 SOP 草稿
    pub fn generate_sop(&self, _patterns: &[StrategyPattern]) -> Vec<SopDraft> {
        // 预留：将策略模式转化为 SOP 草稿
        Vec::new()
    }

    /// 判断是否应该运行反思
    pub fn should_run(&self, last_run: chrono::DateTime<chrono::Utc>) -> bool {
        let elapsed = chrono::Utc::now() - last_run;
        elapsed.num_days() >= self.interval_days as i64
    }
}

impl Default for ReflectionEngine {
    fn default() -> Self { Self::new() }
}

// ---- 策略模式 ----

/// 从复盘数据中提取的策略模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyPattern {
    pub pattern_name: String,
    pub description: String,
    pub applicable_scenarios: Vec<String>,
    pub confidence: f64,
    pub adoption_count: u64,
    pub source_review_ids: Vec<String>,
}

// ---- 天工阁 (SOP 库) ----

/// SOP（标准操作流程）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopEntry {
    pub id: String,
    pub title: String,
    pub version: String,
    /// 适用场景
    pub triggers: Vec<String>,
    /// 步骤列表
    pub steps: Vec<SopStep>,
    /// 前置条件
    pub preconditions: Vec<String>,
    /// 后置条件
    pub postconditions: Vec<String>,
    /// 作者
    pub author: String,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 使用次数
    pub usage_count: u64,
    /// 成功率（基于用户反馈）
    pub success_rate: f64,
    /// 状态
    pub status: SopStatus,
}

/// SOP 步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopStep {
    pub order: u32,
    pub action: String,
    pub expected_result: String,
    pub tool_hint: Option<String>,
}

/// SOP 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SopStatus {
    Draft,
    Active,
    Deprecated,
    SupersededBy(String),
}

/// SOP 草稿（反思引擎自动生成）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SopDraft {
    pub title: String,
    pub proposed_triggers: Vec<String>,
    pub proposed_steps: Vec<SopStep>,
    pub confidence: f64,
    pub source_patterns: Vec<String>,
}

/// 天工阁 (SOP 库)
pub struct SopLibrary {
    entries: Vec<SopEntry>,
    /// 基于触发条件的索引
    trigger_index: std::collections::HashMap<String, Vec<String>>,
}

impl SopLibrary {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            trigger_index: std::collections::HashMap::new(),
        }
    }

    /// 添加 SOP
    pub fn add(&mut self, sop: SopEntry) {
        for trigger in &sop.triggers {
            self.trigger_index
                .entry(trigger.clone())
                .or_default()
                .push(sop.id.clone());
        }
        self.entries.push(sop);
    }

    /// 根据上下文匹配 SOP
    pub fn match_context(&self, _context: &str) -> Vec<&SopEntry> {
        // 预留：使用向量相似度匹配最相关的 SOP
        self.entries.iter().collect()
    }

    /// 按标签搜索
    pub fn search_by_trigger(&self, trigger: &str) -> Vec<&SopEntry> {
        self.trigger_index
            .get(trigger)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.entries.iter().find(|e| &e.id == id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// SOP 总数
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for SopLibrary {
    fn default() -> Self { Self::new() }
}

// ---- 代码变异器 ----

/// 变异策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MutationStrategy {
    /// 重命名变量
    RenameVariable { from: String, to: String },
    /// 提取函数
    ExtractFunction { block_start: usize, block_end: usize, suggested_name: String },
    /// 内联变量
    InlineVariable { name: String },
    /// 简化条件表达式
    SimplifyConditional { line: usize },
    /// 移除死代码
    RemoveDeadCode { start_line: usize, end_line: usize },
}

/// 变异提案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationProposal {
    pub id: String,
    pub strategy: MutationStrategy,
    pub file_path: String,
    pub description: String,
    pub confidence: f64,
    /// 是否通过测试验证
    pub test_verified: bool,
}

/// 代码变异器
///
/// 仅在 `self-evolve/` 分支运行，需全测试通过 + 锻师会审核 + 大锻师确认 PR
pub struct Mutator {
    /// 安全模式：仅允许安全的变异策略
    safe_mode: bool,
}

impl Mutator {
    pub fn new() -> Self {
        Self { safe_mode: true }
    }

    /// 分析代码生成变异提案
    pub fn analyze(&self, _source: &str) -> Vec<MutationProposal> {
        // 预留：基于 AST 分析生成变异提案
        Vec::new()
    }

    /// 应用变异
    pub fn apply(&self, _proposal: &MutationProposal, _source: &str) -> Option<String> {
        // 预留：应用变异策略生成新代码
        None
    }

    /// 验证变异后测试是否全绿
    pub fn verify(&self, _proposal: &MutationProposal) -> bool {
        // 预留：运行完整测试套件
        false
    }

    /// 设置安全模式
    pub fn with_safe_mode(mut self, safe: bool) -> Self {
        self.safe_mode = safe;
        self
    }
}

impl Default for Mutator {
    fn default() -> Self { Self::new() }
}

// ---- 进化协调器 ----

/// 进化协调器：串联反思→SOP→变异 全流程
pub struct EvolutionCoordinator {
    pub reflection: ReflectionEngine,
    pub sop_library: SopLibrary,
    pub mutator: Mutator,
    /// 上次反思时间
    pub last_reflection: Option<chrono::DateTime<chrono::Utc>>,
}

impl EvolutionCoordinator {
    pub fn new() -> Self {
        Self {
            reflection: ReflectionEngine::new(),
            sop_library: SopLibrary::new(),
            mutator: Mutator::new(),
            last_reflection: None,
        }
    }

    /// 运行一次完整进化周期
    pub fn run_cycle(&mut self, reviews: &[ReviewRecord]) -> EvolutionReport {
        let mut report = EvolutionReport::default();

        // 1. 反思引擎分析复盘
        if self.should_reflect() {
            let patterns = self.reflection.analyze(reviews);
            report.patterns_discovered = patterns.len();

            // 2. 生成 SOP 草稿
            let drafts = self.reflection.generate_sop(&patterns);
            report.sops_generated = drafts.len();

            self.last_reflection = Some(chrono::Utc::now());
        }

        report
    }

    fn should_reflect(&self) -> bool {
        match self.last_reflection {
            Some(last) => self.reflection.should_run(last),
            None => true,
        }
    }
}

impl Default for EvolutionCoordinator {
    fn default() -> Self { Self::new() }
}

/// 进化周期报告
#[derive(Debug, Clone, Default)]
pub struct EvolutionReport {
    pub patterns_discovered: usize,
    pub sops_generated: usize,
    pub mutations_proposed: usize,
    pub mutations_applied: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflection_engine_default() {
        let engine = ReflectionEngine::new();
        let last_run = chrono::Utc::now() - chrono::Duration::days(8);
        assert!(engine.should_run(last_run));
        let recent = chrono::Utc::now() - chrono::Duration::days(1);
        assert!(!engine.should_run(recent));
    }

    #[test]
    fn test_sop_library_search() {
        let mut lib = SopLibrary::new();
        let sop = SopEntry {
            id: "test-1".into(),
            title: "测试 SOP".into(),
            version: "1.0".into(),
            triggers: vec!["rust".into(), "error".into()],
            steps: vec![],
            preconditions: vec![],
            postconditions: vec![],
            author: "test".into(),
            created_at: chrono::Utc::now(),
            usage_count: 0,
            success_rate: 1.0,
            status: SopStatus::Active,
        };
        lib.add(sop);

        assert_eq!(lib.len(), 1);
        assert_eq!(lib.search_by_trigger("rust").len(), 1);
        assert!(lib.search_by_trigger("python").is_empty());
    }

    #[test]
    fn test_mutator_safe_mode() {
        let mutator = Mutator::new();
        let proposals = mutator.analyze("fn main() {}");
        assert!(proposals.is_empty()); // 预留实现，应返回空
    }

    #[test]
    fn test_coordinator_cycle() {
        let mut coordinator = EvolutionCoordinator::new();
        let report = coordinator.run_cycle(&[]);
        // 首次应运行反思
        assert!(coordinator.last_reflection.is_some());
    }
}
