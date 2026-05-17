//! 任务编排器：拆解用户意图为子任务，并行调度子 Agent
//!
//! 核心策略：
//! 1. 读优先：所有读取操作并行执行
//! 2. 依赖检测：写操作等待依赖的读操作完成
//! 3. 负载均衡：最多 max_parallel 个并发

use std::collections::{HashMap, HashSet, VecDeque};

/// 子任务
#[derive(Debug, Clone)]
pub struct SubTask {
    pub id: String,
    pub description: String,
    /// 依赖的子任务 ID 列表
    pub depends_on: Vec<String>,
    /// 是否为只读任务
    pub read_only: bool,
    /// 估计 token 数
    pub estimated_tokens: usize,
    /// 优先级（越小越优先）
    pub priority: u8,
}

/// 编排结果
#[derive(Debug)]
pub struct OrchestrationPlan {
    pub tasks: Vec<SubTask>,
    /// 并行组：每组内的任务可同时执行
    pub parallel_groups: Vec<Vec<String>>,
    /// 预估总 token 数
    pub estimated_total_tokens: usize,
    /// 预估并行度提升倍数
    pub parallelism_gain: f64,
}

/// 任务编排器
pub struct Orchestrator {
    max_parallel: usize,
}

impl Orchestrator {
    pub fn new(max_parallel: usize) -> Self {
        Self { max_parallel }
    }

    /// 将用户意图拆解为可并行执行的子任务
    pub fn decompose(&self, intent: &str) -> OrchestrationPlan {
        let intent_lower = intent.to_lowercase();

        // 检测任务模式
        let tasks = if intent_lower.contains("搜索") || intent_lower.contains("找") {
            self.decompose_search(intent)
        } else if intent_lower.contains("重构") || intent_lower.contains("改") {
            self.decompose_refactor(intent)
        } else if intent_lower.contains("测试") {
            self.decompose_test(intent)
        } else if intent_lower.contains("分析") || intent_lower.contains("检查") {
            self.decompose_analyze(intent)
        } else {
            self.decompose_general(intent)
        };

        let parallel_groups = self.group_by_dependency(&tasks);

        let total_tokens: usize = tasks.iter().map(|t| t.estimated_tokens).sum();
        let ideal_tokens = total_tokens as f64 / self.max_parallel as f64;
        let parallelism_gain = if parallel_groups.is_empty() { 1.0 }
            else { total_tokens as f64 / (ideal_tokens * parallel_groups.len() as f64) };

        OrchestrationPlan {
            tasks,
            parallel_groups,
            estimated_total_tokens: total_tokens,
            parallelism_gain,
        }
    }

    /// 搜索类任务：可高度并行
    fn decompose_search(&self, _intent: &str) -> Vec<SubTask> {
        vec![
            SubTask { id: "s1".into(), description: "搜索当前项目文件".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 200, priority: 0 },
            SubTask { id: "s2".into(), description: "搜索依赖库文档".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 500, priority: 0 },
            SubTask { id: "s3".into(), description: "汇总搜索结果".into(),
                depends_on: vec!["s1".into(), "s2".into()], read_only: true, estimated_tokens: 300, priority: 1 },
        ]
    }

    /// 重构类任务：先分析再修改
    fn decompose_refactor(&self, intent: &str) -> Vec<SubTask> {
        vec![
            SubTask { id: "r1".into(), description: "分析现有代码结构".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 400, priority: 0 },
            SubTask { id: "r2".into(), description: "查找所有引用点".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 300, priority: 0 },
            SubTask { id: "r3".into(), description: "生成重构方案".into(),
                depends_on: vec!["r1".into(), "r2".into()], read_only: true, estimated_tokens: 500, priority: 1 },
            SubTask { id: "r4".into(), description: "执行重构修改".into(),
                depends_on: vec!["r3".into()], read_only: false, estimated_tokens: 800, priority: 2 },
            SubTask { id: "r5".into(), description: "验证重构后测试".into(),
                depends_on: vec!["r4".into()], read_only: true, estimated_tokens: 300, priority: 3 },
        ]
    }

    /// 测试类任务
    fn decompose_test(&self, intent: &str) -> Vec<SubTask> {
        vec![
            SubTask { id: "t1".into(), description: "分析被测代码".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 300, priority: 0 },
            SubTask { id: "t2".into(), description: "编写单元测试".into(),
                depends_on: vec!["t1".into()], read_only: false, estimated_tokens: 500, priority: 1 },
            SubTask { id: "t3".into(), description: "运行测试验证".into(),
                depends_on: vec!["t2".into()], read_only: true, estimated_tokens: 200, priority: 2 },
        ]
    }

    /// 分析类任务：只读，高度并行
    fn decompose_analyze(&self, intent: &str) -> Vec<SubTask> {
        vec![
            SubTask { id: "a1".into(), description: "代码结构分析".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 400, priority: 0 },
            SubTask { id: "a2".into(), description: "性能瓶颈分析".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 400, priority: 0 },
            SubTask { id: "a3".into(), description: "安全漏洞检查".into(),
                depends_on: vec![], read_only: true, estimated_tokens: 400, priority: 0 },
            SubTask { id: "a4".into(), description: "汇总分析报告".into(),
                depends_on: vec!["a1".into(), "a2".into(), "a3".into()], read_only: true, estimated_tokens: 600, priority: 1 },
        ]
    }

    /// 通用任务拆解
    fn decompose_general(&self, intent: &str) -> Vec<SubTask> {
        vec![
            SubTask { id: "g1".into(), description: format!("执行: {}", intent),
                depends_on: vec![], read_only: false, estimated_tokens: intent.len() / 4, priority: 0 },
        ]
    }

    /// 按依赖关系分组：读优先 + 拓扑排序
    fn group_by_dependency(&self, tasks: &[SubTask]) -> Vec<Vec<String>> {
        if tasks.is_empty() {
            return vec![];
        }

        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut completed: HashSet<String> = HashSet::new();
        let mut remaining: VecDeque<SubTask> = tasks.iter().cloned().collect();

        while !remaining.is_empty() {
            let mut current_group: Vec<String> = Vec::new();
            let mut next_round: VecDeque<SubTask> = VecDeque::new();

            for task in remaining {
                let deps_met = task.depends_on.iter().all(|d| completed.contains(d));
                let space_left = current_group.len() < self.max_parallel;

                if deps_met && space_left {
                    if task.read_only || current_group.is_empty() {
                        current_group.push(task.id.clone());
                        // 注意：此处不立即标记为 completed，而是等整组完成后
                    } else {
                        next_round.push_back(task);
                    }
                } else if deps_met {
                    next_round.push_back(task);
                } else {
                    next_round.push_back(task);
                }
            }

            // 整组执行完成后才标记为 completed
            for id in &current_group {
                completed.insert(id.clone());
            }

            if current_group.is_empty() && !next_round.is_empty() {
                // 如果没有能放入的任务，放一个进去防止死锁
                let task = next_round.pop_front().unwrap();
                current_group.push(task.id.clone());
                completed.insert(task.id.clone());
            }

            if !current_group.is_empty() {
                groups.push(current_group);
            }
            remaining = next_round;
        }

        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_decomposition() {
        let orch = Orchestrator::new(8);
        let plan = orch.decompose("搜索用户认证相关代码");
        assert!(plan.tasks.len() >= 3);
        assert!(plan.parallel_groups.len() <= 3);
    }

    #[test]
    fn test_dependency_grouping() {
        let orch = Orchestrator::new(8);
        let plan = orch.decompose("重构登录模块");
        assert!(!plan.parallel_groups.is_empty());
        // r1 和 r2 应该在第一批并行
        let first_group = &plan.parallel_groups[0];
        assert!(first_group.contains(&"r1".to_string()));
        assert!(first_group.contains(&"r2".to_string()));
    }

    #[test]
    fn test_analyze_full_parallel() {
        let orch = Orchestrator::new(8);
        let plan = orch.decompose("分析代码质量");
        // a1, a2, a3 都应能并行
        let first_group = &plan.parallel_groups[0];
        assert_eq!(first_group.len(), 3);
    }
}
