//! 进化引擎：经验采集 → 反思提炼 → SOP 匹配 → 自我进化

pub mod collector;
pub mod reflection;
pub mod sop;

use collector::ExperienceCollector;
use reflection::ReflectionEngine;
use sop::{SopLibrary, MatchResult};
use serde::Serialize;
use std::path::PathBuf;

/// 进化协调器
pub struct EvolutionCoordinator {
    pub collector: ExperienceCollector,
    pub reflection: ReflectionEngine,
    pub sop_library: SopLibrary,
    last_reflection: Option<chrono::DateTime<chrono::Utc>>,
}

impl EvolutionCoordinator {
    pub fn new(data_dir: PathBuf) -> Self {
        let exp_dir = data_dir.join("experiences");
        let sop_file = data_dir.join("sop_library.json");
        Self {
            collector: ExperienceCollector::new(exp_dir),
            reflection: ReflectionEngine::new(),
            sop_library: SopLibrary::new(sop_file),
            last_reflection: None,
        }
    }

    pub fn record_turn(&mut self, user_input: &str, assistant_output: &str, success: bool) {
        self.collector.record(user_input, assistant_output, success);
    }

    /// 会话结束，可能触发反思
    pub fn end_session(&mut self) -> collector::SessionDigest {
        let digest = self.collector.end_session();
        if digest.total_turns >= 20 { self.try_reflect(); }
        digest
    }

    pub fn try_reflect(&mut self) {
        let records = self.collector.load_all_records();
        let new_sops = self.reflection.reflect(&records, &mut self.sop_library);
        if !new_sops.is_empty() {
            tracing::info!("🧠 反思引擎生成了 {} 条新 SOP", new_sops.len());
        }
        self.last_reflection = Some(chrono::Utc::now());
    }

    pub fn match_sop(&self, input: &str) -> Vec<MatchResult> {
        self.sop_library.match_input(input)
    }

    pub fn summary(&self) -> EvolutionSummary {
        let records = self.collector.load_all_records();
        let total = records.len();
        let success_rate = if total > 0 {
            records.iter().filter(|r| r.success).count() as f64 / total as f64
        } else { 0.0 };

        EvolutionSummary {
            total_experiences: total,
            success_rate,
            sop_count: self.sop_library.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EvolutionSummary {
    pub total_experiences: usize,
    pub success_rate: f64,
    pub sop_count: usize,
}
