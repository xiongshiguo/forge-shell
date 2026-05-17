//! 任务分发器：将子任务分发给 AI 后端，支持并行执行
//!
//! 实现 8-16 路并行调度，结果合并 diff

use crate::config::Config;
use crate::agent::orchestrator::{OrchestrationPlan, SubTask};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// 子任务执行结果
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub tokens_used: u64,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// 合并后的结果
#[derive(Debug)]
pub struct MergedResult {
    pub results: Vec<TaskResult>,
    pub total_tokens: u64,
    pub total_duration_ms: u64,
    pub success_count: usize,
    pub failure_count: usize,
}

/// 任务分发器
pub struct Dispatcher {
    config: Config,
    semaphore: Arc<Semaphore>,
}

impl Dispatcher {
    pub fn new(config: Config, max_concurrent: usize) -> Self {
        Self {
            config,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// 分发执行编排计划
    pub async fn dispatch(&self, plan: OrchestrationPlan) -> MergedResult {
        let mut all_results: Vec<TaskResult> = Vec::new();
        let mut total_tokens = 0u64;
        let mut total_duration = 0u64;
        let mut success_count = 0;
        let mut failure_count = 0;

        for group in &plan.parallel_groups {
            let mut handles = Vec::new();

            for task_id in group {
                if let Some(task) = plan.tasks.iter().find(|t| &t.id == task_id) {
                    let task = task.clone();
                    let config = self.config.clone();
                    let sem = self.semaphore.clone();

                    let handle = tokio::spawn(async move {
                        let _permit = sem.acquire().await.unwrap();
                        Self::execute_task(&config, &task).await
                    });
                    handles.push(handle);
                }
            }

            for handle in handles {
                match handle.await {
                    Ok(result) => {
                        total_tokens += result.tokens_used;
                        total_duration += result.duration_ms;
                        if result.success {
                            success_count += 1;
                        } else {
                            failure_count += 1;
                        }
                        all_results.push(result);
                    }
                    Err(e) => {
                        failure_count += 1;
                        all_results.push(TaskResult {
                            task_id: "unknown".into(),
                            success: false,
                            output: String::new(),
                            tokens_used: 0,
                            error: Some(e.to_string()),
                            duration_ms: 0,
                        });
                    }
                }
            }
        }

        MergedResult {
            results: all_results,
            total_tokens,
            total_duration_ms: total_duration,
            success_count,
            failure_count,
        }
    }

    /// 执行单个任务（当前为模拟实现，阶段 3 接入真实 AI 调用）
    async fn execute_task(config: &Config, task: &SubTask) -> TaskResult {
        let start = std::time::Instant::now();

        // 模拟执行延迟
        let delay_ms = if task.read_only { 50 } else { 200 };
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;

        let tokens = task.estimated_tokens as u64;
        TaskResult {
            task_id: task.id.clone(),
            success: true,
            output: format!("[{}] 完成: {}", if task.read_only { "读" } else { "写" }, task.description),
            tokens_used: tokens,
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }
}
