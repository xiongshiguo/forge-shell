//! 任务分发器：将子任务分发给 AI 后端执行

use crate::config::Config;

/// 任务分发器
pub struct Dispatcher {
    config: Config,
}

impl Dispatcher {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}
