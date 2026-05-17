//! 工具注册表：管理所有可用工具

use std::collections::HashMap;

/// 工具描述
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 工具注册表
pub struct ToolRegistry {
    tools: HashMap<String, ToolInfo>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// 注册工具
    pub fn register(&mut self, tool: ToolInfo) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// 列出所有工具
    pub fn list(&self) -> Vec<&ToolInfo> {
        self.tools.values().collect()
    }

    /// 按名称获取工具
    pub fn get(&self, name: &str) -> Option<&ToolInfo> {
        self.tools.get(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
