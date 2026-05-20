//! 反思引擎：从经验池提炼通用策略模式，生成 SOP
//! 本地运行，不依赖外部 API，隐私优先

use super::collector::ExperienceRecord;
use super::sop::{SopEntry, SopLibrary, SopStep};
use std::collections::HashMap;

/// 策略模式（从经验中提取）
#[derive(Debug, Clone)]
pub struct StrategyPattern {
    pub pattern_name: String,
    pub description: String,
    pub trigger_keywords: Vec<String>,
    pub recommendation: String,
    pub confidence: f64,
    pub sample_count: usize,
}

/// 反思引擎
pub struct ReflectionEngine {
    /// 最小置信度阈值
    min_confidence: f64,
    /// 形成策略所需的最小样本数
    min_samples: usize,
}

impl ReflectionEngine {
    pub fn new() -> Self {
        Self {
            min_confidence: 0.6,
            min_samples: 3,
        }
    }

    /// 分析经验记录，生成新 SOP
    pub fn reflect(&self, records: &[ExperienceRecord], sop_lib: &mut SopLibrary) -> Vec<String> {
        let patterns = self.extract_patterns(records);
        let mut new_sop_ids = Vec::new();

        for pattern in patterns {
            if pattern.confidence >= self.min_confidence && pattern.sample_count >= self.min_samples {
                // 检查是否已有相似的 SOP
                if sop_lib.find_similar(&pattern.pattern_name).is_some() {
                    continue;
                }

                let sop = SopEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: pattern.pattern_name.clone(),
                    description: pattern.description,
                    triggers: pattern.trigger_keywords,
                    steps: vec![SopStep {
                        order: 1,
                        action: pattern.recommendation.clone(),
                        expected_result: "任务正确完成".into(),
                        tool_hint: None,
                    }],
                    created_at: chrono::Utc::now(),
                    usage_count: 0,
                    success_rate: pattern.confidence,
                    source: "reflection".into(),
                    fingerprint: None,
                };

                sop_lib.add(sop.clone());
                new_sop_ids.push(sop.id);
            }
        }

        new_sop_ids
    }

    /// 从经验记录中提取策略模式
    fn extract_patterns(&self, records: &[ExperienceRecord]) -> Vec<StrategyPattern> {
        let mut patterns: Vec<StrategyPattern> = Vec::new();

        // 只分析成功的记录
        let successes: Vec<_> = records.iter().filter(|r| r.success).collect();
        if successes.is_empty() {
            return patterns;
        }

        // 按意图类型分组
        let mut by_intent: HashMap<String, Vec<&ExperienceRecord>> = HashMap::new();
        for r in &successes {
            by_intent.entry(r.intent_type.clone()).or_default().push(r);
        }

        for (intent, group) in &by_intent {
            if group.len() >= self.min_samples {
                // 计算平均输入/输出长度
                let avg_in: f64 = group.iter().map(|r| r.input_length as f64).sum::<f64>() / group.len() as f64;
                let avg_out: f64 = group.iter().map(|r| r.output_length as f64).sum::<f64>() / group.len() as f64;

                // 收集常用标签
                let mut tag_counts: HashMap<String, usize> = HashMap::new();
                for r in group.iter() {
                    for tag in &r.tags {
                        *tag_counts.entry(tag.clone()).or_default() += 1;
                    }
                }
                let top_tags: Vec<_> = {
                    let mut v: Vec<_> = tag_counts.into_iter().collect();
                    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
                    v.truncate(3);
                    v.into_iter().map(|(k, _)| k).collect()
                };

                let confidence = if group.len() >= 10 { 0.9 }
                    else if group.len() >= 5 { 0.75 }
                    else { 0.6 };

                let recommendation = match intent.as_str() {
                    "search" => "先搜索项目中相关代码 → 再查依赖库文档 → 汇总结果".into(),
                    "refactor" => "分析现有代码 → 查找所有引用 → 生成方案 → 执行 → 验证".into(),
                    "test" => "分析被测代码 → 编写测试 → 运行验证 → 修复失败".into(),
                    "analyze" => "并行分析：代码结构/性能/安全 → 汇总报告".into(),
                    "explain" => "先给简洁答案 → 再给代码示例 → 最后讲原理".into(),
                    _ => format!("{} 类任务已积累 {} 条成功经验", intent, group.len()),
                };

                patterns.push(StrategyPattern {
                    pattern_name: format!("{}_{}", intent, top_tags.join("_")),
                    description: format!("{} 类任务，平均输入 {} 字符，输出 {} 字符，成功率 100%", intent, avg_in as usize, avg_out as usize),
                    trigger_keywords: top_tags,
                    recommendation,
                    confidence,
                    sample_count: group.len(),
                });
            }
        }

        patterns
    }

    /// 设置最小样本数
    pub fn with_min_samples(mut self, n: usize) -> Self {
        self.min_samples = n;
        self
    }
}

impl Default for ReflectionEngine {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(intent: &str, tags: Vec<&str>, success: bool) -> ExperienceRecord {
        ExperienceRecord {
            id: uuid::Uuid::new_v4().to_string(),
            intent_type: intent.into(),
            complexity: "simple".into(),
            input_length: 100,
            output_length: 200,
            success,
            tools_used: vec![],
            turn_count: 1,
            timestamp: chrono::Utc::now(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            strategy_note: "test".into(),
        }
    }

    #[test]
    fn test_extract_patterns_from_successes() {
        let engine = ReflectionEngine::new().with_min_samples(3);
        let mut records = Vec::new();
        for _ in 0..4 {
            records.push(make_record("search", vec!["rust", "api"], true));
        }
        let patterns = engine.extract_patterns(&records);
        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].sample_count, 4);
    }

    #[test]
    fn test_ignores_failures() {
        let engine = ReflectionEngine::new().with_min_samples(1);
        let records = vec![
            make_record("search", vec!["rust"], false),
            make_record("search", vec!["rust"], false),
        ];
        let patterns = engine.extract_patterns(&records);
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_min_samples_respected() {
        let engine = ReflectionEngine::new().with_min_samples(5);
        let mut records = Vec::new();
        for _ in 0..2 {
            records.push(make_record("search", vec!["rust"], true));
        }
        let patterns = engine.extract_patterns(&records);
        assert!(patterns.is_empty());
    }
}
