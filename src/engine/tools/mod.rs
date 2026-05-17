//! 沙箱化工具：执行、读、写、搜索、Git 操作

/// 工具类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolType {
    Bash,
    Read,
    Write,
    Search,
    Git,
}

/// 工具执行结果
#[derive(Debug)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// 工具执行器（预留沙箱接口）
pub struct ToolExecutor;

impl ToolExecutor {
    pub fn new() -> Self {
        Self
    }

    pub async fn execute(&self, _tool: ToolType, _input: &str) -> ToolResult {
        // 阶段 3 完善
        ToolResult {
            success: true,
            output: String::new(),
            error: None,
            duration_ms: 0,
        }
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}
